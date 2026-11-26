//! Serializable summaries of the plan and completed project import.

use super::Plan;
use crate::paths::RelPath;
use serde::Serialize;
use folio_model::Document;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub count: usize,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCounts {
    /// Placed links (graphics, movies, sounds).
    pub placements: usize,
    /// Distinct linked files found.
    pub files: usize,
    /// Found inside the project: kept where they are.
    pub inside: Count,
    /// Found outside the project: copied in.
    pub outside: Count,
    /// Not found: the original URI is kept as a hint, the preview is kept.
    pub missing: usize,
    /// Several equally good candidates: treated as missing (nothing is guessed).
    pub ambiguous: usize,
    /// Embedded in the document.
    pub embedded: usize,
    /// Found, but not the file InDesign placed: its size differs from the recorded one
    /// (copied and marked modified, as InDesign would show it).
    pub modified: usize,
    pub modified_names: Vec<String>,
    pub missing_names: Vec<String>,
    pub ambiguous_rows: Vec<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRow {
    pub family: String,
    pub style: String,
    /// `copy`, `record` (name + version + hash only), `missing` or `substituted`.
    pub action: &'static str,
    pub code: Option<&'static str>,
    pub reason: Option<String>,
    pub file: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Renamed {
    pub from: String,
    pub to: String,
}

/// Progress of a running import.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
    pub files: usize,
    pub files_total: usize,
}

/// What an import made.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub document: RelPath,
    pub path: PathBuf,
    pub id: String,
    pub folio_bytes: u64,
    pub references: usize,
    /// Files written into the project (after content de-duplication) and their bytes.
    pub files_written: usize,
    pub bytes_written: u64,
    /// Items whose content was already in the project (or earlier in this import).
    pub deduplicated: usize,
    pub links: Value,
    pub timings: Value,
    pub report: Value,
}

pub(super) fn import_report(plan: &Plan, doc: &Document, now: &str, dedup: usize, report_extra: Value) -> Value {
    let unsupported: Vec<&String> = doc.warnings.iter().collect();
    json!({
        "format": "folio-import-report", "version": 1,
        "source": plan.source, "document": plan.document, "time": now,
        "links": plan.links, "previews": plan.previews, "embedded": plan.embedded,
        "fonts": plan.fonts, "fontCounts": plan.font_counts, "otherFiles": plan.other_files,
        "renamed": plan.renamed, "longPaths": plan.long_paths, "warnings": plan.warnings,
        "warningMessages": plan.warning_messages,
        "deduplicated": dedup,
        // principle 2: what Folio does not support or approximates (from the IDML reading)
        "unsupported": unsupported,
        "limits": crate::limits::report(&plan.differences),
        "extra": report_extra,
    })
}
