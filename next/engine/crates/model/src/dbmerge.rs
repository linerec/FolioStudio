//! Data Merge from a database: what the document keeps of it (dbmerge DESIGN 3). The data source InDesign and the merge read is
//! the snapshot CSV in the project (`DataMerge/@DataSourceFile`, a project reference); the database side lives in the Folio-only
//! document label `folio.datamerge`, key `database` (written by `edit/src/hyperlinks/dbsource.rs`, the host side in
//! `server/src/dbmerge.rs`, the drivers in `folio-dbsource`). No user name, password or path of a computer is kept.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSource {
    /// The query definition (`folio_dbsource::model::Query`: connection name, hint, table, columns, filter, order, limit, SQL, parameters, refresh).
    pub query: serde_json::Value,
    /// The parameter values of the last fetch (relative ones worked out).
    pub param_values: serde_json::Value,
    /// When the rows were fetched (RFC 3339, UTC).
    pub fetched_at: String,
    /// How many records the snapshot has.
    pub rows: usize,
    /// SHA-256 of the snapshot CSV: the snapshot is the data source whose project reference has this hash.
    pub sha256: String,
}
