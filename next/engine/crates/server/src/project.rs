//! Folio projects and `.folio` documents in the engine:
//!   {"cmd":"projectCreate","dir":"…","name":"…"}         make a folder a project
//!   {"cmd":"projectInfo","path":"…"}                       the project holding a path: tree, documents, conflicts
//!   {"cmd":"importPlan","source":"…indd","project":"…","document":"x.folio","roots":[…]}
//!                                                          what an import would do (runs the INDD reader)
//!   {"cmd":"importRun", same fields}                       does it (progress {job:"import"}); reply {result}
//!   {"cmd":"importCancel"}
//!   {"cmd":"save"}                                         rewrites the open .folio atomically
//!   {"cmd":"saveCopy","path":"…folio"}                     File ▸ Save a Copy: the document as it is
//!                                                          now, to another .folio in the project
//!   {"cmd":"exportIdml","path":"…idml"}                    IDML for InDesign, with the export report
//!   {"cmd":"lockOverride"}                                 takes over another user's lock (read-only → editable)
//!   {"cmd":"projectStatus"}                                references of the open .folio on this machine
//!
//! `folio_project` owns the rules and formats; this module runs the reader, builds the model
//! and hands both over. Imports run on their own thread; renders keep flowing.

use crate::{Fonts, Out, Session, send};
use folio_project::format;
use folio_project::import::{self, Plan};
use folio_project::lock::{self, Acquired, LockGuard, LockInfo};
use folio_project::open::Opened;
use folio_project::{Project, RelPath};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// The `.folio` side of an open document.
pub struct FolioSession {
    pub opened: Opened,
    pub lock: Mutex<Option<LockGuard>>,
    /// Who holds the lock when this session is read-only.
    pub holder: Mutex<Option<(LockInfo, Option<i64>, bool)>>,
    pub replaced: Option<LockInfo>,
}

impl FolioSession {
    pub fn read_only(&self) -> bool {
        self.lock.lock().expect("shared state must remain unpoisoned after each operation").is_none()
    }

    pub fn lock_json(&self) -> Value {
        let read_only = self.read_only();
        match &*self.holder.lock().expect("shared state must remain unpoisoned after each operation") {
            Some((h, age, same)) if read_only => json!({"readOnly": true, "user": h.user, "machine": h.machine, "time": h.time, "ageSecs": age, "sameMachine": same, "pid": h.pid}),
            _ if read_only => json!({"readOnly": true, "user": "", "machine": "", "time": "", "ageSecs": null, "sameMachine": false, "pid": 0}),
            _ => json!({"readOnly": false, "replacedStale": self.replaced.as_ref().map(|r| json!({"user": r.user, "machine": r.machine, "time": r.time}))}),
        }
    }

    pub fn summary(&self) -> Value {
        let o = &self.opened;
        json!({
            "project": {"root": o.project.root.display().to_string(), "id": o.project.file.id, "name": o.project.file.name},
            "document": {"id": o.manifest.id, "path": o.rel, "file": o.file.display().to_string(),
                         "source": o.manifest.source, "created": o.manifest.created, "saved": o.manifest.saved},
            "references": o.summary,
            "pinChanges": o.pin_changes,
            "warnings": o.warnings,
            "lock": self.lock_json(),
            "readOnly": self.read_only(),
        })
    }
}

/// Resolves the project references and locks an open `.folio`.
pub fn open_folio(path: &Path) -> Result<FolioSession, String> {
    let opened = folio_project::open::resolve(path).map_err(|e| e.to_string())?;
    let (guard, holder, replaced) = match lock::acquire(&opened.project.root, &opened.manifest.id, opened.rel.as_str()) {
        Ok(Acquired::Held { guard, replaced }) => (Some(guard), None, replaced),
        Ok(Acquired::HeldBy { info, age_secs, same_machine }) => (None, Some((info, age_secs, same_machine)), None),
        Err(e) => {
            // a read-only share: open read-only, saying why
            let info = LockInfo { user: String::new(), machine: String::new(), pid: 0, process_start: 0, time: String::new(), document: opened.rel.to_string(), app: format!("잠금 파일을 만들 수 없음: {e}"), session: String::new() };
            (None, Some((info, None, false)), None)
        }
    };
    Ok(FolioSession { opened, lock: Mutex::new(guard), holder: Mutex::new(holder), replaced })
}

/// Font folders of a project (`fonts/` by default).
pub fn project_font_dirs(s: &FolioSession) -> Vec<PathBuf> {
    let p = &s.opened.project;
    let dir = p.root.join(&p.file.settings.fonts_folder);
    let dir = folio_links::uri::resolve_native(&dir).unwrap_or(dir);
    if dir.is_dir() { vec![dir] } else { Vec::new() }
}

// ---------------------------------------------------------------------------------------
// projects

fn project_json(p: &Project) -> Value {
    json!({"root": p.root.display().to_string(), "id": p.file.id, "name": p.file.name, "created": p.file.created,
           "settings": p.file.settings, "locations": p.file.locations})
}

pub fn create(req: &Value) -> Value {
    let dir = PathBuf::from(req["dir"].as_str().unwrap_or(""));
    if dir.as_os_str().is_empty() {
        return json!({"ok": false, "error": "프로젝트 폴더가 없습니다"});
    }
    match folio_project::manifest::create(&dir, req["name"].as_str().unwrap_or("")) {
        Ok(p) => json!({"ok": true, "type": "project", "project": project_json(&p)}),
        Err(e) => json!({"ok": false, "error": e.to_string(), "code": match e { folio_project::manifest::Error::AlreadyProject(_) => "exists", _ => "io" }}),
    }
}

pub fn info(req: &Value) -> Value {
    let path = PathBuf::from(req["path"].as_str().unwrap_or(""));
    match folio_project::manifest::open(&path) {
        Ok(p) => json!({"ok": true, "type": "project", "project": project_json(&p), "tree": folio_project::tree::list(&p)}),
        Err(e) => json!({"ok": false, "error": e.to_string(), "code": "notProject"}),
    }
}

// ---------------------------------------------------------------------------------------
// import

struct Prepared {
    key: String,
    project: Project,
    source: PathBuf,
    kind: &'static str,
    sha256: String,
    bytes: u64,
    staging: Option<PathBuf>,
    idml: PathBuf,
    reader_dir: Option<PathBuf>,
    result: Option<Value>,
    doc: folio_model::Document,
    links: folio_links::LinkReport,
    search: folio_links::LinkSearch,
    /// Set for an attached MCP client's import (`_projectOnly`, attach_paths.rs): files outside this project root (canonical)
    /// are not read or copied in; such links stay missing with their address as a hint.
    inside: Option<PathBuf>,
    plan: Plan,
    timings: serde_json::Map<String, Value>,
}

/// A link file below `inside` (an attached client's import), or any (the app's own import).
fn admitted(inside: Option<&Path>, path: &Path) -> bool {
    inside.is_none_or(|root| path.canonicalize().is_ok_and(|p| p.starts_with(root)))
}

/// `resolve` limited to `inside` (see `Prepared::inside`).
fn resolve_inside(u: &str, search: &folio_links::LinkSearch, inside: Option<&Path>) -> folio_links::LinkStatus {
    let status = folio_links::resolve(u, search, &folio_links::FileIndex::empty());
    match &status {
        folio_links::LinkStatus::Found { path, .. } if !admitted(inside, path) => folio_links::LinkStatus::Missing,
        folio_links::LinkStatus::Ambiguous(paths) if paths.iter().any(|p| !admitted(inside, p)) => folio_links::LinkStatus::Missing,
        _ => status,
    }
}

impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(s) = &self.staging {
            let _ = std::fs::remove_dir_all(s);
        }
    }
}

fn job() -> &'static Mutex<Option<Prepared>> {
    static J: OnceLock<Mutex<Option<Prepared>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(None))
}

fn cancel_flag() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    static C: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

pub fn cancel() -> Value {
    let f = cancel_flag().lock().expect("shared state must remain unpoisoned after each operation").clone();
    if let Some(f) = &f {
        f.store(true, Ordering::Relaxed);
    }
    json!({"ok": true, "type": "importCancel", "cancelling": f.is_some()})
}

fn roots(req: &Value) -> Vec<PathBuf> {
    req["roots"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(PathBuf::from).collect()).unwrap_or_default()
}

fn target_document(project: &Project, source: &Path, req: &Value) -> Result<RelPath, String> {
    if let Some(d) = req["document"].as_str().filter(|d| !d.trim().is_empty()) {
        // a root-relative path, or an absolute one inside the project
        let rel = RelPath::parse(&d.replace('\\', "/")).or_else(|_| folio_project::paths::relative_to_root(&project.root, Path::new(d))).map_err(|e| e.to_string())?;
        if !rel.file_name().to_lowercase().ends_with(".folio") {
            return Err("문서 이름은 .folio로 끝나야 합니다".into());
        }
        return Ok(rel);
    }
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
    RelPath::parse(&format!("{}.folio", folio_project::paths::fix_name(&stem))).map_err(|e| e.to_string())
}

fn key_of(req: &Value) -> String {
    json!([req["source"], req["project"], req["document"], req["roots"], req["_projectOnly"]]).to_string()
}

/// Reads the source (running the INDD reader into the project's staging folder), resolves
/// its links and plans the import.
fn prepare(req: &Value, fonts: &folio_text::FontStore, cancel: &AtomicBool, progress: &dyn Fn(&str, Value)) -> Result<Prepared, (String, &'static str)> {
    let t0 = Instant::now();
    let mut timings = serde_json::Map::new();
    let source = PathBuf::from(req["source"].as_str().unwrap_or(""));
    let lower = source.to_string_lossy().to_lowercase();
    // File ▸ New ▸ Document hands over the IDML it wrote itself: the same import, recorded as "new"
    let new_document = req["newDocument"].as_bool() == Some(true);
    let kind = if new_document { "new" } else if lower.ends_with(".idml") { "idml" } else if lower.ends_with(".indd") || lower.ends_with(".indt") { "indd" } else { return Err(("INDD 또는 IDML만 가져올 수 있습니다".into(), "source")) };
    let bytes = std::fs::metadata(&source).map_err(|e| (format!("원본을 읽을 수 없습니다: {}: {e}", source.display()), "source"))?.len();
    let project = folio_project::manifest::open(Path::new(req["project"].as_str().unwrap_or(""))).map_err(|e| (e.to_string(), "notProject"))?;
    let document = target_document(&project, &source, req).map_err(|e| (e, "name"))?;
    // the source must not be overwritten, and never is: imports only read it
    let hasher = {
        let s = source.clone();
        std::thread::spawn(move || folio_project::hash::sha256_file(&s))
    };
    let (idml, reader_dir, result, staging) = if kind == "indd" {
        progress("reader", json!({"bytes": bytes}));
        let exe = crate::reader_exe().ok_or(("INDD reader (folio-indd-reader.exe) not found".to_string(), "io"))?;
        let staging = project.data_dir(folio_project::manifest::TMP_DIR).join(format!("import-{}", folio_project::util::short_id()));
        let out = staging.join("reader");
        std::fs::create_dir_all(&out).map_err(|e| (format!("staging folder: {e}"), "io"))?;
        let t = Instant::now();
        let status = std::process::Command::new(&exe)
            .arg(&source)
            .arg(&out)
            .arg("document")
            .arg("--embedded")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| (format!("reader failed to start: {e}"), "io"))?;
        timings.insert("readerMs".into(), json!(t.elapsed().as_millis() as u64));
        if !status.status.success() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err((format!("reader failed: {}", String::from_utf8_lossy(&status.stderr).trim()), "reader"));
        }
        let result: Value = std::fs::read_to_string(out.join("result.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        (out.join("document.idml"), Some(out), Some(result), Some(staging))
    } else {
        (source.clone(), None, None, None)
    };
    if cancel.load(Ordering::Relaxed) {
        if let Some(s) = &staging {
            let _ = std::fs::remove_dir_all(s);
        }
        return Err(("가져오기를 취소했습니다".into(), "cancelled"));
    }
    progress("parse", json!({}));
    let t = Instant::now();
    let doc = match &reader_dir {
        Some(d) => folio_idml::read_package(&idml, Some(&d.join("result.json"))),
        None => folio_idml::read_package(&idml, None),
    };
    let doc = match doc {
        Ok(d) => d,
        Err(e) => {
            if let Some(s) = &staging {
                let _ = std::fs::remove_dir_all(s);
            }
            return Err((format!("IDML: {e:?}"), "source"));
        }
    };
    timings.insert("parseMs".into(), json!(t.elapsed().as_millis() as u64));
    progress("links", json!({}));
    let t = Instant::now();
    let state = crate::print::build_links(&doc, source.parent(), &roots(req));
    let mut links = (*state.links).clone();
    let inside = if req["_projectOnly"] == true { Some(project.root.canonicalize().map_err(|e| (format!("project: {e}"), "notProject"))?) } else { None };
    for entry in &mut links.links {
        let outside = match &entry.status {
            Some(folio_links::LinkStatus::Found { path, .. }) => !admitted(inside.as_deref(), path),
            Some(folio_links::LinkStatus::Ambiguous(paths)) => paths.iter().any(|p| !admitted(inside.as_deref(), p)),
            _ => false,
        };
        if outside { entry.status = Some(folio_links::LinkStatus::Missing); entry.modified = None; }
    }
    timings.insert("linksMs".into(), json!(t.elapsed().as_millis() as u64));
    let (sha256, _) = hasher.join().map_err(|_| ("hash thread failed".to_string(), "io"))?.map_err(|e| (format!("원본 해시: {e}"), "source"))?;
    progress("plan", json!({}));
    let search = folio_links::LinkSearch { document_dir: source.parent().map(Path::to_path_buf), roots: Vec::new() };
    let resolve = |u: &str| resolve_inside(u, &search, inside.as_deref());
    let t = Instant::now();
    let plan = {
        let p = import::Prepared {
            source: &source,
            kind,
            source_sha256: &sha256,
            source_bytes: bytes,
            idml: &idml,
            reader_dir: reader_dir.as_deref(),
            reader_result: result.as_ref(),
            doc: &doc,
            fonts,
            links: &links,
            resolve_uri: &resolve,
        };
        import::plan(&project, &document, &p)
    };
    timings.insert("planMs".into(), json!(t.elapsed().as_millis() as u64));
    timings.insert("prepareMs".into(), json!(t0.elapsed().as_millis() as u64));
    Ok(Prepared { key: key_of(req), project, source, kind, sha256, bytes, staging, idml, reader_dir, result, doc, links, search, inside, plan, timings })
}

fn with_fonts<R>(pool: &Fonts, f: impl FnOnce(&folio_text::FontStore) -> R) -> R {
    let store = pool.take();
    let r = f(&store);
    let mut spare = pool.spare.lock().expect("shared state must remain unpoisoned after each operation");
    if spare.is_none() {
        *spare = Some(store);
    }
    r
}

fn import_event(out: &Out, id: &Value, stage: &str, extra: Value) {
    let mut h = json!({"id": id, "event": "progress", "job": "import", "stage": stage});
    if let (Some(m), Some(e)) = (h.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            m.insert(k.clone(), v.clone());
        }
    }
    send(out, h, &[]);
}

// ---------------------------------------------------------------------------------------
// File ▸ New ▸ Document

/// The face of a family and style among the installed fonts.
fn font_choice(fonts: &folio_text::FontStore, family: &str, style: &str) -> Option<folio_project::newdoc::FontChoice> {
    let f = fonts.faces().iter().find(|f| f.family.eq_ignore_ascii_case(family) && f.style.eq_ignore_ascii_case(style))?;
    let font_type = if f.cff {
        "OpenTypeCFF"
    } else if f.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("otf")) {
        "OpenTypeTT"
    } else {
        "TrueType"
    };
    Some(folio_project::newdoc::FontChoice { family: f.family.clone(), style: f.style.clone(), postscript: f.postscript.clone(), font_type: font_type.into() })
}

/// The font a new document starts with: the first of these that is installed, else any upright
/// Regular face (a document made here uses what the computer has; nothing is bundled).
fn default_font(fonts: &folio_text::FontStore) -> Option<folio_project::newdoc::FontChoice> {
    const PREFERRED: [&str; 8] = ["Minion Pro", "Myriad Pro", "Arial", "Helvetica", "Times New Roman", "Segoe UI", "Liberation Sans", "DejaVu Sans"];
    let regular = |f: &&folio_text::fonts::FaceInfo| f.style.eq_ignore_ascii_case("Regular") && !f.italic;
    for fam in PREFERRED {
        if let Some(f) = fonts.faces().iter().filter(regular).find(|f| f.family.eq_ignore_ascii_case(fam)) {
            return font_choice(fonts, &f.family, &f.style);
        }
    }
    let f = fonts.faces().iter().find(regular)?;
    font_choice(fonts, &f.family, &f.style)
}

/// `newDocumentFonts`: the installed families with their styles, and the default.
pub fn new_document_fonts(pool: &Fonts) -> Value {
    with_fonts(pool, |fonts| {
        let mut families: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> = std::collections::BTreeMap::new();
        for f in fonts.faces() {
            families.entry(f.family.clone()).or_default().entry(f.style.clone()).or_insert_with(|| f.postscript.clone());
        }
        let list: Vec<Value> = families
            .into_iter()
            .map(|(family, styles)| json!({"family": family, "styles": styles.into_iter().map(|(style, postscript)| json!({"style": style, "postscript": postscript})).collect::<Vec<_>>()}))
            .collect();
        json!({"ok": true, "type": "newDocumentFonts", "fonts": list, "default": default_font(fonts).map(|d| json!({"family": d.family, "style": d.style}))})
    })
}

fn number(v: &Value, key: &str) -> Result<f64, String> {
    v[key].as_f64().ok_or_else(|| format!("설정 {key}이(가) 숫자가 아닙니다"))
}

fn four(v: &Value, key: &str) -> Result<[f64; 4], String> {
    let a = v[key].as_array().filter(|a| a.len() == 4).ok_or_else(|| format!("설정 {key}은(는) 숫자 네 개여야 합니다"))?;
    let mut out = [0.0; 4];
    for (o, x) in out.iter_mut().zip(a) {
        *o = x.as_f64().ok_or_else(|| format!("설정 {key}에 숫자가 아닌 값이 있습니다"))?;
    }
    Ok(out)
}

/// `documentNew {project, document, settings}`: writes the blank IDML for the dialog's settings
/// and imports it into the project like any IDML (so the result is `imported`, ready to open).
/// With `untitled` (a name with `{0}`, "제목 없음-{0}") and no `document`, the document is named with the first number from 1
/// the project does not have yet ("제목 없음-1.folio", then "-2", …), as File ▸ New ▸ Document names it.
pub fn new_document(req: &Value, out: &Out, pool: &Fonts) -> Value {
    let Some(pattern) = req["untitled"].as_str().filter(|p| p.contains("{0}") && req["document"].as_str().is_none_or(str::is_empty)) else {
        return new_document_named(req, out, pool);
    };
    let mut last = json!({"ok": false, "code": "exists", "error": "no free name"});
    for n in 1..=99 {
        let name = pattern.replace("{0}", &n.to_string());
        let mut one = req.clone();
        one["settings"]["name"] = json!(name);
        one["document"] = json!(format!("{name}.folio"));
        last = new_document_named(&one, out, pool);
        if last["code"] != "exists" { return last; }
    }
    last
}

fn new_document_named(req: &Value, out: &Out, pool: &Fonts) -> Value {
    let id = req["id"].clone();
    let fail = |code: &str, message: String| json!({"id": id, "ok": false, "code": code, "error": message});
    let s = &req["settings"];
    let project = match folio_project::manifest::open(Path::new(req["project"].as_str().unwrap_or(""))) {
        Ok(p) => p,
        Err(e) => return fail("notProject", e.to_string()),
    };
    let font = with_fonts(pool, |fonts| match (s["font"]["family"].as_str(), s["font"]["style"].as_str()) {
        (Some(family), Some(style)) => font_choice(fonts, family, style).ok_or_else(|| format!("설치된 글꼴이 아닙니다: {family} {style}")),
        _ => default_font(fonts).ok_or_else(|| "설치된 글꼴이 없습니다".to_owned()),
    });
    let font = match font {
        Ok(f) => f,
        Err(e) => return fail("font", e),
    };
    let doc = (|| -> Result<folio_project::newdoc::NewDocument, String> {
        let mut d = folio_project::newdoc::NewDocument::letter(font);
        d.name = s["name"].as_str().unwrap_or("Untitled-1").to_owned();
        d.width = number(s, "width")?;
        d.height = number(s, "height")?;
        d.pages = number(s, "pages")? as u32;
        d.facing = s["facing"].as_bool().unwrap_or(true);
        d.start = s["start"].as_f64().unwrap_or(1.0) as u32;
        d.top = number(s, "top")?;
        d.bottom = number(s, "bottom")?;
        d.inside = number(s, "inside")?;
        d.outside = number(s, "outside")?;
        d.columns = number(s, "columns")? as u32;
        d.gutter = number(s, "gutter")?;
        d.bleed = four(s, "bleed")?;
        d.slug = four(s, "slug")?;
        if let Some(l) = s["layer"].as_str().filter(|l| !l.trim().is_empty()) {
            d.layer = l.to_owned();
        }
        if let Some(p) = s["profiles"]["cmyk"].as_str().filter(|p| !p.trim().is_empty()) {
            d.cmyk_profile = p.to_owned();
        }
        if let Some(p) = s["profiles"]["rgb"].as_str().filter(|p| !p.trim().is_empty()) {
            d.rgb_profile = p.to_owned();
        }
        if let Some(u) = s["units"]["horizontal"].as_str() {
            d.horizontal_units = u.to_owned();
        }
        if let Some(u) = s["units"]["vertical"].as_str() {
            d.vertical_units = u.to_owned();
        }
        d.validate()?;
        Ok(d)
    })();
    let doc = match doc {
        Ok(d) => d,
        Err(e) => return fail("settings", e),
    };
    let bytes = match doc.package() {
        Ok(b) => b,
        Err(e) => return fail("settings", e),
    };
    let staging = project.data_dir(folio_project::manifest::TMP_DIR).join(format!("new-{}", folio_project::util::short_id()));
    if let Err(e) = std::fs::create_dir_all(&staging) {
        return fail("io", format!("임시 폴더를 만들 수 없습니다: {e}"));
    }
    let source = staging.join(format!("{}.idml", folio_project::paths::fix_name(&doc.name)));
    let reply = match std::fs::write(&source, bytes) {
        Ok(()) => {
            let import = json!({"id": id, "cmd": "importRun", "source": source.display().to_string(), "project": req["project"], "document": req["document"], "roots": [], "newDocument": true});
            import_command(&import, out, pool)
        }
        Err(e) => fail("io", format!("새 문서를 쓸 수 없습니다: {e}")),
    };
    let _ = std::fs::remove_dir_all(&staging);
    reply
}

/// `importPlan` / `importRun` (on their own thread).
pub fn import_command(req: &Value, out: &Out, pool: &Fonts) -> Value {
    let id = req["id"].clone();
    let run = req["cmd"] == "importRun";
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut c = cancel_flag().lock().expect("shared state must remain unpoisoned after each operation");
        if c.is_some() {
            return json!({"id": id, "ok": false, "code": "busy", "error": "다른 가져오기가 진행 중입니다"});
        }
        *c = Some(flag.clone());
    }
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            *cancel_flag().lock().expect("shared state must remain unpoisoned after each operation") = None;
        }
    }
    let _release = Release;
    let progress = |stage: &str, detail: Value| import_event(out, &id, stage, json!({"detail": detail}));
    // reuse the plan the window showed, when it is for the same request
    let mut prepared = {
        let mut j = job().lock().expect("shared state must remain unpoisoned after each operation");
        match j.take() {
            Some(p) if p.key == key_of(req) => Some(p),
            _ => None,
        }
    };
    if prepared.is_none() {
        match with_fonts(pool, |fonts| prepare(req, fonts, &flag, &progress)) {
            Ok(p) => prepared = Some(p),
            Err((e, code)) => return json!({"id": id, "ok": false, "code": code, "error": e}),
        }
    }
    let p = prepared.expect("prepared");
    if !run {
        let mut plan = serde_json::to_value(&p.plan).unwrap_or(Value::Null);
        plan["timings"] = Value::Object(p.timings.clone());
        // principle 2, item 3: the differences from InDesign this document uses, and the fixed ones of this version
        plan["limits"] = folio_project::limits::report(&p.plan.differences);
        let reply = json!({"id": id, "ok": true, "type": "importPlan", "plan": plan});
        *job().lock().expect("shared state must remain unpoisoned after each operation") = Some(p);
        return reply;
    }
    let t = Instant::now();
    let resolve = |u: &str| resolve_inside(u, &p.search, p.inside.as_deref());
    let report_extra = json!({"svgWarnings": crate::svgplace::report(&p.links), "readerWarnings": p.result.as_ref().and_then(|r| r.get("warnings")).cloned(), "coverage": p.result.as_ref().and_then(|r| r.get("coverage")).cloned(), "readerEmbedded": p.result.as_ref().and_then(|r| r.get("embedded")).cloned()});
    let outcome = with_fonts(pool, |fonts| {
        let prep = import::Prepared {
            source: &p.source,
            kind: p.kind,
            source_sha256: &p.sha256,
            source_bytes: p.bytes,
            idml: &p.idml,
            reader_dir: p.reader_dir.as_deref(),
            reader_result: p.result.as_ref(),
            doc: &p.doc,
            fonts,
            links: &p.links,
            resolve_uri: &resolve,
        };
        import::run(&p.project, &prep, &p.plan, report_extra, &flag, &|pr| {
            import_event(out, &id, pr.stage, json!({"done": pr.done, "total": pr.total, "files": pr.files, "filesTotal": pr.files_total}));
        })
    });
    let mut timings = p.timings.clone();
    timings.insert("runMs".into(), json!(t.elapsed().as_millis() as u64));
    drop(p);
    match outcome {
        Ok(o) => {
            let mut result = serde_json::to_value(&o).unwrap_or(Value::Null);
            result["timings"]["prepare"] = Value::Object(timings);
            json!({"id": id, "ok": true, "type": "imported", "result": result})
        }
        Err(e) => json!({"id": id, "ok": false, "code": e.code, "error": e.message}),
    }
}

// ---------------------------------------------------------------------------------------
// the open .folio

pub(crate) fn folio_of(s: &Session) -> Result<&FolioSession, Value> {
    s.folio.as_deref().ok_or_else(|| json!({"ok": false, "code": "notFolio", "error": "열린 문서가 .folio가 아닙니다 (INDD·IDML은 먼저 프로젝트로 가져오세요)"}))
}

/// `save`: the open `.folio` written again atomically (moved references healed, pins and
/// time refreshed). The IDML parts are kept as they are.
pub fn save(s: &Session) -> Value {
    let f = match folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    if f.read_only() {
        return json!({"ok": false, "code": "readOnly", "error": "다른 사용자가 이 문서를 열고 있어 읽기 전용입니다. 저장할 수 없습니다."});
    }
    // someone may have taken the lock over explicitly since: then this session is read-only
    let lost = {
        let mut guard = f.lock.lock().expect("shared state must remain unpoisoned after each operation");
        let mine = guard.as_ref().map(|g| g.info.session.clone());
        let now = lock::holder(&f.opened.project.root, &f.opened.manifest.id);
        if now.as_ref().map(|h| &h.session) != mine.as_ref() {
            // dropping our guard leaves the new holder's file alone (it checks the session)
            *guard = None;
            Some(now)
        } else {
            None
        }
    };
    if let Some(now) = lost {
        if let Some(h) = now {
            let age = folio_project::util::parse_rfc3339(&h.time).map(|t| folio_project::util::now_secs() - t);
            let same = h.machine.eq_ignore_ascii_case(&lock::machine_name());
            *f.holder.lock().expect("shared state must remain unpoisoned after each operation") = Some((h, age, same));
        }
        return json!({"ok": false, "code": "lockLost", "error": "다른 사용자가 잠금을 가져갔습니다. 이 창은 이제 읽기 전용이며 저장하지 않았습니다.", "lock": f.lock_json()});
    }
    write_to(s, f, &f.opened.file, None, None)
}

/// File ▸ Save a Copy: the document as it is now (edits included) written to another `.folio`
/// in the same project, under a document id of its own (its lock and its `folio.document`
/// label), so both can be open at once. The open document stays as it is, not saved. The
/// copy's references are project-relative like the original's, so they hold (principle 1).
/// Refused: outside the project, a name not every OS takes, the document's own file, or a
/// `.folio` someone has open.
pub fn save_copy(req: &Value, s: &Session) -> Value {
    let f = match folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let target = PathBuf::from(req["path"].as_str().unwrap_or(""));
    if !target.extension().is_some_and(|e| e.eq_ignore_ascii_case("folio")) {
        return json!({"ok": false, "code": "invalid", "error": "사본은 .folio 파일로 저장합니다"});
    }
    let root = &f.opened.project.root;
    if let Err(e) = folio_project::paths::relative_to(&root.to_string_lossy(), &target.to_string_lossy()) {
        return json!({"ok": false, "code": "outsideProject", "error": format!("사본은 이 프로젝트 폴더 안에 저장합니다 ({e})")});
    }
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b) || a == b;
    if same(&target, &f.opened.file) {
        return json!({"ok": false, "code": "invalid", "error": "문서 자신의 파일에는 사본을 쓰지 않습니다 (저장을 쓰세요)"});
    }
    if target.exists()
        && let Ok((m, _)) = format::read_manifest(&target)
        && lock::holder(root, &m.id).is_some_and(|h| !lock::is_stale(&h))
    {
        return json!({"ok": false, "code": "locked", "error": "그 문서는 누군가 열고 있어 덮어쓸 수 없습니다"});
    }
    write_to(s, f, &target, Some(folio_project::util::uuid_v4()), None)
}

/// Writes the open document, edits included, to `target` atomically (moved references healed,
/// pins and time refreshed). `new_id`: a copy under that document id (not marked saved).
/// `merged`: parts a command changed on a copy of the document (Data Merge's merged document), written as
/// the copy's own, with the stories to compose afresh.
pub(crate) type Merged = (BTreeMap<String, Option<String>>, BTreeSet<String>, Vec<format::Reference>);

pub(crate) fn write_to(s: &Session, f: &FolioSession, target: &Path, new_id: Option<String>, merged: Option<Merged>) -> Value {
    let t = Instant::now();
    let o = &f.opened;
    let mut m = o.manifest.clone();
    let mut healed = 0;
    for r in &mut m.references {
        if let Some(st) = o.state(&r.id)
            && st.state == "moved"
            && let Some(at) = &st.found_at
        {
            r.path = Some(at.clone());
            healed += 1;
        }
    }
    // a source document of a content link moved or renamed since this document was opened: its reference names where it is now
    for r in &mut m.references {
        if let Some(doc) = r.extra.get("document").and_then(|v| v.as_str())
            && let Ok(Some(at)) = folio_project::docfind::resolve(&o.project.root, r.path.as_ref(), doc)
            && r.path.as_ref() != Some(&at)
        {
            r.path = Some(at);
            healed += 1;
        }
    }
    for p in &mut m.pins {
        if let Some(r) = m.references.iter().find(|r| r.id == p.reference)
            && let (Some(path), Some(sha), Some(b)) = (&r.path, &r.sha256, r.bytes)
        {
            p.path = path.clone();
            p.sha256 = sha.clone();
            p.bytes = b;
        }
    }
    m.saved = folio_project::util::now_rfc3339();
    m.engine = env!("CARGO_PKG_VERSION").into();
    if let Some(id) = &new_id {
        m.id = id.clone();
        m.created = m.saved.clone();
    }
    // edits: the parts they changed (copied graphics' frames labelled), the graphic records,
    // and saved lines without the stories composed afresh
    // the revision these parts are, taken with them: an edit made while the file is written
    // stays unsaved
    let mut saved_rev = None;
    let (mut changes, mut recompose) = match &s.edit {
        Some(shared) => {
            let es = match crate::edit::lock_for_change(shared) {
                Ok(es) => es,
                Err(refusal) => return refusal,
            };
            saved_rev = Some(es.editor.version());
            let mut changes = crate::edit::part_changes(&es);
            if let Some(g) = &es.graphics {
                m.graphics = g.synced.clone();
                for r in &g.references {
                    if !m.references.iter().any(|old|old.id==r.id) {m.references.push(r.clone());}
                }
                if !g.labels.is_empty() {
                    for (name, text) in changes.iter_mut() {
                        let Some(t) = text else { continue };
                        let mut edits = folio_project::idmlparts::Edits { labels: g.labels.clone(), ..Default::default() };
                        if let (Some(out), _) = folio_project::idmlparts::rewrite(name, t, &mut edits) {
                            *t = out;
                        }
                    }
                }
            }
            (changes, es.editor.recompose().clone())
        }
        None => Default::default(),
    };
    if let Some((extra, stories, references)) = merged {
        changes.extend(extra);
        recompose.extend(stories);
        for r in references {
            if !m.references.iter().any(|old| old.id == r.id) { m.references.push(r); }
        }
    }
    let saved_lines = m
        .saved_lines
        .as_ref()
        .and_then(|p| format::read_part(&o.file, &p.part).ok().flatten())
        .map(|b| crate::edit::filter_saved_lines(&b, &recompose));
    let report = m.import_report.as_ref().and_then(|p| format::read_part(&o.file, &p.part).ok().flatten());
    // a copy: the designmap's `folio.document` label names the copy's id
    let label = new_id.as_ref().map(|id| json!({"id": id, "project": o.project.id(), "format": format::VERSION}).to_string());
    let mut action = |name: &str, read: &mut dyn FnMut() -> std::io::Result<String>| {
        if let Some(label) = &label && name == "designmap.xml" {
            let text = match changes.get(name) {
                Some(Some(text)) => text.clone(),
                _ => read()?,
            };
            let mut edits = folio_project::idmlparts::Edits { document_labels: vec![(format::LABEL_DOCUMENT.into(), label.clone())], ..Default::default() };
            let (out, _) = folio_project::idmlparts::rewrite(name, &text, &mut edits);
            return Ok(format::PartAction::Replace(out.unwrap_or(text)));
        }
        Ok(match changes.get(name) {
            Some(Some(text)) => format::PartAction::Replace(text.clone()),
            Some(None) => format::PartAction::Drop,
            None => format::PartAction::Keep,
        })
    };
    let added: Vec<_> = changes.iter().filter_map(|(name, value)| value.as_deref().map(|t| (name.as_str(), t))).collect();
    match format::write_folio_with_parts(target, &o.file, &mut action, &m, saved_lines.as_deref(), report.as_deref(), &added) {
        Ok(bytes) => {
            if new_id.is_none()
                && let Some(shared) = &s.edit
            {
                crate::edit::mark_saved(shared, saved_rev);
            }
            json!({"ok": true, "type": if new_id.is_some() { "savedCopy" } else { "saved" }, "path": target.display().to_string(), "bytes": bytes, "healed": healed, "parts": changes.len(), "ms": t.elapsed().as_secs_f64() * 1000.0})
        }
        Err(e) => json!({"ok": false, "code": "io", "error": e.to_string()}),
    }
}

pub fn export_idml(req: &Value, s: &Session) -> Value {
    let f = match folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let path = PathBuf::from(req["path"].as_str().unwrap_or(""));
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("idml")) {
        return json!({"ok": false, "error": "내보낼 IDML 경로가 없습니다"});
    }
    if path == f.opened.file {
        return json!({"ok": false, "error": "문서 자신의 파일에는 쓸 수 없습니다"});
    }
    let saved_stories = s.layout.stats.saved_stories;
    let opts = folio_project::export::ExportOptions {
        contents: req["contents"].as_str() != Some("none"),
        contents_raster_only: req["contents"].as_str() == Some("raster"),
    };
    // the computed fields of Data Merge go into a CSV InDesign reads (datamerge_expr.rs)
    let mut computed_fields: Option<Value> = None;
    let (live,changes) = if let Some(shared)=&s.edit {
        let es=match crate::edit::lock_for_change(shared) { Ok(es)=>es, Err(refusal)=>return refusal };
        let mut changes=crate::edit::part_changes(&es);
        if let Some(g)=&es.graphics {
            for (name,text) in &mut changes {
                if let Some(t)=text {
                    let mut labels=folio_project::idmlparts::Edits{labels:g.labels.clone(),..Default::default()};
                    if let (Some(out),_)=folio_project::idmlparts::rewrite(name,t,&mut labels){*t=out;}
                }
            }
        }
        match crate::datamerge_expr::export_prepare(s,&es,&mut changes) {
            Ok(done)=>computed_fields=done,
            Err(e)=>return json!({"ok": false, "code": "io", "error": format!("데이터 병합 계산 열의 값을 CSV 로 쓰지 못했습니다: {e}")}),
        }
        (Some(crate::place::snapshot(&f.opened,&es)),Some(changes))
    } else {(None,None)};
    match folio_project::export::export_idml_with(live.as_ref().unwrap_or(&f.opened), &path, saved_stories, opts, changes.as_ref()) {
        Ok(mut report) => {
            // principle 2, item 3: Data Merge computed fields are values in InDesign
            if let Some(c) = computed_fields {
                let written = c["written"] == true;
                let effect = if written {
                    "데이터 병합의 계산 열은 InDesign 에 없습니다. 계산한 값을 열로 더한 CSV(프로젝트 안 assets/문서/datamerge/)를 데이터 원본으로 가리키므로, InDesign 에서는 이 시점의 값으로 굳은 열이며 원본이 바뀌어도 다시 계산하지 않습니다. 그림 필드의 상대 경로는 이 컴퓨터의 전체 경로로 바뀝니다."
                } else {
                    "데이터 병합의 계산 열은 InDesign 에 없습니다. 데이터 원본 파일을 읽을 수 없어 계산한 값을 담지 못했고, IDML 은 원래 데이터 원본을 가리킵니다."
                };
                if let Some(list) = report["differences"].as_array_mut() { list.push(json!({"what": "computedFields", "count": c["count"], "effect": effect})); }
                report["computedFields"] = c;
            }
            json!({"ok": true, "type": "exportedIdml", "path": path.display().to_string(), "report": report})
        }
        Err(e) => json!({"ok": false, "code": "io", "error": e.to_string()}),
    }
}

pub fn lock_override(s: &Session) -> Value {
    let f = match folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    if !f.read_only() {
        return json!({"ok": true, "type": "lock", "lock": f.lock_json()});
    }
    match lock::take_over(&f.opened.project.root, &f.opened.manifest.id, f.opened.rel.as_str()) {
        Ok((guard, previous)) => {
            *f.lock.lock().expect("shared state must remain unpoisoned after each operation") = Some(guard);
            *f.holder.lock().expect("shared state must remain unpoisoned after each operation") = None;
            json!({"ok": true, "type": "lock", "lock": f.lock_json(), "previous": previous})
        }
        Err(e) => json!({"ok": false, "code": "io", "error": e.to_string()}),
    }
}

pub fn status(s: &Session) -> Value {
    match folio_of(s) {
        Ok(f) => {
            let live=match s.edit.as_ref().map(crate::edit::lock_for_change).transpose() {
                Ok(es)=>es.map(|es|crate::place::snapshot(&f.opened,&es)),
                Err(refusal)=>return refusal,
            };
            let o=live.as_ref().unwrap_or(&f.opened);
            let mut status=f.summary();
            status["references"]["references"]=json!(o.refs.len());
            for state in ["ok","changed","moved","missing"] {
                status["references"][state]=json!(o.refs.iter().filter(|r|r.state==state).count());
            }
            status["references"]["linksInProject"]=json!(o.refs.iter().filter(|r|r.kind==format::RefKind::Link && r.path.is_some()).count());
            status["references"]["linksResolved"]=json!(o.refs.iter().filter(|r|r.kind==format::RefKind::Link && matches!(r.state,"ok"|"moved")).count());
            json!({"ok": true, "type": "projectStatus", "status": status, "refs": o.refs,
                   "conflicts": folio_project::conflicts::scan(&f.opened.project.root)})
        },
        Err(e) => e,
    }
}

/// `{"cmd":"projectNewFolder","root":…,"folder":"assets","name":"표지"}`: an empty folder in the project (the explorer's New Folder);
/// answers `{"path":"assets/표지"}` or the error code of `folio_project::newfile::make_folder`.
pub fn new_folder(req: &Value) -> Result<Value, String> {
    let root = req["root"].as_str().filter(|r| !r.is_empty()).ok_or("root is required")?;
    let path = folio_project::newfile::make_folder(Path::new(root), req["folder"].as_str().unwrap_or(""), req["name"].as_str().unwrap_or(""))?;
    Ok(json!({"path": path}))
}
