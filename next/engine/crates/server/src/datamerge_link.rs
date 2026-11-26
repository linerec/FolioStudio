//! The data source a document remembers (Data Merge ▸ Select Data Source). The edit crate stores the reference in the preferences
//! (`DataMerge/@DataSourceFile` = `folio:path-ref/<id>`); this host side keeps the file as a project reference (root-relative path,
//! SHA-256, size: principle 1) before the edit, and reads it again when the document is opened on any computer.
//! Rules: `next/oracle/rules/datamerge.md` (datamerge-c4).
use crate::{Session, edit::EditState, place::Prepared};
use folio_edit::{Command, commands::RegisteredCommand};
use folio_project::format::{self, RefKind, Reference};
use serde_json::{Value, json};

/// The references of the document: the manifest's and the ones this session added.
fn references<'a>(cur: &'a Session, es: Option<&'a EditState>) -> Vec<&'a Reference> {
    let mut all: Vec<&Reference> = cur.folio.as_ref().map(|f| f.opened.manifest.references.iter().collect()).unwrap_or_default();
    if let Some(g) = es.and_then(|e| e.graphics.as_ref()) {
        let more: Vec<&Reference> = g.references.iter().filter(|r| !all.iter().any(|o| o.id == r.id)).collect();
        all.extend(more);
    }
    all
}

/// `dataMergeSource` without a path: the data source the document remembers (`DataMerge` of its preferences), read from the
/// project. Reply `{linked: false}` for a document without one, else `{linked: true, path, state, sha256, storedSha256, source}`;
/// `state` is `ok` (the file is what was selected), `changed` (the file at the path has other content: the user is told, `source` is
/// the file now), `moved` (the same content is elsewhere in the project, `path` is where) or `missing` (no `source`).
pub fn linked(session: Option<&Session>) -> Result<Value, String> {
    let cur = session.ok_or("Open a project document")?;
    cur.folio.as_ref().ok_or("Open a project document")?;
    let Some(edit) = cur.edit.as_ref() else { return Ok(json!({"type": "dataMergeSource", "linked": false})) };
    let es = edit.lock().expect("shared state must remain unpoisoned after each operation");
    linked_in(cur, &es)
}

/// The same where the caller already holds the edit state (an edit's preparation, the IDML export).
pub(crate) fn linked_in(cur: &Session, es: &EditState) -> Result<Value, String> {
    let folio = cur.folio.as_ref().ok_or("Open a project document")?;
    let none = || Ok(json!({"type": "dataMergeSource", "linked": false}));
    let Some((file, _)) = es.editor.data_source() else { return none() };
    let Some(id) = file.strip_prefix(format::PATH_REF_SCHEME) else { return none() };
    let refs = references(cur, Some(es));
    let Some(reference) = refs.iter().find(|r| r.id == id) else { return Ok(json!({"type": "dataMergeSource", "linked": true, "state": "missing"})) };
    let root = &folio.opened.project.root;
    let stored = reference.sha256.clone();
    let mut state = "missing";
    let mut shown = reference.path.as_ref().map(|p| p.as_str().to_owned());
    let mut bytes = None;
    if let Some(rel) = &reference.path
        && let Ok(path) = folio_project::paths::checked_project_path(root, rel.as_str())
        && path.is_file()
        && let Ok(found) = std::fs::read(&path)
    {
        state = if stored.as_deref().is_none_or(|s| s == folio_project::hash::sha256_bytes(&found)) { "ok" } else { "changed" };
        bytes = Some(found);
    }
    if bytes.is_none()
        && let (Some(sha), Some(size)) = (&stored, reference.bytes)
        && let Some(found) = folio_project::hash::HashIndex::load(root).find_by_hash(sha, size)
        && let Ok(data) = std::fs::read(found.on_disk(root))
    {
        state = "moved";
        shown = Some(found.as_str().to_owned());
        bytes = Some(data);
    }
    let mut reply = json!({"type": "dataMergeSource", "linked": true, "path": shown, "state": state, "storedSha256": stored});
    if let Some(bytes) = bytes {
        reply["sha256"] = json!(folio_project::hash::sha256_bytes(&bytes));
        let stored = reference.extra.get("importOptions").cloned().unwrap_or(Value::Null);
        reply["source"] = json!(crate::datamerge::parse_request(&bytes, &stored)?);
        reply["importOptions"] = stored;
    }
    Ok(reply)
}

/// Before an edit with `selectDataSource`: the file is read (it must be a data source), kept as a project reference of the document
/// and the command gets what `DataSourceFile` holds (`folio:path-ref/<id>`) and the file type InDesign writes. The errors are the
/// reasons the source is refused.
pub(crate) fn prepare(commands: &mut [Command], cur: &Session, es: &EditState, prepared: &mut Prepared) -> Result<(), Value> {
    let refusal = |code: &str, error: String| json!({"ok": false, "code": code, "error": error});
    for command in commands {
        let Command::Registered(RegisteredCommand::SelectDataSource { path, source, file_type, delimiter, encoding, platform, preserve_spaces }) = command else { continue };
        if source.is_some() { continue; } // given (the checks, the oracle)
        let folio = cur.folio.as_ref().ok_or_else(|| refusal("notProject", "A data source can only be selected in a project document".into()))?;
        let bytes = crate::datamerge::read(&folio.opened.project, path).map_err(|e| refusal("source", e))?;
        let given = |v: &Option<String>| v.clone().unwrap_or_default();
        let preserve = preserve_spaces.unwrap_or(false);
        let parsed = crate::datamerge::parse_named(&bytes, &given(delimiter), &given(encoding), &given(platform), preserve).map_err(|e| refusal("source", e))?;
        // the choices the file was read with are kept in the reference (Folio only: IDML has no place for them), none = read as InDesign reads it unasked
        let options = if given(delimiter).is_empty() && given(encoding).is_empty() && given(platform).is_empty() && !preserve { Value::Null }
            else { json!({"delimiter": given(delimiter), "encoding": given(encoding), "platform": given(platform), "preserveSpaces": preserve}) };
        let root = &folio.opened.project.root;
        let file = folio_project::paths::checked_project_path(root, path).map_err(|e| refusal("invalid", e))?;
        let rel = folio_project::paths::relative_to_root(root, &file).map_err(|e| refusal("invalid", e.to_string()))?;
        let sha = folio_project::hash::sha256_bytes(&bytes);
        let same = |r: &Reference| r.path.as_ref() == Some(&rel) && r.sha256.as_deref() == Some(sha.as_str()) && r.extra.get("importOptions").cloned().unwrap_or(Value::Null) == options;
        let existing = references(cur, Some(es)).into_iter().find(|r| r.kind == RefKind::File && same(r)).map(|r| r.id.clone())
            .or_else(|| prepared.refs.iter().find(|r| same(r)).map(|r| r.id.clone()));
        let id = match existing {
            Some(id) => id,
            None => {
                let r = Reference {
                    id: format!("r{}", folio_project::util::short_id()), kind: RefKind::File, path: Some(rel), sha256: Some(sha),
                    bytes: Some(bytes.len() as u64), hint: None, original_name: None, status: Some("found".into()), font: None, extra: Default::default(),
                };
                let mut r = r;
                if !options.is_null() { r.extra.insert("importOptions".into(), options.clone()); }
                let id = r.id.clone();
                prepared.refs.push(r);
                id
            }
        };
        *source = Some(format!("{}{id}", format::PATH_REF_SCHEME));
        *file_type = Some(parsed.file_type().unwrap_or("").to_owned());
    }
    Ok(())
}
