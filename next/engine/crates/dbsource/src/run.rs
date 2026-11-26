//! What the host asks: test a connection, list tables and columns, fetch a query (a preview or everything).
use crate::error::{DbError, Kind, Result};
use crate::model::{ColumnInfo, Endpoint, Query, TableInfo, TableRef};
use crate::params::{self, Date};
use crate::snapshot::Table;
use serde_json::{Map, Value};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub struct Tested { pub server_version: String, pub ms: u64 }

#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub table: Table,
    /// The SQL that ran (shown for transparency; the values are in `binds`).
    pub sql: String,
    pub binds: Vec<String>,
    /// A preview: how many rows the whole query has.
    pub count: Option<usize>,
    pub ms: u64,
    /// The parameter values used (relative ones worked out), as the document remembers them.
    pub param_values: Map<String, Value>,
}

fn ms(t: Instant) -> u64 { t.elapsed().as_millis() as u64 }

fn connect(ep: &Endpoint) -> Result<Box<dyn crate::drivers::Conn>> {
    ep.connection.validate()?;
    let pw = ep.password.clone();
    crate::drivers::connect(ep).map_err(|e| {
        let e = e.scrub(pw.as_ref());
        // no password was known and the server refused the login: that is "a password is needed" (the screen asks), not "wrong password"
        if pw.is_none() && e.kind == Kind::Auth && ep.connection.driver != crate::model::Driver::Sqlite {
            return DbError::new(Kind::NeedsPassword, format!("connection '{}' needs a password: {}", ep.connection.name, e.message));
        }
        e
    })
}

use crate::drivers::Rows;

pub fn test(ep: &Endpoint) -> Result<Tested> {
    let t = Instant::now();
    let mut c = connect(ep)?;
    let server_version = c.version().map_err(|e| e.scrub(ep.password.as_ref()))?;
    Ok(Tested { server_version, ms: ms(t) })
}

pub fn tables(ep: &Endpoint) -> Result<Vec<TableInfo>> { connect(ep)?.tables().map_err(|e| e.scrub(ep.password.as_ref())) }

pub fn columns(ep: &Endpoint, table: &TableRef) -> Result<Vec<ColumnInfo>> { connect(ep)?.columns(table).map_err(|e| e.scrub(ep.password.as_ref())) }

/// The query run: `preview` = at most that many rows and the whole count; else every row (more than `MAX_ROWS` is refused).
pub fn fetch(ep: &Endpoint, query: &Query, given: &Map<String, Value>, today: Date, preview: Option<usize>) -> Result<Fetched> {
    let t = Instant::now();
    let resolved = params::resolve(&query.params, given, today)?;
    let driver = ep.connection.driver;
    let mut c = connect(ep)?;
    let scrub = |e: DbError| e.scrub(ep.password.as_ref());
    let rows = match preview { Some(n) => Rows::Stop(n.min(crate::MAX_ROWS)), None => Rows::Refuse(crate::MAX_ROWS) };
    let (sql, binds, count_sql) = match query.sql.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(text) => {
            let (sql, binds) = crate::advanced::bind(text, driver, &resolved)?;
            let count = format!("SELECT COUNT(*) FROM ({sql}) AS folio_count");
            (sql, binds, count)
        }
        None => {
            let table = query.table.as_ref().ok_or_else(|| DbError::invalid("the query names no table"))?;
            let listed = c.tables().map_err(scrub)?;
            let same_schema = |s: &Option<String>| s.as_deref().filter(|s| !s.is_empty()) == table.schema.as_deref().filter(|s| !s.is_empty()) || table.schema.is_none();
            if !listed.iter().any(|l| l.name == table.name && same_schema(&l.schema)) {
                return Err(DbError::new(Kind::NotFound, format!("the database has no table {}", table.name)));
            }
            let columns = c.columns(table).map_err(scrub)?;
            // one more than allowed comes back when there are too many: that row is the refusal
            let cap = match rows { Rows::Stop(n) => n, Rows::Refuse(n) => n + 1 };
            let built = crate::sql::build(query, driver, table, &columns, &resolved, cap)?;
            (built.sql, built.binds, built.count_sql)
        }
    };
    let table = c.select(&sql, &binds, rows).map_err(scrub)?;
    let advanced = query.sql.as_deref().is_some_and(|s| !s.trim().is_empty());
    let count = match preview {
        Some(_) => match c.select(&count_sql, &binds, Rows::Stop(1)) {
            Ok(n) => n.rows.first().and_then(|r| r.first().cloned().flatten()).and_then(|s| s.parse().ok()),
            // SQL Server takes no ORDER BY and no WITH inside a derived table: the advanced SQL's rows are counted as they come instead
            Err(e) if advanced && driver == crate::model::Driver::Sqlserver && e.kind == Kind::Sql =>
                Some(c.select(&sql, &binds, Rows::Stop(crate::MAX_ROWS)).map_err(scrub)?.rows.len()),
            Err(e) => return Err(scrub(e)),
        },
        None => None,
    };
    Ok(Fetched { table, sql, binds, count, ms: ms(t), param_values: params::values_json(&resolved) })
}
