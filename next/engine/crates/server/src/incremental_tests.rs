//! The incremental read of an edit (`edit::read_document`) must give the session a full read gives. Two sessions of the same document get
//! the same edits, one with `FOLIO_FULL_REBUILD` on (every part read again) and one without; after each edit, undo and redo their
//! digests (`edit::digest`: the whole model and composition) and the replies must agree. A second test holds the page settings
//! cache (`pages::settings_with`) to the function it replaced.

use crate::{Fonts, Profiles, Session, edit, load_session, pages};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ["basic", "cjk", "wrapspan", "index-c3-ranges", "xref-fmt", "wrapjump", "anchored", "basic-margins"].iter().map(|n| root().join(format!("oracle/fixtures/{n}.idml"))).collect();
    out.push(root().join("../fixtures/indesign-feature-atlas-20261018/InDesign_Feature_Atlas.idml"));
    out.retain(|p| p.is_file());
    out
}

struct Env {
    fonts: Fonts,
    profiles: Arc<Profiles>,
}

fn env() -> Env {
    Env { fonts: Fonts { startup: Mutex::new(None), spare: Mutex::new(None) }, profiles: Arc::new(Profiles::start()) }
}

fn open(env: &Env, path: &std::path::Path) -> Session {
    load_session(path.to_str().unwrap(), true, &env.fonts, &env.profiles, &|_, _| {}).expect("opens")
}

/// What an edit reply says that does not depend on how long anything took or when the document was opened.
fn steady(reply: &Value) -> Value {
    let mut r = reply.clone();
    r.as_object_mut().map(|o| o.remove("ms"));
    if let Some(d) = r.get_mut("document").and_then(Value::as_object_mut) {
        for k in ["timings", "color"] {
            d.remove(k);
        }
    }
    r
}

/// The movable items and the stories with text of the document's first pages.
fn targets(s: &Session, most: usize) -> (Vec<String>, Vec<String>) {
    let (mut items, mut stories) = (Vec::new(), Vec::new());
    for sp in &s.doc.spreads {
        for it in &sp.items {
            if items.len() < most && it.visible && !it.locked && matches!(it.kind, folio_model::ItemKind::Rectangle { .. } | folio_model::ItemKind::Oval { .. } | folio_model::ItemKind::TextFrame(_)) {
                items.push(it.id.clone());
            }
            if let Some(f) = it.text_frame()
                && stories.len() < most
                && !stories.contains(&f.story)
                && s.doc.stories.get(&f.story).is_some_and(|st| st.paragraphs.iter().flat_map(|p| &p.runs).any(|r| !r.text.trim().is_empty()))
            {
                stories.push(f.story.clone());
            }
        }
    }
    (items, stories)
}

struct Pair {
    a: Session,
    b: Session,
    edits: usize,
    incremental: usize,
}

impl Pair {
    fn new(env: &Env, path: &std::path::Path) -> Pair {
        Pair { a: open(env, path), b: open(env, path), edits: 0, incremental: 0 }
    }

    fn agree(&self, what: &str) {
        let (a, b) = (edit::digest(&self.a), edit::digest(&self.b));
        assert_eq!(a, b, "{what}: the sessions differ");
    }

    fn send(&mut self, req: Value, what: &str) -> Value {
        edit::force_full_rebuild(None);
        let ra = edit::run(&req, &self.a);
        edit::force_full_rebuild(Some(true));
        let rb = edit::run(&req, &self.b);
        edit::force_full_rebuild(None);
        let (ra, rb) = match (ra, rb) {
            (Ok((na, ra)), Ok((nb, rb))) => {
                if let Some(n) = na {
                    self.a = n;
                }
                if let Some(n) = nb {
                    self.b = n;
                }
                if ra["ms"]["incremental"] == json!(true) {
                    self.incremental += 1;
                }
                (ra, rb)
            }
            (Err(ea), Err(eb)) => (ea, eb),
            (a, b) => panic!("{what}: one engine refused, the other did not: {:?} / {:?}", a.err(), b.err()),
        };
        assert_eq!(steady(&ra), steady(&rb), "{what}: the replies differ");
        self.edits += 1;
        self.agree(what);
        rb
    }
}

#[test]
fn incremental_sessions_equal_full_rebuilds() {
    let env = env();
    let (mut edits, mut incremental) = (0, 0);
    for path in fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut p = Pair::new(&env, &path);
        p.agree(&format!("{name}: open"));
        let (items, stories) = targets(&p.a, if name.starts_with("InDesign") { 14 } else { 5 });
        assert!(!items.is_empty() || !stories.is_empty(), "{name}: nothing to edit");
        // The base is prepared on open, so the first move is already incremental.
        let mut row = Vec::new();
        for (i, id) in items.iter().enumerate() {
            let r = p.send(json!({"cmd": "edit", "label": "move", "commands": [{"op": "move", "ids": [id], "by": [3, 2]}]}), &format!("{name}: move {id}"));
            assert_eq!(r["ok"], json!(true), "{name}: move {id}: {r}");
            if i == 0 { assert_eq!(p.incremental, 1, "{name}: first move was not incremental"); }
            row.push(json!({"cmd": "edit", "label": "move", "commands": [{"op": "move", "ids": [id], "by": [1, 1]}]}));
            p.send(json!({"cmd": "undo"}), &format!("{name}: undo move {id}"));
            if i % 2 == 0 {
                p.send(json!({"cmd": "redo"}), &format!("{name}: redo move {id}"));
                p.send(json!({"cmd": "undo"}), &format!("{name}: undo move {id} again"));
            }
        }
        for (i, s) in stories.iter().enumerate() {
            p.send(json!({"cmd": "edit", "label": "type", "commands": [{"op": "insertText", "story": s, "at": 0, "text": "ab"}]}), &format!("{name}: type in {s}"));
            row.push(json!({"cmd": "edit", "label": "type", "commands": [{"op": "insertText", "story": s, "at": 0, "text": "z"}]}));
            p.send(json!({"cmd": "undo"}), &format!("{name}: undo typing in {s}"));
            if i % 2 == 0 {
                p.send(json!({"cmd": "redo"}), &format!("{name}: redo typing in {s}"));
                p.send(json!({"cmd": "undo"}), &format!("{name}: undo typing in {s} again"));
            }
        }
        // ten edits in a row, then all of them undone and redone
        let ten: Vec<Value> = row.iter().cycle().take(10.min(row.len().max(1) * 3)).cloned().collect();
        for (i, q) in ten.iter().enumerate() {
            p.send(q.clone(), &format!("{name}: edit {i} of a row"));
        }
        for i in 0..ten.len() {
            p.send(json!({"cmd": "undo"}), &format!("{name}: undo {i} of a row"));
        }
        for i in 0..ten.len() {
            p.send(json!({"cmd": "redo"}), &format!("{name}: redo {i} of a row"));
        }
        eprintln!("{name}: {} edits, {} of them through the incremental read", p.edits, p.incremental);
        edits += p.edits;
        incremental += p.incremental;
    }
    assert!(incremental * 10 >= edits * 7, "only {incremental} of {edits} edits took the incremental path");
}

#[test]
fn item_cache_survives_preview_rollback_and_structural_fallback() {
    let env = env();
    let path = root().join("oracle/fixtures/basic-margins.idml");
    let mut p = Pair::new(&env, &path);
    let id = targets(&p.a, 1).0.remove(0);
    let move_by = |n| json!({"op": "move", "ids": [&id], "by": [n, 0]});
    let mut send = |commands: Value, what: &str| {
        let r = p.send(json!({"cmd": "edit", "label": what, "commands": commands}), what);
        assert_eq!(r["ok"], true, "{what}: {r}");
    };
    send(json!([move_by(1)]), "first move");
    send(json!([{"op":"previewStart", "label":"preview"}]), "start preview");
    send(json!([{"op":"previewApply", "commands":[move_by(1000)]}]), "longer attribute");
    send(json!([{"op":"previewApply", "commands":[move_by(-3)]}]), "replace preview");
    send(json!([{"op":"previewCancel"}]), "cancel preview");
    send(json!([{"op":"setLabel", "id":&id, "key":"cache-test", "value":"new subtree"}]), "structural edit");
    send(json!([move_by(2)]), "move after structural edit");
    p.send(json!({"cmd":"undo"}), "undo move");
    p.send(json!({"cmd":"undo"}), "undo structural edit");
    p.send(json!({"cmd":"redo"}), "redo structural edit");
    let before = edit::digest(&p.a);
    let r = p.send(json!({"cmd":"edit", "commands":[move_by(4), {"op":"move", "ids":["missing-object"], "by":[1,0]}]}), "failed transaction rollback");
    assert_eq!(r["ok"], false);
    assert_eq!(edit::digest(&p.a), before);
    p.send(json!({"cmd":"edit", "commands":[move_by(1)]}), "move after rollback");
}

#[test]
fn page_settings_cache_equals_the_function_it_replaced() {
    for path in fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let parts = edit::package_parts(&path).expect("parts");
        let mut cache = pages::SettingsCache::default();
        assert_eq!(pages::settings_with(&parts, &Default::default(), &mut cache), pages::reference::settings(&parts), "{name}: first reading");
        // edits change spread parts and their revisions: the cache must follow
        let mut ed = folio_edit::Editor::new(parts);
        let ids: Vec<String> = ed.parts().iter().filter(|(n, _)| n.starts_with("Spreads/")).flat_map(|(_, t)| {
            let mut out = Vec::new();
            let mut rest = t.as_str();
            while let Some(at) = rest.find("<Rectangle Self=\"") {
                let tail = &rest[at + 17..];
                let end = tail.find('"').unwrap_or(0);
                out.push(tail[..end].to_owned());
                rest = &tail[end..];
            }
            out
        }).take(6).collect();
        for id in ids {
            let cmd: folio_edit::Command = serde_json::from_value(json!({"op": "move", "ids": [id], "by": [2, 2]})).expect("command");
            if ed.apply("move", &[cmd]).is_err() {
                continue;
            }
            assert_eq!(pages::settings_with(ed.parts(), ed.part_revs(), &mut cache), pages::reference::settings(ed.parts()), "{name}: after a move of {id}");
            ed.undo();
            assert_eq!(pages::settings_with(ed.parts(), ed.part_revs(), &mut cache), pages::reference::settings(ed.parts()), "{name}: after undoing it");
        }
    }
}
