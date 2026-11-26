//! SQLite: a file inside the project, opened read-only (`SQLITE_OPEN_READ_ONLY` and `query_only`).
use super::{Budget, Conn, Rows};
use crate::error::{DbError, Kind, Result};
use crate::model::{ColumnInfo, Endpoint, TableInfo, TableRef};
use crate::snapshot::Table;
use rusqlite::{OpenFlags, types::ValueRef};
use std::time::{Duration, Instant};

pub struct Sqlite { conn: rusqlite::Connection }

fn err(e: rusqlite::Error) -> DbError {
    let message = e.to_string();
    let kind = match &e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::OperationInterrupted => Kind::Timeout,
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::CannotOpen => Kind::NotFound,
        rusqlite::Error::SqliteFailure(f, _) if matches!(f.code, rusqlite::ErrorCode::NotADatabase) => Kind::Invalid,
        _ if message.contains("no such table") || message.contains("no such column") => Kind::NotFound,
        _ => Kind::Sql,
    };
    DbError::new(kind, message)
}

pub fn open(ep: &Endpoint) -> Result<Sqlite> {
    let file = ep.file.as_ref().ok_or_else(|| DbError::invalid("the SQLite file was not given"))?;
    if !file.is_file() { return Err(DbError::new(Kind::NotFound, format!("the SQLite file {} is not in the project", ep.connection.database))); }
    let conn = rusqlite::Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(err)?;
    conn.pragma_update(None, "query_only", true).map_err(err)?;
    conn.busy_timeout(Duration::from_millis(crate::CONNECT_TIMEOUT_MS)).map_err(err)?;
    Ok(Sqlite { conn })
}

fn text(v: ValueRef<'_>) -> Option<String> {
    match v {
        ValueRef::Null => None,
        ValueRef::Integer(i) => Some(i.to_string()),
        ValueRef::Real(f) => Some(super::float(f)),
        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        // a picture or other bytes in a cell: no text for a data source
        ValueRef::Blob(_) => Some(String::new()),
    }
}

impl Sqlite {
    fn rows(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> {
        let start = Instant::now();
        // the query time limit: SQLite asks every 1000 steps whether to go on
        let _ = self.conn.progress_handler(1000, Some(move || start.elapsed() > Duration::from_millis(crate::QUERY_TIMEOUT_MS)));
        let result = (|| {
            let mut stmt = self.conn.prepare(sql).map_err(err)?;
            if !stmt.readonly() { return Err(DbError::invalid("the SQL may only read")); }
            let columns: Vec<String> = stmt.column_names().map(str::to_owned).collect();
            let n = columns.len();
            let mut q = stmt.query(rusqlite::params_from_iter(binds.iter())).map_err(err)?;
            let mut budget = Budget::new(rows);
            let mut out = Vec::new();
            while let Some(row) = q.next().map_err(err)? {
                let values: Vec<Option<String>> = (0..n).map(|i| row.get_ref(i).map(text).map_err(err)).collect::<Result<_>>()?;
                if !budget.take(&values)? { break; }
                out.push(values);
            }
            Ok(Table { columns, rows: out })
        })();
        let _ = self.conn.progress_handler(0, None::<fn() -> bool>);
        result
    }
}

impl Conn for Sqlite {
    fn version(&mut self) -> Result<String> { Ok(format!("SQLite {}", rusqlite::version())) }
    fn tables(&mut self) -> Result<Vec<TableInfo>> {
        let t = self.rows("SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite!_%' ESCAPE '!' ORDER BY name", &[], Rows::Stop(100_000))?;
        Ok(t.rows.into_iter().map(|r| TableInfo { schema: None, name: r[0].clone().unwrap_or_default(), kind: r[1].clone().unwrap_or_default() }).collect())
    }
    fn columns(&mut self, table: &TableRef) -> Result<Vec<ColumnInfo>> {
        let t = self.rows("SELECT name, type, \"notnull\" FROM pragma_table_info(?)", std::slice::from_ref(&table.name), Rows::Stop(100_000))?;
        if t.rows.is_empty() { return Err(DbError::new(Kind::NotFound, format!("no table {}", table.name))); }
        Ok(t.rows.into_iter().map(|r| ColumnInfo { name: r[0].clone().unwrap_or_default(), kind: r[1].clone().unwrap_or_default(), nullable: r[2].as_deref() != Some("1") }).collect())
    }
    fn select(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table> { self.rows(sql, binds, rows) }
}
