use serde_json::{Value, json};

/// The project-relative CSV or TXT as bytes (inside the project, 16 MiB at most).
pub(crate) fn read(project: &folio_project::Project, relative: &str) -> Result<Vec<u8>, String> {
    let path = folio_project::paths::checked_project_path(&project.root, relative)?;
    if !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("csv") || s.eq_ignore_ascii_case("txt")) {
        return Err("Select a CSV or TXT source".into());
    }
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err("Data source exceeds 16 MiB".into());
    }
    std::fs::read(&path).map_err(|e| e.to_string())
}

/// The data source read with the choices of the Data Source Import Options dialog (`delimiter` comma / tab / semicolon, `encoding`
/// ascii / unicode / shiftJis, `platform` pc / macintosh, `preserveSpaces`); with none of them as InDesign reads without the options.
pub(crate) fn parse_named(bytes: &[u8], delimiter: &str, encoding: &str, platform: &str, preserve: bool) -> Result<folio_idml::datamerge::Source, String> {
    if delimiter.is_empty() && encoding.is_empty() && platform.is_empty() && !preserve { return folio_idml::datamerge::parse(bytes); }
    folio_idml::datamerge::parse_with(bytes, &folio_idml::datamerge::choice(delimiter, encoding, platform, preserve)?)
}

/// The same, the choices from a request (`delimiter`, `encoding`, `platform`, `preserveSpaces`).
pub(crate) fn parse_request(bytes: &[u8], req: &Value) -> Result<folio_idml::datamerge::Source, String> {
    let text = |key: &str| req[key].as_str().unwrap_or("");
    parse_named(bytes, text("delimiter"), text("encoding"), text("platform"), req["preserveSpaces"].as_bool().unwrap_or(false))
}

/// `dataMergeSource {path, delimiter?, encoding?, platform?, preserveSpaces?}`: the project file read as a data source; the reply
/// holds the options that were applied (`source.options`). Without a path: the data source the document remembers.
pub fn source(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let project = &session.and_then(|s| s.folio.as_ref()).ok_or("Open a project document")?.opened.project;
    // no path: the data source the document remembers (datamerge_link.rs)
    let Some(relative) = req["path"].as_str() else { return crate::datamerge_expr::linked(req, session) };
    let bytes = read(project, relative)?;
    let source = parse_request(&bytes, req)?;
    // the computed fields of the document are fields of this source too (datamerge_expr.rs)
    let (source, computed) = crate::datamerge_expr::attach(source, req, session)?;
    Ok(json!({"type":"dataMergeSource", "path":relative,
        "sha256":folio_project::hash::sha256_bytes(&bytes), "source":source, "computed":computed}))
}

/// `dataMergeCreate {path, delimiter?, encoding?, platform?, preserveSpaces?, linkImages?, range?, multiple?, arrange?, rowSpacing?, columnSpacing?, margins?, removeBlankLines?, name?}`:
/// Data Merge ▸ Create Merged Document. The open document, as it is now, merged with the project-relative data
/// source and written as a new `.folio` next to it under a document id of its own; the open document stays as it is.
/// Reply: `{ok, type: "mergedDocument", documentId, path, relative, pages, missing}`.
pub fn create(req: &Value, s: &crate::Session) -> Value {
    merge_into(req, s, None)
}

/// `dataMergeExportPdf {path, …the options of dataMergeCreate, output, …the PDF export settings of `export`}`: Data Merge ▸ Export to PDF
/// (build-c). The document merged with the data source goes straight to the project-relative PDF `output`: the merged document is only
/// written to the project cache (`.folio/cache/datamerge/`, deleted again), so no new document appears in the project. Pictures the
/// records name are brought into the project as Create Merged Document does. Reply `{ok, type: "mergedPdf", path, pages, missing, …}`.
pub fn export_pdf(req: &Value, s: &crate::Session, profiles: &std::sync::Arc<crate::Profiles>) -> Value {
    let fail = |code: &str, message: String| json!({"ok": false, "code": code, "error": message});
    let f = match crate::project::folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let root = f.opened.project.root.clone();
    let Some(output) = req["output"].as_str().filter(|o| std::path::Path::new(o).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))) else {
        return fail("invalid", "Project-relative PDF path (output) required".into());
    };
    let destination = match folio_project::paths::checked_project_path(&root, output) {
        Ok(d) => d,
        Err(e) => return fail("invalid", e),
    };
    let cache = root.join(".folio").join("cache").join("datamerge");
    if let Err(e) = std::fs::create_dir_all(&cache) { return fail("io", e.to_string()); }
    let temp = cache.join(format!("{}.folio", folio_project::util::uuid_v4()));
    let mut reply = merge_into(req, s, Some(&temp));
    if reply["ok"] != true {
        let _ = std::fs::remove_file(&temp);
        return reply;
    }
    let result = (|| -> Result<(usize, f64, Value), String> {
        let fonts = crate::Fonts { startup: std::sync::Mutex::new(None), spare: std::sync::Mutex::new(None) };
        let session = crate::load_session(temp.to_str().ok_or("invalid cache path")?, false, &fonts, profiles, &|_, _| {})?;
        let (exported, pages, ms) = crate::render_pdf(&session, req, &|_| {})?;
        crate::print::write_atomically(&destination, &exported.pdf)?;
        Ok((pages, ms, crate::print::export_view(&exported.report, std::path::Path::new(output), pages, ms)))
    })();
    let _ = std::fs::remove_file(&temp);
    match result {
        Ok((pages, _, report)) => {
            reply["type"] = json!("mergedPdf");
            reply["path"] = json!(output);
            reply["pages"] = json!(pages);
            reply["report"] = report;
            for key in ["documentId", "relative"] { if let Some(m) = reply.as_object_mut() { m.remove(key); } }
            reply
        }
        Err(e) => fail("export", e),
    }
}

/// `dataMergeUpdateSource {}`: Data Merge ▸ Update Content in Data Fields of a merged document (build-c2): the data source it
/// remembers read again the way the merge read it, with the document's computed fields: `{fields, records, sha256, path, state,
/// changed}` (`state` ok, or moved when the file is found elsewhere in the project by its SHA-256). The screen then sends the edit
/// command `updateMergedFields {fields, records, sha256, at}` (one undo step).
pub fn update_source(req: &Value, s: Option<&crate::Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let f = s.folio.as_ref().ok_or("Open a project document")?;
    let merged = s.edit.as_ref().ok_or("The document cannot be edited")?.lock().expect("shared state must remain unpoisoned after each operation").editor.merged_document()
        .ok_or("notMerged: this document was not made by Create Merged Document")?;
    let src = &merged["source"];
    let mut path = src["path"].as_str().ok_or("the merged document names no data source")?.to_owned();
    let root = &f.opened.project.root;
    let mut state = "ok";
    let exists = folio_project::paths::checked_project_path(root, &path).is_ok_and(|p| p.is_file());
    if !exists {
        // the file the merge read, found elsewhere in the project by its content (principle 1)
        let found = match (src["sha256"].as_str(), src["bytes"].as_u64()) {
            (Some(sha), Some(size)) => folio_project::hash::HashIndex::load(root).find_by_hash(sha, size),
            _ => None,
        };
        let found = found.ok_or_else(|| format!("missing: the data source {path} is not in the project"))?;
        path = found.as_str().to_owned();
        state = "moved";
    }
    let bytes = read(&f.opened.project, &path)?;
    let text = |key: &str| src[key].as_str().unwrap_or("");
    let source = parse_named(&bytes, text("delimiter"), text("encoding"), text("platform"), src["preserveSpaces"] == true)?;
    let computed = crate::datamerge_expr::computed_of(req, Some(s))?;
    let source = folio_idml::datamerge_expr::apply(&source, &computed).source;
    let sha = folio_project::hash::sha256_bytes(&bytes);
    Ok(json!({"type": "dataMergeUpdateSource", "path": path, "state": state, "sha256": sha, "changed": src["sha256"] != sha.as_str(),
        "fields": source.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(), "records": source.records}))
}

/// `dataMergeLog {}`: Data Merge ▸ Show Log of Update Results: what the merged document remembers (template, source) and its
/// update log (each field updated or left alone, and a summary per update), oldest first.
pub fn update_log(s: Option<&crate::Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let merged = s.edit.as_ref().ok_or("The document cannot be edited")?.lock().expect("shared state must remain unpoisoned after each operation").editor.merged_document()
        .ok_or("notMerged: this document was not made by Create Merged Document")?;
    Ok(json!({"type": "dataMergeLog", "template": merged["template"], "source": merged["source"], "updatedAt": merged["updatedAt"], "log": merged["log"]}))
}

/// The template (the open document's file) and the data source of a merge, project-relative with their SHA-256, and the reading
/// options of the source: kept in the merged document (`folio.datamerge` label, key `merged`).
fn link_of(req: &Value, f: &crate::project::FolioSession, source: &str) -> Value {
    let root = &f.opened.project.root;
    let template = folio_project::paths::relative_to(&root.to_string_lossy(), &f.opened.file.to_string_lossy()).ok().map(|r| r.to_string());
    let hash = |p: &std::path::Path| std::fs::read(p).ok().map(|b| folio_project::hash::sha256_bytes(&b));
    let source_file = folio_project::paths::checked_project_path(root, source).ok();
    let text = |key: &str| req[key].as_str().unwrap_or("");
    json!({
        "template": {"path": template, "sha256": hash(&f.opened.file)},
        "source": {"path": source, "sha256": source_file.as_deref().and_then(hash), "bytes": source_file.as_deref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()),
            "delimiter": text("delimiter"), "encoding": text("encoding"),
            "platform": text("platform"), "preserveSpaces": req["preserveSpaces"].as_bool().unwrap_or(false)},
        "removeBlankLines": req["removeBlankLines"].as_bool().unwrap_or(false),
    })
}

fn merge_into(req: &Value, s: &crate::Session, into: Option<&std::path::Path>) -> Value {
    let fail = |code: &str, message: String| json!({"ok": false, "code": code, "error": message});
    let f = match crate::project::folio_of(s) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Some(shared) = &s.edit else { return fail("notEditable", "The document cannot be edited".into()) };
    let Some(relative) = req["path"].as_str() else { return fail("invalid", "Project-relative CSV or TXT path required".into()) };
    let source = match read(&f.opened.project, relative).and_then(|b| parse_request(&b, req)) {
        Ok(source) => source,
        Err(e) => return fail("source", e),
    };
    let number = |key: &str, default: f64| req[key].as_f64().unwrap_or(default);
    let margins = req["margins"].as_array().filter(|m| m.len() == 4).map_or([36.0; 4], |m| std::array::from_fn(|i| m[i].as_f64().unwrap_or(36.0)));
    let fitting = match req["fitting"].as_str().unwrap_or("proportional") {
        f @ ("proportional" | "fill" | "contentToFrame" | "frameToContent" | "none") => f.to_owned(),
        _ => return fail("invalid", "fitting is proportional, fill, contentToFrame, frameToContent or none".into()),
    };
    let range = req["range"].as_str().unwrap_or("").to_owned();
    // the computed fields (the document's, or the request's own list) are fields of the source like its columns
    let applied = match crate::datamerge_expr::computed_of(req, Some(s)) {
        Ok(computed) => folio_idml::datamerge_expr::apply(&source, &computed),
        Err(e) => return fail("invalid", e),
    };
    let (computed_errors, computed_error_count, computed_definition_errors) = (applied.record_errors, applied.error_count, applied.definition_errors);
    let source = applied.source;
    // the pictures the chosen records name, brought into the project (principle 1): a path is relative to the
    // data source's folder (InDesign's rule, measured); a file that is not there leaves its frame empty
    let mut prepared = crate::place::Prepared::default();
    let mut graphics = std::collections::BTreeMap::new();
    // "Link Images" off (`linkImages: false`): the pictures are embedded, their bytes go into the document (the file in the project stays what it links to)
    let link_images = req["linkImages"].as_bool().unwrap_or(true);
    let mut embed = std::collections::BTreeMap::new();
    let mut missing: Vec<String> = Vec::new();
    {
        let es = match crate::edit::lock_for_change(shared) {
            Ok(es) => es,
            Err(refusal) => return refusal,
        };
        let picked = match folio_edit::data_merge_selection(&range, source.records.len()) {
            Ok(p) => p,
            Err(e) => return fail(e.code, e.message),
        };
        let folder = std::path::Path::new(relative).parent().map(|p| f.opened.project.root.join(p)).unwrap_or_else(|| f.opened.project.root.clone());
        for (column, field) in source.fields.iter().enumerate().filter(|(_, field)| field.kind == "image") {
            let _ = field;
            for &record in &picked {
                let value = source.records[record].get(column).map(String::as_str).unwrap_or("");
                if value.is_empty() || graphics.contains_key(value) || missing.iter().any(|m| m == value) { continue; }
                let file = if std::path::Path::new(value).is_absolute() { std::path::PathBuf::from(value) } else { folder.join(value.replace('\\', "/")) };
                // an attached MCP client (attach_paths.rs): a picture outside the project is not read, its frame stays empty
                let outside = req["_projectImages"] == true && !file.canonicalize().is_ok_and(|p| f.opened.project.root.canonicalize().is_ok_and(|r| p.starts_with(r)));
                let source_file = (|| -> Result<folio_edit::GraphicSource, String> {
                    if outside || !file.is_file() { return Err("missing".into()); }
                    let info = folio_links::probe::probe(&file).map_err(|e| e.to_string())?;
                    // a PDF is placed as InDesign's merge places it (measured, `datamerge-c4-u`): page 1, cropped to the content of the visible
                    // layers, transparent background; the same for a PDF of many pages
                    let pdf = (!info.page_boxes.is_empty()).then_some(folio_links::pdfplace::Options { page_number: 1, crop: folio_links::pdfplace::Crop::CropContentVisibleLayers, transparent_background: true });
                    let plain = folio_edit::GraphicSource::read_with_pdf(&file, folio_project::paths::file_uri(&file), pdf.clone())?;
                    let (reference, path) = crate::place::import_link(&file, plain.extension(), f, &es, &mut prepared)?;
                    if !link_images {
                        // the document holds the file's bytes: a very large picture is refused (as Links ▸ Embed is)
                        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 128 << 20 { return Err("pictures over 128 MB cannot be embedded".into()); }
                        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                        embed.insert(value.to_owned(), folio_project::idmlparts::base64_lines(&bytes));
                    }
                    let placed = folio_edit::GraphicSource::read_with_pdf(&path, format!("{}{}", folio_project::format::REF_SCHEME, reference.id), pdf)?;
                    crate::pdfplace::prepare(&path, &placed, Some(f))?;
                    Ok(placed)
                })();
                match source_file {
                    Ok(g) => { graphics.insert(value.to_owned(), g); }
                    Err(_) => missing.push(value.to_owned()),
                }
            }
        }
    }
    let command: folio_edit::Command = folio_edit::commands::RegisteredCommand::MergeRecords {
        fields: source.fields.iter().map(|f| f.name.clone()).collect(),
        records: source.records.clone(),
        range,
        multiple: req["multiple"].as_bool().unwrap_or(false),
        arrange: req["arrange"].as_str().unwrap_or("rows").to_owned(),
        row_spacing: number("rowSpacing", 0.0),
        column_spacing: number("columnSpacing", 0.0),
        margins,
        remove_blank_lines: req["removeBlankLines"].as_bool().unwrap_or(false),
        fitting,
        center_image: req["centerImage"].as_bool().unwrap_or(false),
        graphics,
        embed: (!link_images).then_some(embed),
        // what the merged document remembers for Update Content in Data Fields (build-c2): the template and the data source by
        // project-relative path and SHA-256, and how the source is read
        link: Some(link_of(req, f, relative).to_string()),
    }.into();
    let (merged, pages) = {
        let es = match crate::edit::lock_for_change(shared) {
            Ok(es) => es,
            Err(refusal) => return refusal,
        };
        let mut copy = folio_edit::Editor::new(es.editor.parts().clone());
        if let Err(e) = copy.apply("Create merged document", &[command]) {
            return fail(e.code, e.message);
        }
        let changes = copy.touched().iter().map(|p| (p.clone(), copy.part(p).map(str::to_owned))).collect();
        let pages = copy.part("designmap.xml").map_or(0, |t| t.matches("<idPkg:Spread ").count());
        ((changes, copy.recompose().clone(), prepared.take_references()), pages)
    };
    let root = &f.opened.project.root;
    let directory = f.opened.file.parent().unwrap_or(root).to_path_buf();
    let stem = req["name"].as_str().filter(|n| !n.trim().is_empty()).map(str::to_owned).unwrap_or_else(|| {
        format!("{} merged", f.opened.file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into()))
    });
    let stem = folio_project::paths::fix_name(stem.trim_end_matches(".folio"));
    let mut target = directory.join(format!("{stem}.folio"));
    let mut n = 2;
    while into.is_none() && target.exists() {
        target = directory.join(format!("{stem} {n}.folio"));
        n += 1;
    }
    if let Some(into) = into { target = into.to_path_buf(); }
    let id = folio_project::util::uuid_v4();
    let mut reply = crate::project::write_to(s, f, &target, Some(id.clone()), Some(merged));
    if reply["ok"] == true {
        let relative = folio_project::paths::relative_to(&root.to_string_lossy(), &target.to_string_lossy()).ok();
        reply["type"] = json!("mergedDocument");
        reply["documentId"] = json!(id);
        reply["relative"] = json!(relative.map(|r| r.to_string()));
        reply["pages"] = json!(pages);
        reply["missing"] = json!(missing);
        // the merge report: the records whose computed field failed (their cells are empty), and the definitions that are wrong
        reply["computedErrors"] = json!(computed_errors);
        reply["computedErrorCount"] = json!(computed_error_count);
        reply["computedDefinitionErrors"] = json!(computed_definition_errors);
    }
    reply
}
