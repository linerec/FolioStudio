use super::{attached, catalog};
use folio_ai::tools::{Gate, Risk};
use serde_json::Value;
use std::sync::OnceLock;

pub struct Attached;

impl Gate for Attached {
    fn catalog(&self) -> &[Value] {
        static TOOLS: OnceLock<Vec<Value>> = OnceLock::new();
        TOOLS.get_or_init(|| {
            catalog::attached_tools()
                .into_iter()
                .filter(|t| !matches!(t["name"].as_str(), Some("coverage" | "appConnection")))
                .collect()
        })
    }
    fn validate(&self, args: &Value, schema: &Value) -> Result<(), String> {
        catalog::validate(args, schema)
    }
    fn risk(&self, name: &str, args: &Value) -> Risk {
        if !self.catalog().iter().any(|t| t["name"] == name) {
            return Risk::Withheld;
        }
        reviewed_risk(name, args)
    }
}

/// Classify a name after membership in the attached registry has been checked.
pub(super) fn reviewed_risk(name: &str, args: &Value) -> Risk {
    if attached::DATABASE.contains(&name)
        || matches!(name, "dataMergeUpdateSource" | "hyperlinkUrlStatus")
        || name == "dataMergeSourceStatus" && args["check"] == true
    {
        return Risk::Database;
    }
    if matches!(name, "book" | "library") {
        return if args["action"] == "read" {
            Risk::Read
        } else {
            Risk::File
        };
    }
    if attached::path_tool(name).is_some_and(|t| t.writes)
        || matches!(
            name,
            "applyDialog"
                | "rendererPreferences"
                | "dockAction"
                | "setPanelState"
                | "setWorkspaceLayout"
                | "setRecentFiles"
                | "rememberRecent"
        )
    {
        return Risk::File;
    }
    if name.starts_with("edit.") || matches!(name, "open" | "importCancel" | "packageCancel") {
        return Risk::Edit;
    }
    if attached::reads::get(name).is_some()
        || attached::path_tool(name).is_some()
        || matches!(
            name,
            "documentInfo"
                | "pageItems"
                | "editState"
                | "viewState"
                | "panelState"
                | "workspaceLayout"
                | "dialogState"
                | "recentFiles"
                | "dataMergeSource"
                | "dataMergeEvaluate"
                | "dataMergeSourceStatus"
                | "dataMergeLog"
                | "copy"
        )
    {
        return Risk::Read;
    }
    Risk::Edit
}

/// Revalidate at the engine boundary, including the exact envelope the MCP endpoint admits.
pub fn prepare(name: &str, args: &Value, database: bool, files: bool) -> Result<Value, String> {
    let tool = Attached
        .catalog()
        .iter()
        .find(|t| t["name"] == name)
        .ok_or("tool is withheld")?;
    match Attached.risk(name, args) {
        Risk::File if !files => return Err("file permission required".into()),
        Risk::Database if !database => return Err("external access permission required".into()),
        _ => {}
    }
    Attached.validate(args, &tool["inputSchema"])?;
    let req = catalog::request_for(name, args)?;
    attached::admit(&req, database, files)?;
    Ok(req)
}
