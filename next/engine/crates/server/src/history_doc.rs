//! History ▸ New Document (build-c, 2026-11-21): `historyNewDocument {done, name?}` writes the document as it was after the first `done`
//! undo steps (0 = as opened) as a new `.folio` next to the open one, under a document id of its own. The open document and its history
//! stay as they are. Reply `{ok, type: "historyDocument", documentId, path, relative}`.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn new_document(req: &Value, s: &crate::Session) -> Value {
    let fail = |code: &str, message: String| json!({"ok": false, "code": code, "error": message});
    let f = match crate::project::folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Some(shared) = &s.edit else { return fail("notEditable", "The document cannot be edited".into()) };
    let Some(done) = req["done"].as_u64() else { return fail("invalid", "done (how many undo steps the new document has) required".into()) };
    // every part as it was at that step; a part that did not exist then is left out of the new document
    let extra: BTreeMap<String, Option<String>> = {
        let es = match crate::edit::lock_for_change(shared) {
            Ok(es) => es,
            Err(refusal) => return refusal,
        };
        let at = match es.editor.parts_at_step(done as usize) {
            Ok(p) => p,
            Err(e) => return fail(e.code, e.message),
        };
        es.editor.parts().keys().chain(at.keys()).map(|n| (n.clone(), at.get(n).cloned())).collect()
    };
    let root = &f.opened.project.root;
    let directory = f.opened.file.parent().unwrap_or(root).to_path_buf();
    let stem = req["name"].as_str().filter(|n| !n.trim().is_empty()).map(str::to_owned).unwrap_or_else(|| {
        format!("{} step {done}", f.opened.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into()))
    });
    let stem = folio_project::paths::fix_name(stem.trim_end_matches(".folio"));
    let mut target = directory.join(format!("{stem}.folio"));
    let mut n = 2;
    while target.exists() {
        target = directory.join(format!("{stem} {n}.folio"));
        n += 1;
    }
    let id = folio_project::util::uuid_v4();
    let mut reply = crate::project::write_to(s, f, &target, Some(id.clone()), Some((extra, BTreeSet::new(), Vec::new())));
    if reply["ok"] == true {
        reply["type"] = json!("historyDocument");
        reply["documentId"] = json!(id);
        reply["relative"] = json!(folio_project::paths::relative_to(&root.to_string_lossy(), &target.to_string_lossy()).ok().map(|r| r.to_string()));
    }
    reply
}
