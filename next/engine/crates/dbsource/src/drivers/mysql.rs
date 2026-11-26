//! MySQL and MariaDB (`mysql`, pure Rust, TLS by rustls with ring). The computer's trusted roots are given to the driver as a file; a pinned
//! certificate is checked before connecting (`tls::probe_mysql`); see `ssl` below.
use super::{Budget, Conn, Rows};
use crate::error::{DbError, Kind, Result};
use crate::model::{ColumnInfo, Endpoint, TableInfo, TableRef, Tls, TlsMode};
use crate::snapshot::Table;
use mysql::prelude::Queryable;
use mysql::{AccessMode, Value};
use std::time::Duration;

pub struct Mysql { conn: mysql::Conn }

fn err(e: mysql::Error) -> DbError {
    let message = e.to_string();
    let kind = match &e {
        mysql::Error::MySqlError(m) => match m.code {
            1044 | 1045 | 1698 => Kind::Auth,
            1049 | 1146 | 1054 => Kind::NotFound,
            3024 | 1969 | 1317 => Kind::Timeout,
            1792 => Kind::Invalid,
            _ => Kind::Sql,
        },
        mysql::Error::IoError(io) if matches!(io.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => Kind::Timeout,
        mysql::Error::IoError(_) => Kind::Network,
        mysql::Error::DriverError(d) if d.to_string().to_lowercase().contains("timed out") => Kind::Timeout,
        mysql::Error::TlsError(_) => Kind::Tls,
        mysql::Error::DriverError(_) => Kind::Network,
        // the handshake runs inside the driver's protocol codec: a refused certificate arrives as an IO error text ("invalid peer certificate: ...")
        other if !matches!(other, mysql::Error::MySqlError(_)) && ["certificate", "tls error", "handshake", "alert"].iter().any(|w| message.to_lowercase().contains(w)) => Kind::Tls,
        _ => Kind::Sql,
    };
    DbError::new(kind, message)
}

/// The TLS options of the connection and the file of roots to remove afterwards. The driver brings Mozilla's list of roots (a data licence the
/// project takes: THIRD_PARTY_NOTICES.txt) and takes more from a file: the computer's own roots (a company's private CA) go there, the same ones
/// PostgreSQL's `verify` reads (`tls::system_roots`). `require` accepts any certificate. A pinned certificate is checked first by
/// `tls::probe_mysql` (the driver cannot pin), then the driver connects without checking it.
fn ssl(c: &crate::model::Connection) -> Result<(Option<mysql::SslOpts>, Option<std::path::PathBuf>)> {
    let lax = || mysql::SslOpts::default().with_danger_accept_invalid_certs(true).with_danger_skip_domain_validation(true);
    Ok(match (c.tls.mode, c.tls.trust_fingerprint.as_deref().filter(|f| !f.trim().is_empty())) {
        (TlsMode::Disable, _) => (None, None),
        (TlsMode::Require, _) => (Some(lax()), None),
        (TlsMode::Verify, Some(_)) => {
            crate::tls::probe_mysql(&c.host, c.port.unwrap_or(3306), &c.tls)?;
            (Some(lax()), None)
        }
        (TlsMode::Verify, None) => {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            let file = std::env::temp_dir().join(format!("folio-roots-{}-{nanos}.pem", std::process::id()));
            std::fs::write(&file, crate::tls::roots_pem()?).map_err(|e| DbError::new(Kind::Tls, format!("the trusted roots could not be written for the driver: {e}")))?;
            (Some(mysql::SslOpts::default().with_root_cert_path(Some(file.clone()))), Some(file))
        }
    })
}

pub fn open(ep: &Endpoint) -> Result<Mysql> {
    let c = &ep.connection;
    let (ssl_opts, roots_file) = ssl(c)?;
    let opts = mysql::OptsBuilder::new()
        .ip_or_hostname(Some(c.host.clone())).tcp_port(c.port.unwrap_or(3306)).db_name(Some(c.database.clone()))
        .user(Some(c.user.clone())).pass(ep.password.as_ref().map(|p| p.expose().to_owned()))
        .tcp_connect_timeout(Some(Duration::from_millis(crate::CONNECT_TIMEOUT_MS)))
        .read_timeout(Some(Duration::from_millis(crate::QUERY_TIMEOUT_MS))).write_timeout(Some(Duration::from_millis(crate::QUERY_TIMEOUT_MS)))
        .prefer_socket(false).ssl_opts(ssl_opts);
    let made = mysql::Conn::new(opts);
    if let Some(f) = roots_file { let _ = std::fs::remove_file(f); }
    match made {
        Ok(conn) => Ok(Mysql { conn }),
        Err(e) => {
            let mut d = err(e).scrub(ep.password.as_ref());
            // a certificate that is not trusted: the one the server showed, for "trust this certificate"
            if d.kind == Kind::Tls && c.tls.mode == TlsMode::Verify {
                let seen = Tls { mode: TlsMode::Require, trust_fingerprint: None };
                d.fingerprint = crate::tls::probe_mysql(&c.host, c.port.unwrap_or(3306), &seen).ok();
            }
            Err(d)
        }
    }
}

fn text(v: &Value, date_only: bool) -> Option<String> {
    Some(match v {
        Value::NULL => return None,
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) => super::float(f64::from(*f)),
        Value::Double(f) => super::float(*f),
        Value::Date(y, m, d, h, mi, s, us) => {
            if date_only { format!("{y:04}-{m:02}-{d:02}") }
            else if *us > 0 { format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}.{us:06}") }
            else { format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}") }
        }
        Value::Time(neg, d, h, m, s, _) => format!("{}{:02}:{m:02}:{s:02}", if *neg { "-" } else { "" }, u32::from(*h) + d * 24),
    })
}

impl Mysql {
    fn rows(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> {
        let opts = mysql::TxOpts::default().set_access_mode(Some(AccessMode::ReadOnly));
        let mut tx = self.conn.start_transaction(opts).map_err(err)?;
        let params = if binds.is_empty() { mysql::Params::Empty } else { mysql::Params::Positional(binds.iter().map(|b| Value::from(b.as_str())).collect()) };
        let out = {
            let mut result = tx.exec_iter(sql, params).map_err(err)?;
            let cols = result.columns();
            let meta: Vec<(String, bool)> = cols.as_ref().iter().map(|c| (c.name_str().into_owned(), c.column_type() == mysql::consts::ColumnType::MYSQL_TYPE_DATE)).collect();
            let mut budget = Budget::new(rows);
            let mut out = Vec::new();
            for row in result.by_ref() {
                let row = row.map_err(err)?;
                let values: Vec<Option<String>> = (0..meta.len()).map(|i| row.as_ref(i).and_then(|v| text(v, meta[i].1))).collect();
                if !budget.take(&values)? { break; }
                out.push(values);
            }
            Table { columns: meta.into_iter().map(|m| m.0).collect(), rows: out }
        };
        tx.rollback().map_err(err)?;
        Ok(out)
    }
}

impl Conn for Mysql {
    fn version(&mut self) -> Result<String> {
        let t = self.rows("SELECT VERSION()", &[], Rows::Stop(1))?;
        Ok(t.rows.first().and_then(|r| r[0].clone()).unwrap_or_default())
    }
    fn tables(&mut self) -> Result<Vec<TableInfo>> {
        let t = self.rows("SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME", &[], Rows::Stop(100_000))?;
        Ok(t.rows.into_iter().map(|r| TableInfo { schema: None, name: r[0].clone().unwrap_or_default(),
            kind: if r[1].as_deref() == Some("VIEW") { "view".into() } else { "table".into() } }).collect())
    }
    fn columns(&mut self, table: &TableRef) -> Result<Vec<ColumnInfo>> {
        let t = self.rows("SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
            std::slice::from_ref(&table.name), Rows::Stop(100_000))?;
        if t.rows.is_empty() { return Err(DbError::new(Kind::NotFound, format!("no table {}", table.name))); }
        Ok(t.rows.into_iter().map(|r| ColumnInfo { name: r[0].clone().unwrap_or_default(), kind: r[1].clone().unwrap_or_default(), nullable: r[2].as_deref() == Some("YES") }).collect())
    }
    fn select(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> { self.rows(sql, binds, rows) }
}
