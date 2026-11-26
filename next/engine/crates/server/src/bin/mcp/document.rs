use super::boundary::Boundary;
use folio_project::format;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Engine {
    boundary: Boundary,
    session: Option<Session>,
}

struct Session {
    relative: String,
    source: PathBuf,
    fingerprint: (String, u64),
    editor: folio_edit::Editor,
    editable: bool,
    opened: Option<folio_project::open::Opened>,
}

impl Engine {
    pub fn new(root: &Path) -> Result<Self, String> {
        let boundary = Boundary::new(root)?;
        folio_project::manifest::load(boundary.root()).map_err(|e| e.to_string())?;
        Ok(Self { boundary, session: None })
    }

    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        // the requested paths, Folio's own folders and the project marker: no walk over the whole tree (mcp-c6)
        self.boundary.own_folders()?;
        self.boundary.existing("folio.project")?;
        if name == "open" { return self.open(args); }
        let session = self.session.as_mut().ok_or("open a document first")?;
        self.boundary.existing(&session.relative)?;
        // the files the open .folio refers to are read again by saveCopy and exportIdml: still below the root, through no link
        if let Some(opened) = &session.opened {
            for reference in opened.manifest.files() { self.boundary.path(reference.path.as_ref().expect("files() have a path").as_str())?; }
        }
        if folio_project::hash::sha256_file(&session.source).map_err(|e| e.to_string())? != session.fingerprint {
            return Err("source changed on disk; reopen before continuing".into());
        }
        match name {
            "documentInfo" => {
                let doc = session.document()?;
                shown("documentInfo", "info", &doc, args)
            }
            "editState" => Ok(session.state()),
            "saveCopy" => session.save_copy(&self.boundary, args["path"].as_str().unwrap()),
            "exportIdml" => {
                let target = self.boundary.output(args["path"].as_str().unwrap(), "idml")?;
                let opened = session.opened.as_ref().ok_or("saveCopy and open the .folio before exporting IDML")?;
                let contents = match args["contents"].as_str().unwrap_or("all") {
                    "all" => true, "none" => false, _ => return Err("contents must be all or none".into()),
                };
                let report = folio_project::export::export_idml_with(opened, &target, 0,
                    folio_project::export::ExportOptions { contents, contents_raster_only: false }, Some(&session.changes()))
                    .map_err(|e| e.to_string())?;
                Ok(json!({"ok":true,"type":"exportIdml","path":args["path"],"report":report}))
            }
            "undo" | "redo" | "edit.createByDrag" => {
                if !session.editable { return Err("open with edit: true first".into()); }
                let outcome = if name == "undo" { session.editor.undo() }
                    else if name == "redo" { session.editor.redo() }
                    else {
                        let mut command = args.clone();
                        command["op"] = json!("createByDrag");
                        let command = serde_json::from_value(command).map_err(|e| e.to_string())?;
                        Some(session.editor.apply("MCP createByDrag", &[command]).map_err(|e| e.to_string())?)
                    };
                let mut result = session.state();
                if let Some(outcome) = outcome {
                    result["created"] = json!(outcome.created);
                    result["parts"] = json!(outcome.parts);
                }
                Ok(result)
            }
            _ => Err("disabled tool".into()),
        }
    }

    fn open(&mut self, args: &Value) -> Result<Value, String> {
        let relative = args["path"].as_str().ok_or("path required")?;
        let source = self.boundary.existing(relative)?;
        let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if extension != "folio" && extension != "idml" { return Err("only .folio and .idml are supported; import INDD in Folio first".into()); }
        let fingerprint = folio_project::hash::sha256_file(&source).map_err(|e| e.to_string())?;
        let opened = if extension == "folio" {
            let (manifest, _) = format::read_manifest(&source).map_err(|e| e.to_string())?;
            for reference in manifest.files() {
                self.boundary.path(reference.path.as_ref().unwrap().as_str())?;
            }
            let opened = folio_project::open::resolve(&source).map_err(|e| e.to_string())?;
            if opened.project.root.canonicalize().map_err(|e| e.to_string())? != self.boundary.root() {
                return Err("nested projects are not supported".into());
            }
            Some(opened)
        } else { None };
        let mut parts = BTreeMap::new();
        folio_project::import::for_each_part(&source, &mut |name, text| { parts.insert(name.into(), text.into()); })?;
        let session = Session { relative: relative.into(), source, fingerprint, editor: folio_edit::Editor::new(parts),
            editable: args["edit"].as_bool().unwrap_or(false), opened };
        let doc = session.document()?;
        self.session = Some(session);
        shown("opened", "document", &doc, args)
    }
}

/// The document as the client asked for it: a compact summary (default) or the complete model (`detail: "full"`).
fn shown(kind: &str, key: &str, doc: &folio_model::Document, args: &Value) -> Result<Value, String> {
    match args["detail"].as_str().unwrap_or("summary") {
        "full" => Ok(json!({"ok":true,"type":kind,key:doc})),
        "summary" => {
            let model = serde_json::to_value(doc).map_err(|e| e.to_string())?;
            Ok(json!({"ok":true,"type":kind,"summary":super::summary::summarize(&model)}))
        }
        _ => Err("detail must be summary or full".into()),
    }
}

impl Session {
    fn document(&self) -> Result<folio_model::Document, String> {
        // Parse only package XML. No font scan, URI resolver, rendering or external sidecar.
        folio_idml::read_parts(self.editor.parts(), Path::new(&self.relative), &folio_idml::ReaderData::default())
            .map_err(|e| e.to_string())
    }

    fn state(&self) -> Value {
        json!({"ok":true,"type":"editState","rev":self.editor.rev(),"undo":self.editor.undo_label(),
            "redo":self.editor.redo_label(),"dirty":self.editor.version()!=0})
    }

    fn changes(&self) -> BTreeMap<String, Option<String>> {
        self.editor.touched().iter().map(|name| (name.clone(), self.editor.part(name).map(str::to_owned))).collect()
    }

    fn save_copy(&self, boundary: &Boundary, raw: &str) -> Result<Value, String> {
        let target = boundary.output(raw, "folio")?;
        let project = folio_project::manifest::load(boundary.root()).map_err(|e| e.to_string())?;
        let Some(opened) = &self.opened else {
            return super::import::save(&project, boundary, &self.source, &self.fingerprint, &self.document()?, &self.changes(), raw);
        };
        let source = self.source.clone();
        let mut manifest = opened.manifest.clone();
        manifest.id = folio_project::util::uuid_v4();
        manifest.created = folio_project::util::now_rfc3339();
        manifest.saved = manifest.created.clone();
        let label = json!({"id":manifest.id,"project":project.id(),"format":format::VERSION}).to_string();
        let changes = self.changes();
        let mut action = |name: &str, read: &mut dyn FnMut() -> std::io::Result<String>| {
            if name == "designmap.xml" {
                let text = changes.get(name).and_then(|s| s.clone()).map(Ok).unwrap_or_else(read)?;
                let mut edits = folio_project::idmlparts::Edits { document_labels: vec![(format::LABEL_DOCUMENT.into(), label.clone())], ..Default::default() };
                let (out, _) = folio_project::idmlparts::rewrite(name, &text, &mut edits);
                return Ok(format::PartAction::Replace(out.unwrap_or(text)));
            }
            Ok(match changes.get(name) {
                Some(Some(text)) => format::PartAction::Replace(text.clone()),
                Some(None) => format::PartAction::Drop, None => format::PartAction::Keep,
            })
        };
        let saved = format::read_part(&source, format::SAVED_LINES_PART).map_err(|e| e.to_string())?;
        let report = format::read_part(&source, format::REPORT_PART).map_err(|e| e.to_string())?;
        let added: Vec<_> = changes.iter().filter_map(|(n,t)| t.as_deref().map(|t| (n.as_str(),t))).collect();
        let bytes = format::write_folio_with_parts(&target, &source, &mut action, &manifest, saved.as_deref(), report.as_deref(), &added)
            .map_err(|e| e.to_string())?;
        Ok(json!({"ok":true,"type":"savedCopy","path":raw,"bytes":bytes}))
    }
}
