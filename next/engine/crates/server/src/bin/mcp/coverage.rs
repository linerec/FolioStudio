//! Coverage decisions use exact registry names. Unknown names require a new review.
use super::{attached, catalog};
use serde_json::{Value, json};

pub struct Reason { pub id: &'static str, pub text: &'static str }

pub const REASONS: &[Reason] = &[
    Reason { id: "secret", text: "Uses credentials or private user settings." },
    Reason { id: "printer", text: "Uses a printer or the system spooler." },
    Reason { id: "external-data", text: "Accesses data or file locations outside the attached project boundary." },
    Reason { id: "engine-process", text: "Controls the shared engine process, internal transactions or undo journal." },
    Reason { id: "gesture-only", text: "A value-based tool implements the same gesture." },
    Reason { id: "needs-session", text: "Requires an attached Folio app session." },
    Reason { id: "path-io", text: "Uses paths resolved by the attached project." },
    Reason { id: "app-state", text: "Requires the running app window." },
];

struct Hold {
    reason: &'static str,
    detail: &'static str,
    names: &'static [&'static str],
    value_tool: Option<&'static str>,
}
const HOLDS: &[Hold] = &[
    Hold { reason: "secret", detail: "These operations take credentials or access the personal credential store.",
        names: &["dbSaveConnection", "dbUnlockConnection", "dbTestConnection", "dbLockConnection", "dbCredentialStoreSelfTest"], value_tool: None },
    Hold { reason: "secret", detail: "Unfiltered user preferences contain private settings and control MCP permissions.",
        names: &["userSettings"], value_tool: None },
    Hold { reason: "printer", detail: "Uses device-specific presets or controls the system print spooler.",
        names: &["printPresets", "listPrinters", "printToDevice", "cancelPrintJob"], value_tool: None },
    Hold { reason: "engine-process", detail: "Controls or diagnoses the running engine, its lock, or the renderer acknowledgement channel.",
        names: &["selfCheck", "dockActionDone", "lockOverride", "close", "quit"], value_tool: None },
    Hold { reason: "engine-process", detail: "The raw dispatcher and shared preview transaction accept nested commands. Use individual named edit tools.",
        names: &["edit", "edit.previewStart", "edit.previewApply", "edit.previewCommit", "edit.previewCancel"], value_tool: None },
    Hold { reason: "engine-process", detail: "Discards entries from the shared undo journal, including recovery history belonging to the user.",
        names: &["historyClear", "historyDelete"], value_tool: None },
    Hold { reason: "external-data", detail: "Accesses saved database connections or browses external tables directly.",
        names: &["dbConnections", "dbRemoveConnection", "dbTables", "dbColumns", "dbPreview"], value_tool: None },
    Hold { reason: "external-data", detail: "Accepts a native destination outside the attached project path checks. Use the project-scoped dialog or export tools.",
        names: &["projectNewFolder", "projectCreate", "documentNew", "printingFile", "bookletFile", "dataMergeExportPdf", "historyNewDocument"], value_tool: None },
    Hold { reason: "external-data", detail: "Imports external document parts, resources or library assets with unvalidated references.",
        names: &["parentsFile", "edit.loadParentPages", "edit.syncBookResources", "edit.importTOCStyles", "edit.placeLibraryAsset", "edit.placeLibraryAssetInText", "edit.loadConditions", "edit.loadTrapPresets", "edit.importIndexTopics"], value_tool: None },
    Hold { reason: "external-data", detail: "Follows links to external source documents or reads linked graphic files at their original locations.",
        names: &["edit.linkPlaced", "edit.placeAndLink", "edit.updateContentLink", "edit.reconnectContentLinks", "edit.setClipping", "edit.setGraphicLayerVisibility", "edit.setGraphicLayerComp", "edit.setGraphicLayerUpdateLink", "edit.generateCaptions"], value_tool: None },
    Hold { reason: "external-data", detail: "Consumes external merge records and image references outside the checked data merge workflow.",
        names: &["edit.fillMergeRecord", "edit.updateMergedFields"], value_tool: None },
    Hold { reason: "gesture-only", detail: "The gap drag delegates its delta to edit.adjustGapKeys with resize and move_items both false.",
        names: &["edit.adjustGap"], value_tool: Some("edit.adjustGapKeys") },
];

fn hold(name: &str) -> Option<&'static Hold> { HOLDS.iter().find(|h| h.names.contains(&name)) }

#[derive(Debug, PartialEq, Eq)]
pub enum Status { Exposed, Withheld(&'static str), Unreviewed }

pub fn status(name: &str) -> Status {
    if catalog::ALLOWED.contains(&name) { return Status::Exposed; }
    if let Some(h) = hold(name) { return Status::Withheld(h.reason); }
    if attached::path_tool(name).is_some() { return Status::Withheld("path-io"); }
    if attached::DATABASE.contains(&name) { return Status::Withheld("external-data"); }
    if attached_status(name) == "exposed" { return Status::Withheld("needs-session"); }
    Status::Unreviewed
}

pub fn attached_status(name: &str) -> &'static str {
    if attached::DATABASE.contains(&name) { "needsDatabaseSetting" }
    else if let Some(tool) = attached::path_tool(name) { if tool.writes { "needsFileSetting" } else { "exposed" } }
    else if attached::reads::get(name).is_some() || attached::document_tools().any(|n| *n == name) { "exposed" }
    else if hold(name).is_some() { "withheld" }
    else { "unreviewed" }
}

pub fn why(reason: &str) -> &'static str {
    REASONS.iter().find(|r| r.id == reason).map_or("", |r| r.text)
}

/// The `coverage` tool: every registry name with its status, or the status of one name.
pub fn report(args: &Value) -> Result<Value, String> {
    let names = catalog::registry_names();
    if let Some(name) = args["name"].as_str() {
        if catalog::NATIVE.contains(&name) || name == "appConnection" {
            return Ok(json!({"ok":true,"type":"coverage","name":name,"status":"exposed","note":"MCP-only tool"}));
        }
        if !names.iter().any(|n| n == name) && (super::attached::legacy_tool(name).is_some() || matches!(name,"edit.placeGraphic" | "edit.relinkGraphic" | "edit.placeText")) {
            let reason = if super::attached::path_tool(name).is_some() { "path-io" } else { "needs-session" };
            return Ok(json!({"ok":true,"type":"coverage","name":name,"status":"withheld","reason":reason,"why":why(reason),
                "attached":attached_status(name),"note":"declared by folio-mcp for an attached connection"}));
        }
        if !names.iter().any(|n| n == name) { return Err("not a registered engine command or query".into()); }
        let mut reply = match status(name) {
            Status::Exposed => json!({"ok":true,"type":"coverage","name":name,"status":"exposed"}),
            Status::Withheld(reason) => json!({"ok":true,"type":"coverage","name":name,"status":"withheld","reason":reason,"why":why(reason)}),
            Status::Unreviewed => json!({"ok":true,"type":"coverage","name":name,"status":"unreviewed"}),
        };
        reply["attached"] = json!(attached_status(name));
        if let Some(h) = hold(name) {
            reply["attachedReason"] = json!(h.reason);
            reply["attachedWhy"] = json!(h.detail);
            if let Some(tool) = h.value_tool { reply["valueTool"] = json!(tool); }
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
        if let Some(h) = hold(name) { attached_groups.entry(h.reason).or_default().push(name); }
    }
    let attached_withheld: Vec<_> = attached_groups.into_iter().map(|(reason, names)|
        json!({"reason":reason,"why":why(reason),"count":names.len(),"names":names})).collect();
    // `status`/`withheld` are for a project server; `attached` is what folio-mcp --attach exposes on the running app
    Ok(json!({"ok":true,"type":"coverage","total":names.len(),"exposed":exposed,"mcpOnly":catalog::NATIVE,
        "withheld":withheld,"unreviewed":unreviewed,
        "attached":{"registryOpen":names.iter().filter(|n| matches!(attached_status(n), "exposed" | "needsDatabaseSetting" | "needsFileSetting")).collect::<Vec<_>>(),
            "unreviewed":names.iter().filter(|n| attached_status(n) == "unreviewed").collect::<Vec<_>>(),
            "reads":attached::reads::READS.iter().map(|r| r.name).collect::<Vec<_>>(),"withheld":attached_withheld,"exposed":attached::document_tools().copied().chain(super::attached::reads::READS.iter().map(|r| r.name)).collect::<Vec<_>>(),"needsDatabaseSetting":super::attached::DATABASE,
            "paths":super::attached::PATHS.iter().map(|t| json!({"name":t.name,"arguments":t.fields,"needsFileSetting":t.writes})).collect::<Vec<_>>(),
            "note":"folio-mcp --attach: tools act on the running app's engine session; dataMergeSourceStatus with check true also needs the database setting. Paths are relative to the project open in the app (MCP.ko.md path rules); tools that write files need the setting LLM may write files. See attached.withheld for reasons on a running app."}}))
}
