//! Data Merge from a database (design: `artifacts/dbmerge-20261119/DESIGN.ko.md`).
//!
//! - `model`: connection settings (app user settings, never the project; no password), query definitions (kept in the document).
//! - `params`: query parameters and their relative values (`today`, `thisMonth`, …), from a "today" the caller gives.
//! - `sql`: a query definition → one parameterised SELECT per database kind; identifiers are checked against the columns the
//!   database listed and quoted for the kind, values are always bound.
//! - `advanced`: the hand-written SQL of the advanced tab: one SELECT/WITH statement, run in a read-only transaction.
//! - `drivers`: PostgreSQL, MySQL/MariaDB, SQLite, SQL Server/Azure SQL (every platform; iOS links the system SQLite).
//! - `snapshot`: the result as the CSV the document keeps in the project (and as a `folio_idml::datamerge::Source`).
//! - `credentials`: passwords in the OS credential store.
//!
//! Nothing here writes to a database.
pub mod advanced;
pub mod credentials;
pub mod error;
pub mod model;
pub mod params;
pub mod run;
pub mod snapshot;
pub mod sql;
mod drivers;
mod tls;

pub use error::{DbError, Kind};
pub use model::Secret;

/// What this device can do with databases (`dbCapabilities`): the platform, whether passwords can be kept (and why not), whether a
/// password can be kept for the session (always), whether certificates can be checked against the trusted roots.
pub fn capabilities() -> serde_json::Value {
    serde_json::json!({
        "platform": std::env::consts::OS,
        "credentialStore": credentials::store_info(),
        "sessionPasswords": true,
        "tlsVerify": tls::verify_info(),
    })
}

/// Connecting gives up after this long (design 0, "한도").
pub const CONNECT_TIMEOUT_MS: u64 = 10_000;
/// A query gives up after this long.
pub const QUERY_TIMEOUT_MS: u64 = 30_000;
/// More rows than this are refused (`limit`).
pub const MAX_ROWS: usize = 50_000;
/// More text than this (all values together, UTF-8) is refused (`limit`).
pub const MAX_BYTES: usize = 64 << 20;
/// The rows a preview shows unless it asks for another number.
pub const PREVIEW_ROWS: usize = 50;
