//! Which engine commands and queries the MCP server exposes, and why the others are withheld.
//!
//! Every name in the generated command registry (`commands.generated.json`) must be reviewed here: either it is exposed
//! (`catalog::ALLOWED`) or a rule gives the reason it is withheld. The test `every_registry_name_is_reviewed` fails for a name
//! no rule covers, so a new command or query cannot appear (or be forgotten) without a decision. To review a new name, add it
//! to a rule below (exact name, or a prefix ending in `*`); the first matching rule wins, so keep specific rules above general ones.
use super::catalog;
use serde_json::{Value, json};

pub struct Reason {
    pub id: &'static str,
    pub text: &'static str,
}

pub const REASONS: &[Reason] = &[
    Reason { id: "secret", text: "Carries a password or credential. Secrets never travel over MCP: a person enters them in the Folio app (database passwords are unlocked on screen)." },
    Reason { id: "external-data", text: "Reaches a database server or other data outside the project. The MCP project boundary does not allow that yet, and a password unlocked in the app is not shared with this process." },
    Reason { id: "printer", text: "Sends data to a printer or the print spooler. Never available over MCP." },
    Reason { id: "app-state", text: "State of the Folio app window (dialogs, panels, recent list, selection, view, user settings). A project server has no window: attach to the running app (folio-mcp --attach) for the dialogs, panels, recent list, selection and page, or pass the target as arguments to the edit tools." },
    Reason { id: "path-io", text: "Reads or writes files by path, or imports, exports, packages or creates projects. The MCP project boundary has no reviewed path rule for it yet." },
    Reason { id: "engine-process", text: "Controls or diagnoses the engine process itself. It is not needed to operate a document over MCP." },
    Reason { id: "needs-session", text: "Reads the document through the full engine session (composed layout, fonts, renderer, links). The first-pass MCP server parses the package only." },
    Reason { id: "edit-unreviewed", text: "A document edit not reviewed for MCP yet. Each edit needs a path and side-effect review first; the first pass exposes only edit.createByDrag." },
];

/// (pattern, reason id): an exact name, or a prefix ending in `*`. The first match wins.
const RULES: &[(&str, &str)] = &[
    // Passwords: no MCP tool takes one (see `catalog::find_secret`).
    ("dbSaveConnection", "secret"), ("dbUnlockConnection", "secret"), ("dbTestConnection", "secret"),
    // Database and data-merge sources outside the project.
    ("db*", "external-data"), ("dataMergeSourceStatus", "external-data"), ("hyperlinkUrlStatus", "external-data"),
    ("edit.dataMergeSelectDatabase", "external-data"), ("edit.dataMergeRefresh", "external-data"),
    ("listPrinters", "printer"), ("printToDevice", "printer"), ("cancelPrintJob", "printer"),
    // The Folio app window.
    ("dialogState", "app-state"), ("openDialog", "app-state"), ("setDialogField", "app-state"), ("applyDialog", "app-state"),
    ("cancelDialog", "app-state"), ("dialogPreview", "app-state"), ("recentFiles", "app-state"), ("setRecentFiles", "app-state"), ("rememberRecent", "app-state"),
    ("panelState", "app-state"), ("setPanelState", "app-state"), ("workspaceLayout", "app-state"), ("setWorkspaceLayout", "app-state"), ("userSettings", "app-state"), ("viewState", "app-state"),
    ("rendererPreferences", "app-state"),
    // the dock's layout commands (uix-d2): the screen applies them, so only an attached connection has them
    ("dockAction", "app-state"), ("dockActionDone", "app-state"),
    ("setViewState", "app-state"), ("setSelection", "app-state"), ("setTextSelection", "app-state"), ("goToPage", "app-state"),
    ("setZoom", "app-state"), ("setTool", "app-state"), ("setScrollCenter", "app-state"), ("uiColors", "app-state"),
    ("pageSizePresets", "app-state"), ("drawingSettings", "app-state"), ("flattenerSettings", "app-state"), ("proofSetup", "app-state"),
    ("printPresets", "app-state"), ("layoutAdjustSettings", "app-state"), ("spellUserDictionary", "app-state"), ("spellAutoCorrect", "app-state"),
    ("spellLanguages", "app-state"),
    // File > Document Presets: the user's presets in the app settings (appdlg-c2)
    ("documentPresets", "app-state"),
    // Glyphs panel ▸ glyph sets: the person's sets in the app settings (build-c)
    ("glyphSets", "app-state"),
    // Files, import, export, packaging, projects.
    ("package", "path-io"), ("packagePlan", "path-io"), ("export", "path-io"), ("importPlan", "path-io"), ("importRun", "path-io"),
    ("projectCreate", "path-io"), ("projectNewFolder", "path-io"), ("documentNew", "path-io"), ("newDocumentFonts", "path-io"), ("stylesFile", "path-io"),
    ("parentsFile", "path-io"), ("strokeStylesFile", "path-io"), ("preflightProfileFile", "path-io"), ("printingFile", "path-io"),
    ("bookletFile", "path-io"), ("tocImportStyles", "path-io"), ("textImportInfo", "path-io"), ("placeInfo", "path-io"),
    ("library", "path-io"), ("book", "path-io"), ("pdfComments", "path-io"), ("pdfCommentsDoc", "path-io"), ("pdfPresets", "path-io"),
    ("pdfCommentMarks", "needs-session"),
    ("dataMergeCreate", "path-io"), ("colorProfiles", "path-io"), ("conditionsFile", "path-io"), ("trapPresetsFile", "path-io"),
    ("fileMetadata", "path-io"),
    // The engine process.
    ("selfCheck", "engine-process"), ("debugDigest", "engine-process"), ("oracleStep", "engine-process"), ("knownLimits", "engine-process"),
    ("describeCommands", "engine-process"), ("quit", "engine-process"), ("close", "engine-process"), ("save", "engine-process"),
    ("lockOverride", "engine-process"), ("projectStatus", "engine-process"), ("projectInfo", "engine-process"), ("importCancel", "engine-process"),
    ("packageCancel", "engine-process"), ("conveyor", "engine-process"), ("edit", "engine-process"),
    // Reads that need the composed document.
    ("render", "needs-session"), ("info", "needs-session"), ("inks", "needs-session"), ("inkAt", "needs-session"), ("links", "needs-session"),
    ("preflight", "needs-session"), ("text*", "needs-session"), ("table*", "needs-session"), ("styleInfo", "needs-session"),
    ("styleShortcut", "needs-session"), ("pageItems", "needs-session"), ("selectionSize", "needs-session"), ("effects", "needs-session"),
    ("copy", "needs-session"), ("labels", "needs-session"), ("dataMerge*", "needs-session"), ("spell*", "needs-session"), ("glyph*", "needs-session"),
    ("fontGlyphs", "needs-session"), ("storyTables", "needs-session"), ("arrangeBy*", "needs-session"), ("xlinkSources", "needs-session"),
    ("flattenerPreview", "needs-session"), ("proofColors", "needs-session"), ("printingPlan", "needs-session"), ("bookletPlan", "needs-session"),
    ("strokeStyles", "needs-session"), ("preflightProfiles", "needs-session"), ("preflightSettings", "needs-session"),
    ("styleOverrideMarks", "needs-session"), ("swatchUsage", "needs-session"), ("convertToProfilePlan", "needs-session"),
    ("captionSetup", "needs-session"), ("captionRange", "needs-session"), ("numberingLists", "needs-session"),
    ("crossReferenceTargets", "needs-session"), ("mixedInkGroupPreview", "needs-session"), ("indexes", "needs-session"),
    ("trapPresets", "needs-session"), ("pageRange", "needs-session"), ("conditions", "needs-session"), ("anchoredObject", "needs-session"),
    ("anchoredDefaults", "needs-session"), ("hyperlinkTarget", "needs-session"), ("opticalMargin", "needs-session"), ("colorPick", "needs-session"),
    ("mathmlLayout", "needs-session"), ("mathExpressions", "needs-session"), ("tocStory", "needs-session"),
    // Document edits (the one reviewed edit is exposed through `catalog::ALLOWED`).
    // History panel (build-c): Delete one step and Clear History change the undo record; New Document writes a file
    ("historyDelete", "edit-unreviewed"), ("historyClear", "edit-unreviewed"), ("historyNewDocument", "path-io"),
    ("edit.*", "edit-unreviewed"),
];

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Exposed,
    Withheld(&'static str),
    Unreviewed,
}

fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

pub fn status(name: &str) -> Status {
    if catalog::ALLOWED.contains(&name) { return Status::Exposed; }
    RULES.iter().find(|(pattern, _)| matches(pattern, name)).map_or(Status::Unreviewed, |(_, reason)| Status::Withheld(reason))
}

/// On a connection attached to the running app (`folio-mcp --attach`): exposed, exposed while the database setting is on,
/// or withheld (the project-server reason still applies).
pub fn attached_status(name: &str) -> &'static str {
    if super::attached::DATABASE.contains(&name) { "needsDatabaseSetting" }
    else if let Some(tool) = super::attached::path_tool(name) { if tool.writes { "needsFileSetting" } else { "exposed" } }
    else if super::attached::reads::get(name).is_some() || super::attached::ATTACHED.contains(&name) { "exposed" }
    else { "withheld" }
}

fn attached_reason(name: &str) -> Option<&'static str> {
    if attached_status(name) != "withheld" { return None; }
    Some(match name {
        "dataMergeExportPdf" => "file-write",
        "parentsFile" => "external-link",
        "printPresets" => "printer",
        "userSettings" => "private-settings",
        "arrangeByColumns" | "arrangeByRows" | "textCursor" | "copy" => "changes-state",
        _ => match status(name) { Status::Withheld(id) => id, _ => "unreviewed" },
    })
}

pub fn why(reason: &str) -> &'static str {
    match reason {
        "file-write" => return "Produces a new PDF on disk; it needs a reviewed output path and the file-write setting.",
        "external-link" => return "Parent imports can resolve linked files outside the open project.",
        "private-settings" => return "Arbitrary personal preferences can contain credentials and private paths.",
        "changes-state" => return "Changes the clipboard, window arrangement or insertion formatting.",
        _ => {}
    }
    REASONS.iter().find(|r| r.id == reason).map_or("", |r| r.text)
}

/// The `coverage` tool: every registry name with its status, or the status of one name.
pub fn report(args: &Value) -> Result<Value, String> {
    let names = catalog::registry_names();
    if let Some(name) = args["name"].as_str() {
        if catalog::NATIVE.contains(&name) || name == "appConnection" {
            return Ok(json!({"ok":true,"type":"coverage","name":name,"status":"exposed","note":"MCP-only tool"}));
        }
        if name == "edit.placeGraphic" && !names.iter().any(|n| n == name) {
            return Ok(json!({"ok":true,"type":"coverage","name":name,"status":"withheld","reason":"path-io","why":why("path-io"),
                "attached":attached_status(name),"note":"declared by folio-mcp for an attached connection (the edit command placeGraphic)"}));
        }
        if !names.iter().any(|n| n == name) { return Err("not a registered engine command or query".into()); }
        let mut reply = match status(name) {
            Status::Exposed => json!({"ok":true,"type":"coverage","name":name,"status":"exposed"}),
            Status::Withheld(reason) => json!({"ok":true,"type":"coverage","name":name,"status":"withheld","reason":reason,"why":why(reason)}),
            Status::Unreviewed => json!({"ok":true,"type":"coverage","name":name,"status":"unreviewed"}),
        };
        reply["attached"] = json!(attached_status(name));
        if let Some(reason) = attached_reason(name) {
            reply["attachedReason"] = json!(reason);
            reply["attachedWhy"] = json!(why(reason));
        }
        return Ok(reply);
    }
    let mut exposed = Vec::new();
    let mut unreviewed = Vec::new();
    let mut groups: Vec<(&str, Vec<&str>)> = REASONS.iter().map(|r| (r.id, Vec::new())).collect();
    for name in &names {
        match status(name) {
            Status::Exposed => exposed.push(name.as_str()),
            Status::Unreviewed => unreviewed.push(name.as_str()),
            Status::Withheld(reason) => groups.iter_mut().find(|(id, _)| *id == reason).expect("rule names a known reason").1.push(name.as_str()),
        }
    }
    let withheld: Vec<Value> = groups.into_iter().filter(|(_, list)| !list.is_empty())
        .map(|(id, list)| json!({"reason":id,"why":why(id),"count":list.len(),"names":list})).collect();
    let mut attached_groups = std::collections::BTreeMap::<_, Vec<_>>::new();
    for name in &names {
        if let Some(reason) = attached_reason(name) { attached_groups.entry(reason).or_default().push(name); }
    }
    let attached_withheld: Vec<_> = attached_groups.into_iter().map(|(reason, names)|
        json!({"reason":reason,"why":why(reason),"count":names.len(),"names":names})).collect();
    // `status`/`withheld` are for a project server; `attached` is what folio-mcp --attach exposes on the running app
    Ok(json!({"ok":true,"type":"coverage","total":names.len(),"exposed":exposed,"mcpOnly":catalog::NATIVE,
        "withheld":withheld,"unreviewed":unreviewed,
        "attached":{"withheld":attached_withheld,"exposed":super::attached::ATTACHED.iter().copied().chain(super::attached::reads::READS.iter().map(|r| r.name)).collect::<Vec<_>>(),"needsDatabaseSetting":super::attached::DATABASE,
            "paths":super::attached::PATHS.iter().map(|t| json!({"name":t.name,"arguments":t.fields,"needsFileSetting":t.writes})).collect::<Vec<_>>(),
            "note":"folio-mcp --attach: tools act on the running app's engine session; dataMergeSourceStatus with check true also needs the database setting. Paths are relative to the project open in the app (MCP.ko.md path rules); tools that write files need the setting LLM may write files. See attached.withheld for reasons on a running app."}}))
}
