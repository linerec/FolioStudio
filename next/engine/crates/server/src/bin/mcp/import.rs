use folio_project::{format, manifest::Project, RelPath};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::{Path, PathBuf}};

// The staged package contains edits before the importer normalizes asset URIs.
// The original is only provenance; it is never replaced or resolved for assets.
pub fn save(project: &Project, boundary: &super::boundary::Boundary, source: &Path, fingerprint: &(String, u64),
    doc: &folio_model::Document, changes: &BTreeMap<String, Option<String>>, raw: &str) -> Result<Value, String> {
    let path = project.root.join(format!(".mcp-{}.idml", folio_project::util::uuid_v4()));
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
    let stage = Stage(path);
    let input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(input).map_err(|e| e.to_string())?;
    let mut action = |name: &str, _: &mut dyn FnMut() -> std::io::Result<String>| {
        Ok(match changes.get(name) {
            Some(Some(text)) => format::PartAction::Replace(text.clone()),
            Some(None) => format::PartAction::Drop,
            None => format::PartAction::Keep,
        })
    };
    let added: Vec<_> = changes.iter().filter_map(|(n, t)| t.as_deref().map(|t| (n.as_str(), t))).collect();
    format::write_package_with_parts(file, &mut archive, &mut action, &[], &added).map_err(|e| e.to_string())?;
    let fonts = folio_text::FontStore::empty();
    let links = folio_links::LinkReport::default();
    let missing = |_: &str| folio_links::LinkStatus::Missing;
    let prepared = folio_project::import::Prepared { source, kind: "idml",
        source_sha256: &fingerprint.0, source_bytes: fingerprint.1, idml: &stage.0,
        reader_dir: None, reader_result: None, doc, fonts: &fonts, links: &links, resolve_uri: &missing };
    let rel = RelPath::parse(raw).map_err(|e| e.to_string())?;
    let plan = folio_project::import::plan(project, &rel, &prepared);
    // the folders the import writes to (its previews and embedded pictures): no link on the way, below the root
    for folder in [plan.assets.as_str().to_owned(), format!("{}/previews", plan.assets.as_str()), format!("{}/embedded", plan.assets.as_str())] {
        boundary.path(&folder).map_err(|e| format!("{folder}: {e}"))?;
    }
    let outcome = folio_project::import::run(project, &prepared, &plan,
        json!({"mcp":"external assets not read"}), &std::sync::atomic::AtomicBool::new(false), &|_| {})
        .map_err(|e| e.to_string())?;
    Ok(json!({"ok":true,"type":"savedCopy","path":raw,"bytes":outcome.folio_bytes}))
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}
