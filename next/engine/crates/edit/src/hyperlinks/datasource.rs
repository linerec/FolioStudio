//! Data Merge ▸ Select Data Source / Remove Data Source: the document remembers its data source.
//!
//! Measured with InDesign 2025 (`artifacts/datamerge-c4-20261023/measurement`, probe `datamerge-c4-a`): the IDML holds
//! `<DataMerge DataSourceFileType="CommaSeparated" DataSourceFile="<path of this computer>" />` as the first element of
//! `Resources/Preferences.xml`, in front of `DataMergeOption`. The type is `CommaSeparated`, `TabDelimited` or `SemicolonSeparated` for an
//! ASCII file and missing for a UTF-16 or Shift_JIS one. An INDD keeps the source, a merged document has no `DataMerge` element.
//! Folio never stores the path of a computer (principle 1): `DataSourceFile` holds the project reference
//! (`folio:path-ref/<id>`, root-relative path and SHA-256 in the manifest) and an IDML export writes this computer's path.
//! The host reads the file and prepares the reference (`server/src/datamerge.rs`): `source` is what `DataSourceFile` gets.
use crate::{EditError, Work, xml::{self, Index}};

const PART: &str = "Resources/Preferences.xml";

fn invalid(m: impl Into<String>) -> EditError { EditError::new("invalid", m) }

/// The `DataMerge` element of the preferences, if there is one.
fn element(ix: &Index, t: &str) -> Option<usize> {
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "DataMerge" && ix.nodes[i].parent.is_some_and(|p| ix.nodes[p].parent.is_none()))
}

impl crate::Editor {
    /// The data source the document remembers (`DataMerge` of its preferences): `(DataSourceFile, DataSourceFileType)`.
    /// `DataSourceFile` is a project reference (`folio:path-ref/<id>`) in a `.folio`.
    pub fn data_source(&self) -> Option<(String, Option<String>)> {
        let t = self.part(PART)?;
        let ix = Index::scan(t).ok()?;
        let n = element(&ix, t)?;
        Some((ix.value(t, n, "DataSourceFile")?, ix.value(t, n, "DataSourceFileType")))
    }
}

/// The import options (`delimiter`, `encoding`, `platform`, `preserve_spaces`) are what the host read the file with: they live in the
/// project reference (Folio only, InDesign's IDML has no place for them), this edit only writes the element.
pub(crate) fn select(w: &mut Work<'_>, path: &str, source: &Option<String>, file_type: &Option<String>, _delimiter: &Option<String>, _encoding: &Option<String>, _platform: &Option<String>, _preserve_spaces: &Option<bool>) -> Result<(), EditError> {
    let source = source.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| invalid(format!("{path}: the data source's file reference is missing (the host reads the file)")))?;
    if let Some(kind) = file_type.as_deref() && !matches!(kind, "CommaSeparated" | "TabDelimited" | "SemicolonSeparated" | "") {
        return Err(invalid("file type is CommaSeparated, TabDelimited or SemicolonSeparated"));
    }
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let kind = file_type.as_deref().filter(|k| !k.is_empty());
    match element(&ix, &t) {
        Some(n) => {
            // the attributes Folio does not know stay, in their order; the two it sets come first as InDesign writes them
            let mut out = String::from("<DataMerge");
            if let Some(kind) = kind { out.push_str(&format!(" DataSourceFileType=\"{}\"", xml::escape(kind))); }
            out.push_str(&format!(" DataSourceFile=\"{}\"", xml::escape(source)));
            for a in ix.attrs(&t, n) {
                let name = &t[a.name.0..a.name.1];
                if name != "DataSourceFile" && name != "DataSourceFileType" { out.push_str(&format!(" {name}=\"{}\"", &t[a.value.0..a.value.1])); }
            }
            if ix.nodes[n].empty {
                out.push_str(" />");
                w.splice(PART, vec![(ix.nodes[n].start..ix.nodes[n].end, out)])
            } else {
                out.push('>');
                w.splice(PART, vec![(ix.nodes[n].start..ix.nodes[n].open_end, out)])
            }
        }
        None => {
            let mut out = String::from("<DataMerge");
            if let Some(kind) = kind { out.push_str(&format!(" DataSourceFileType=\"{}\"", xml::escape(kind))); }
            out.push_str(&format!(" DataSourceFile=\"{}\" />", xml::escape(source)));
            // in front of DataMergeOption (InDesign's place), else the first element of the preferences
            let root = ix.roots().next().ok_or_else(|| EditError::new("xml", "Preferences has no root"))?;
            let at = ix.children(root).iter().find(|&&i| ix.local(&t, i) == "DataMergeOption").map(|&i| ix.nodes[i].start).unwrap_or(ix.nodes[root].open_end);
            let sep = if at == ix.nodes[root].open_end { "\n\t" } else { "" };
            let trail = if sep.is_empty() { "\n\t" } else { "" };
            w.splice(PART, vec![(at..at, format!("{sep}{out}{trail}"))])
        }
    }
}

pub(crate) fn remove(w: &mut Work<'_>) -> Result<(), EditError> {
    let t = w.text(PART)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let Some(n) = element(&ix, &t) else { return Ok(()) };
    // the element goes with the white space that led to it
    let b = t.as_bytes();
    let mut from = ix.nodes[n].start;
    while from > 0 && matches!(b[from - 1], b'\t' | b' ') { from -= 1; }
    if from > 0 && b[from - 1] == b'\n' { from -= 1; if from > 0 && b[from - 1] == b'\r' { from -= 1; } }
    w.splice(PART, vec![(from..ix.nodes[n].end, String::new())])
}
