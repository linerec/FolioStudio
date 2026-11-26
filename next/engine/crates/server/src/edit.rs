//! Editing the open document (`folio-edit`):
//!   {"cmd":"edit","label":"이동","commands":[{"op":"move","ids":["u1"],"by":[10,5]}, …]}
//!   {"cmd":"undo"}   {"cmd":"redo"}   {"cmd":"editState"}
//!   {"cmd":"labels","key":"oracle.id"}   script label value → object id, for scripts
//!   {"cmd":"labels","ids":["u12"]}       the Script Label panel's label (key "Label") of each object: id → text
//! Reply: {"ok":true,"type":"edited","rev":3,"label":"이동","pages":[0,1],"parts":[…],
//!         "undo":"이동","redo":null,"dirty":true,"ms":…}
//!
//! A `.folio` the user holds the lock of is editable; an IDML is editable only when opened
//! with `"edit": true` (tests; there is nowhere to save it). INDD opened read-only is not.
//! Each change builds a new session from the edited parts (the model read again, text laid
//! out again) and replaces the current one; renders in flight finish on the old one.

use crate::{Session, wire};
use folio_edit::{Command, Editor, Outcome, commands::RegisteredCommand};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct EditState {
    pub editor: Editor,
    /// A `.folio`'s placed-graphic records (manifest), kept in step with the edits: see
    /// `sync_graphics`. None for an IDML.
    pub graphics: Option<Graphics>,
    /// What came with the package from outside it (previews, link locations, saved lines).
    pub data: folio_idml::ReaderData,
    /// The document's path (its folder is where relative things are found).
    pub path: PathBuf,
    /// Editor version (`Editor::version`: apply, undo and redo count, a preview does not) at the last save.
    pub saved_rev: u64,
    /// The Story Editor's order of each table's cells (table id): a view setting of this open document, not an edit
    /// (see `table_view`).
    pub table_views: BTreeMap<String, crate::table_view::Order>,
    /// The document as the parts were last read (before anything is done to it for a session), and the revisions of the parts
    /// (`Editor::part_revs`) it reflects: an edit reads again only the spread and story parts whose revision moved (see `read_document`).
    /// Prepared when editing opens; None with `FOLIO_FULL_REBUILD` set.
    pub inc: Option<IncBase>,
    /// The page settings of each spread part, read again only when the part changed.
    pub settings_cache: crate::pages::SettingsCache,
    /// Whether each spread, parent spread or styles part has an Auto-Size setting on, by part with the revision it was looked at.
    autosize: std::collections::HashMap<String, (u64, bool)>,
}

/// What an incremental read keeps between edits.
pub struct IncBase {
    base: folio_idml::ReadBase,
    stamp: BTreeMap<String, u64>,
}

/// How the last rebuild went, for the reply's `ms`.
#[derive(Default)]
pub struct RebuildInfo {
    /// The model was updated from the parts that changed instead of read from all of them.
    pub incremental: bool,
    /// Why not, when it was not.
    pub why: Option<String>,
    pub read_ms: f64,
    pub layout_ms: f64,
    pub after_ms: f64,
    page_settings_unchanged: bool,
}

thread_local! {
    /// Tests: this thread's answer to "is the incremental read off" instead of the environment's.
    static FORCE_FULL: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Tests: read everything again on every edit (`Some(true)`), or as the environment says (`None`), on this thread.
#[cfg(test)]
pub fn force_full_rebuild(on: Option<bool>) {
    FORCE_FULL.with(|f| f.set(on));
}

/// `FOLIO_FULL_REBUILD=1` reads every part again on every edit, as before the incremental read: the way to turn it off if an edit ever
/// shows what a full read would not.
fn full_rebuild_forced() -> bool {
    FORCE_FULL.with(|f| f.get()).unwrap_or_else(|| std::env::var_os("FOLIO_FULL_REBUILD").is_some_and(|v| !v.is_empty() && v != "0"))
}

pub type Shared = Arc<Mutex<EditState>>;

/// Every part of the package (`folio/…` left out: the `.folio`'s own parts).
pub fn package_parts(file: &Path) -> Result<BTreeMap<String, String>, String> {
    folio_idml::read_editor_parts(file).map_err(|e| e.to_string())
}

pub fn start(file: &Path, path: &Path, data: folio_idml::ReaderData, manifest: Option<&folio_project::format::DocManifest>) -> Result<(folio_model::Document, Shared), String> {
    let folio_idml::EditablePackage { document, parts, base } =
        folio_idml::read_editable_package(file, path, &data, !full_rebuild_forced()).map_err(|e| format!("IDML: {e:?}"))?;
    // Snapshot owners before the first edit can remove/replace a graphic (place-img).
    let graphics = manifest.map(|m| {
        let part_of = parts.iter().filter(|(n, _)| n.starts_with("Spreads/") || n.starts_with("MasterSpreads/"))
            .flat_map(|(name, t)| graphics_in(t).into_iter().map(move |(id, ..)| (id, name.clone()))).collect();
        Graphics { base: m.graphics.clone(), references: m.references.clone(), part_of: Some(part_of), synced: m.graphics.clone(), labels: Default::default(), placed_sources: Default::default() }
    });
    let inc = base.map(|base| IncBase { base, stamp: BTreeMap::new() });
    let mut editor = Editor::new(parts);
    // the document's file name: what a cross-reference's File Name block shows
    editor.set_file_name(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let state = EditState { editor, graphics, data, path: path.to_path_buf(), saved_rev: 0, table_views: BTreeMap::new(), inc, settings_cache: Default::default(), autosize: Default::default() };
    Ok((document, Arc::new(Mutex::new(state))))
}

/// A `.folio`'s placed graphics as the edits leave them.
pub struct Graphics {
    /// Reference id → resolved project file for newly placed graphics (retained for redo).
    pub placed_sources: std::collections::HashMap<String, folio_idml::LinkOverride>,
    /// The manifest's records when the document was opened.
    pub base: Vec<folio_project::format::GraphicRefs>,
    pub references: Vec<folio_project::format::Reference>,
    /// Graphic `Self` → its part, for the opened document (found on the first edit).
    part_of: Option<std::collections::HashMap<String, String>>,
    /// Records now: copies added (their own Folio id), deleted graphics left out.
    pub synced: Vec<folio_project::format::GraphicRefs>,
    /// `folio.ref` labels for the frames of copies (frame `Self` → pairs), written on save.
    pub labels: std::collections::HashMap<String, Vec<(String, String)>>,
}

const GRAPHIC_ELEMENTS: &[&str] = &["Image", "EPS", "PDF", "ImportedPage", "WMF", "PICT", "SVG"];

/// Graphics of a part: (graphic Self, frame Self, Link Self, Link URI).
type GraphicLinks = (String, Option<String>, Option<String>, Option<String>);

fn graphics_in(t: &str) -> Vec<GraphicLinks> {
    let Ok(ix) = folio_edit::xml::Index::scan(t) else { return Vec::new() };
    let mut out = Vec::new();
    for i in 0..ix.nodes.len() {
        if !GRAPHIC_ELEMENTS.contains(&ix.local(t, i)) {
            continue;
        }
        let Some(id) = ix.attr(t, i, "Self") else { continue };
        let frame = ix.nodes[i].parent.and_then(|p| ix.attr(t, p, "Self")).map(str::to_owned);
        let link = ix.named(t, i, "Link").first().copied();
        out.push((id.to_owned(), frame, link.and_then(|l| ix.attr(t, l, "Self")).map(str::to_owned), link.and_then(|l| ix.value(t, l, "LinkResourceURI"))));
    }
    out
}

/// Keeps a `.folio`'s graphic records in step with its parts: a graphic copied by an edit
/// gets a record of its own (a new Folio id, the original's link, preview and embedded file,
/// found through its `folio:ref/…` link) and a `folio.ref` label for its frame; a deleted one
/// loses its record. Only the parts the edits touched can hold such graphics. `data` gets
/// the copies' link locations and previews, so they draw at once.
pub fn sync_graphics(es: &mut EditState, data: &mut folio_idml::ReaderData) {
    let Some(g) = es.graphics.as_mut() else { return };
    let parts = es.editor.parts();
    let part_of = g.part_of.get_or_insert_with(|| {
        let mut m = std::collections::HashMap::new();
        for (name, t) in parts {
            if name.starts_with("Spreads/") || name.starts_with("MasterSpreads/") || name.starts_with("Stories/") {
                for (id, ..) in graphics_in(t) {
                    m.insert(id, name.clone());
                }
            }
        }
        m
    });
    let touched: Vec<&String> = es.editor.touched().iter().filter(|n| n.starts_with("Spreads/") || n.starts_with("MasterSpreads/") || n.starts_with("Stories/")).collect();
    let mut present: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut found = Vec::new();
    for name in &touched {
        if let Some(t) = parts.get(*name) {
            for x in graphics_in(t) {
                present.insert(x.0.clone());
                found.push(x);
            }
        }
    }
    let touched_set: std::collections::HashSet<&str> = touched.iter().map(|s| s.as_str()).collect();
    // records: the opened ones still there (or in parts not touched), then copies
    let mut synced: Vec<folio_project::format::GraphicRefs> = g
        .base
        .iter()
        .filter(|r| match part_of.get(&r.self_id) {
            Some(p) if touched_set.contains(p.as_str()) => present.contains(&r.self_id),
            _ => true,
        })
        .cloned()
        .collect();
    let known: std::collections::HashSet<String> = g.base.iter().map(|r| r.self_id.clone()).collect();
    let mut labels = std::collections::HashMap::new();
    for (id, frame, link_self, uri) in found {
        if known.contains(&id) {
            continue;
        }
        let Some(rid) = uri.as_deref().and_then(|u| u.strip_prefix(folio_project::format::REF_SCHEME)) else { continue };
        let mut orig = match g.base.iter().find(|r| r.link.as_deref() == Some(rid)).cloned() {
            Some(orig)=>orig,
            None if g.placed_sources.contains_key(rid)=>folio_project::format::GraphicRefs {link:Some(rid.into()),preview:Some(rid.into()),..Default::default()},
            None=>continue,
        };
        let newly_placed=g.placed_sources.contains_key(rid);
        if newly_placed {
            // Reusing a file imported with the document must also survive reopen when
            // that old IDML graphic had no preview. Use the source raster itself.
            orig.preview=Some(rid.into());
            orig.embedded=None;
        }
        // the same id as before when this copy was already seen (each edit syncs again)
        let folio_id = g.synced.iter().find(|r| r.self_id == id).map(|r| r.folio_id.clone()).unwrap_or_else(|| format!("g{rid}_{id}"));
        let reference = |k: &Option<String>| k.as_deref().and_then(|k| g.references.iter().find(|r| r.id == k));
        if let Some(f) = &frame {
            let value = folio_project::import::label_value(&folio_id, &[("link", reference(&orig.link)), ("embedded", reference(&orig.embedded)), ("preview", reference(&orig.preview))]);
            labels.insert(f.clone(), vec![(folio_project::format::LABEL_REF.to_string(), value)]);
        }
        if let (Some(new_link), Some(old_link)) = (&link_self, &orig.link_self)
            && let Some(o) = data.links.get(old_link).cloned()
        {
            data.links.insert(new_link.clone(), o);
        }
        if let Some(new_link)=&link_self && let Some(source)=g.placed_sources.get(rid) {
            data.links.insert(new_link.clone(),source.clone());
            if let Some(path)=folio_links::parse_uri(&source.uri).and_then(|u|u.local_path()) {
                data.previews.insert(id.clone(),path.to_string_lossy().into_owned());
            }
        }
        if !newly_placed && let Some(p) = data.previews.get(&orig.self_id).cloned() {
            data.previews.insert(id.clone(), p);
        }
        if !newly_placed && let Some(e) = data.embedded.get(&orig.self_id).cloned() {
            data.embedded.insert(id.clone(), e);
        }
        synced.push(folio_project::format::GraphicRefs { self_id: id, folio_id, frame, link_self, ..orig });
    }
    g.synced = synced;
    g.labels = labels;
}

/// The edit state for a change. After an edit panicked while holding it, the parts may be
/// half changed: editing and writing them stop until the document is opened again.
pub fn lock_for_change(shared: &Shared) -> Result<std::sync::MutexGuard<'_, EditState>, Value> {
    shared.lock().map_err(|_| refusal("editorBroken", "편집 엔진 안에서 오류가 나서 이 문서의 편집을 멈췄습니다. 문서를 다시 여세요 (저장하지 않은 편집은 안전하게 남길 수 없습니다)"))
}

/// The edit state for reading only; a poisoned lock is read anyway (the engine keeps running).
pub fn lock_for_read(shared: &Shared) -> std::sync::MutexGuard<'_, EditState> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

fn refusal(code: &str, error: impl Into<String>) -> Value {
    json!({"ok": false, "code": code, "error": error.into()})
}

/// The preview command a request carries alone (`previewStart`, `previewApply`, `previewCommit`, `previewCancel`), if it is one.
fn preview_op(req: &Value) -> Option<&str> {
    let list = req["commands"].as_array().filter(|l| l.len() == 1)?;
    list[0]["op"].as_str().filter(|op| matches!(*op, "previewStart" | "previewApply" | "previewCommit" | "previewCancel"))
}

/// Takes an open preview session back (the document goes back to what it was, exactly) and builds the session that shows it.
/// None when there was nothing to take back; the session the host holds is then right. The second is what a commit applies
/// for real: the session's name and the commands of its last apply.
type SettledPreview = (Option<Session>, Option<(String, Vec<Command>)>);

fn settle_session(cur: &Session, shared: &Shared) -> Result<SettledPreview, Value> {
    let mut es = lock_for_change(shared)?;
    if es.editor.preview_label().is_none() {
        return Ok((None, None));
    }
    let (shown, parts) = es.editor.preview_close();
    if parts.is_empty() {
        return Ok((None, shown));
    }
    let (next, _) = rebuild(cur, &mut es, shared.clone()).map_err(|e| refusal("rebuild", e))?;
    Ok((Some(next), shown))
}

/// An open preview session taken back before something that keeps the document (save, export): what is written is never a preview.
pub fn settle(cur: &Session) -> Option<Session> {
    let shared = cur.edit.as_ref()?;
    settle_session(cur, shared).ok().and_then(|(next, _)| next)
}

/// Runs an edit, undo or redo on the current session; the new session to put in its place
/// and the reply. An open preview session is taken back first by any request but its own commands
/// (`previewCommit` applies the shown change for real, through the same path as any edit).
pub fn run(req: &Value, cur: &Session) -> Result<(Option<Session>, Value), Value> {
    let Some(shared) = &cur.edit else { return run_inner(req, cur) };
    let op = preview_op(req);
    if matches!(op, Some("previewStart" | "previewApply" | "previewCancel")) {
        return run_inner(req, cur);
    }
    let (base, shown) = settle_session(cur, shared)?;
    let commit = op == Some("previewCommit");
    let request;
    let req = if commit {
        let Some((label, commands)) = shown else {
            return Ok((base, reply(&lock_for_read(shared), &Outcome::default(), &[], 0.0, 0.0, None)));
        };
        let mut r = json!({"cmd": "edit", "label": label, "commands": commands});
        if req["selectionChanged"].as_bool() == Some(true) { r["selectionChanged"] = json!(true); }
        request = r;
        &request
    } else { req };
    let seen = base.as_ref().unwrap_or(cur);
    match run_inner(req, seen) {
        Ok((next, reply)) => Ok((next.or(base), reply)),
        // what failed came after the session was taken back: the host must still show that
        Err(e) if base.is_some() => Ok((base, e)),
        Err(e) => Err(e),
    }
}

fn run_inner(req: &Value, cur: &Session) -> Result<(Option<Session>, Value), Value> {
    let preview_run = preview_op(req).is_some();
    let Some(shared) = &cur.edit else {
        return Err(refusal("notEditable", "이 문서는 편집할 수 없습니다 (읽기 전용으로 열었거나 다른 사용자가 열고 있습니다)"));
    };
    if let Some(f) = &cur.folio
        && f.read_only()
    {
        return Err(refusal("readOnly", "다른 사용자가 이 문서를 열고 있어 읽기 전용입니다"));
    }
    let t = Instant::now();
    let mut es = lock_for_change(shared)?;
    let kind = match req["cmd"].as_str().unwrap_or("") {
        "undo" => folio_edit::Rollback::Undo,
        "redo" => folio_edit::Rollback::Redo,
        _ => folio_edit::Rollback::Apply,
    };
    // the installed fonts placed text took for characters its fonts lack (reply `glyphFonts`, the import report)
    let mut glyph_fonts: Vec<Value> = Vec::new();
    let outcome = match req["cmd"].as_str().unwrap_or("") {
        "edit" => {
            let (values,mut prepared)=crate::place::prepare(req,cur,&es).map_err(|e|refusal("place",e))?;
            // a paste pastes what the engine holds (copied here or in another document)
            if let Some(c) = prepared.clipboard.take() {
                es.editor.set_clipboard(Some(c));
            }
            // links to objects of another document: the numbers of the source (xlink.rs); none for any other edit
            es.editor.set_foreign_source(prepared.foreign.take());
            es.editor.set_foreign_documents(std::mem::take(&mut prepared.foreign_docs));
            let mut commands: Vec<Command> = serde_json::from_value(values).map_err(|e| refusal("invalid", format!("명령을 읽지 못했습니다: {e}")))?;
            // `previewApply` shows the commands it carries provisionally: they are prepared like any edit's
            let mut show = false;
            if let [Command::Registered(RegisteredCommand::PreviewApply { .. })] = commands.as_slice()
                && let Some(Command::Registered(RegisteredCommand::PreviewApply { commands: inner })) = commands.pop()
            {
                commands = inner;
                show = true;
            }
            crate::toc::prepare(&mut commands,cur,req["documentOnlyTOC"] == true).map_err(|e|refusal("toc",e))?;
            crate::listconvert::prepare(&mut commands,cur).map_err(|e|refusal("lists",e))?;
            crate::xrefloc::prepare(&mut commands,cur).map_err(|e|refusal("xref",e))?;
            crate::bookindex::prepare(&mut commands,cur,&es.editor).map_err(|e|refusal("index",e))?;
            crate::outlines::prepare(&mut commands,cur).map_err(|e|refusal("outlines",e))?;
            crate::layoutfit::prepare(&mut commands,cur).map_err(|e|refusal("layoutfit",e))?;
            crate::psdlayers::prepare(&mut commands,cur,&es).map_err(|e|refusal("psdlayers",e))?;
            crate::clippaths::prepare(&mut commands,cur,&es).map_err(|e|refusal("clipping",e))?;
            crate::pdfcomments::prepare(&mut commands,cur,&es,&mut prepared)?;
            crate::datamerge_link::prepare(&mut commands,cur,&es,&mut prepared)?;
            crate::dbmerge::prepare(&mut commands,cur,&es,&mut prepared)?;
            crate::datamerge_expr::prepare(&commands,cur,&es)?;
            if commands.iter().any(|c| matches!(c, Command::Registered(RegisteredCommand::FillPlaceholderText { .. }))) {
                let mut data = es.data.clone();
                sync_graphics(&mut es, &mut data);
                let (parts, path) = (es.editor.parts().clone(), es.path.clone());
                crate::placeholder::prepare(&mut commands, &parts, |ed, story| {
                    let doc = folio_idml::read_parts(ed.parts(), &path, &data).map_err(|e| format!("IDML: {e:?}"))?;
                    Ok(folio_text::layout_document(&doc, &cur.fonts).overset.iter().any(|id| id == story))
                }).map_err(|e| refusal("placeholder", e))?;
            }
            let label = req["label"].as_str().unwrap_or("편집");
            // the window reports a selection that changed since the last edit (Transform Again's sequence)
            if req["selectionChanged"].as_bool() == Some(true) {
                es.editor.selection_changed();
            }
            es.editor.set_anchor_layout(&cur.doc, &cur.layout);
            // the control of a selected object dragged into text anchors it where the composition now has the line (ops/anchored.rs)
            if commands.iter().any(|c| matches!(c, Command::Registered(RegisteredCommand::AnchorObject { .. }))) {
                es.editor.set_anchor_targets(&cur.doc, &cur.layout);
            }
            es.editor.set_frame_starts(folio_text::DocumentFrames::new(&cur.doc).frame_ids()
                .map(|id| ((*id).to_owned(), cur.layout.frames.get(*id).and_then(|f| f.lines.first()).map(|l| l.start))).collect());
            es.editor.set_frame_spans(Editor::spans_of(folio_text::DocumentFrames::new(&cur.doc).frame_ids().copied(), &cur.layout));
            if commands.iter().any(|c| matches!(c, Command::Registered(RegisteredCommand::GenerateCaptions { .. }))) {
                let mut data = es.data.clone();
                sync_graphics(&mut es, &mut data);
                es.editor.set_caption_link_uris(data.links.into_iter().map(|(id, link)| (id, link.uri)).collect());
            }
            es.editor.set_font_records(commands.iter().flat_map(|c| -> Vec<_> { match c {
                Command::TextFormat { props, .. } => (|| cur.fonts.resource_record(props.get("appliedFont")?.as_str()?, props.get("fontStyle")?.as_str()?, &cur.doc.font_table))().into_iter().collect(),
                Command::Registered(RegisteredCommand::GlyphReplace { change_family, change_style, .. }) => cur.fonts.resource_record(change_family, change_style, &cur.doc.font_table).into_iter().collect(),
                Command::Registered(RegisteredCommand::InsertGlyph { family, style, .. }) => cur.fonts.resource_record(family, style, &cur.doc.font_table).into_iter().collect(),
                // a text file that names fonts (RTF): every style of each family
                Command::PlaceText { source, .. } => source.font_families().iter()
                    .flat_map(|family| ["Regular", "Italic", "Bold", "Bold Italic"].map(|style| cur.fonts.resource_record(family, style, &cur.doc.font_table)))
                    .flatten().collect(),
                _ => Vec::new(),
            } }).collect());
            // Change Glyph needs the cmap of the face searched and of the face written (supplied here, as the editor has no font files)
            es.editor.set_glyph_fonts(crate::glyphs::tables(&cur.fonts, &cur.doc.font_table, &commands.iter().filter_map(|c| match c {
                Command::Registered(RegisteredCommand::GlyphReplace { find_family, find_style, change_family, change_style, .. }) =>
                    Some(vec![(find_family.clone(), find_style.clone()), (change_family.clone(), change_style.clone())]),
                Command::Registered(RegisteredCommand::InsertGlyph { family, style, .. }) => Some(vec![(family.clone(), style.clone())]),
                _ => None,
            }).flatten().collect::<Vec<_>>()));
            // (`amend`: what a dialog's first commands made is taken on in their undo step, dialog_state::then_request)
            let outcome = if show { es.editor.preview_apply(label, &commands) } else if req["amend"] == true { es.editor.amend(&commands) } else { es.editor.apply(label, &commands) }
                .map_err(|e| json!({"ok": false, "code": e.code, "error": e.message}))?;
            let fill_glyphs = prepared.fill_glyphs;
            prepared.commit(&mut es);
            // a preview is only shown: what follows an edit (row heights, Auto-Size, variables) amends the last undo step, which
            // is not this change; the commit applies the commands for real and those follow then
            // placed or pasted text: characters its fonts lack take installed fonts that have them (glyphfill.rs)
            let outcome = if preview_run { outcome } else { glyph_fill(&mut es, cur, outcome, &commands, fill_glyphs, &mut glyph_fonts) };
            let outcome = if preview_run { outcome } else { row_heights(&mut es, cur, outcome) };
            let outcome = if preview_run { outcome } else { auto_size(&mut es, cur, outcome) };
            let edited = commands.iter().any(|c| matches!(c, Command::EditTextVariable { .. }));
            if preview_run { outcome } else if edited || commands.iter().any(|c| matches!(c, Command::InsertVariable { .. } | Command::Registered(RegisteredCommand::GenerateCaptions { live: true, .. }))) { variable_results(&mut es, cur, outcome, edited) } else { outcome }
        }
        "undo" => match es.editor.undo() {
            Some(o) => o,
            None => return Err(refusal("nothing", "실행 취소할 것이 없습니다")),
        },
        "redo" => match es.editor.redo() {
            Some(o) => o,
            None => return Err(refusal("nothing", "다시 실행할 것이 없습니다")),
        },
        // History ▸ Delete (build-c): undo step `index` (0 = oldest) out of the record when no later step depends on it (folio_edit history.rs)
        "historyDelete" => {
            let index = wire::required_index(req, "index", "index (the undo step, 0 = oldest) required")
                .map_err(|message| refusal("invalid", message))?;
            es.editor.delete_history_step(index).map_err(|e| json!({"ok": false, "code": e.code, "error": e.message}))?
        }
        // History ▸ Clear History: the undo and redo steps are forgotten, the document stays
        "historyClear" => {
            let settled = es.editor.clear_history();
            folio_edit::Outcome { rev: es.editor.rev(), parts: settled, ..Default::default() }
        }
        other => return Err(refusal("invalid", format!("unknown edit command {other}"))),
    };
    let edit_ms = t.elapsed().as_secs_f64() * 1000.0;
    if outcome.parts.is_empty() {
        return Ok((None, reply(&es, &outcome, &[], edit_ms, 0.0, None)));
    }
    let t2 = Instant::now();
    // A change the document cannot be read back from is taken back, so the editor and the
    // shown document never disagree.
    let (next, info) = match rebuild(cur, &mut es, shared.clone()) {
        Ok(next) => next,
        Err(e) => {
            if preview_run {
                let _ = es.editor.preview_cancel();
            } else if !matches!(req["cmd"].as_str(), Some("historyDelete" | "historyClear")) {
                es.editor.rollback(outcome, kind);
            }
            return Err(refusal("rebuild", e));
        }
    };
    let pages = changed_pages(&cur.doc, &next.doc, &outcome);
    let rebuild_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let mut r = reply(&es, &outcome, &pages, edit_ms, rebuild_ms, Some(&info));
    if !glyph_fonts.is_empty() {
        r["glyphFonts"] = json!(glyph_fonts);
    }
    r["overset"] = json!(next.layout.overset.len());
    r["document"] = next.summary.clone();
    Ok((Some(next), r))
}

/// Edits made to follow the composition, in the edit's undo step (`amend`): the outcome
/// grown by theirs.
fn amend(es: &mut EditState, outcome: Outcome, commands: &[Command]) -> Outcome {
    match es.editor.amend(commands) {
        Ok(more) => {
            let union = |a: &[String], b: &[String]| a.iter().chain(b).cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
            Outcome { rev: more.rev, label: outcome.label.clone(), parts: union(&outcome.parts, &more.parts), recompose: union(&outcome.recompose, &more.recompose), created: outcome.created }
        }
        Err(_) => outcome,
    }
}

/// Placed text (Place of a text file with `fillMissingGlyphs` on) and pasted text: the characters the font set on them lacks
/// take installed fonts that have them (glyphfill.rs), in the edit's undo step. `report` gets the fonts taken.
fn glyph_fill(es: &mut EditState, cur: &Session, outcome: Outcome, commands: &[Command], placed: bool, report: &mut Vec<Value>) -> Outcome {
    let mut targets = crate::glyphfill::Targets::new();
    if placed && commands.iter().any(|c| matches!(c, Command::PlaceText { .. })) {
        // the placed story is the one this edit made or filled
        for s in changed_stories(&outcome) {
            targets.insert(s, None);
        }
    }
    let pasted: Vec<(String, usize)> = commands.iter().filter_map(|c| match c {
        Command::PasteText { story, at, .. } => Some((story.clone(), *at)),
        _ => None,
    }).collect();
    if targets.is_empty() && pasted.is_empty() {
        return outcome;
    }
    let mut data = es.data.clone();
    sync_graphics(es, &mut data);
    let Ok(mut doc) = folio_idml::read_parts(es.editor.parts(), &es.path, &data) else { return outcome };
    for id in es.editor.recompose().iter().chain(targets.keys()).chain(pasted.iter().map(|(s, _)| s)) {
        if let Some(s) = doc.stories.get_mut(id) {
            s.saved = folio_model::SavedComposition::None;
        }
    }
    // a paste's new characters: as many as the story grew by, from where they went in
    for (story, at) in pasted {
        let (Some(before), Some(after)) = (cur.doc.stories.get(&story), doc.stories.get(&story)) else { continue };
        let grown = folio_idml::story_chars(after).saturating_sub(folio_idml::story_chars(before));
        if grown > 0 && !targets.contains_key(&story) {
            targets.insert(story, Some((at, at + grown)));
        }
    }
    let fill = crate::glyphfill::fill(&doc, &cur.fonts, &targets);
    if fill.commands.is_empty() {
        return outcome;
    }
    es.editor.set_font_records(fill.records);
    let before = outcome.rev;
    let outcome = amend(es, outcome, &fill.commands);
    if outcome.rev != before {
        *report = fill.report;
    }
    outcome
}

/// The stories an edit changed (to compose again).
fn changed_stories(outcome: &Outcome) -> BTreeSet<String> {
    outcome.recompose.iter().cloned()
        .chain(outcome.parts.iter().filter_map(|p| p.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned)))
        .collect()
}

/// Tables: InDesign stores an auto-grow row's composed height (`SingleRowHeight`); the rows of
/// the tables in the stories this edit changed take theirs, in the same undo step.
fn row_heights(es: &mut EditState, cur: &Session, outcome: Outcome) -> Outcome {
    let stories: Vec<String> = changed_stories(&outcome).into_iter()
        .filter(|s| es.editor.part(&format!("Stories/Story_{s}.xml")).is_some_and(|t| t.contains("<Table ")))
        .collect();
    if stories.is_empty() {
        return outcome;
    }
    let mut data = es.data.clone();
    sync_graphics(es, &mut data);
    let Ok(mut doc) = folio_idml::read_parts(es.editor.parts(), &es.path, &data) else { return outcome };
    for id in es.editor.recompose().iter().chain(&stories) {
        if let Some(s) = doc.stories.get_mut(id) {
            s.saved = folio_model::SavedComposition::None;
        }
    }
    let frames = folio_text::DocumentFrames::new(&doc);
    let commands: Vec<Command> = stories.iter()
        .filter_map(|id| folio_text::layout_story(&doc, &frames, id, &cur.fonts))
        .flat_map(|l| l.table_rows)
        .map(|(table, heights)| Command::SetRowHeights { table, heights })
        .collect();
    if commands.is_empty() {
        return outcome;
    }
    amend(es, outcome, &commands)
}

/// Text Frame Options ▸ Auto-Size: the auto-sized frames that end the stories this edit
/// composes again take the size of their text (folio-text autosize.rs), in the same undo step.
fn auto_size(es: &mut EditState, cur: &Session, outcome: Outcome) -> Outcome {
    // an edit that changed no story has nothing to compose again
    let stories = changed_stories(&outcome);
    if stories.is_empty() {
        return outcome;
    }
    // nothing to do unless some frame or object style has Auto-Size on (looked for once per version of each part)
    let on = |t: &str| ["AutoSizingType=\"HeightOnly", "AutoSizingType=\"WidthOnly", "AutoSizingType=\"HeightAndWidth"].iter().any(|k| t.contains(k));
    let EditState { editor, autosize, .. } = &mut *es;
    let revs = editor.part_revs();
    let any_on = editor.parts().iter().any(|(n, t)| {
        if !(n.starts_with("Spreads/") || n.starts_with("MasterSpreads/") || n == "Resources/Styles.xml") {
            return false;
        }
        let rev = revs.get(n).copied().unwrap_or(0);
        match autosize.get(n) {
            Some((r, v)) if *r == rev && !full_rebuild_forced() => *v,
            _ => {
                let v = on(t);
                autosize.insert(n.clone(), (rev, v));
                v
            }
        }
    });
    if !any_on {
        return outcome;
    }
    let mut data = es.data.clone();
    sync_graphics(es, &mut data);
    let Ok(mut doc) = folio_idml::read_parts(es.editor.parts(), &es.path, &data) else { return outcome };
    for id in es.editor.recompose().iter().chain(&stories) {
        if let Some(s) = doc.stories.get_mut(id) {
            s.saved = folio_model::SavedComposition::None;
        }
    }
    let ids: Vec<&str> = stories.iter().map(String::as_str).collect();
    let fits = folio_text::fit_frames(&doc, &cur.fonts, &ids);
    if fits.is_empty() {
        return outcome;
    }
    let commands: Vec<Command> = fits.into_iter().map(|f| Command::FitFrame { id: f.frame, size: [f.width, f.height], point: [f.point.0, f.point.1] }).collect();
    amend(es, outcome, &commands)
}

/// Text variables: an instance on a document page carries the value it shows (`ResultText`, as InDesign stores it). Run for an edit
/// that inserted an instance or edited a variable (the document is laid out for it: typing in a story that holds one does not pay
/// that); the instances whose value is out of date take the current one, in the same undo step. Instances nothing touched keep what
/// they had until the next insertion: InDesign works them out again when it opens the file.
fn variable_results(es: &mut EditState, cur: &Session, outcome: Outcome, edited: bool) -> Outcome {
    let stories = changed_stories(&outcome);
    if !edited && !stories.iter().any(|s| es.editor.part(&format!("Stories/Story_{s}.xml")).is_some_and(|t| t.contains("<TextVariableInstance "))) {
        return outcome;
    }
    let mut data = es.data.clone();
    sync_graphics(es, &mut data);
    let Ok(mut doc) = folio_idml::read_parts(es.editor.parts(), &es.path, &data) else { return outcome };
    crate::textvars::attach(&mut doc, &cur.source_path, cur.folio.as_deref());
    let layout = folio_text::layout_document(&doc, &cur.fonts);
    let results = folio_text::variable_results(&doc, &cur.fonts, &layout);
    if results.is_empty() {
        return outcome;
    }
    amend(es, outcome, &[Command::SetVariableResults { results }])
}

/// A digest of the model and the composition a session shows (the document's warnings left out: the host keeps its own). Two engines
/// given the same edits, one that reads every part again on each edit and one that updates what changed, must agree on it: a diagnostic
/// for tests (`{"cmd":"debugDigest"}`).
pub fn digest(s: &Session) -> Value {
    use std::fmt::Write as _;
    use std::hash::Hasher as _;
    struct H(std::collections::hash_map::DefaultHasher, usize);
    impl std::fmt::Write for H {
        fn write_str(&mut self, t: &str) -> std::fmt::Result {
            self.0.write(t.as_bytes());
            self.1 += t.len();
            Ok(())
        }
    }
    let mut doc = H(Default::default(), 0);
    let mut d = s.doc.clone();
    d.warnings.clear();
    let _ = write!(doc, "{d:?}");
    // through serde_json: its maps are ordered, a HashMap's Debug is not; the time the layout took is not part of it
    let mut l = s.layout.clone();
    l.stats.elapsed_ms = 0;
    let mut layout = H(Default::default(), 0);
    let _ = write!(layout, "{}", serde_json::to_value(&l).unwrap_or(Value::Null));
    json!({"ok": true, "type": "debugDigest", "doc": format!("{:016x}", doc.0.finish()), "docLen": doc.1, "layout": format!("{:016x}", layout.0.finish()), "layoutLen": layout.1,
           "pageSettings": s.summary["pageSettings"], "pages": s.summary["pages"], "stories": s.summary["stories"], "overset": s.summary["overset"]})
}

/// Window ▸ History: `{"undo": [oldest … last], "redo": [next …]}` step labels.
fn history(es: &EditState) -> Value {
    let (undo, redo) = es.editor.history();
    // which undo steps can be deleted on their own (no later step changed what they changed: History ▸ Delete, build-c)
    let deletable: Vec<bool> = (0..undo.len()).map(|i| es.editor.history_step_deletable(i).is_ok()).collect();
    json!({"undo": undo, "redo": redo, "deletable": deletable})
}

fn reply(es: &EditState, o: &Outcome, pages: &[usize], edit_ms: f64, rebuild_ms: f64, info: Option<&RebuildInfo>) -> Value {
    let round1 = |v: f64| (v * 10.0).round() / 10.0;
    let mut ms = json!({"edit": (edit_ms * 100.0).round() / 100.0, "rebuild": round1(rebuild_ms)});
    // how the document was brought up to date: `incremental` false says why (read everything again)
    if let Some(i) = info {
        ms["incremental"] = json!(i.incremental);
        if let Some(why) = &i.why {
            ms["why"] = json!(why);
        }
        ms["read"] = json!(round1(i.read_ms));
        ms["layout"] = json!(round1(i.layout_ms));
        ms["after"] = json!(round1(i.after_ms));
    }
    json!({
        "ok": true, "type": "edited", "rev": o.rev, "label": o.label, "parts": o.parts, "pages": pages, "created": o.created,
        "undo": es.editor.undo_label(), "redo": es.editor.redo_label(), "dirty": es.editor.version() != es.saved_rev, "history": history(es), "preview": es.editor.preview_label(),
        "ms": ms,
    })
}

/// The model of the edited parts. The parts that changed since the last read (their revisions moved) and are only spreads, parent
/// spreads or stories are read again and put in the place of the old ones (`folio_idml::ReadBase`); anything else, or anything
/// that cannot be told to give the same document as reading everything, reads everything again and starts a new base. The
/// document comes out as `read_parts` gives it (its warnings are the caller's).
fn read_document(es: &mut EditState, data: &mut folio_idml::ReaderData) -> Result<(folio_model::Document, RebuildInfo), String> {
    let mut info = RebuildInfo::default();
    if full_rebuild_forced() {
        es.inc = None;
        data.saved_lines = es.data.saved_lines.clone();
        info.why = Some("FOLIO_FULL_REBUILD".into());
        let doc = folio_idml::read_parts(es.editor.parts(), &es.path, data).map_err(|e| format!("IDML: {e:?}"))?;
        return Ok((doc, info));
    }
    let EditState { editor, inc, path, data: es_data, .. } = es;
    let revs = editor.part_revs();
    match inc.as_mut() {
        Some(b) => {
            let changed: BTreeSet<String> = revs.keys().chain(b.stamp.keys())
                .filter(|k| revs.get(*k) != b.stamp.get(*k)).cloned().collect();
            let fragments = changed.iter().filter_map(|name| {
                editor.item_fragments(name, b.stamp.get(name).copied().unwrap_or(0)).map(|items| (name.clone(), items))
            }).collect::<BTreeMap<_, _>>();
            match b.base.update_with_items(editor.parts(), path, data, &changed, &fragments) {
                Ok(()) => {
                    b.stamp = revs.clone();
                    info.incremental = true;
                    info.page_settings_unchanged = changed.iter().all(|n| n.starts_with("Stories/") || fragments.contains_key(n));
                    return Ok((b.base.doc().clone(), info));
                }
                Err(why) => info.why = Some(why),
            }
        }
        None => info.why = Some("no earlier read of this document".into()),
    }
    data.saved_lines = es_data.saved_lines.clone();
    let base = folio_idml::ReadBase::read(editor.parts(), path, data).map_err(|e| format!("IDML: {e:?}"))?;
    let doc = base.doc().clone();
    *inc = Some(IncBase { base, stamp: revs.clone() });
    Ok((doc, info))
}

/// The data a read needs but the saved-lines sidecar (a deep copy of it per edit costs more than the edit): the copies of placed
/// graphics add to its links and previews (`sync_graphics`).
fn light_data(d: &folio_idml::ReaderData) -> folio_idml::ReaderData {
    folio_idml::ReaderData { parser: d.parser.clone(), previews: d.previews.clone(), saved_lines: None, links: d.links.clone(), embedded: d.embedded.clone(), warnings: d.warnings.clone() }
}

/// A session from the edited parts: the model read again (or updated), stories laid out again (those
/// whose frames changed size without InDesign's saved lines), everything else shared.
fn rebuild(cur: &Session, es: &mut EditState, shared: Shared) -> Result<(Session, RebuildInfo), String> {
    let t0 = Instant::now();
    // copies of placed graphics in a .folio: their records, link locations and previews
    let mut data = light_data(&es.data);
    sync_graphics(es, &mut data);
    let (mut doc, mut info) = read_document(es, &mut data)?;
    info.read_ms = t0.elapsed().as_secs_f64() * 1000.0;
    if folio_text::layout::listconvert::changed(&cur.doc, &doc) {
        for story in doc.stories.values_mut() { story.saved = folio_model::SavedComposition::None; }
    }
    for id in es.editor.recompose() {
        if let Some(s) = doc.stories.get_mut(id) {
            s.saved = folio_model::SavedComposition::None;
        }
    }
    doc.warnings = cur.doc.warnings.clone();
    crate::pdfplace::attach(&mut doc, cur.folio.as_deref(), cur.source_dir.as_deref());
    crate::psdlayers::attach(&mut doc, cur.folio.as_deref(), cur.source_dir.as_deref());
    crate::textvars::attach(&mut doc, &cur.source_path, cur.folio.as_deref());
    let t_layout = Instant::now();
    let mut layout = folio_text::layout_document(&doc, &cur.fonts);
    layout.page_frames = folio_text::layout_master_page_numbers(&doc, &cur.fonts, &layout)
        .into_iter()
        .map(|p| folio_model::layout::PageFrameText { page: p.page, frame: p.frame })
        .collect();
    info.layout_ms = t_layout.elapsed().as_secs_f64() * 1000.0;
    let mut summary = cur.summary.clone();
    summary["pages"] = crate::pages::summary(&doc);
    if !info.page_settings_unchanged {
        summary["pageSettings"] = if full_rebuild_forced() { crate::pages::settings(es.editor.parts()) } else { crate::pages::settings_with(es.editor.parts(), es.editor.part_revs(), &mut es.settings_cache) };
    }
    summary["stories"] = json!(doc.stories.len());
    summary["layout"] = json!({
        "lines": layout.stats.lines, "glyphs": layout.stats.glyphs,
        "savedLines": {"stories": layout.stats.saved_stories, "fallbacks": layout.stats.saved_fallbacks,
            "fallbackStories": layout.saved_fallbacks.iter().take(50)
                .map(|(id, why)| json!({"story": id, "reason": why})).collect::<Vec<_>>()}
    });
    summary["overset"] = json!(layout.overset.len());
    summary["missingFonts"] = json!(layout.missing_fonts);
    let roots = cur.print.lock().expect("shared state must remain unpoisoned after each operation").as_ref().map(|p| p.roots.clone());
    let print = roots.map(|r| crate::print::build_links(&doc, cur.source_dir.as_deref(), &r));
    let mut next=Session {
        view: cur.view.clone(),
        dialog: cur.dialog.clone(),
        doc,
        layout,
        colors: cur.colors.clone(),
        fonts: cur.fonts.clone(),
        glyph_cache: cur.glyph_cache.clone(),
        images: cur.images.clone(),
        summary,
        source_path: cur.source_path.clone(),
        source_dir: cur.source_dir.clone(),
        source_name: cur.source_name.clone(),
        cmyk_profile: cur.cmyk_profile.clone(),
        rgb_profile: cur.rgb_profile.clone(),
        print: Mutex::new(print),
        lazy: crate::print::Lazy::default(),
        inks: std::sync::OnceLock::new(),
        ink_images: folio_render::sep::InkImages::new(),
        planes: crate::sep::PlaneCache::default(),
        folio: cur.folio.clone(),
        inspect: std::sync::OnceLock::new(),
        edit: Some(shared),
    };
    crate::color_setup::refresh(cur, &mut next);
    crate::place::configure_previews(&next);
    info.after_ms = (t0.elapsed().as_secs_f64() * 1000.0 - info.read_ms - info.layout_ms).max(0.0);
    Ok((next, info))
}

/// Pages to draw again: those of spreads whose parts changed, of frames whose stories
/// changed or were laid out again, and every page when the designmap or resources changed
/// (layers, swatches, styles).
fn changed_pages(old: &folio_model::Document, new: &folio_model::Document, o: &Outcome) -> Vec<usize> {
    // the document's pages and the master spreads' (a master edit redraws the master view as well as the pages that use it)
    let all = |d: &folio_model::Document| d.spreads.iter().chain(&d.master_spreads).flat_map(|s| s.pages.iter().map(|p| p.index)).collect::<BTreeSet<_>>();
    if folio_text::layout::listconvert::changed(old, new)
        || o.parts.iter().any(|p| p == "designmap.xml" || p.starts_with("Resources/") || p.starts_with("MasterSpreads/")) {
        return all(old).union(&all(new)).copied().collect();
    }
    let spread_ids: BTreeSet<&str> = o.parts.iter().filter_map(|p| p.strip_prefix("Spreads/Spread_").and_then(|s| s.strip_suffix(".xml"))).collect();
    let story_ids: BTreeSet<&str> = o
        .parts
        .iter()
        .filter_map(|p| p.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")))
        .chain(o.recompose.iter().map(String::as_str))
        .collect();
    let mut out = BTreeSet::new();
    for d in [old, new] {
        for s in d.spreads.iter().chain(&d.master_spreads) {
            let spread_hit = spread_ids.contains(s.id.as_str());
            let story_hit = !story_ids.is_empty() && spread_shows(s, &story_ids);
            if spread_hit || story_hit {
                out.extend(s.pages.iter().map(|p| p.index));
            }
        }
    }
    out.into_iter().collect()
}

fn spread_shows(s: &folio_model::Spread, stories: &BTreeSet<&str>) -> bool {
    fn walk(items: &[folio_model::PageItem], stories: &BTreeSet<&str>) -> bool {
        items.iter().any(|it| it.text_frame().is_some_and(|f| stories.contains(f.story.as_str())) || match &it.kind {
            folio_model::ItemKind::Group { children } => walk(children, stories),
            _ => false,
        })
    }
    walk(&s.items, stories)
}

/// For saving: each touched part's text, or None when it was removed.
pub fn part_changes(es: &EditState) -> BTreeMap<String, Option<String>> {
    es.editor.touched().iter().map(|p| (p.clone(), es.editor.part(p).map(str::to_owned))).collect()
}

/// The saved-lines sidecar without the stories that are composed afresh now.
pub fn filter_saved_lines(bytes: &[u8], drop: &BTreeSet<String>) -> Vec<u8> {
    if drop.is_empty() {
        return bytes.to_vec();
    }
    let Ok(mut v) = serde_json::from_slice::<Value>(bytes) else { return bytes.to_vec() };
    if let Some(stories) = v.get_mut("stories").and_then(Value::as_array_mut) {
        stories.retain(|s| s.get("story").and_then(Value::as_str).is_none_or(|id| !drop.contains(id)));
    }
    serde_json::to_vec(&v).unwrap_or_else(|_| bytes.to_vec())
}

/// Marks `rev` (the version whose parts were written) as saved.
pub fn mark_saved(shared: &Shared, rev: Option<u64>) {
    let mut es = shared.lock().unwrap_or_else(|p| p.into_inner());
    es.saved_rev = rev.unwrap_or_else(|| es.editor.version());
}

pub fn state(s: &Session) -> Value {
    match &s.edit {
        None => json!({"editable": false}),
        Some(shared) => {
            let broken = shared.is_poisoned();
            let es = lock_for_read(shared);
            let read_only = s.folio.as_ref().is_some_and(|f| f.read_only());
            json!({"editable": !read_only && !broken, "broken": broken, "rev": es.editor.rev(), "undo": es.editor.undo_label(), "redo": es.editor.redo_label(),
                   "dirty": es.editor.version() != es.saved_rev, "saveable": s.folio.is_some(), "history": history(&es)})
        }
    }
}

/// The drawing layer is stored in designmap, independently of the selected object.
pub fn drawing_layer(s: &Session) -> Option<String> {
    let es = s.edit.as_ref()?.lock().ok()?;
    let dm = es.editor.parts().get("designmap.xml")?;
    let ix = folio_edit::xml::Index::scan(dm).ok()?;
    let doc = (0..ix.nodes.len()).find(|&i| ix.local(dm, i) == "Document")?;
    ix.value(dm, doc, "ActiveLayer")
}

/// Exact group/selection W/H; a group itself has no stroke, its members do.
pub fn selection_size(s: &Session, req: &Value) -> Value {
    let Some(shared) = &s.edit else { return refusal("notEditable", "no edit session") };
    let ids: Vec<_> = req["ids"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut es = lock_for_read(shared);
    let mut bounds: Option<[f64; 4]> = None;
    for id in &ids {
        // Dimensions Include Stroke Weight off (`includeStroke: false`): the path's size
        let found = if req["includeStroke"] == false { es.editor.path_bounds(id, ids.len() == 1) } else { es.editor.outline_bounds(id, ids.len() == 1) };
        let Some(b) = found else { return refusal("notFound", *id) };
        bounds = Some(bounds.map_or(b, |a| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]));
    }
    match bounds {
        Some(b) => json!({"ok":true, "size":[b[2]-b[0], b[3]-b[1]], "rev":es.editor.rev()}),
        None => refusal("invalid", "empty selection"),
    }
}

/// The Effects dialog's view of object `item`: each level's opacity, blend mode and every effect
/// with all its settings (folio-edit effects.rs), and the document's global light (no `item`:
/// the light only).
/// Edit ▸ Copy (and Cut's first half): the objects as they are now, with the composition their
/// threaded frames show, kept for Paste.
pub fn copy(s: &Session, req: &Value) -> Value {
    let Some(shared) = &s.edit else { return refusal("notEditable", "no edit session") };
    let mut es = lock_for_read(shared);
    let ids: Vec<String> = req["ids"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default();
    es.editor.set_frame_starts(folio_text::DocumentFrames::new(&s.doc).frame_ids()
        .map(|id| ((*id).to_owned(), s.layout.frames.get(*id).and_then(|f| f.lines.first()).map(|l| l.start))).collect());
    match es.editor.copy(&ids) {
        Ok(()) => {
            crate::clip::hold(s, &es);
            json!({"ok": true, "type": "copied", "count": ids.len()})
        }
        Err(e) => refusal(e.code, e.message),
    }
}

pub fn effects(s: &Session, req: &Value) -> Value {
    let Some(shared) = &s.edit else { return refusal("notEditable", "no edit session") };
    let mut es = lock_for_read(shared);
    match es.editor.effects(req["item"].as_str()) {
        Ok(v) => json!({"ok": true, "type": "effects", "effects": v, "rev": es.editor.rev()}),
        Err(e) => refusal(e.code, e.message),
    }
}

/// Objects carrying a script label `key` (InDesign's `insertLabel`): value → `Self`; with `ids`, the label `key` (default `Label`,
/// what the Script Label panel shows) of each of those objects: `Self` → text (objects without one are left out).
pub fn labels(s: &Session, key: &str, ids: Option<&[String]>) -> Value {
    let Some(shared) = &s.edit else { return refusal("notEditable", "편집하는 문서가 아닙니다") };
    let es = lock_for_read(shared);
    if let Some(ids) = ids {
        let key = if key.is_empty() { "Label" } else { key };
        let mut out = serde_json::Map::new();
        for (name, t) in es.editor.parts() {
            if !(name.starts_with("Spreads/") || name.starts_with("MasterSpreads/") || name.starts_with("Stories/")) {
                continue;
            }
            let Ok(ix) = folio_edit::xml::Index::scan(t) else { continue };
            for i in 0..ix.nodes.len() {
                if ix.local(t, i) == "KeyValuePair" && ix.value(t, i, "Key").as_deref() == Some(key)
                    && let Some(owner) = ix.ancestors(i).find(|&a| ix.attr(t, a, "Self").is_some())
                    && let (Some(v), Some(id)) = (ix.value(t, i, "Value"), ix.attr(t, owner, "Self"))
                    && ids.iter().any(|x| x == id)
                {
                    out.insert(id.to_owned(), json!(v));
                }
            }
        }
        return json!({"ok": true, "type": "itemLabels", "labels": out});
    }
    let mut out = serde_json::Map::new();
    for (name, t) in es.editor.parts() {
        if !(name.starts_with("Spreads/") || name.starts_with("MasterSpreads/") || name.starts_with("Stories/")) {
            continue;
        }
        let Ok(ix) = folio_edit::xml::Index::scan(t) else { continue };
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "KeyValuePair" && ix.value(t, i, "Key").as_deref() == Some(key)
                && let Some(owner) = ix.ancestors(i).find(|&a| ix.attr(t, a, "Self").is_some())
                && let (Some(v), Some(id)) = (ix.value(t, i, "Value"), ix.attr(t, owner, "Self"))
            {
                out.insert(v, json!(id));
            }
        }
    }
    json!({"ok": true, "type": "labels", "labels": out})
}
