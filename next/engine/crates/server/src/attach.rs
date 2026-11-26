//! The running app's engine opens a local endpoint for `folio-mcp --attach`, only while the user allows it
//! (Preferences ▸ General ▸ Allow LLM connection (MCP), user setting `folio.mcp.allowConnection`, off by default).
//!
//! - Endpoint: a named pipe on Windows (remote clients rejected), a Unix socket (mode 0600) on macOS and Linux. Never TCP.
//!   iOS and Android have none (`sync` does nothing there).
//! - Each time it opens, a new random token is written with the endpoint to `<user data>/mcp-attach.json` (mode 0600 on
//!   Unix; on Windows the app data folder under the user profile is the user's own). A client sends
//!   `{"attach":1,"token":…}` as its first line; a missing or wrong token is refused and the connection closed.
//! - Then each line is an engine request, checked by `attached::admit` here (the engine never trusts the client), and put
//!   on the same request line as the screen's (`Incoming`): it acts on the same session. Its reply goes back to that
//!   connection only (by id, `deliver`); events (view-state, dialog-state, external-edit…) still go to the screen.
use crate::attached;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

/// One request for the engine's request loop: a line from the screen (stdin) or from an attached connection.
pub enum Incoming {
    Line { text: String, mcp: bool },
    Quit,
}

struct Running {
    endpoint: String,
    token: String,
    file: PathBuf,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CONNECTIONS: AtomicU64 = AtomicU64::new(0);
type Reply = (Value, Vec<u8>);
/// Routed request id → the client's own id and where its reply goes.
static ROUTES: Mutex<BTreeMap<String, (Value, mpsc::Sender<Reply>, Option<crate::attach_read_reply::ReadReply>)>> = Mutex::new(BTreeMap::new());
/// Routed request id → the project root its paths were relative to (the reply shows paths relative to it) and the paths to
/// check again when the reply comes (attach_paths.rs).
static ROOTS: Mutex<BTreeMap<String, crate::attach_paths::Resolved>> = Mutex::new(BTreeMap::new());

pub fn routed(id: &str) -> bool { id.starts_with(PREFIX) }

/// The reply to the attached request `id` names paths relative to the root, not this machine's folders, once its paths are
/// checked again (a folder swapped for a link while the command ran turns the reply into a refusal).
pub fn reply_relative_to(id: &Value, resolved: crate::attach_paths::Resolved) {
    if let Some(id) = id.as_str().filter(|id| id.starts_with(PREFIX)) { ROOTS.lock().expect("attach roots").insert(id.to_owned(), resolved); }
}
const MAX_LINE: u64 = 1024 * 1024;
const PREFIX: &str = "mcp:";

pub(crate) fn preferences() -> Value {
    crate::user_settings::run(&json!({})).map(|s| s["settings"]["preferences"].clone()).unwrap_or(Value::Null)
}

fn allowed() -> bool { attached::on(preferences().get(attached::ALLOW_CONNECTION)) }
fn database_allowed() -> bool { attached::on(preferences().get(attached::ALLOW_DATABASE_REFRESH)) }
pub(crate) fn files_allowed() -> bool { attached::on(preferences().get(attached::ALLOW_FILE_WRITES)) }
fn current(generation: u64) -> bool { GENERATION.load(Ordering::SeqCst) == generation }

/// Opens or closes the endpoint to match the setting (at start and after every settings change).
pub fn sync(tx: &mpsc::Sender<Incoming>) {
    let want = allowed() && platform::SUPPORTED;
    let mut running = RUNNING.lock().expect("attach state");
    if want == running.is_some() { return; }
    if let Some(old) = running.take() { stop(old); return; }
    let Some(root) = std::env::var_os("FOLIO_USER_DATA").map(PathBuf::from) else { return };
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let token = format!("{}{}", folio_project::util::uuid_v4(), folio_project::util::uuid_v4()).replace('-', "");
    match platform::listen(&root, generation, token.clone(), tx.clone()) {
        Ok(endpoint) => {
            let file = root.join(attached::CONNECTION_FILE);
            let text = json!({"version": attached::PROTOCOL, "endpoint": endpoint, "token": token, "pid": std::process::id()}).to_string();
            match platform::write_private(&file, text.as_bytes()) {
                Ok(()) => *running = Some(Running { endpoint, token, file }),
                Err(e) => {
                    eprintln!("folio-engine: MCP connection file not written: {e}");
                    stop(Running { endpoint, token, file });
                }
            }
        }
        Err(e) => eprintln!("folio-engine: MCP endpoint not opened: {e}"),
    }
}

/// Closes the endpoint when the engine ends.
pub fn shutdown() {
    if let Some(old) = RUNNING.lock().expect("attach state").take() { stop(old); }
}

fn stop(old: Running) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    // the file is removed only when it is still this engine's (a later app may have written its own)
    if std::fs::read_to_string(&old.file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v["token"] == old.token.as_str()) {
        let _ = std::fs::remove_file(&old.file);
    }
    platform::wake(&old.endpoint);
}

/// A reply for an attached connection goes to it, not to the screen. True when `header` was one.
pub fn deliver(header: &Value, body: &[u8]) -> bool {
    let Some(id) = header["id"].as_str().filter(|id| id.starts_with(PREFIX)) else { return false };
    // progress events of an attached request are not its reply (and not the screen's)
    if header.get("event").is_some() { return true; }
    if let Some((own, reply, read)) = ROUTES.lock().expect("attach routes").remove(id) {
        let mut header = header.clone();
        let mut body = body.to_vec();
        if let Some(resolved) = ROOTS.lock().expect("attach roots").remove(id) {
            match resolved.recheck() {
                Ok(()) => crate::attach_paths::relativize(&mut header, &resolved.root),
                Err(error) => { header = json!({"ok": false, "code": "refused", "error": error, "bodyBytes": 0}); body.clear(); }
            }
        }
        if let Some(read) = read { read.finish(&mut header); }
        header["id"] = own;
        let _ = reply.send((header, body));
    }
    true
}

/// `deliver` for a header already written as JSON object text (main.rs `send_text`).
pub fn deliver_text(id: &Value, object: &str) -> bool {
    if !id.as_str().is_some_and(|id| id.starts_with(PREFIX)) { return false; }
    let mut header: Value = serde_json::from_str(object).unwrap_or_else(|_| json!({}));
    header["id"] = id.clone();
    header["ok"] = json!(true);
    header["bodyBytes"] = json!(0);
    deliver(&header, &[])
}

fn frame(header: &Value, body: &[u8]) -> Vec<u8> {
    let mut header = header.clone();
    header["bodyBytes"] = json!(body.len());
    let text = serde_json::to_vec(&header).unwrap_or_default();
    let mut out = (text.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&text);
    out.extend_from_slice(body);
    out
}

fn same(a: &str, b: &str) -> bool {
    // the token is compared in time independent of where it differs
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn read_line(reader: &mut impl BufRead) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    match reader.take(MAX_LINE + 1).read_until(b'\n', &mut bytes) {
        Ok(0) | Err(_) => None,
        Ok(_) if bytes.len() as u64 > MAX_LINE => None,
        Ok(_) => Some(bytes),
    }
}

/// One attached connection: the token first, then one request at a time (the reply is written before the next is read).
fn serve<S: Read + Write>(stream: S, reader: impl Read, generation: u64, token: String, tx: mpsc::Sender<Incoming>) {
    let mut out = stream;
    let mut reader = BufReader::new(reader);
    let refuse = |out: &mut S, id: &Value, code: &str, error: &str| {
        let _ = out.write_all(&frame(&json!({"id": id, "ok": false, "code": code, "error": error}), &[]));
        let _ = out.flush();
    };
    let Some(first) = read_line(&mut reader) else { return };
    let hello: Value = serde_json::from_slice(&first).unwrap_or(Value::Null);
    if !current(generation) || hello["attach"] != attached::PROTOCOL || !hello["token"].as_str().is_some_and(|t| same(t, &token)) {
        refuse(&mut out, &Value::Null, "refused", "missing or wrong token: read it from the connection file the running Folio app wrote");
        return;
    }
    let connection = CONNECTIONS.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = out.write_all(&frame(&json!({"event": "attached", "ok": true, "protocol": attached::PROTOCOL,
        "version": env!("CARGO_PKG_VERSION")}), &[]));
    let _ = out.flush();
    let mut count = 0u64;
    while let Some(line) = read_line(&mut reader) {
        if line.iter().all(u8::is_ascii_whitespace) { continue; }
        if !current(generation) {
            refuse(&mut out, &Value::Null, "closed", "LLM connection (MCP) was turned off in Folio");
            return;
        }
        let Ok(mut request) = serde_json::from_slice::<Value>(&line) else {
            refuse(&mut out, &Value::Null, "invalid", "bad request: one JSON object per line");
            continue;
        };
        let own = request["id"].clone();
        if let Err(error) = attached::admit(&request, database_allowed(), files_allowed()) {
            refuse(&mut out, &own, "refused", &error);
            continue;
        }
        count += 1;
        let routed = format!("{PREFIX}{connection}:{count}");
        let (reply_tx, reply_rx) = mpsc::channel();
        let read = crate::attach_read_reply::ReadReply::take(&mut request);
        ROUTES.lock().expect("attach routes").insert(routed.clone(), (own.clone(), reply_tx, read));
        request["id"] = json!(routed);
        if tx.send(Incoming::Line { text: request.to_string(), mcp: true }).is_err() {
            refuse(&mut out, &own, "closed", "the Folio engine is ending");
            return;
        }
        match reply_rx.recv_timeout(Duration::from_secs(600)) {
            Ok((header, body)) => {
                if out.write_all(&frame(&header, &body)).and_then(|_| out.flush()).is_err() { return; }
            }
            Err(_) => {
                ROUTES.lock().expect("attach routes").remove(&routed);
                ROOTS.lock().expect("attach roots").remove(&routed);
                refuse(&mut out, &own, "timeout", "the engine did not answer in time");
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    //! A named pipe `\\.\pipe\folio-mcp-<random>`: the first instance is ours (no squatting), remote clients are rejected.
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use std::path::Path;
    use std::sync::mpsc;

    pub const SUPPORTED: bool = true;
    const PIPE_ACCESS_DUPLEX: u32 = 3;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 8;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const ERROR_PIPE_CONNECTED: i32 = 535;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateNamedPipeW(name: *const u16, open_mode: u32, pipe_mode: u32, instances: u32, out_size: u32, in_size: u32,
            timeout: u32, security: *const c_void) -> *mut c_void;
        fn ConnectNamedPipe(pipe: *mut c_void, overlapped: *mut c_void) -> i32;
    }

    fn instance(name: &[u16], first: bool) -> std::io::Result<std::fs::File> {
        let mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        // SAFETY: `name` is NUL-terminated UTF-16; the handle is checked before it is owned by a File.
        let handle = unsafe { CreateNamedPipeW(name.as_ptr(), mode, PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, 65536, 65536, 0, std::ptr::null()) };
        if handle.is_null() || handle as isize == -1 { return Err(std::io::Error::last_os_error()); }
        // SAFETY: a valid pipe handle owned by nobody else
        Ok(unsafe { std::fs::File::from_raw_handle(handle) })
    }

    pub fn listen(_root: &Path, generation: u64, token: String, tx: mpsc::Sender<super::Incoming>) -> std::io::Result<String> {
        let endpoint = format!(r"\\.\pipe\folio-mcp-{}", folio_project::util::short_id());
        let name: Vec<u16> = std::ffi::OsStr::new(&endpoint).encode_wide().chain([0]).collect();
        let mut pipe = instance(&name, true)?;
        std::thread::Builder::new().name("folio-mcp-attach".into()).spawn(move || loop {
            use std::os::windows::io::AsRawHandle;
            // SAFETY: the handle is the File's, alive for the call
            let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0
                || std::io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED);
            if !super::current(generation) { break; }
            let next = match instance(&name, false) { Ok(next) => next, Err(_) => break };
            let client = std::mem::replace(&mut pipe, next);
            if connected && let Ok(reader) = client.try_clone() {
                let (token, tx) = (token.clone(), tx.clone());
                std::thread::spawn(move || super::serve(client, reader, generation, token, tx));
            }
        })?;
        Ok(endpoint)
    }

    /// Unblocks the listener so it sees it has been stopped.
    pub fn wake(endpoint: &str) {
        let _ = std::fs::OpenOptions::new().read(true).write(true).open(endpoint);
    }

    pub fn write_private(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
        folio_project::atomic::write_bytes(file, bytes)
    }
}

#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
mod platform {
    //! A Unix socket in the user data folder (or the per-user temporary folder when that path is too long), mode 0600.
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    pub const SUPPORTED: bool = true;

    pub fn listen(root: &Path, generation: u64, token: String, tx: mpsc::Sender<super::Incoming>) -> std::io::Result<String> {
        let name = format!("folio-mcp-{}.sock", std::process::id());
        let mut path: PathBuf = root.join(&name);
        if path.as_os_str().len() > 100 { path = std::env::temp_dir().join(&name); }
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let endpoint = path.to_string_lossy().into_owned();
        std::thread::Builder::new().name("folio-mcp-attach".into()).spawn(move || {
            for client in listener.incoming() {
                if !super::current(generation) { break; }
                let Ok(client) = client else { continue };
                if let Ok(reader) = client.try_clone() {
                    let (token, tx) = (token.clone(), tx.clone());
                    std::thread::spawn(move || super::serve(client, reader, generation, token, tx));
                }
            }
            let _ = std::fs::remove_file(&path);
        })?;
        Ok(endpoint)
    }

    pub fn wake(endpoint: &str) {
        let _ = UnixStream::connect(endpoint);
    }

    pub fn write_private(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let temp = file.with_extension("json.tmp");
        let _ = std::fs::remove_file(&temp);
        let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        out.write_all(bytes)?;
        out.sync_all()?;
        std::fs::rename(&temp, file)
    }
}

#[cfg(not(any(windows, all(unix, not(any(target_os = "ios", target_os = "android"))))))]
mod platform {
    //! iOS and Android: no endpoint (an app there cannot be reached by a local MCP client process).
    use std::path::Path;
    use std::sync::mpsc;

    pub const SUPPORTED: bool = false;

    pub fn listen(_root: &Path, _generation: u64, _token: String, _tx: mpsc::Sender<super::Incoming>) -> std::io::Result<String> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no MCP endpoint on this platform"))
    }

    pub fn wake(_endpoint: &str) {}

    pub fn write_private(_file: &Path, _bytes: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no MCP endpoint on this platform"))
    }
}

