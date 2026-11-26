//! The drivers: one connection kind each (PostgreSQL, MySQL/MariaDB, SQLite, SQL Server), all read-only. A select runs inside a read-only transaction with the query time limit.
use crate::error::Result;
use crate::model::{ColumnInfo, Driver, Endpoint, TableInfo, TableRef};
use crate::snapshot::Table;

mod mysql;
mod postgres;
mod sqlite;
mod sqlserver;

/// How many rows a select may bring back: `Stop` quietly stops (a preview), `Refuse` is the `limit` error past it.
#[derive(Debug, Clone, Copy)]
pub enum Rows { Stop(usize), Refuse(usize) }

pub trait Conn {
    fn version(&mut self) -> Result<String>;
    fn tables(&mut self) -> Result<Vec<TableInfo>>;
    fn columns(&mut self, table: &TableRef) -> Result<Vec<ColumnInfo>>;
    fn select(&mut self, sql: &str, binds: &[String], rows: Rows) -> Result<Table>;
}

pub fn connect(ep: &Endpoint) -> Result<Box<dyn Conn>> {
    let conn: Box<dyn Conn> = match ep.connection.driver {
        Driver::Postgres => Box::new(postgres::open(ep)?),
        Driver::Mysql => Box::new(mysql::open(ep)?),
        Driver::Sqlite => Box::new(sqlite::open(ep)?),
        Driver::Sqlserver => Box::new(sqlserver::open(ep)?),
    };
    Ok(conn)
}

/// Counts the rows and the bytes of a result as it comes (design 0: 50,000 rows, 64 MiB).
pub(crate) struct Budget { rows: Rows, count: usize, bytes: usize }

impl Budget {
    pub fn new(rows: Rows) -> Self { Budget { rows, count: 0, bytes: 0 } }
    /// `Ok(false)`: stop reading (a preview has its rows).
    pub fn take(&mut self, row: &[Option<String>]) -> Result<bool> {
        self.bytes += row.iter().map(|v| v.as_ref().map_or(0, String::len)).sum::<usize>();
        if self.bytes > crate::MAX_BYTES { return Err(crate::DbError::new(crate::Kind::Limit, format!("the result is larger than {} MiB", crate::MAX_BYTES >> 20))); }
        match self.rows {
            Rows::Stop(n) if self.count >= n => Ok(false),
            Rows::Refuse(n) if self.count >= n => Err(crate::DbError::new(crate::Kind::Limit, format!("more than {n} rows"))),
            _ => { self.count += 1; Ok(true) }
        }
    }
}

/// A float as text: the shortest form that reads back as the same number.
pub(crate) fn float(f: f64) -> String {
    if f.is_finite() && f == f.trunc() && f.abs() < 1e15 { format!("{}", f as i64) } else { f.to_string() }
}
