//! The conveyor of the Content Collector (B) and Content Placer (Shift+B) tools: objects collected from a document, kept in
//! the order they were collected, for the Content Placer to place (View ▸ Extras ▸ Show Conveyor shows its bar).
//!
//! It is state of this engine process, like the objects clipboard (`clip.rs`): never saved in a document or a project, and it
//! survives opening another document, so what is collected in one can be placed in another (graphics' files come into the
//! project as Paste brings them). Each item is the copy Edit ▸ Copy makes (folio-edit `Editor::collect`, `ops/clipboard.rs`);
//! placing is a Paste of that item (`edit` with `conveyor` in the request, see `place::prepare`), so it is one undo step and
//! places exactly as Paste does.
//!
//! Request `{cmd: "conveyor", action, …}`; every answer is the conveyor `{ok, type: "conveyor", current, items}`:
//! - `state` (default): the items, each `{id, document, objects: [{id, kind, name}]}` (`id` is the item's number, not an object's);
//! - `collect {ids, together?}`: one item per object (`together`: one item holding them all, as copied together); answer adds `added`;
//! - `select {index | step}`: the item the Placer places next (`step`: +-1 from the current one, stopping at the ends);
//! - `remove {item?}`: the item (default: the current one) leaves; `clear`: all leave;
//! - `load {ids}`: Edit ▸ Place and Link: the objects (as one set) are loaded into the Content Placer's cursor, a slot of its own
//!   outside the list (the answer's `loaded`, `null` when empty); `unload` empties it (Esc). InDesign loads the cursor from the
//!   selection (menu action "Place and Link", measured with the input harness, conveyor-c4) and the next click places it once.
//!
//! Placing: `{cmd: "edit", conveyor: {item?}, commands: [{op: "paste", page, at}]}` places the item (default: the current one);
//! `{cmd: "edit", conveyor: {loaded: true}, commands: [{op: "placeAndLink", page, at, date}]}` places the loaded set with links
//! and empties the slot once the edit went through (a refused edit leaves it loaded).
use crate::{Session, clip::Held, edit::EditState, place::Prepared};
use serde_json::{Value, json};
use std::sync::{Mutex, MutexGuard};

/// Every item holds the document state it was collected in: the conveyor stays small.
const MAX_ITEMS: usize = 50;

type CollectedItem = (Held, Vec<(String, String, String)>);

struct Entry {
    id: u64,
    held: Held,
    /// Each object as `folio_edit::Clipboard::summary` gives it: (`Self` where it was collected, IDML element, name).
    objects: Vec<(String, String, String)>,
    /// The file name (never the path) of the document it was collected in.
    document: String,
}

struct Conveyor {
    entries: Vec<Entry>,
    current: usize,
    next_id: u64,
    /// The cursor of Place and Link: one set, placed by the next click and then gone.
    loaded: Option<Entry>,
}

static CONVEYOR: Mutex<Conveyor> = Mutex::new(Conveyor { entries: Vec::new(), current: 0, next_id: 1, loaded: None });

fn conveyor() -> MutexGuard<'static, Conveyor> {
    CONVEYOR.lock().unwrap_or_else(|p| p.into_inner())
}

fn refusal(code: &str, error: impl Into<String>) -> Value {
    json!({"ok": false, "code": code, "error": error.into()})
}

fn describe(e: &Entry) -> Value {
    json!({
        "id": e.id,
        "document": e.document,
        "objects": e.objects.iter().map(|(id, kind, name)| json!({"id": id, "kind": kind, "name": name})).collect::<Vec<_>>(),
    })
}

impl Conveyor {
    fn state(&self) -> Value {
        let items: Vec<Value> = self.entries.iter().map(describe).collect();
        json!({"ok": true, "type": "conveyor", "current": if self.entries.is_empty() { Value::Null } else { json!(self.current) }, "items": items,
            "loaded": self.loaded.as_ref().map(describe)})
    }

    /// Keeps `current` on an item after items left.
    fn clamp(&mut self) {
        self.current = self.current.min(self.entries.len().saturating_sub(1));
    }

    /// The new items go at the end (the Placer places the first one first); the number of each.
    fn add(&mut self, items: Vec<CollectedItem>, document: &str) -> Result<Vec<u64>, Value> {
        if self.entries.len() + items.len() > MAX_ITEMS {
            return Err(refusal("full", format!("컨베이어에는 {MAX_ITEMS}개까지 담을 수 있습니다")));
        }
        let mut added = Vec::new();
        for (held, objects) in items {
            let id = self.next_id;
            self.next_id += 1;
            self.entries.push(Entry { id, held, objects, document: document.to_owned() });
            added.push(id);
        }
        Ok(added)
    }

    fn select(&mut self, req: &Value) -> Value {
        if self.entries.is_empty() {
            return refusal("empty", "컨베이어가 비어 있습니다");
        }
        let last = self.entries.len() - 1;
        if let Some(i) = req["index"].as_u64() {
            if i as usize > last {
                return refusal("invalid", format!("컨베이어에는 {}번째 항목이 없습니다", i + 1));
            }
            self.current = i as usize;
        } else if let Some(d) = req["step"].as_i64() {
            self.current = (self.current as i64 + d).clamp(0, last as i64) as usize;
        } else {
            return refusal("invalid", "index 나 step 이 있어야 합니다");
        }
        self.state()
    }

    fn remove(&mut self, req: &Value) -> Value {
        let at = match req["item"].as_u64() {
            Some(id) => self.entries.iter().position(|e| e.id == id),
            None => (!self.entries.is_empty()).then_some(self.current),
        };
        let Some(at) = at else { return refusal("notFound", "컨베이어에 그 항목이 없습니다") };
        self.entries.remove(at);
        // the item after the removed one becomes the current one; the current one stays when an earlier item left
        if at < self.current {
            self.current -= 1;
        }
        self.clamp();
        self.state()
    }

    fn clear(&mut self) -> Value {
        self.entries.clear();
        self.current = 0;
        self.state()
    }

    /// The item `spec.item`, else the current one.
    fn item(&self, spec: &Value) -> Result<&Entry, &'static str> {
        match spec["item"].as_u64() {
            Some(id) => self.entries.iter().find(|e| e.id == id).ok_or("컨베이어에 그 항목이 없습니다"),
            None => self.entries.get(self.current).ok_or("컨베이어가 비어 있습니다"),
        }
    }
}

pub fn run(s: Option<&Session>, req: &Value) -> Value {
    match req["action"].as_str().unwrap_or("state") {
        "state" => conveyor().state(),
        "collect" => collect(s, req),
        "select" => conveyor().select(req),
        "remove" => conveyor().remove(req),
        "clear" => conveyor().clear(),
        "load" => load(s, req),
        "unload" => {
            let mut c = conveyor();
            c.loaded = None;
            c.state()
        }
        other => refusal("invalid", format!("모르는 컨베이어 동작입니다: {other}")),
    }
}

fn collect(s: Option<&Session>, req: &Value) -> Value {
    let Some(s) = s else { return refusal("noDocument", "열린 문서가 없습니다") };
    let Some(shared) = &s.edit else { return refusal("notEditable", "편집하는 문서가 아닙니다") };
    let ids: Vec<String> = req["ids"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default();
    let mut es = crate::edit::lock_for_read(shared);
    es.editor.set_frame_starts(folio_text::DocumentFrames::new(&s.doc).frame_ids()
        .map(|id| ((*id).to_owned(), s.layout.frames.get(*id).and_then(|f| f.lines.first()).map(|l| l.start))).collect());
    let clip = match es.editor.collect(&ids) {
        Ok(c) => c,
        Err(e) => return refusal(e.code, e.message),
    };
    let clips = if req["together"].as_bool() == Some(true) { vec![clip] } else { clip.split() };
    let document = es.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let items = clips.into_iter().map(|clip| { let objects = clip.summary(); (Held::of(s, &es, clip), objects) }).collect();
    let mut c = conveyor();
    match c.add(items, &document) {
        Ok(added) => {
            let mut out = c.state();
            out["added"] = json!(added);
            out
        }
        Err(refused) => refused,
    }
}

/// Place and Link, the first half: the objects are copied as one set into the cursor (the slot replaces what it held).
fn load(s: Option<&Session>, req: &Value) -> Value {
    let Some(s) = s else { return refusal("noDocument", "열린 문서가 없습니다") };
    let Some(shared) = &s.edit else { return refusal("notEditable", "편집하는 문서가 아닙니다") };
    let ids: Vec<String> = req["ids"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default();
    if ids.is_empty() {
        return refusal("invalid", "배치할 개체를 고르세요");
    }
    let mut es = crate::edit::lock_for_read(shared);
    es.editor.set_frame_starts(folio_text::DocumentFrames::new(&s.doc).frame_ids()
        .map(|id| ((*id).to_owned(), s.layout.frames.get(*id).and_then(|f| f.lines.first()).map(|l| l.start))).collect());
    let clip = match es.editor.collect(&ids) {
        Ok(c) => c,
        Err(e) => return refusal(e.code, e.message),
    };
    let document = es.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let objects = clip.summary();
    let held = Held::of(s, &es, clip);
    let mut c = conveyor();
    let id = c.next_id;
    c.next_id += 1;
    c.loaded = Some(Entry { id, held, objects, document });
    c.state()
}

/// Empties the cursor after an edit that placed it went through.
pub fn unload() {
    conveyor().loaded = None;
}

/// What a Paste in the edit of `cur` pastes when the request names the conveyor: the item `spec.item`, else the current one,
/// its graphics linking to files of this project when it was collected in another document.
pub fn for_place(cur: &Session, es: &EditState, prepared: &mut Prepared, spec: &Value) -> Result<Option<folio_edit::Clipboard>, String> {
    let c = conveyor();
    if spec["loaded"].as_bool() == Some(true) {
        let e = c.loaded.as_ref().ok_or("배치할 개체가 올려져 있지 않습니다. 배치 및 연결을 먼저 하세요")?;
        let clip = crate::clip::arrive(&e.held, cur, es, prepared)?;
        if prepared.wants_link {
            crate::xlink::prepare(&e.held, cur, es, prepared)?;
        }
        prepared.unload_after = true;
        return Ok(Some(clip));
    }
    let held = &c.item(spec)?.held;
    let clip = crate::clip::arrive(held, cur, es, prepared)?;
    if prepared.wants_link {
        crate::xlink::prepare(held, cur, es, prepared)?;
    }
    Ok(Some(clip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A document with three rectangles `a`, `b`, `c`.
    fn editor() -> folio_edit::Editor {
        let rect = |id: &str| format!(r#"<Rectangle Self="{id}" Name="rect {id}" ItemLayer="L" ItemTransform="1 0 0 1 0 0"/>"#);
        folio_edit::Editor::new(BTreeMap::from([
            ("designmap.xml".to_owned(), r#"<Document Self="d"><Layer Self="L" Name="Layer 1"/></Document>"#.to_owned()),
            ("Spreads/Spread_s.xml".to_owned(), format!(r#"<idPkg:Spread><Spread Self="s">{}{}{}</Spread></idPkg:Spread>"#, rect("a"), rect("b"), rect("c"))),
        ]))
    }

    fn items(ed: &folio_edit::Editor, ids: &[&str]) -> Vec<CollectedItem> {
        let ids: Vec<String> = ids.iter().map(|s| (*s).to_owned()).collect();
        ed.collect(&ids).unwrap().split().into_iter().map(|clip| { let o = clip.summary(); (Held::test(clip), o) }).collect()
    }

    fn ids(state: &Value) -> Vec<u64> {
        state["items"].as_array().unwrap().iter().map(|i| i["id"].as_u64().unwrap()).collect()
    }

    fn new() -> Conveyor {
        Conveyor { entries: Vec::new(), current: 0, next_id: 1, loaded: None }
    }

    #[test]
    fn items_queue_in_the_order_collected_and_the_first_is_current() {
        let (ed, mut c) = (editor(), new());
        assert_eq!(c.state()["current"], Value::Null);
        assert_eq!(c.add(items(&ed, &["b", "a"]), "x.folio").unwrap(), vec![1, 2]);
        assert_eq!(c.add(items(&ed, &["c"]), "x.folio").unwrap(), vec![3]);
        let s = c.state();
        assert_eq!(ids(&s), vec![1, 2, 3]);
        assert_eq!(s["current"], 0);
        // stacking order of a set (bottom to top), kind and name of each object, the document by its file name
        assert_eq!(s["items"][0]["objects"][0], json!({"id": "a", "kind": "Rectangle", "name": "rect a"}));
        assert_eq!(s["items"][0]["document"], "x.folio");
        assert_eq!(c.item(&json!({})).unwrap().id, 1);
        assert_eq!(c.item(&json!({"item": 3})).unwrap().id, 3);
        assert!(c.item(&json!({"item": 9})).is_err());
    }

    #[test]
    fn select_by_index_or_step_stops_at_the_ends() {
        let (ed, mut c) = (editor(), new());
        assert_eq!(c.select(&json!({"step": 1}))["code"], "empty");
        c.add(items(&ed, &["a", "b", "c"]), "x").unwrap();
        assert_eq!(c.select(&json!({"step": 1}))["current"], 1);
        assert_eq!(c.select(&json!({"step": 5}))["current"], 2);
        assert_eq!(c.select(&json!({"step": -9}))["current"], 0);
        assert_eq!(c.select(&json!({"index": 2}))["current"], 2);
        assert_eq!(c.item(&json!({})).unwrap().id, 3);
        assert_eq!(c.select(&json!({"index": 3}))["code"], "invalid");
        assert_eq!(c.select(&json!({}))["code"], "invalid");
        assert_eq!(c.current, 2);
    }

    #[test]
    fn removing_keeps_the_current_item_on_an_item() {
        let (ed, mut c) = (editor(), new());
        c.add(items(&ed, &["a", "b", "c"]), "x").unwrap();
        c.select(&json!({"index": 1}));
        // an earlier item leaves: the current one stays the same item
        assert_eq!(ids(&c.remove(&json!({"item": 1}))), vec![2, 3]);
        assert_eq!(c.item(&json!({})).unwrap().id, 2);
        // the current one leaves: the next one takes its place
        assert_eq!(ids(&c.remove(&json!({}))), vec![3]);
        assert_eq!(c.item(&json!({})).unwrap().id, 3);
        assert_eq!(c.remove(&json!({"item": 7}))["code"], "notFound");
        assert_eq!(c.remove(&json!({}))["current"], Value::Null);
        assert_eq!(c.remove(&json!({}))["code"], "notFound");
    }

    #[test]
    fn the_last_item_leaving_moves_current_back_and_clear_empties() {
        let (ed, mut c) = (editor(), new());
        c.add(items(&ed, &["a", "b"]), "x").unwrap();
        c.select(&json!({"index": 1}));
        assert_eq!(c.remove(&json!({}))["current"], 0);
        c.add(items(&ed, &["c"]), "x").unwrap();
        assert_eq!(ids(&c.state()), vec![1, 3], "numbers are not used again");
        let s = c.clear();
        assert_eq!((ids(&s), s["current"].clone()), (vec![], Value::Null));
    }

    #[test]
    fn the_conveyor_holds_fifty_items() {
        let (ed, mut c) = (editor(), new());
        for _ in 0..MAX_ITEMS {
            c.add(items(&ed, &["a"]), "x").unwrap();
        }
        let full = c.add(items(&ed, &["a"]), "x").unwrap_err();
        assert_eq!(full["code"], "full");
        assert_eq!(c.entries.len(), MAX_ITEMS);
    }

    #[test]
    fn the_cursor_of_place_and_link_is_a_slot_outside_the_list() {
        let (ed, mut c) = (editor(), new());
        c.add(items(&ed, &["a"]), "x").unwrap();
        assert_eq!(c.state()["loaded"], Value::Null);
        // loaded (the objects as one set), it shows in the answer and the list is as it was
        let ids: Vec<String> = ["c", "b"].iter().map(|s| (*s).to_owned()).collect();
        let clip = ed.collect(&ids).unwrap();
        let objects = clip.summary();
        c.loaded = Some(Entry { id: 9, held: Held::test(clip), objects, document: "x.folio".into() });
        let s = c.state();
        assert_eq!(s["loaded"]["objects"].as_array().unwrap().len(), 2);
        assert_eq!(s["loaded"]["document"], "x.folio");
        assert_eq!(ids_of(&s), vec![1]);
        // what the list does never touches the cursor
        c.clear();
        assert!(c.state()["loaded"].is_object());
        c.loaded = None;
        assert_eq!(c.state()["loaded"], Value::Null);
    }

    fn ids_of(state: &Value) -> Vec<u64> {
        ids(state)
    }

    #[test]
    fn a_set_collected_together_is_one_item() {
        let (ed, mut c) = (editor(), new());
        let ids: Vec<String> = ["c", "a"].iter().map(|s| (*s).to_owned()).collect();
        let clip = ed.collect(&ids).unwrap();
        let o = clip.summary();
        c.add(vec![(Held::test(clip), o)], "x").unwrap();
        let s = c.state();
        assert_eq!(s["items"].as_array().unwrap().len(), 1);
        assert_eq!(s["items"][0]["objects"].as_array().unwrap().len(), 2);
    }
}
