//! PostgreSQL (`postgres`, rustls). Values are bound as text in the text format, so the server reads each in the type it
//! infers for it (like a literal); results come back as the text of the whole row (`q::text`), so every type reads as
//! PostgreSQL prints it.
use super::{Budget, Conn, Rows};
use crate::error::{DbError, Kind, Result};
use crate::model::{ColumnInfo, Endpoint, TableInfo, TableRef, TlsMode};
use crate::snapshot::Table;
use postgres::types::{Format, IsNull, ToSql, Type, to_sql_checked};
use std::time::Duration;

pub struct Postgres { client: postgres::Client }

/// A value in the text format, accepted for any parameter type.
#[derive(Debug)]
struct TextParam<'a>(&'a str);

impl ToSql for TextParam<'_> {
    fn to_sql(&self, _: &Type, out: &mut postgres::types::private::BytesMut) -> std::result::Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        out.extend_from_slice(self.0.as_bytes());
        Ok(IsNull::No)
    }
    fn accepts(_: &Type) -> bool { true }
    fn encode_format(&self, _: &Type) -> Format { Format::Text }
    to_sql_checked!();
}

pub(crate) fn err(e: postgres::Error) -> DbError {
    use postgres::error::SqlState;
    let text = e.to_string();
    let detail = e.as_db_error().map(|d| d.message().to_owned());
    let kind = match e.code() {
        Some(c) if *c == SqlState::INVALID_PASSWORD || *c == SqlState::INVALID_AUTHORIZATION_SPECIFICATION => Kind::Auth,
        Some(c) if *c == SqlState::INVALID_CATALOG_NAME || *c == SqlState::UNDEFINED_TABLE || *c == SqlState::UNDEFINED_COLUMN => Kind::NotFound,
        Some(c) if *c == SqlState::QUERY_CANCELED => Kind::Timeout,
        Some(c) if *c == SqlState::READ_ONLY_SQL_TRANSACTION => Kind::Invalid,
        Some(_) => Kind::Sql,
        None => {
            let lower = format!("{text} {:?}", std::error::Error::source(&e)).to_lowercase();
            if lower.contains("timed out") || lower.contains("timeout") { Kind::Timeout }
            else if lower.contains("certificate") || lower.contains("tls") || lower.contains("ssl") || lower.contains("handshake") { Kind::Tls }
            else if lower.contains("password") { Kind::Auth }
            else { Kind::Network }
        }
    };
    let message = match (detail, std::error::Error::source(&e)) {
        (Some(d), _) => d,
        (None, Some(s)) => format!("{text}: {s}"),
        (None, None) => text,
    };
    DbError::new(kind, message)
}

pub fn open(ep: &Endpoint) -> Result<Postgres> {
    let c = &ep.connection;
    let mut cfg = postgres::Config::new();
    cfg.host(&c.host).port(c.port.unwrap_or(5432)).dbname(&c.database).user(&c.user).application_name("Folio Studio")
        .connect_timeout(Duration::from_millis(crate::CONNECT_TIMEOUT_MS));
    if let Some(p) = &ep.password { cfg.password(p.expose()); }
    let scrub = |e: DbError| e.scrub(ep.password.as_ref());
    let client = match c.tls.mode {
        TlsMode::Disable => { cfg.ssl_mode(postgres::config::SslMode::Disable); cfg.connect(postgres::NoTls) }
        _ => {
            cfg.ssl_mode(postgres::config::SslMode::Require);
            let (config, seen) = crate::tls::client_config(&c.tls)?;
            let made = cfg.connect(tokio_postgres_rustls::MakeRustlsConnect::new(config));
            if let Err(e) = made {
                let mut d = err(e);
                d.fingerprint = seen.lock().ok().and_then(|s| s.clone());
                return Err(scrub(d));
            }
            made
        }
    }.map_err(|e| scrub(err(e)))?;
    Ok(Postgres { client })
}

/// The text PostgreSQL prints for a row (`(1,"a b",,"x""y")`) → its values; an empty unquoted value is NULL.
pub(crate) fn record(text: &str) -> Vec<Option<String>> {
    let inner = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')).unwrap_or(text);
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    loop {
        let mut value = String::new();
        let mut quoted = false;
        if chars.peek() == Some(&'"') {
            quoted = true;
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '"' if chars.peek() == Some(&'"') => { value.push('"'); chars.next(); }
                    '"' => break,
                    '\\' => { if let Some(n) = chars.next() { value.push(n); } }
                    _ => value.push(c),
                }
            }
        }
        while let Some(&c) = chars.peek() {
            if c == ',' { break; }
            value.push(c);
            chars.next();
        }
        out.push(if quoted || !value.is_empty() { Some(value) } else { None });
        if chars.next().is_none() { break; }
    }
    out
}

impl Postgres {
    fn rows(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> {
        let mut tx = self.client.build_transaction().read_only(true).start().map_err(err)?;
        tx.batch_execute(&format!("SET LOCAL statement_timeout = {}", crate::QUERY_TIMEOUT_MS)).map_err(err)?;
        let inner = tx.prepare(sql).map_err(err)?;
        let columns: Vec<String> = inner.columns().iter().map(|c| c.name().to_owned()).collect();
        let whole = format!("SELECT q::text FROM ({sql}) AS q");
        let params: Vec<TextParam<'_>> = binds.iter().map(|b| TextParam(b)).collect();
        let iter = tx.query_raw(whole.as_str(), params.iter()).map_err(err)?;
        let mut budget = Budget::new(rows);
        let mut out = Vec::new();
        let mut iter = iter;
        use postgres::fallible_iterator::FallibleIterator;
        while let Some(row) = iter.next().map_err(err)? {
            let text: Option<String> = row.try_get(0).map_err(err)?;
            let mut values = text.as_deref().map(record).unwrap_or_default();
            values.resize(columns.len(), None);
            if !budget.take(&values)? { break; }
            out.push(values);
        }
        drop(iter);
        tx.rollback().map_err(err)?;
        Ok(Table { columns, rows: out })
    }
}

impl Conn for Postgres {
    fn version(&mut self) -> Result<String> {
        let t = self.rows("SELECT version()", &[], Rows::Stop(1))?;
        Ok(t.rows.first().and_then(|r| r[0].clone()).unwrap_or_default())
    }
    fn tables(&mut self) -> Result<Vec<TableInfo>> {
        let t = self.rows("SELECT table_schema, table_name, table_type FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY table_schema, table_name", &[], Rows::Stop(100_000))?;
        Ok(t.rows.into_iter().map(|r| TableInfo { schema: r[0].clone(), name: r[1].clone().unwrap_or_default(),
            kind: if r[2].as_deref() == Some("VIEW") { "view".into() } else { "table".into() } }).collect())
    }
    fn columns(&mut self, table: &TableRef) -> Result<Vec<ColumnInfo>> {
        let schema = table.schema.clone().unwrap_or_else(|| "public".into());
        let t = self.rows("SELECT column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position",
            &[schema, table.name.clone()], Rows::Stop(100_000))?;
        if t.rows.is_empty() { return Err(DbError::new(Kind::NotFound, format!("no table {}", table.name))); }
        Ok(t.rows.into_iter().map(|r| ColumnInfo { name: r[0].clone().unwrap_or_default(), kind: r[1].clone().unwrap_or_default(), nullable: r[2].as_deref() == Some("YES") }).collect())
    }
    fn select(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> { self.rows(sql, binds, rows) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn row_text_is_split_into_values() {
        assert_eq!(super::record(r#"(1,"a, b",,"x""y","",2026-11-01,"back\\slash")"#),
            vec![Some("1".into()), Some("a, b".into()), None, Some("x\"y".into()), Some("".into()), Some("2026-11-01".into()), Some("back\\slash".into())]);
        assert_eq!(super::record("()"), vec![None]);
    }
}
