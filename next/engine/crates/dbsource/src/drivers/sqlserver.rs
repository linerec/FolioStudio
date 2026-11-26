//! SQL Server and Azure SQL (`tiberius`, pure Rust, TDS 7.3+). tiberius is built without TLS of its own: the stream it is given (`Wire`)
//! does the TLS of TDS with the same rustls settings as the other drivers (`tls::client_config`: the computer's roots, or the pinned
//! fingerprint, so a self-signed server gets the same "이 인증서 신뢰" flow). The stream asks for encryption in the PRELOGIN, does the TLS
//! handshake inside PRELOGIN packets (TDS 7.x), then encrypts everything that follows, the login too.
//!
//! Values are bound as `@P1…` (`sp_executesql`, as NVARCHAR: the server reads each in the type of the column it is compared with). A select
//! runs inside a transaction that is always rolled back, with `SET ROWCOUNT` as the row cap (the server stops; nothing is left to drain)
//! and `SET LOCK_TIMEOUT` as a bound on waiting for locks. Values come back as text the way SQL Server prints them (`CONVERT(…, 121)` for
//! dates and times, `1`/`0` for a bit, upper-case GUIDs); a binary value has no text (an empty cell, like SQLite's blobs).
use super::{Budget, Conn, Rows};
use crate::error::{DbError, Kind, Result};
use crate::model::{ColumnInfo, Endpoint, TableInfo, TableRef, TlsMode};
use crate::snapshot::Table;
use futures_util::io::{AsyncRead, AsyncWrite};
use rustls::pki_types::ServerName;
use std::future::Future;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tiberius::ColumnData;

/// What went wrong in the stream itself (TLS, the network, a time limit), kept because tiberius only passes on an I/O error kind.
type Slot = Arc<Mutex<Option<DbError>>>;

pub struct SqlServer {
    client: tiberius::Client<Wire>,
    slot: Slot,
    /// The same socket, to change its time limits once logged in.
    socket: TcpStream,
}

/// Runs a future to its end on this thread. The stream blocks instead of waiting (its socket has time limits), so a poll never pends in
/// practice; should one, it is polled again after a moment.
fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// the stream: TCP, and TLS inside the TDS PRELOGIN

const PRELOGIN: u8 = 0x12;
const ENCRYPTION: u8 = 0x01;
const ENCRYPT_ON: u8 = 0x01;
const ENCRYPT_NOT_SUP: u8 = 0x02;
const ENCRYPT_REQ: u8 = 0x03;
/// A TDS packet during the PRELOGIN is at most 4096 bytes, its 8-byte header included.
const PACKET: usize = 4096;

enum Stage {
    /// tiberius is writing its PRELOGIN (gathered until the packet is whole).
    Prelogin(Vec<u8>),
    /// The PRELOGIN is sent; the server's answer is read next (and the TLS handshake done, if any).
    Reply,
    Plain,
    Tls(Box<rustls::ClientConnection>),
}

struct Wire {
    tcp: TcpStream,
    /// TLS settings (`None`: TLS off) and where the certificate the server showed is noted.
    tls: Option<(Arc<rustls::ClientConfig>, ServerName<'static>, crate::tls::Seen)>,
    stage: Stage,
    /// The server's PRELOGIN answer, handed to tiberius as it reads.
    pending: Vec<u8>,
    at: usize,
    slot: Slot,
}

fn io_kind(e: &io::Error) -> Kind {
    if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) { Kind::Timeout } else { Kind::Network }
}

/// One TDS packet header field: the whole length (big endian, header included).
fn packet_len(head: &[u8]) -> usize { usize::from(u16::from_be_bytes([head[2], head[3]])) }

/// Where the ENCRYPTION option's byte is in a PRELOGIN payload (the option table: token, offset, length; 0xFF ends it).
pub(crate) fn encryption_at(payload: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < payload.len() && payload[i] != 0xFF {
        let opt = payload.get(i..i + 5)?;
        let (offset, len) = (usize::from(u16::from_be_bytes([opt[1], opt[2]])), u16::from_be_bytes([opt[3], opt[4]]));
        if opt[0] == ENCRYPTION && len >= 1 && offset < payload.len() { return Some(offset); }
        i += 5;
    }
    None
}

/// TLS bytes in PRELOGIN packets (the last one marks the end of the message).
pub(crate) fn wrap(tls: &[u8], id: &mut u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(tls.len() + 8 * (tls.len() / (PACKET - 8) + 1));
    let chunks: Vec<&[u8]> = tls.chunks(PACKET - 8).collect();
    for (k, chunk) in chunks.iter().enumerate() {
        let len = (chunk.len() + 8) as u16;
        let status = if k + 1 == chunks.len() { 0x01 } else { 0x00 };
        out.extend([PRELOGIN, status]);
        out.extend(len.to_be_bytes());
        out.extend([0, 0, *id, 0]);
        *id = id.wrapping_add(1);
        out.extend_from_slice(chunk);
    }
    out
}

impl Wire {
    fn fail(&self, e: DbError) -> io::Error {
        let kind = if e.kind == Kind::Timeout { io::ErrorKind::TimedOut } else { io::ErrorKind::Other };
        let message = e.message.clone();
        if let Ok(mut s) = self.slot.lock() { s.get_or_insert(e); }
        io::Error::new(kind, message)
    }
    fn net(&self, e: io::Error) -> io::Error {
        let kind = io_kind(&e);
        self.fail(DbError::new(kind, format!("SQL Server connection: {e}")))
    }

    /// One whole TDS packet from the server (header and payload).
    fn packet(&mut self) -> io::Result<Vec<u8>> {
        let mut head = [0u8; 8];
        self.tcp.read_exact(&mut head).map_err(|e| self.net(e))?;
        let len = packet_len(&head);
        if len < 8 { return Err(self.fail(DbError::new(Kind::Network, "the server did not answer like SQL Server"))); }
        let mut out = head.to_vec();
        out.resize(len, 0);
        self.tcp.read_exact(&mut out[8..]).map_err(|e| self.net(e))?;
        Ok(out)
    }

    /// The server's PRELOGIN answer: kept for tiberius; with TLS on, the handshake follows right away.
    fn reply(&mut self) -> io::Result<()> {
        let mut payload = Vec::new();
        loop {
            let p = self.packet()?;
            payload.extend_from_slice(&p[8..]);
            self.pending.extend_from_slice(&p);
            if p[1] & 0x01 != 0 { break; }
            if self.pending.len() > 64 * 1024 { return Err(self.fail(DbError::new(Kind::Network, "the server's PRELOGIN answer is too long"))); }
        }
        let server = encryption_at(&payload).map(|i| payload[i]);
        match (self.tls.is_some(), server) {
            (true, Some(ENCRYPT_NOT_SUP) | None) => Err(self.fail(DbError::new(Kind::Tls, "the server does not offer TLS (choose TLS off only on a trusted network)"))),
            (false, Some(ENCRYPT_REQ)) => Err(self.fail(DbError::new(Kind::Tls, "the server requires TLS: turn TLS on for this connection"))),
            (false, _) => { self.stage = Stage::Plain; Ok(()) }
            (true, _) => self.handshake(),
        }
    }

    /// The TLS handshake, its records inside PRELOGIN packets (MS-TDS 2.2.6.5); afterwards every byte goes as TLS.
    fn handshake(&mut self) -> io::Result<()> {
        let (config, name, seen) = self.tls.clone().expect("TLS settings");
        let shown = || seen.lock().ok().and_then(|s| s.clone());
        let tls_err = |w: &Self, m: String| { let mut d = DbError::new(Kind::Tls, m); d.fingerprint = shown(); w.fail(d) };
        let mut conn = rustls::ClientConnection::new(config, name).map_err(|e| tls_err(self, e.to_string()))?;
        let mut id = 1u8;
        loop {
            while conn.wants_write() {
                let mut out = Vec::new();
                conn.write_tls(&mut out).map_err(|e| tls_err(self, e.to_string()))?;
                let packets = wrap(&out, &mut id);
                self.tcp.write_all(&packets).map_err(|e| self.net(e))?;
            }
            if !conn.is_handshaking() { break; }
            let p = self.packet()?;
            let mut rest = &p[8..];
            while !rest.is_empty() {
                conn.read_tls(&mut rest).map_err(|e| tls_err(self, e.to_string()))?;
                if let Err(e) = conn.process_new_packets() {
                    // the alert that says why, then the refusal (with the certificate the server showed)
                    let mut out = Vec::new();
                    if conn.write_tls(&mut out).is_ok() && !out.is_empty() { let _ = self.tcp.write_all(&wrap(&out, &mut id)); }
                    return Err(tls_err(self, format!("TLS: {e}")));
                }
            }
        }
        self.stage = Stage::Tls(Box::new(conn));
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at < self.pending.len() {
            let n = buf.len().min(self.pending.len() - self.at);
            buf[..n].copy_from_slice(&self.pending[self.at..self.at + n]);
            self.at += n;
            if self.at == self.pending.len() { self.pending.clear(); self.at = 0; }
            return Ok(n);
        }
        if matches!(self.stage, Stage::Reply) { self.reply()?; return self.read(buf); }
        if matches!(self.stage, Stage::Tls(_)) { return self.tls_read(buf); }
        if matches!(self.stage, Stage::Plain) { return self.tcp.read(buf).map_err(|e| self.net(e)); }
        Err(self.fail(DbError::new(Kind::Network, "read before the PRELOGIN was sent")))
    }

    fn tls_read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let Stage::Tls(conn) = &mut self.stage else { unreachable!() };
            let r = conn.reader().read(buf);
            match r {
                Ok(n) => return Ok(n),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(0),
                Err(e) => return Err(self.net(e)),
            }
            let got = conn.read_tls(&mut self.tcp);
            let n = match got { Ok(n) => n, Err(e) => return Err(self.net(e)) };
            if n == 0 { return Ok(0); }
            let Stage::Tls(conn) = &mut self.stage else { unreachable!() };
            if let Err(e) = conn.process_new_packets() {
                let m = format!("TLS: {e}");
                return Err(self.fail(DbError::new(Kind::Tls, m)));
            }
            self.tls_flush()?;
        }
    }

    fn tls_flush(&mut self) -> io::Result<()> {
        let Stage::Tls(conn) = &mut self.stage else { return Ok(()) };
        while conn.wants_write() {
            if let Err(e) = conn.write_tls(&mut self.tcp) { return Err(self.net(e)); }
        }
        Ok(())
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Stage::Prelogin(out) = &mut self.stage {
            out.extend_from_slice(buf);
            if out.len() < 8 || out.len() < packet_len(out) { return Ok(buf.len()); }
            let mut packet = std::mem::take(out);
            self.stage = Stage::Reply;
            if self.tls.is_some() {
                // ask for encryption: tiberius (built without TLS) says "not supported"
                let len = packet_len(&packet);
                match encryption_at(&packet[8..len]) {
                    Some(i) => packet[8 + i] = ENCRYPT_ON,
                    None => return Err(self.fail(DbError::new(Kind::Tls, "the PRELOGIN has no encryption option"))),
                }
            }
            self.tcp.write_all(&packet).map_err(|e| self.net(e))?;
            return Ok(buf.len());
        }
        if let Stage::Tls(conn) = &mut self.stage {
            let r = conn.writer().write_all(buf);
            if let Err(e) = r { return Err(self.net(e)); }
            self.tls_flush()?;
            return Ok(buf.len());
        }
        if matches!(self.stage, Stage::Plain) { return self.tcp.write(buf).map_err(|e| self.net(e)); }
        Err(self.fail(DbError::new(Kind::Network, "write before the server answered the PRELOGIN")))
    }
}

impl AsyncRead for Wire {
    fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> { Poll::Ready(self.get_mut().read(buf)) }
}

impl AsyncWrite for Wire {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> { Poll::Ready(self.get_mut().write(buf)) }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        let w = self.get_mut();
        Poll::Ready(w.tls_flush().and_then(|()| w.tcp.flush()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        let w = self.get_mut();
        if let Stage::Tls(conn) = &mut w.stage { conn.send_close_notify(); }
        let _ = w.tls_flush();
        let _ = w.tcp.shutdown(std::net::Shutdown::Write);
        Poll::Ready(Ok(()))
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// errors

fn err(e: tiberius::error::Error, slot: &Slot) -> DbError {
    if let Some(d) = slot.lock().ok().and_then(|mut s| s.take()) { return d; }
    use tiberius::error::Error as E;
    match e {
        E::Server(t) => {
            let kind = match t.code() {
                18456 | 18452 | 18470 | 18486 | 18487 | 18488 => Kind::Auth,
                4060 | 208 | 207 | 911 => Kind::NotFound,
                1222 => Kind::Timeout,
                _ => Kind::Sql,
            };
            DbError::new(kind, format!("{} (SQL Server error {})", t.message(), t.code()))
        }
        E::Io { kind, message } => DbError::new(if matches!(kind, io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) { Kind::Timeout } else { Kind::Network }, message),
        E::Tls(m) => DbError::new(Kind::Tls, m),
        other => DbError::new(Kind::Network, other.to_string()),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// connecting

/// A named instance's port from the SQL Server Browser (UDP 1434, MC-SQLR `CLNT_UCAST_INST`): the answer is
/// `ServerName;…;InstanceName;…;tcp;1433;;`.
fn browse(host: &str, instance: &str) -> Result<u16> {
    let timeout = Duration::from_millis(crate::CONNECT_TIMEOUT_MS);
    let net = |e: io::Error| DbError::new(io_kind(&e), format!("SQL Server Browser on {host}: {e}"));
    let addr = (host, 1434).to_socket_addrs().map_err(net)?.next().ok_or_else(|| DbError::new(Kind::Network, "the host has no address"))?;
    let sock = std::net::UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).map_err(net)?;
    sock.set_read_timeout(Some(timeout)).map_err(net)?;
    let mut ask = vec![0x04];
    ask.extend_from_slice(instance.as_bytes());
    sock.send_to(&ask, addr).map_err(net)?;
    let mut buf = vec![0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf).map_err(|e| if io_kind(&e) == Kind::Timeout {
        DbError::new(Kind::NotFound, format!("the SQL Server Browser on {host} did not answer for instance {instance} (give the port instead)"))
    } else { net(e) })?;
    port_of_browser_answer(&buf[..n]).ok_or_else(|| DbError::new(Kind::NotFound, format!("SQL Server instance {instance} has no TCP port on {host}")))
}

pub(crate) fn port_of_browser_answer(answer: &[u8]) -> Option<u16> {
    if answer.len() < 3 || answer[0] != 0x05 { return None; }
    let text = String::from_utf8_lossy(&answer[3..]);
    let parts: Vec<&str> = text.split(';').collect();
    parts.iter().position(|p| p.eq_ignore_ascii_case("tcp")).and_then(|i| parts.get(i + 1)).and_then(|p| p.parse().ok())
}

/// The host and the instance: the `instance` setting, or `host\instance` as SQL Server tools write it.
pub(crate) fn host_and_instance(c: &crate::model::Connection) -> (String, Option<String>) {
    let given = c.instance.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    match c.host.split_once('\\') {
        Some((h, i)) if given.is_none() && !i.trim().is_empty() => (h.trim().to_owned(), Some(i.trim().to_owned())),
        Some((h, _)) => (h.trim().to_owned(), given),
        None => (c.host.trim().to_owned(), given),
    }
}

pub fn open(ep: &Endpoint) -> Result<SqlServer> {
    let c = &ep.connection;
    let (host, instance) = host_and_instance(c);
    let port = match (c.port, &instance) {
        (Some(p), _) => p,
        (None, Some(i)) => browse(&host, i)?,
        (None, None) => 1433,
    };
    let scrub = |e: DbError| e.scrub(ep.password.as_ref());
    // Azure SQL may send the client to another server (routing): followed once
    let mut target = (host, port, instance);
    for redirect in 0..2 {
        match connect_to(ep, &target.0, target.1, target.2.as_deref()) {
            Ok(s) => return Ok(s),
            Err(Routed(h, p)) if redirect == 0 => target = (h, p, None),
            Err(Routed(..)) => break,
            Err(Failed(e)) => return Err(scrub(e)),
        }
    }
    Err(DbError::new(Kind::Network, "the server redirected the connection more than once"))
}

enum Opened { Routed(String, u16), Failed(DbError) }
use Opened::{Failed, Routed};

fn connect_to(ep: &Endpoint, host: &str, port: u16, instance: Option<&str>) -> std::result::Result<SqlServer, Opened> {
    let c = &ep.connection;
    let connect_limit = Duration::from_millis(crate::CONNECT_TIMEOUT_MS);
    let net = |e: io::Error| Failed(DbError::new(io_kind(&e), format!("{host}:{port}: {e}")));
    let addrs: Vec<_> = (host, port).to_socket_addrs().map_err(net)?.collect();
    let mut last = None;
    let mut tcp = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, connect_limit) { Ok(t) => { tcp = Some(t); break; } Err(e) => last = Some(e) }
    }
    let tcp = match tcp { Some(t) => t, None => return Err(net(last.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the host has no address")))) };
    tcp.set_read_timeout(Some(connect_limit)).map_err(net)?;
    tcp.set_write_timeout(Some(connect_limit)).map_err(net)?;
    let _ = tcp.set_nodelay(true);
    let socket = tcp.try_clone().map_err(net)?;
    let tls = match c.tls.mode {
        TlsMode::Disable => None,
        _ => {
            let (config, seen) = crate::tls::client_config(&c.tls).map_err(Failed)?;
            let name = ServerName::try_from(host.to_owned()).map_err(|e| Failed(DbError::new(Kind::Tls, e.to_string())))?;
            Some((Arc::new(config), name, seen))
        }
    };
    let slot: Slot = Arc::new(Mutex::new(None));
    let wire = Wire { tcp, tls, stage: Stage::Prelogin(Vec::new()), pending: Vec::new(), at: 0, slot: slot.clone() };
    let mut cfg = tiberius::Config::new();
    cfg.host(host);
    cfg.port(port);
    cfg.database(&c.database);
    cfg.application_name("Folio Studio");
    if let Some(i) = instance { cfg.instance_name(i); }
    cfg.authentication(tiberius::AuthMethod::sql_server(&c.user, ep.password.as_ref().map_or("", |p| p.expose())));
    // tiberius has no TLS here: the stream does it (see `Wire`)
    cfg.encryption(tiberius::EncryptionLevel::NotSupported);
    // the socket's own time limits bound every read (a timer would need a thread)
    cfg.handshake_timeout(None);
    cfg.command_timeout(None);
    match block_on(tiberius::Client::connect(cfg, wire)) {
        Ok(client) => {
            let query_limit = Some(Duration::from_millis(crate::QUERY_TIMEOUT_MS));
            socket.set_read_timeout(query_limit).map_err(net)?;
            socket.set_write_timeout(query_limit).map_err(net)?;
            Ok(SqlServer { client, slot, socket })
        }
        Err(tiberius::error::Error::Routing { host, port }) => Err(Routed(host, port)),
        Err(e) => Err(Failed(err(e, &slot))),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// values as text

/// Days since 1970-01-01 → (year, month, day) (H. Hinnant's `civil_from_days`).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// 0001-01-01 and 1900-01-01 as days since 1970-01-01.
const DAY_1: i64 = -719_162;
const DAY_1900: i64 = -25_567;

fn date(days_since_1970: i64) -> String {
    let (y, m, d) = civil(days_since_1970);
    format!("{y:04}-{m:02}-{d:02}")
}

/// A time of day from units of 10^-scale seconds: `HH:MM:SS` and, for a scale above 0, that many digits of fraction.
fn time(units: u64, scale: u8) -> String {
    let per_second = 10u64.pow(u32::from(scale));
    let (secs, frac) = (units / per_second, units % per_second);
    let base = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    if scale == 0 { base } else { format!("{base}.{frac:0w$}", w = usize::from(scale)) }
}

/// A decimal (`value` × 10^-scale) as SQL Server prints it: every digit of the scale.
fn numeric(value: i128, scale: u8) -> String {
    let digits = value.unsigned_abs().to_string();
    let scale = usize::from(scale);
    let sign = if value < 0 { "-" } else { "" };
    if scale == 0 { return format!("{sign}{digits}"); }
    let padded = format!("{digits:0>w$}", w = scale + 1);
    let (int, frac) = padded.split_at(padded.len() - scale);
    format!("{sign}{int}.{frac}")
}

pub(crate) fn text(v: &ColumnData<'_>) -> Option<String> {
    use ColumnData as D;
    Some(match v {
        D::U8(v) => v.as_ref()?.to_string(),
        D::I16(v) => v.as_ref()?.to_string(),
        D::I32(v) => v.as_ref()?.to_string(),
        D::I64(v) => v.as_ref()?.to_string(),
        D::F32(v) => super::float(f64::from((*v)?)),
        D::F64(v) => super::float((*v)?),
        D::Bit(v) => if (*v)? { "1".into() } else { "0".into() },
        D::String(v) => v.as_ref()?.to_string(),
        D::Guid(v) => v.as_ref()?.hyphenated().to_string().to_uppercase(),
        // a picture or other bytes in a cell: no text for a data source
        D::Binary(v) => { v.as_ref()?; String::new() }
        D::Numeric(v) => { let n = (*v)?; numeric(n.value(), n.scale()) }
        D::Xml(v) => v.as_ref()?.to_string(),
        D::DateTime(v) => {
            let dt = (*v)?;
            // 1/300 of a second, rounded to the millisecond as SQL Server shows it (.000, .003, .007)
            let ms = (u64::from(dt.seconds_fragments()) * 1000 + 150) / 300;
            format!("{} {}", date(DAY_1900 + i64::from(dt.days())), time(ms, 3))
        }
        D::SmallDateTime(v) => {
            let dt = (*v)?;
            format!("{} {}", date(DAY_1900 + i64::from(dt.days())), time(u64::from(dt.seconds_fragments()) * 60, 0))
        }
        D::Time(v) => { let t = (*v)?; time(t.increments(), t.scale()) }
        D::Date(v) => date(DAY_1 + i64::from((*v)?.days())),
        D::DateTime2(v) => { let dt = (*v)?; format!("{} {}", date(DAY_1 + i64::from(dt.date().days())), time(dt.time().increments(), dt.time().scale())) }
        D::DateTimeOffset(v) => {
            // the wire holds UTC; SQL Server shows the local time of the offset
            let dto = (*v)?;
            let (dt, offset) = (dto.datetime2(), i64::from(dto.offset()));
            let scale = dt.time().scale();
            let per_day = 86_400 * 10i64.pow(u32::from(scale));
            let total = i64::from(dt.date().days()) * per_day + dt.time().increments() as i64 + offset * 60 * 10i64.pow(u32::from(scale));
            let (days, units) = (total.div_euclid(per_day), total.rem_euclid(per_day));
            let sign = if offset < 0 { '-' } else { '+' };
            format!("{} {} {sign}{:02}:{:02}", date(DAY_1 + days), time(units as u64, scale), offset.abs() / 60, offset.abs() % 60)
        }
    })
}

/// A cell of a column: money and smallmoney (tiberius reads them as floats) print their four decimals as SQL Server does.
pub(crate) fn cell(kind: tiberius::ColumnType, v: &ColumnData<'_>) -> Option<String> {
    match (kind, v) {
        (tiberius::ColumnType::Money | tiberius::ColumnType::Money4, ColumnData::F64(Some(f))) => Some(format!("{f:.4}")),
        _ => text(v),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// queries

impl SqlServer {
    /// Statements of the session (the row cap, the transaction): a plain batch, whose results are read to the end.
    fn batch(&mut self, sql: &str) -> Result<()> {
        let slot = self.slot.clone();
        block_on(async { self.client.simple_query(sql).await?.into_results().await }).map_err(|e| err(e, &slot))?;
        Ok(())
    }

    fn rows(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> {
        let start = Instant::now();
        // SET ROWCOUNT 0 would mean "no cap": a preview of 0 rows asks for 1 and keeps none
        let cap = match rows { Rows::Stop(n) => n, Rows::Refuse(n) => n + 1 }.max(1);
        self.batch(&format!("SET ROWCOUNT {cap}; SET LOCK_TIMEOUT {}", crate::QUERY_TIMEOUT_MS))?;
        let slot = self.slot.clone();
        block_on(self.client.begin_transaction_with_isolation(tiberius::IsolationLevel::ReadCommitted)).map_err(|e| err(e, &slot))?;
        let client = &mut self.client;
        let result: Result<Table> = block_on(async {
            let mut q = tiberius::Query::new(sql);
            for b in binds { q.bind(b.as_str()); }
            let mut stream = q.query(client).await.map_err(|e| err(e, &slot))?;
            let columns: Vec<String> = stream.columns().await.map_err(|e| err(e, &slot))?.map(|c| c.iter().map(|c| c.name().to_owned()).collect()).unwrap_or_default();
            let mut rs = stream.into_row_stream();
            let mut budget = Budget::new(rows);
            let mut out = Vec::new();
            let mut full = false;
            use futures_util::TryStreamExt;
            while let Some(row) = rs.try_next().await.map_err(|e| err(e, &slot))? {
                if start.elapsed() > Duration::from_millis(crate::QUERY_TIMEOUT_MS) {
                    return Err(DbError::new(Kind::Timeout, format!("the query took longer than {} seconds", crate::QUERY_TIMEOUT_MS / 1000)));
                }
                // a second result (the SQL had more than one statement) is not read into the table
                if full || row.result_index() > 0 { continue; }
                let values: Vec<Option<String>> = row.cells().map(|(c, v)| cell(c.column_type(), v)).collect();
                if !budget.take(&values)? { full = true; continue; }
                out.push(values);
            }
            Ok(Table { columns, rows: out })
        });
        // the transaction never commits; the row cap is lifted for the next statement
        let undone = block_on(self.client.rollback_transaction()).map_err(|e| err(e, &self.slot.clone()));
        let table = result?;
        undone?;
        self.batch("SET ROWCOUNT 0")?;
        Ok(table)
    }
}

impl Conn for SqlServer {
    fn version(&mut self) -> Result<String> {
        let t = self.rows("SELECT @@VERSION", &[], Rows::Stop(1))?;
        // the first line: "Microsoft SQL Server 2022 (RTM-CU…) (KB…) - 16.0.… (X64)"
        Ok(t.rows.first().and_then(|r| r[0].clone()).unwrap_or_default().lines().next().unwrap_or_default().trim().to_owned())
    }
    fn tables(&mut self) -> Result<Vec<TableInfo>> {
        let t = self.rows("SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM INFORMATION_SCHEMA.TABLES ORDER BY TABLE_SCHEMA, TABLE_NAME", &[], Rows::Stop(100_000))?;
        Ok(t.rows.into_iter().map(|r| TableInfo { schema: r[0].clone(), name: r[1].clone().unwrap_or_default(),
            kind: if r[2].as_deref() == Some("VIEW") { "view".into() } else { "table".into() } }).collect())
    }
    fn columns(&mut self, table: &TableRef) -> Result<Vec<ColumnInfo>> {
        let sql = "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ";
        let t = match table.schema.as_deref().filter(|s| !s.is_empty()) {
            Some(schema) => self.rows(&format!("{sql}@P1 AND TABLE_NAME = @P2 ORDER BY ORDINAL_POSITION"), &[schema.to_owned(), table.name.clone()], Rows::Stop(100_000))?,
            None => self.rows(&format!("{sql}SCHEMA_NAME() AND TABLE_NAME = @P1 ORDER BY ORDINAL_POSITION"), std::slice::from_ref(&table.name), Rows::Stop(100_000))?,
        };
        if t.rows.is_empty() { return Err(DbError::new(Kind::NotFound, format!("no table {}", table.name))); }
        Ok(t.rows.into_iter().map(|r| ColumnInfo { name: r[0].clone().unwrap_or_default(), kind: r[1].clone().unwrap_or_default(), nullable: r[2].as_deref() == Some("YES") }).collect())
    }
    fn select(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> { self.rows(sql, binds, rows) }
}

impl Drop for SqlServer {
    fn drop(&mut self) { let _ = self.socket.shutdown(std::net::Shutdown::Both); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiberius::time::{Date, DateTime, DateTime2, DateTimeOffset, SmallDateTime, Time};

    #[test]
    fn values_read_as_sql_server_prints_them() {
        let t = |v: ColumnData<'static>| text(&v);
        assert_eq!(t(ColumnData::I32(Some(-5))), Some("-5".into()));
        assert_eq!(t(ColumnData::I64(Some(9_007_199_254_740_993))), Some("9007199254740993".into()));
        assert_eq!(t(ColumnData::U8(Some(255))), Some("255".into()));
        assert_eq!(t(ColumnData::I32(None)), None);
        assert_eq!(t(ColumnData::Bit(Some(true))), Some("1".into()));
        assert_eq!(t(ColumnData::Bit(Some(false))), Some("0".into()));
        assert_eq!(cell(tiberius::ColumnType::Money4, &ColumnData::F64(Some(-1.5))), Some("-1.5000".into()), "money keeps its four decimals");
        assert_eq!(cell(tiberius::ColumnType::Money, &ColumnData::F64(Some(12.3456))), Some("12.3456".into()));
        assert_eq!(cell(tiberius::ColumnType::Float8, &ColumnData::F64(Some(-1.5))), Some("-1.5".into()));
        assert_eq!(cell(tiberius::ColumnType::Money, &ColumnData::F64(None)), None);
        assert_eq!(t(ColumnData::F64(Some(0.1))), Some("0.1".into()));
        assert_eq!(t(ColumnData::F64(Some(12000.0))), Some("12000".into()));
        assert_eq!(t(ColumnData::String(Some("한글".into()))), Some("한글".into()));
        assert_eq!(t(ColumnData::Binary(Some(vec![0, 255].into()))), Some(String::new()), "bytes have no text");
        assert_eq!(t(ColumnData::Binary(None)), None);
        assert_eq!(t(ColumnData::Guid(Some(tiberius::Uuid::parse_str("6f9619ff-8b86-d011-b42d-00c04fc964ff").unwrap()))), Some("6F9619FF-8B86-D011-B42D-00C04FC964FF".into()));
        assert_eq!(t(ColumnData::Numeric(Some(tiberius::numeric::Numeric::new_with_scale(1_234_500, 3)))), Some("1234.500".into()));
        assert_eq!(t(ColumnData::Numeric(Some(tiberius::numeric::Numeric::new_with_scale(-5, 2)))), Some("-0.05".into()));
        assert_eq!(t(ColumnData::Numeric(Some(tiberius::numeric::Numeric::new_with_scale(42, 0)))), Some("42".into()));
        // 2026-11-19 is day 46343 since 1900-01-01 and day 739938 since 0001-01-01
        assert_eq!(t(ColumnData::Date(Some(Date::new(739_938)))), Some("2026-11-19".into()));
        assert_eq!(t(ColumnData::Date(Some(Date::new(0)))), Some("0001-01-01".into()));
        assert_eq!(t(ColumnData::DateTime(Some(DateTime::new(46_343, (13 * 3600 + 45 * 60 + 7) * 300 + 1)))), Some("2026-11-19 13:45:07.003".into()));
        assert_eq!(t(ColumnData::DateTime(Some(DateTime::new(0, 0)))), Some("1900-01-01 00:00:00.000".into()));
        assert_eq!(t(ColumnData::SmallDateTime(Some(SmallDateTime::new(46_343, 13 * 60 + 45)))), Some("2026-11-19 13:45:00".into()));
        assert_eq!(t(ColumnData::Time(Some(Time::new(49_507_250, 3)))), Some("13:45:07.250".into()));
        let dt2 = DateTime2::new(Date::new(739_938), Time::new(495_071_234_567, 7));
        assert_eq!(t(ColumnData::DateTime2(Some(dt2))), Some("2026-11-19 13:45:07.1234567".into()));
        // stored as UTC 04:45:07.120 with +09:00 → shown 13:45:07.120 +09:00; and across midnight backwards
        let utc = DateTime2::new(Date::new(739_938), Time::new((4 * 3600 + 45 * 60 + 7) * 1000 + 120, 3));
        assert_eq!(t(ColumnData::DateTimeOffset(Some(DateTimeOffset::new(utc, 540)))), Some("2026-11-19 13:45:07.120 +09:00".into()));
        let utc = DateTime2::new(Date::new(739_938), Time::new(3 * 3600 * 1000, 3));
        assert_eq!(t(ColumnData::DateTimeOffset(Some(DateTimeOffset::new(utc, -330)))), Some("2026-11-18 21:30:00.000 -05:30".into()));
    }

    #[test]
    fn prelogin_and_browser_bits() {
        // PRELOGIN options: VERSION (0, at 11, 6 bytes), ENCRYPTION (1, at 17, 1 byte), end
        let payload = [0, 0, 11, 0, 6, 1, 0, 17, 0, 1, 0xFF, 16, 0, 0, 0, 0, 0, 2];
        assert_eq!(encryption_at(&payload), Some(17));
        assert_eq!(payload[17], ENCRYPT_NOT_SUP);
        assert_eq!(encryption_at(&[0xFF]), None);
        assert_eq!(encryption_at(&[1, 0]), None, "a cut table is not read past its end");
        let mut id = 1;
        let w = wrap(&vec![7u8; 5000], &mut id);
        assert_eq!(&w[..2], &[PRELOGIN, 0x00], "the first of two packets does not end the message");
        assert_eq!(packet_len(&w), PACKET);
        assert_eq!(w[PACKET + 1], 0x01, "the last packet ends it");
        assert_eq!(packet_len(&w[PACKET..]), 5000 - (PACKET - 8) + 8);
        assert_eq!(id, 3);
        let mut answer = vec![0x05, 0, 0];
        answer.extend(b"ServerName;DB1;InstanceName;SQLEXPRESS;IsClustered;No;Version;16.0.1000.6;tcp;51433;;");
        assert_eq!(port_of_browser_answer(&answer), Some(51433));
        assert_eq!(port_of_browser_answer(b"\x05\0\0ServerName;DB1;InstanceName;X;np;\\\\DB1\\pipe;;"), None);
        let mut c: crate::model::Connection = serde_json::from_value(serde_json::json!({"name": "a", "driver": "sqlserver", "host": "db1\\SQLEXPRESS", "database": "m", "user": "u"})).unwrap();
        assert_eq!(host_and_instance(&c), ("db1".into(), Some("SQLEXPRESS".into())));
        c.instance = Some("OTHER".into());
        assert_eq!(host_and_instance(&c), ("db1".into(), Some("OTHER".into())));
        c.host = "db1".into();
        c.instance = Some("  ".into());
        assert_eq!(host_and_instance(&c), ("db1".into(), None));
    }
}
