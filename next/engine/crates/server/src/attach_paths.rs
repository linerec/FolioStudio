//! Paths from a connection attached to the running app (`folio-mcp --attach`): the rules of `MCP.ko.md` "경로를 받는 도구와
//! 경로 규칙", checked by the engine for every attached request (the client is never trusted; `attached::admit` has already
//! checked the names, the fields and the file-writes setting).
//!
//! A path is relative to the project open in the app (the open `.folio`'s, else the one the screen last loaded) and is checked
//! with the project MCP server's `Boundary` (mcp-c1): `RelPath` rules, no link or junction on the way, nothing outside the
//! root, no link anywhere in the tree. Then it is turned into the path on this machine, so the app's own command runs on it
//! and the document stores what it always stores (project-relative references and hashes).
use crate::attached;
use crate::boundary::Boundary;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The project the screen loaded last (`projectInfo` from the screen), for paths while no `.folio` is open.
static SCREEN_PROJECT: Mutex<Option<PathBuf>> = Mutex::new(None);
/// IDML and PDF files a person exported to from the screen in this app session: an attached client may export to them again.
static PERSON_EXPORTS: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// The screen loaded a project (its `projectInfo` reply).
pub fn screen_project(root: &str) {
    if !root.is_empty() { *SCREEN_PROJECT.lock().expect("attach paths") = Some(PathBuf::from(root)); }
}

/// The screen exported the document to `path` (IDML or PDF): the person chose that file.
pub fn person_exported(path: &str) {
    if let Ok(path) = Path::new(path).canonicalize() { PERSON_EXPORTS.lock().expect("attach paths").insert(path); }
}

fn chosen_by_person(path: &Path) -> bool {
    path.canonicalize().is_ok_and(|p| PERSON_EXPORTS.lock().expect("attach paths").contains(&p))
}

/// The root an attached request's paths are relative to.
fn project_root(s: Option<&crate::Session>) -> Result<PathBuf, String> {
    if let Some(f) = s.and_then(|s| s.folio.as_ref()) { return Ok(f.opened.project.root.clone()); }
    SCREEN_PROJECT.lock().expect("attach paths").clone()
        .ok_or_else(|| "No project is open in Folio: open a project (or one of its documents) in the app first.".to_owned())
}

struct Checked<'a> {
    root: &'a Path,
    boundary: Boundary,
    /// Every project path this request uses: checked again once the command has used it (`recheck`).
    used: std::cell::RefCell<Vec<String>>,
}

/// What `resolve` found for an attached request: the root its paths are relative to, and the paths to check again when its
/// reply comes (a folder swapped for a link between the check and the use: the reply is replaced by a refusal).
pub struct Resolved {
    pub root: PathBuf,
    boundary: Boundary,
    used: Vec<String>,
}

impl Resolved {
    /// The paths of the request, name by name again after the command used them: still below the root, through no link.
    pub fn recheck(&self) -> Result<(), String> {
        for rel in &self.used {
            self.boundary.path(rel).map_err(|e| format!("{rel}: the project changed while Folio used this path ({e}); the result is not reported"))?;
        }
        Ok(())
    }
}

impl Checked<'_> {
    /// The rules every attached path follows besides `Boundary`'s: no named location, nothing in Folio's own folders.
    fn rel(&self, raw: &str) -> Result<folio_project::RelPath, String> {
        let rel = folio_project::RelPath::parse(raw).map_err(|e| format!("{raw}: {e}"))?;
        let first = rel.names().next().unwrap_or("");
        if first.starts_with('@') {
            return Err(format!("{raw}: named locations (@…) outside the project are chosen by a person in the Folio app, not over MCP"));
        }
        if first.eq_ignore_ascii_case(".folio") || rel.as_str().eq_ignore_ascii_case("folio.project") {
            return Err(format!("{raw}: Folio's own project files are not available over MCP"));
        }
        // links, junctions and the canonical place, name by name from the root to the target (mcp-c1's rule, without the walk
        // over the whole tree): the real path of every existing step, the file itself included, must be below the root
        self.boundary.path(rel.as_str()).map_err(|e| format!("{raw}: {e}"))?;
        self.used.borrow_mut().push(rel.as_str().to_owned());
        Ok(rel)
    }

    /// An existing project file with one of `extensions` (empty: any): its project-relative path and the path on this machine.
    fn existing(&self, raw: &str, extensions: &[&str]) -> Result<(folio_project::RelPath, PathBuf), String> {
        let rel = self.rel(raw)?;
        self.boundary.existing(rel.as_str()).map_err(|e| format!("{raw}: {e}"))?;
        let ext = extension(rel.file_name());
        if !extensions.is_empty() && !extensions.contains(&ext.as_str()) {
            return Err(format!("{raw}: a .{} file of the project is required", extensions.join(" or .")));
        }
        let native = rel.on_disk(self.root);
        Ok((rel, native))
    }

    /// A file to write: a new path in an existing project folder, or (`reexport`) a file a person exported to from the screen.
    fn output(&self, raw: &str, ext: &str, reexport: bool) -> Result<PathBuf, String> {
        let rel = self.rel(raw)?;
        if extension(rel.file_name()) != ext { return Err(format!("{raw}: the file must end in .{ext}")); }
        let native = rel.on_disk(self.root);
        if native.exists() {
            if reexport && chosen_by_person(&native) { return Ok(native); }
            return Err(format!("{raw}: existing files are not overwritten over MCP (originals stay read-only); choose a new path"));
        }
        self.boundary.output(rel.as_str(), ext).map_err(|e| format!("{raw}: {e}"))?;
        Ok(native)
    }

    /// A folder to make (File ▸ Package): a new name in an existing project folder; nothing there is replaced.
    fn new_folder(&self, raw: &str) -> Result<PathBuf, String> {
        let rel = self.rel(raw)?;
        let native = rel.on_disk(self.root);
        if native.exists() { return Err(format!("{raw}: the folder already exists; packages go into a new folder of the project")); }
        if !native.parent().is_some_and(Path::is_dir) { return Err(format!("{raw}: its parent folder must exist in the project")); }
        Ok(native)
    }

    /// An existing project folder ("" or "." is the root), as the project-relative text the app's command takes.
    fn folder(&self, raw: &str) -> Result<String, String> {
        let raw = raw.trim().trim_matches('/');
        if raw.is_empty() || raw == "." { return Ok(String::new()); }
        let rel = self.rel(raw)?;
        if !rel.on_disk(self.root).is_dir() { return Err(format!("{raw}: not a folder of the project")); }
        Ok(rel.as_str().to_owned())
    }
}

/// Book and library actions an attached client may ask (`attached::BOOK_ACTIONS` / `LIBRARY_ACTIONS`, MCP.ko.md): open (`read`)
/// and make (`new`, `create`) a book or library file of the project; a book's documents added (`add`), its styles synchronized
/// (`synchronize`: the book's documents are written), the book exported to one PDF (`exportPdf`: a new `output`); a library item
/// stored from the open document (`store`: its pictures are copied to `assets/library/`) or placed in it (`place`).
/// `book` / `library` with `action`: the file it names (`path`, or `folder` + `fileName` for `new`) checked like any other path,
/// and the project root the app's command takes as `root`.
fn book_or_library(req: &mut Value, checked: &Checked, root: &Path) -> Result<(), String> {
    let extension = if req["cmd"] == "book" { "foliobook" } else { "foliolibrary" };
    match req["action"].as_str().unwrap_or("") {
        "read" => { let (rel, _) = checked.existing(text(req, "path")?, &[extension])?; req["path"] = json!(rel.as_str()); }
        // File ▸ Book ▸ Add Document: a .folio / .idml of the project
        "add" => {
            let (rel, _) = checked.existing(text(req, "path")?, &[extension])?;
            req["path"] = json!(rel.as_str());
            let (document, _) = checked.existing(text(req, "document")?, &["folio", "idml"])?;
            req["document"] = json!(document.as_str());
        }
        // Synchronize Book writes the book's documents, Export Book reads them: every one checked like a requested path
        "synchronize" | "exportPdf" => {
            let (rel, native) = checked.existing(text(req, "path")?, &[extension])?;
            req["path"] = json!(rel.as_str());
            let book: Value = serde_json::from_slice(&std::fs::read(&native).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            for entry in book["documents"].as_array().into_iter().flatten() {
                let document = entry["path"].as_str().ok_or("a book document has no path")?;
                checked.rel(document).map_err(|e| format!("book document {e}"))?;
            }
            if req["action"] == "exportPdf" {
                let raw = text(req, "output")?.to_owned();
                checked.output(&raw, "pdf", false)?;
                // the book's PDF export takes the project-relative path
                req["output"] = json!(checked.rel(&raw)?.as_str());
            }
        }
        // Library ▸ Add Item: the pictures of the stored items are copied to assets/library/ of the project
        "store" | "place" => {
            let (rel, _) = checked.existing(text(req, "path")?, &[extension])?;
            req["path"] = json!(rel.as_str());
            for folder in ["assets", "assets/library"] { checked.rel(folder)?; }
        }
        "create" => {
            let raw = text(req, "path")?.to_owned();
            checked.output(&raw, extension, false)?;
            req["path"] = json!(checked.rel(&raw)?.as_str());
        }
        "new" => {
            let folder = checked.folder(req["folder"].as_str().unwrap_or(""))?;
            let name = req["fileName"].as_str().ok_or("fileName (one file name) is required")?;
            if name.contains(['/', '\\']) || name.starts_with('@') { return Err(format!("{name}: fileName is one file name, usable on every OS")); }
            let stem = name.trim().trim_end_matches(&format!(".{extension}")[..]);
            let file = if folder.is_empty() { format!("{stem}.{extension}") } else { format!("{folder}/{stem}.{extension}") };
            checked.output(&file, extension, false)?;
            req["folder"] = json!(folder);
        }
        other => {
            let cmd = req["cmd"].as_str().unwrap_or("");
            return Err(format!("{cmd} action {other} is not available to an attached MCP connection (only {})", attached::file_actions(cmd).0.join(", ")));
        }
    }
    req["root"] = json!(root.to_string_lossy());
    Ok(())
}

fn extension(name: &str) -> String {
    Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("{key} (a project-relative path) is required"))
}

/// Why an attached request is refused: the message, and more for the client to act on (`reason`, `setting`, `howTo`).
pub struct Refused {
    pub error: String,
    pub detail: Value,
}

impl From<String> for Refused {
    fn from(error: String) -> Self { Self { error, detail: Value::Null } }
}

impl From<&str> for Refused {
    fn from(error: &str) -> Self { error.to_owned().into() }
}

/// The folder a person chose for new projects (Preferences ▸ General, `attached::NEW_PROJECT_FOLDER`): an existing folder of
/// this machine that is not a link. None chosen: refused with where to choose it.
fn new_project_folder() -> Result<PathBuf, Refused> {
    let preferences = crate::attach::preferences();
    let chosen = preferences[attached::NEW_PROJECT_FOLDER].as_str().unwrap_or("").trim().to_owned();
    if chosen.is_empty() {
        return Err(Refused {
            error: "No folder for new projects is chosen: a person chooses it in Folio (Preferences ▸ General ▸ Folder the LLM may make new projects in).".into(),
            detail: json!({"reason": "noNewProjectFolder", "setting": attached::NEW_PROJECT_FOLDER,
                "howTo": "In Folio: Edit ▸ Preferences ▸ General ▸ Folder the LLM may make new projects in ▸ Choose…, then OK. Until then new projects are made by the person (File ▸ New ▸ Project…)."}),
        });
    }
    // (one separator: the reply names the new project relative to this folder, as the engine writes its root)
    let folder: PathBuf = Path::new(&chosen).components().collect();
    Boundary::folder(&folder).map_err(|e| format!("the folder for new projects cannot be used ({e}): choose it again in Preferences ▸ General"))?;
    Ok(folder)
}

/// The same folder on both sides (canonical paths; a missing one is never the same).
fn same_folder(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

/// A dialog request of an attached client (`openDialog`, `setDialogField`, `applyDialog`; `admit` already refused any
/// `folio.mcp.*` field). App dialogs: only `attached::APP_DIALOGS`; the path fields of those that make a file or folder are the
/// engine's (the open project, the folder a person chose for new projects), applying them needs the file-writes setting and no
/// unsaved changes (the screen opens what they made), and nothing is replaced. Every dialog: what applying it sends to the app
/// (`dialog_state::attached_dialog`) is checked again here, so a dialog the person opened and filled is held to the same rules,
/// and no app setting of `folio.mcp.*` changes. Returns the folder its reply is shown relative to.
fn dialog(req: &mut Value, s: Option<&crate::Session>) -> Result<Option<Resolved>, Refused> {
    let Some(view) = crate::dialog_state::attached_dialog(req, s)? else { return Ok(None) };
    let cmd = req["cmd"].as_str().unwrap_or("").to_owned();
    let name = view.dialog.as_str();
    if view.app && view.declared == "app" && !attached::APP_DIALOGS.contains(&name) {
        return Err(format!("the {name} dialog is not available to an attached MCP connection (only {})", attached::APP_DIALOGS.join(", ")).into());
    }
    let makes = view.app && attached::APP_DIALOG_FILES.contains(&name);
    match cmd.as_str() {
        "openDialog" if makes => {
            if let Some(key) = req["values"].as_object().and_then(|v| v.keys().find(|k| attached::APP_DIALOG_PATHS.contains(&k.as_str())).cloned()) {
                return Err(format!("{key} of the {name} dialog is filled by Folio (the open project, or the folder a person chose for new projects)").into());
            }
            if !req["values"].is_object() { req["values"] = json!({}); }
            if name == "newProject" { req["values"]["parent"] = json!(new_project_folder()?.to_string_lossy()); }
            else { req["values"]["project"] = json!(project_root(s)?.to_string_lossy()); }
        }
        "setDialogField" if makes => {
            let top = req["field"].as_str().unwrap_or("").split('/').next().unwrap_or("").to_owned();
            if attached::APP_DIALOG_PATHS.contains(&top.as_str()) {
                return Err(format!("{top} of the {name} dialog is filled by Folio, not over MCP").into());
            }
        }
        "applyDialog" => {
            if makes {
                if !crate::attach::files_allowed() { return Err(attached::FILES_REFUSED.into()); }
                if req["action"].as_str().unwrap_or("ok") != "ok" {
                    return Err(format!("the {name} dialog's {} button is the person's (nothing is replaced over MCP)", req["action"].as_str().unwrap_or("")).into());
                }
                if let Some(s) = s && crate::edit::state(s)["dirty"] == true {
                    return Err("The document open in Folio has unsaved changes: save it (save) or ask the person first (the screen opens what the dialog makes).".into());
                }
            }
            for request in &view.requests { app_request(request, s)?; }
        }
        _ => {}
    }
    // the reply shows paths relative to the open project (or to the folder for new projects), never this machine's folders
    let root = if name == "newProject" && view.app { new_project_folder().ok() } else { project_root(s).ok() };
    Ok(root.and_then(|root| Some(Resolved { boundary: Boundary::folder(&root).ok()?, root, used: Vec::new() })))
}

/// An app request an applied dialog would send for an attached client: the reviewed ones, in the open project or the folder for
/// new projects, replacing nothing, changing no `folio.mcp.*` setting.
fn app_request(request: &Value, s: Option<&crate::Session>) -> Result<(), Refused> {
    match request["cmd"].as_str().unwrap_or("") {
        "projectCreate" => {
            let dir = PathBuf::from(request["dir"].as_str().unwrap_or(""));
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            folio_project::paths::check_name(&name).map_err(|e| format!("{name}: the project folder's name cannot be used ({e})"))?;
            if !dir.parent().is_some_and(|p| same_folder(p, &new_project_folder().unwrap_or_default())) {
                return Err("new projects are made in the folder a person chose for them (Preferences ▸ General), not elsewhere over MCP".into());
            }
            if dir.exists() { return Err(format!("{name} already exists in the folder for new projects; choose another name").into()); }
        }
        "documentNew" => {
            let root = project_root(s)?;
            if !same_folder(Path::new(request["project"].as_str().unwrap_or("")), &root) {
                return Err("a new document is made in the project open in Folio, not elsewhere over MCP".into());
            }
        }
        "book" | "library" => {
            let root = project_root(s)?;
            if request["action"] != "new" || request["replace"] == true || !same_folder(Path::new(request["root"].as_str().unwrap_or("")), &root) {
                return Err(format!("a new {} is made in the project open in Folio and replaces nothing over MCP", request["cmd"].as_str().unwrap_or("")).into());
            }
            let boundary = Boundary::components(&root).map_err(|e| format!("The project cannot be used over MCP: {e}"))?;
            let checked = Checked { root: &root, boundary, used: Default::default() };
            checked.folder(request["folder"].as_str().unwrap_or(""))?;
        }
        "userSettings" => {
            let now = crate::attach::preferences();
            let asked = [&request["values"], &request["patch"]["preferences"], &request["patch"]];
            for (key, value) in asked.iter().filter_map(|v| v.as_object()).flatten() {
                if !key.starts_with(attached::MCP_SETTINGS) { continue; }
                let same = match value { Value::Bool(_) | Value::Null => attached::on(Some(value)) == attached::on(now.get(key)),
                    _ => value.as_str().unwrap_or("") == now[key].as_str().unwrap_or("") };
                if !same { return Err(format!("{key} is set by a person in Folio (Preferences ▸ General), not over MCP: the dialog was not applied").into()); }
            }
        }
        "documentPresets" => {}
        // File ▸ Package (mcp-c5): a new folder of the open project, whose parent is in the project and that does not exist yet
        "package" => {
            if !crate::attach::files_allowed() { return Err(attached::FILES_REFUSED.into()); }
            let root = project_root(s)?;
            let dir = PathBuf::from(request["dir"].as_str().unwrap_or(""));
            let inside = dir.parent().and_then(|p| p.canonicalize().ok()).zip(root.canonicalize().ok()).is_some_and(|(p, r)| p.starts_with(&r));
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !inside || dir.exists() || folio_project::paths::check_name(&name).is_err() {
                return Err("a package is made in a new folder of the project open in Folio, replacing nothing, over MCP".into());
            }
        }
        "packageCancel" => {}
        other => return Err(format!("an applied dialog would send {other}, which is not available to an attached MCP connection").into()),
    }
    Ok(())
}

/// Checks the paths of an attached request and puts this machine's paths where the app's command takes them. Returns the
/// project root the paths were relative to (replies show paths relative to it) with the paths to check again when the reply
/// comes, or None when the request takes no path.
pub fn resolve(req: &mut Value, s: Option<&crate::Session>) -> Result<Option<Resolved>, Refused> {
    let cmd = req["cmd"].as_str().unwrap_or("").to_owned();
    if matches!(cmd.as_str(), "openDialog" | "setDialogField" | "applyDialog") { return dialog(req, s); }
    if attached::reads::get(&cmd).is_some() {
        let input = attached::reads::input_file(&cmd);
        let root = match project_root(s) {
            Ok(root) => root,
            Err(e) if input.is_some() || cmd == "projectInfo" => return Err(e.into()),
            Err(_) => {
                if cmd == "pdfPresets" { req["action"] = json!("list"); }
                return Ok(None);
            }
        };
        let boundary = Boundary::components(&root).map_err(|e| e.to_string())?;
        let checked = Checked { root: &root, boundary, used: Default::default() };
        if let Some(input) = input {
            let (rel, file) = checked.existing(text(req, input.field)?, input.extensions)?;
            req[input.field] = if input.native { json!(file) } else { json!(rel.as_str()) };
        }
        match cmd.as_str() {
            "preflightProfileFile" => req["action"] = json!("import"),
            "pdfPresets" => { req["action"] = json!("list"); req["project"] = json!(&root); }
            "projectInfo" => req["path"] = json!(&root),
            _ => {}
        }
        let Checked { boundary, used, .. } = checked;
        return Ok(Some(Resolved { root, boundary, used: used.into_inner() }));
    }
    let edits: Vec<usize> = if cmd == "edit" {
        req["commands"].as_array().map(|list| list.iter().enumerate()
            .filter(|(_, c)| attached::path_tool(&format!("edit.{}", c["op"].as_str().unwrap_or(""))).is_some())
            .map(|(i, _)| i).collect()).unwrap_or_default()
    } else { Vec::new() };
    if cmd != "edit" && attached::path_tool(&cmd).is_none() { return Ok(None); }
    if cmd == "edit" && edits.is_empty() { return Ok(None); }
    let root = project_root(s)?;
    // the requested paths only, name by name (no walk over the whole project tree: mcp-c5)
    let boundary = Boundary::components(&root).map_err(|e| format!("The project cannot be used over MCP: {e}"))?;
    let checked = Checked { root: &root, boundary, used: Default::default() };
    match cmd.as_str() {
        "open" => {
            let (_, native) = checked.existing(text(req, "path")?, &["folio"])?;
            if let Some(s) = s && crate::edit::state(s)["dirty"] == true {
                return Err("The document open in Folio has unsaved changes: save it (save) or ask the person before opening another.".into());
            }
            req["path"] = json!(native.to_string_lossy());
        }
        "save" => {}
        "saveCopy" => { let path = checked.output(text(req, "path")?, "folio", false)?; req["path"] = json!(path.to_string_lossy()); }
        "exportIdml" => { let path = checked.output(text(req, "path")?, "idml", true)?; req["path"] = json!(path.to_string_lossy()); }
        "export" => { let path = checked.output(text(req, "path")?, "pdf", true)?; req["path"] = json!(path.to_string_lossy()); }
        "dataMergeCreate" => {
            let (rel, _) = checked.existing(text(req, "path")?, &["csv", "txt"])?;
            req["path"] = json!(rel.as_str());
            if let Some(name) = req.get("name").filter(|n| !n.is_null()) {
                let name = name.as_str().ok_or("name is the merged document's file name")?;
                let stem = name.trim_end_matches(".folio");
                if name.contains('/') || name.starts_with('@') || folio_project::paths::check_name(&format!("{stem}.folio")).is_err() {
                    return Err(format!("{name}: the merged document's name is one file name, usable on every OS").into());
                }
            }
            // a picture column that points outside the project leaves its frame empty (datamerge.rs)
            req["_projectImages"] = json!(true);
        }
        // File ▸ Import of an INDD / IDML already in the project, into the project open in the app (never another one)
        "importPlan" | "importRun" => {
            let (rel, native) = checked.existing(text(req, "source")?, &["indd", "indt", "idml"])?;
            let document = match req.get("document").and_then(Value::as_str).filter(|d| !d.trim().is_empty()) {
                Some(d) => d.to_owned(),
                // the app's own default (project.rs target_document): the source's name, at the project root
                None => {
                    let stem = Path::new(rel.file_name()).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
                    format!("{}.folio", folio_project::paths::fix_name(&stem))
                }
            };
            if cmd == "importRun" { checked.output(&document, "folio", false)?; } else {
                let target = checked.rel(&document)?;
                if extension(target.file_name()) != "folio" { return Err(format!("{document}: the document must end in .folio").into()); }
            }
            req["source"] = json!(native.to_string_lossy());
            req["project"] = json!(root.to_string_lossy());
            req["document"] = json!(checked.rel(&document)?.as_str());
            // links of the source that are outside the project are not copied in (they stay missing, with their address as a hint)
            req["_projectOnly"] = json!(true);
        }
        // File ▸ Package into a new folder of the project
        "package" | "packagePlan" => {
            let dir = checked.new_folder(text(req, "dir")?)?;
            req["dir"] = json!(dir.to_string_lossy());
        }
        // Load Styles…: the styles of a document of the project (the person's dialog reads the file the same way)
        "stylesFile" => { let (_, native) = checked.existing(text(req, "path")?, &["idml", "folio"])?; req["path"] = json!(native.to_string_lossy()); }
        "book" | "library" => book_or_library(req, &checked, &root)?,
        // stopping an import or a package takes no path
        "importCancel" | "packageCancel" => {}
        _ => {
            for i in edits {
                let command = &mut req["commands"][i];
                match command["op"].as_str().unwrap_or("") {
                    "placeGraphic" => { let (_, native) = checked.existing(text(command, "file")?, &[])?; command["file"] = json!(native.to_string_lossy()); }
                    "selectDataSource" => { let (rel, _) = checked.existing(text(command, "path")?, &["csv", "txt"])?; command["path"] = json!(rel.as_str()); }
                    "importPdfComments" => { let (rel, _) = checked.existing(text(command, "path")?, &["pdf"])?; command["path"] = json!(rel.as_str()); }
                    // the parts the dialog hands over, read here from the project file (`stylesFile` reads them the same way)
                    "loadStyles" => {
                        let (_, native) = checked.existing(text(command, "path")?, &["idml", "folio"])?;
                        let parts: std::collections::BTreeMap<String, String> = crate::edit::package_parts(&native)?.into_iter()
                            .filter(|(n, _)| matches!(n.as_str(), "Resources/Styles.xml" | "Resources/Graphic.xml")).collect();
                        let object = command.as_object_mut().ok_or("an edit command is a JSON object")?;
                        object.remove("path");
                        object.insert("parts".into(), json!(parts));
                    }
                    other => return Err(format!("edit.{other} takes no reviewed path").into()),
                }
            }
        }
    }
    let Checked { boundary, used, .. } = checked;
    Ok(Some(Resolved { root, boundary, used: used.into_inner() }))
}

/// What an attached client is told: paths below `root` relative to it (`/`), never this machine's folders.
pub fn relativize(value: &mut Value, root: &Path) {
    let prefix = root.to_string_lossy().into_owned();
    fn walk(value: &mut Value, prefix: &str) {
        match value {
            Value::String(text) => {
                if text == prefix { *text = ".".into(); }
                else if let Some(rest) = text.strip_prefix(prefix).filter(|r| r.starts_with(['/', '\\'])) {
                    *text = rest[1..].replace('\\', "/");
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, prefix)),
            Value::Object(map) => map.values_mut().for_each(|v| walk(v, prefix)),
            _ => {}
        }
    }
    if !prefix.is_empty() { walk(value, &prefix); }
}
