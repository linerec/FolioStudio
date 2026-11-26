//! What a connection attached to the running Folio app may ask its engine. One list, two users: the engine checks every
//! attached request with `admit` (it never trusts the client), and `folio-mcp --attach` lists the same names as tools.
//! The engine includes this file by path (`src/main.rs`), so it must not refer to the other MCP modules.
#![allow(dead_code)]
use serde_json::Value;

#[path = "reads.rs"]
pub mod reads;

/// User settings (`preferences`), both off by default: Preferences ▸ General.
pub const ALLOW_CONNECTION: &str = "folio.mcp.allowConnection";
pub const ALLOW_DATABASE_REFRESH: &str = "folio.mcp.allowDatabaseRefresh";
/// In the app's user data folder while the endpoint is open: `{version, endpoint, token, pid}`.
pub const CONNECTION_FILE: &str = "mcp-attach.json";
pub const PROTOCOL: u64 = 1;

/// Registry names (`commands.generated.json`) an attached connection may call. They act on the engine session the
/// screen shows: the open document, its selection and current page, the common dialogs, panels and recent list.
/// Edits are the reviewed ones: arguments are ids and values inside the open document, never a path or another file.
pub const ATTACHED: &[&str] = &[
    // the open document, its history, selection and page
    "documentInfo", "pageItems", "editState", "undo", "redo", "viewState", "setSelection", "goToPage",
    // the common dialogs (next/shared/dialogs.json)
    "dialogState", "openDialog", "setDialogField", "applyDialog", "cancelDialog",
    // panels and the recent list
    "panelState", "setPanelState", "workspaceLayout", "setWorkspaceLayout", "recentFiles", "setRecentFiles", "rememberRecent",
    "rendererPreferences",
    // the dock's layout commands (uix-d2): move, join, split, float, pop out, close, open; workspaces saved, loaded, reset
    "dockAction",
    // data merge: project files and the document's own source (a database check needs ALLOW_DATABASE_REFRESH)
    "dataMergeSource", "dataMergeEvaluate", "dataMergeSourceStatus", "dataMergeUpdateSource", "dataMergeLog",
    // hyperlinks: the URL status check (uses the network; not blocked, said in its description)
    "hyperlinkUrlStatus",
    // reviewed edits
    "edit.createByDrag", "edit.createPolygon", "edit.createInView", "edit.scaleAbout", "edit.shearAbout",
    "edit.rotateByDrag", "edit.resizeByHandle", "edit.stepRepeat", "edit.setObjectAttributes", "edit.formatFrameText",
    "edit.setFrameFitting", "edit.clearFrameFitting", "edit.joinPaths", "edit.pathfinder", "edit.createOutlines",
    "edit.typeText", "edit.insertNote", "edit.deleteNote", "edit.createCondition", "edit.setConditionVisible",
    "edit.setPageSize", "edit.movePage", "edit.insertDataMergeTextField", "edit.insertDataMergeImageField",
    "edit.insertDataMergeQrField", "edit.setDataMergeComputedFields", "edit.setDataMergeRefreshMode",
    "edit.removeDataSource", "edit.mergeRecords",
];

/// Reads a database through a connection a person unlocked in the app (the password stays in the app). Only with
/// ALLOW_DATABASE_REFRESH on; `dataMergeSourceStatus` with `check: true` is one too.
pub const DATABASE: &[&str] = &["edit.dataMergeSelectDatabase", "edit.dataMergeRefresh"];

/// User setting, off by default (Preferences ▸ General ▸ LLM may write files): saving, exporting and placing over MCP.
pub const ALLOW_FILE_WRITES: &str = "folio.mcp.allowFileWrites";

/// A command or edit (`edit.<op>`) that takes a project path (MCP.ko.md "경로를 받는 도구와 경로 규칙"): the only arguments an
/// attached client may give it, and whether it writes a file (then ALLOW_FILE_WRITES must be on). The engine checks and turns
/// the paths into this machine's (`src/attach_paths.rs`); arguments that would skip that check (`source` read before, a print
/// job, preset settings) are not among them.
pub struct PathTool {
    pub name: &'static str,
    pub fields: &'static [&'static str],
    pub writes: bool,
}

pub const PATHS: &[PathTool] = &[
    PathTool { name: "open", fields: &["path"], writes: false },
    PathTool { name: "save", fields: &[], writes: true },
    PathTool { name: "saveCopy", fields: &["path"], writes: true },
    PathTool { name: "exportIdml", fields: &["path", "contents"], writes: true },
    PathTool { name: "export", fields: &["path", "preset", "bleed", "spreads", "marks", "markOffset", "markWeight", "customBleed", "pages"], writes: true },
    PathTool { name: "dataMergeCreate", fields: &["path", "delimiter", "encoding", "platform", "preserveSpaces", "margins", "fitting", "range",
        "linkImages", "multiple", "arrange", "rowSpacing", "columnSpacing", "removeBlankLines", "centerImage", "name"], writes: true },
    // not in the engine registry (its argument there is the prepared `source`): folio-mcp declares this tool itself
    PathTool { name: "edit.placeGraphic", fields: &["file", "id", "page", "point", "rect", "layer"], writes: true },
    PathTool { name: "edit.selectDataSource", fields: &["path", "delimiter", "encoding", "platform", "preserve_spaces"], writes: false },
    PathTool { name: "edit.importPdfComments", fields: &["path", "again"], writes: false },
    // mcp-c5: File ▸ Import of an INDD / IDML of the project into it (links outside the project are not copied in)
    PathTool { name: "importPlan", fields: &["source", "document"], writes: false },
    PathTool { name: "importRun", fields: &["source", "document"], writes: true },
    PathTool { name: "importCancel", fields: &[], writes: false },
    // File ▸ Package into a new folder of the project (no search folders or protected roots of this machine)
    PathTool { name: "packagePlan", fields: &["dir", "fonts", "links", "pdf", "cjkFonts", "instructions", "jobs"], writes: false },
    PathTool { name: "package", fields: &["dir", "fonts", "links", "pdf", "cjkFonts", "instructions", "jobs"], writes: true },
    PathTool { name: "packageCancel", fields: &[], writes: false },
    // Load Styles…: a .folio / .idml of the project (folio-mcp declares edit.loadStyles: path instead of the dialog's parts)
    PathTool { name: "stylesFile", fields: &["path", "kinds"], writes: false },
    PathTool { name: "edit.loadStyles", fields: &["path", "kinds", "clash", "only"], writes: false },
    // a book / library file of the project: open (read), make (new, create), and (mcp-c6) a book's documents added, its styles
    // synchronized, the book exported to one PDF; a library's asset stored from the open document, placed in it. The ones that
    // write a file are BOOK_WRITES / LIBRARY_WRITES. The PDF fields are `export`'s (no preset settings object, print job or booklet).
    PathTool { name: "book", fields: &["action", "path", "folder", "fileName", "name", "document", "output",
        "preset", "bleed", "spreads", "marks", "markOffset", "markWeight", "customBleed", "pages"], writes: false },
    PathTool { name: "library", fields: &["action", "path", "folder", "fileName", "name", "search", "sort",
        "ids", "description", "label", "kind", "asset", "page", "story", "start", "end"], writes: false },
];

/// `book` actions an attached client may ask, and those of them that write a file (ALLOW_FILE_WRITES). Moving, removing,
/// relinking, page numbering and chapter options, preflight, cross-references and packaging a book stay with the person.
pub const BOOK_ACTIONS: &[&str] = &["read", "new", "create", "add", "synchronize", "exportPdf"];
pub const BOOK_WRITES: &[&str] = &["new", "create", "add", "synchronize", "exportPdf"];
/// `library` actions (mcp-c6: `store` an item from the open document, `place` one in it). Removing, updating, labels and
/// relinking stay with the person. `place` copies the item's pictures into the project like Place, so it needs the setting too.
pub const LIBRARY_ACTIONS: &[&str] = &["read", "new", "create", "store", "place"];
pub const LIBRARY_WRITES: &[&str] = &["new", "create", "store", "place"];

/// The actions of `book` or `library` an attached client may ask, and those that write a file.
pub fn file_actions(cmd: &str) -> (&'static [&'static str], &'static [&'static str]) {
    if cmd == "library" { (LIBRARY_ACTIONS, LIBRARY_WRITES) } else { (BOOK_ACTIONS, BOOK_WRITES) }
}

/// User setting (Preferences ▸ General): the folder of this machine an attached client may make new projects in (File ▸ New ▸
/// Project…, the `newProject` dialog). Empty or missing: no new project over MCP (the refusal says where to set it). Only a person
/// sets it: an attached client may not change any `folio.mcp.*` setting (`MCP_SETTINGS`).
pub const NEW_PROJECT_FOLDER: &str = "folio.mcp.newProjectFolder";
/// The settings that say what an attached client may do: never changed over MCP (`admit` and attach_paths.rs check the dialogs).
pub const MCP_SETTINGS: &str = "folio.mcp.";

/// App dialogs (`"scope": "app"`) an attached client may open and apply: New Document, New Book and New Library in the open
/// project, New Project in the folder a person chose, the user name and the language, and saving a document preset. The engine
/// fills their path fields itself (`APP_DIALOG_PATHS`); an attached client may not set them. Other app dialogs stay with the person.
pub const APP_DIALOGS: &[&str] = &["newDocument", "newBook", "newLibrary", "newProject", "user", "language", "documentPreset"];
/// The app dialogs that make a file or folder (they need ALLOW_FILE_WRITES) and the path fields the engine fills in them (the open
/// project, the new-project folder).
pub const APP_DIALOG_FILES: &[&str] = &["newDocument", "newBook", "newLibrary", "newProject"];
pub const APP_DIALOG_PATHS: &[&str] = &["project", "parent"];

/// The `folio.mcp.*` key named in a dialog request (`openDialog` values, `setDialogField` field), if any.
fn mcp_setting(request: &Value) -> Option<String> {
    let field = request["field"].as_str().filter(|f| f.starts_with(MCP_SETTINGS)).map(str::to_owned);
    field.or_else(|| request["values"].as_object().and_then(|v| v.keys().find(|k| k.starts_with(MCP_SETTINGS)).cloned()))
}

pub fn path_tool(name: &str) -> Option<&'static PathTool> { PATHS.iter().find(|t| t.name == name) }

fn path_fields(tool: &PathTool, object: &serde_json::Map<String, Value>, fixed: &[&str]) -> Result<(), String> {
    match object.keys().find(|k| !fixed.contains(&k.as_str()) && !tool.fields.contains(&k.as_str())) {
        Some(key) => Err(format!("{} argument {key} is not available to an attached MCP connection", tool.name)),
        None => Ok(()),
    }
}

/// A preference value that means "on" (the screen writes booleans; older values are strings).
pub fn on(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(on)) => *on,
        Some(Value::String(text)) => text == "1" || text == "true",
        _ => false,
    }
}

/// A key that looks like it carries a secret. MCP refuses such arguments for every tool (passwords are entered in the app).
pub fn is_secret_name(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["password", "passwd", "passphrase", "secret", "credential", "apikey", "api_key", "accesstoken", "access_token", "bearer"]
        .iter().any(|word| key.contains(word))
}

/// The path of the first secret-looking key in `value` (object keys at any depth), without its value.
pub fn find_secret(value: &Value) -> Option<String> {
    fn walk(value: &Value, path: &str) -> Option<String> {
        match value {
            Value::Object(map) => map.iter().find_map(|(key, inner)| {
                let here = format!("{path}.{key}");
                if is_secret_name(key) { Some(here) } else { walk(inner, &here) }
            }),
            Value::Array(items) => items.iter().enumerate().find_map(|(i, inner)| walk(inner, &format!("{path}[{i}]"))),
            _ => None,
        }
    }
    walk(value, "arguments")
}

pub const FILES_REFUSED: &str = "Saving, exporting and placing files over MCP needs Preferences ▸ General ▸ LLM may write files \
    turned on in Folio.";

/// Why an attached connection may not send `request` (an engine request line), or Ok. `database`: ALLOW_DATABASE_REFRESH is on;
/// `files`: ALLOW_FILE_WRITES is on. The paths themselves are checked against the open project by the engine (attach_paths.rs).
pub fn admit(request: &Value, database: bool, files: bool) -> Result<(), String> {
    if let Some(key) = find_secret(request) {
        return Err(format!("Secrets are not accepted over MCP ({key}). Enter passwords in the Folio app."));
    }
    let object = request.as_object().ok_or("a request is a JSON object")?;
    let cmd = request["cmd"].as_str().ok_or("cmd required")?;
    let database_refused = || Err("Database sources are refreshed over MCP only when Preferences ▸ General ▸ \
        LLM may refresh database sources is on in Folio.".to_string());
    if cmd == "edit" {
        if let Some(key) = object.keys().find(|k| !matches!(k.as_str(), "id" | "cmd" | "label" | "commands" | "selectionChanged")) {
            return Err(format!("edit field {key} is not available to an attached MCP connection"));
        }
        let commands = request["commands"].as_array().filter(|c| !c.is_empty()).ok_or("edit needs a list of commands")?;
        for command in commands {
            let op = command["op"].as_str().ok_or("every edit command needs op")?;
            let name = format!("edit.{op}");
            if DATABASE.contains(&name.as_str()) {
                if !database { return database_refused(); }
            } else if let Some(tool) = path_tool(&name) {
                path_fields(tool, command.as_object().ok_or("an edit command is a JSON object")?, &["op"])?;
                if tool.writes && !files { return Err(FILES_REFUSED.into()); }
            } else if !ATTACHED.contains(&name.as_str()) {
                return Err(format!("{name} is not available to an attached MCP connection"));
            }
        }
        return Ok(());
    }
    if let Some(tool) = path_tool(cmd).filter(|t| !t.name.starts_with("edit.")) {
        path_fields(tool, object, &["id", "cmd"])?;
        if tool.writes && !files { return Err(FILES_REFUSED.into()); }
        if matches!(cmd, "book" | "library") {
            let action = request["action"].as_str().unwrap_or("");
            let (actions, writes) = file_actions(cmd);
            if !actions.contains(&action) {
                return Err(format!("{cmd} action {action} is not available to an attached MCP connection (only {})", actions.join(", ")));
            }
            if writes.contains(&action) && !files { return Err(FILES_REFUSED.into()); }
        }
        return Ok(());
    }
    if let Some(read) = reads::get(cmd) {
        for key in object.keys().map(String::as_str) {
            if matches!(key, "id" | "cmd") || read.fields.contains(&key) { continue; }
            if cmd != "render" && matches!(key, "resultOffset" | "resultLimit") {
                let n = request[key].as_u64().ok_or("pagination expects unsigned integers")?;
                if key == "resultLimit" && !(1..=32768).contains(&n) {
                    return Err("resultLimit must be between 1 and 32768".into());
                }
                continue;
            }
            return Err(format!("{cmd} argument {key} is not available for a read"));
        }
        return Ok(());
    }
    if !ATTACHED.contains(&cmd) || cmd.starts_with("edit.") {
        return Err(format!("{cmd} is not available to an attached MCP connection"));
    }
    // what an attached client may do is the person's choice: Preferences' folio.mcp.* fields are not set over MCP (mcp-c6)
    if matches!(cmd, "openDialog" | "setDialogField") && let Some(key) = mcp_setting(request) {
        return Err(format!("{key} is set by a person in Folio (Preferences ▸ General), not over MCP"));
    }
    if cmd == "dataMergeSourceStatus" && request["check"].as_bool() == Some(true) && !database {
        return database_refused();
    }
    Ok(())
}

/// The history label of an edit an attached client made: the Undo menu says it came from the LLM.
pub fn llm_label(label: &str) -> String {
    if label.starts_with("LLM: ") { label.to_owned() } else { format!("LLM: {label}") }
}
