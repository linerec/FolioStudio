//! Connection settings and query definitions.
//!
//! A `Connection` lives in the app's user settings (principle 1 rule 8: it differs from computer to computer); its password
//! lives in the OS credential store (`credentials`). A `Query` lives in the document (`folio.datamerge` label) and names its
//! connection; its `hint` (kind, host, port, database: no user, no password) lets another computer fill in the settings.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Driver { Postgres, Mysql, Sqlite, Sqlserver }

impl Driver {
    pub fn default_port(self) -> Option<u16> {
        match self { Driver::Postgres => Some(5432), Driver::Mysql => Some(3306), Driver::Sqlserver => Some(1433), Driver::Sqlite => None }
    }
    pub fn name(self) -> &'static str {
        match self { Driver::Postgres => "postgres", Driver::Mysql => "mysql", Driver::Sqlite => "sqlite", Driver::Sqlserver => "sqlserver" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsMode {
    /// TLS with the certificate checked against the computer's trusted roots (or the pinned fingerprint).
    #[default]
    Verify,
    /// TLS, any certificate (encryption without knowing who answers).
    Require,
    /// No TLS.
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tls {
    #[serde(default)]
    pub mode: TlsMode,
    /// SHA-256 of the server's certificate (hex, colons allowed) the user chose to trust ("이 인증서 신뢰"): a self-signed server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_fingerprint: Option<String>,
}

/// The settings of one connection, as the user settings keep them (no password).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub name: String,
    pub driver: Driver,
    #[serde(default)]
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// The database name; for SQLite the project-relative path of the file.
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub tls: Tls,
    /// SQL Server: a named instance (`SQLEXPRESS`); without a port its port is asked of the SQL Server Browser (UDP 1434).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl Connection {
    pub fn validate(&self) -> crate::error::Result<()> {
        use crate::DbError;
        if self.name.trim().is_empty() { return Err(DbError::invalid("a connection needs a name")); }
        if self.name.chars().count() > 100 || self.name.chars().any(char::is_control) { return Err(DbError::invalid("the connection name is too long or holds control characters")); }
        match self.driver {
            Driver::Sqlite => if self.database.trim().is_empty() { return Err(DbError::invalid("a SQLite connection needs the project-relative path of its file")) },
            _ => {
                if self.host.trim().is_empty() { return Err(DbError::invalid("a connection needs a host")); }
                if self.database.trim().is_empty() { return Err(DbError::invalid("a connection needs a database name")); }
                if self.user.trim().is_empty() { return Err(DbError::invalid("a connection needs a user name")); }
            }
        }
        Ok(())
    }
    /// What the document may keep of it (design 0: kind, host, port, database; no user, no password).
    pub fn hint(&self) -> Hint {
        let instance = self.instance.clone().filter(|i| !i.trim().is_empty());
        // a named instance without a port finds its port through the SQL Server Browser: no default port in the hint then
        let port = self.port.or(if instance.is_some() { None } else { self.driver.default_port() });
        Hint { driver: self.driver, host: self.host.clone(), port, database: self.database.clone(), instance }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub driver: Driver,
    #[serde(default)]
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub database: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

/// A password. Never serialized, never printed; the memory it holds is overwritten when it is dropped (as far as Rust allows:
/// the copies a driver or the caller's own request text made are theirs).
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(zeroize::Zeroizing<String>);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self { Secret(zeroize::Zeroizing::new(s.into())) }
    pub fn expose(&self) -> &str { &self.0 }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Secret(***)") }
}

/// A connection ready to open: the settings, the password, and for SQLite the file on this computer.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub connection: Connection,
    pub password: Option<Secret>,
    /// SQLite: the file (resolved inside the project by the host).
    pub file: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
    /// `table` or `view`.
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub column: String,
    #[serde(default = "asc")]
    pub dir: String,
}
fn asc() -> String { "asc".into() }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType { Text, Number, Date, Month, List }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ParamType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Refresh {
    #[default]
    Manual,
    AskOnOpen,
    AutoOnOpen,
}

/// A query definition (design 2, "질의 정의"); stored in the document as it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub connection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<Hint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableRef>,
    /// Empty: every column.
    #[serde(default)]
    pub columns: Vec<String>,
    /// `{all:[…]}`, `{any:[…]}` (two levels at most) or one condition `{column, op, value}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Value>,
    #[serde(default)]
    pub order_by: Vec<Order>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// The advanced tab: one SELECT/WITH statement in place of table, filter and order; parameters as `:name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub refresh: Refresh,
}
