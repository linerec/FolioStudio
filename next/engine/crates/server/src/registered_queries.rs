//! Small query registry for features introduced after the legacy request dispatcher.
use serde_json::{Value, json};

pub fn document_fields(s: &crate::Session, info: &mut Value) {
    info["strokeStyles"] = json!(s.doc.stroke_styles);
    info["preflightProfiles"] = json!(s.doc.preflight_profiles);
    info["preflightSettings"] = json!(s.doc.preflight_settings);
    info["mixedInkGroups"] = json!(s.doc.print.mixed_ink_groups);
    info["tocStyles"] = json!(s.doc.toc_styles);
    info["hyperlinks"] = json!(s.doc.hyperlinks);
    info["tocStories"] = json!(s.doc.toc_stories);
    if let Some(edit) = &s.edit {
        for (kind, field) in [("paragraph", "paragraphStyles"), ("character", "characterStyles"), ("table", "tableStyles"), ("cell", "cellStyles")] {
            if let Ok(used) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.used_text_styles(kind)
                && let Some(rows) = info[field].as_array_mut() {
                    for row in rows { row["used"] = json!(row["id"].as_str().is_some_and(|id| used.contains(id))); }
                }
        }
        // the styles that have a keyboard shortcut, the one that wins a key press first (rules/stylekeys.ko.md)
        if let Ok(rows) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.style_shortcuts() {
            info["styleShortcuts"] = json!(rows);
        }
        if let Ok(options) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.footnote_options() {
            info["footnoteOptions"] = json!(options);
        }
        if let Ok(options) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.endnote_options() {
            info["endnoteOptions"] = json!(options);
        }
        if let Ok(prefs) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.document_preferences() {
            info["documentPreferences"] = json!(prefs);
        }
        if let Ok(metadata) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.file_info() {
            info["fileInfo"] = json!(metadata);
        }
        if let Ok(colors) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.color_preferences() {
            info["colorPreferences"] = json!(colors);
        }
        if let Ok(languages) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.language_quotes() {
            info["languageQuotes"] = json!(languages);
        }
        info["languageChoices"] = json!(folio_edit::text::languages::choices());
        info["colorGroups"] = json!(folio_edit::color_groups(edit.lock().expect("shared state must remain unpoisoned after each operation").editor.parts()));
        // Index panel ▸ Sort Options: the language groups in priority order (setIndexSortOptions changes them)
        info["indexSortOptions"] = json!(folio_edit::index_sort_options(edit.lock().expect("shared state must remain unpoisoned after each operation").editor.parts()));
        // the content links of the Content Placer (Create Link) with their status; the commands updateContentLink / unlinkContentLink take `link`
        info["contentLinks"] = json!(crate::xlink::content_links(s, &edit.lock().expect("shared state must remain unpoisoned after each operation")));
        // Data Merge: the data source the document remembers (its project reference; the panel reads the file by the data merge query)
        info["dataMergeSource"] = json!(edit.lock().expect("shared state must remain unpoisoned after each operation").editor.data_source().map(|(file, kind)| json!({"file": file, "fileType": kind})));
        // Data Merge computed fields (the panel reads them again when they change: set, undo, redo)
        info["dataMergeComputed"] = json!(edit.lock().expect("shared state must remain unpoisoned after each operation").editor.data_merge_computed());
        // a merged document (build-c2): its template and data source, the last update and how many log entries (Update Content in Data Fields)
        info["dataMergeMerged"] = json!(edit.lock().expect("shared state must remain unpoisoned after each operation").editor.merged_document().map(|m| json!({
            "template": m["template"]["path"], "source": m["source"]["path"], "sha256": m["source"]["sha256"], "updatedAt": m["updatedAt"],
            "log": m["log"].as_array().map_or(0, Vec::len)})));
        let (primary, empty) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.parent_text_frames();
        info["parentTextFrames"] = json!({"primary": primary, "empty": empty});
        if let Ok(setup) = edit.lock().expect("shared state must remain unpoisoned after each operation").editor.caption_setup() {
            info["captionSetup"] = setup;
        }
        // the fill and stroke the next object drawn gets (the toolbox chips show them when nothing is selected; setDefaultFillStroke sets them)
        if let (Some(d), Some(index)) = (edit.lock().expect("shared state must remain unpoisoned after each operation").editor.default_fill_stroke(), s.inspect.get()) {
            info["defaultFillStroke"] = json!({
                "fill": crate::inspect::paint_value(&s.doc, &s.colors, index, &d.fill, d.fill_tint),
                "stroke": crate::inspect::paint_value(&s.doc, &s.colors, index, &d.stroke, d.stroke_tint),
            });
        }
    }
}
type Query = fn(&Value, Option<&crate::Session>, &crate::Profiles) -> Result<Value, String>;
// Descriptions and legacy dispatch metadata: registered_queries.catalog.tsv.
// Generated with next/tools/gen-command-docs.py; parsing remains in each handler.
const QUERIES: &[(&str, Query)] = &[
    ("dialogState", |r,s,_|crate::dialog_state::run(r,s)),
    ("openDialog", |r,s,p|crate::dialog_state::run_with(r,s,Some(p))),
    ("setDialogField", |r,s,_|crate::dialog_state::run(r,s)),
    ("applyDialog", |r,s,_|crate::dialog_state::run(r,s)),
    ("cancelDialog", |r,s,_|crate::dialog_state::run(r,s)),
    ("dialogPreview", |r,s,_|crate::dialog_state::run(r,s)),

    ("recentFiles", |r,_,_|crate::recent_files::run(r)),
    ("setRecentFiles", |r,_,_|crate::recent_files::run(r)),
    ("rememberRecent", |r,_,_|crate::recent_files::run(r)),
    ("panelState", |r,_,_|crate::panel_state::run(r)),
    ("setPanelState", |r,_,_|crate::panel_state::run(r)),
    // Story Editor table view (rules/arrange-c1.ko.md): the tables of a story with their cells in display order; the order of one table
    ("storyTables", |r,s,_| crate::table_view::query(r,s)),
    ("arrangeByRows", |r,s,_| crate::table_view::arrange(r,s,crate::table_view::Order::Rows)),
    ("arrangeByColumns", |r,s,_| crate::table_view::arrange(r,s,crate::table_view::Order::Columns)),
    // content links to other documents and what they can be joined to (conveyor-c5, xlink.rs)
    ("xlinkSources", |_,s,_|crate::xlink::sources(s)),
    ("drawingSettings", |r,_,_|crate::drawing_settings::run(r)),
    ("flattenerPreview", |r,s,_|crate::flatpreview::report(r,s)),
    ("flattenerSettings", |r,_,_|crate::flatpreview::settings(r)),
    ("proofSetup", crate::proof_setup::query),
    ("proofColors", crate::proof_setup::colors),
    ("printingPlan", |r,s,_|crate::printing::plan(r,s)),
    ("printingFile", |r,s,_|crate::printing::file(r,s)),
    ("bookletPlan", |r,s,_|crate::printing::booklet(r,s)),
    ("bookletFile", |r,s,_|crate::printing::booklet_file(r,s)),
    ("printPresets", |r,_,_|crate::printing::presets(r)),
    ("strokeStylesFile",crate::stroke_files::run),
    ("strokeStyles",|_,s,_|Ok(json!({"styles":s.ok_or("no document")?.doc.stroke_styles}))),
    ("preflightProfiles", |_,s,_|Ok(json!({"state":s.ok_or("no document")?.doc.preflight_profiles,"schema":folio_preflight::profiles::schema()}))),
    ("preflightProfileFile", crate::preflight_files::run),
    ("preflightSettings", |_,s,_|{let d=&s.ok_or("no document")?.doc;Ok(json!({"state":d.preflight_settings,"profiles":d.preflight_profiles}))}),
    ("styleOverrideMarks", |_,s,_|crate::text_edit::overrides::query(s.ok_or("no document open")?)),
    ("oracleStep", |r,_,p|oracle_step(r,p)),
    ("textImportInfo", |r,s,_|crate::place::text_info(r,s)),
    ("tocImportStyles", |r,_,_|crate::toc::import_styles(r)),
    ("tocStory", |r,s,_|crate::toc::story(r,s)),
    ("colorProfiles", |r,_,p|color_profiles(r,p)),
    ("swatchUsage", |_,s,_|swatch_usage(s)),
    ("convertToProfilePlan", |r,s,_| {
        // Edit > Convert to Profile as a trial: the swatches that would change (the same arguments as the command convertToProfile)
        let state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        let text = |k: &str| r[k].as_str().unwrap_or_default().to_owned();
        state.editor.convert_to_profile_plan(&text("rgbProfile"), &text("cmykProfile"), &text("intent"), r["blackPointCompensation"].as_bool().unwrap_or(true)).map_err(|e|e.to_string())
    }),
    ("stylesFile", |r,s,_|styles_file(r,s)),
    ("parentsFile", |r,_,_|crate::parent_links::file(r)),
    ("captionSetup", |_,s,_| {
        let state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        Ok(json!({"setup":state.editor.caption_setup().map_err(|e|e.to_string())?}))
    }),
    ("numberingLists", |_,s,_| {
        let state=s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        Ok(json!({"lists":state.editor.numbering_lists().map_err(|e|e.to_string())?}))
    }),
    ("captionRange", |r,s,_| {
        let mut state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        let story = r["story"].as_str().ok_or("story required")?;
        let start = r["start"].as_u64().ok_or("start required")? as usize;
        let end = r["end"].as_u64().ok_or("end required")? as usize;
        Ok(json!({"live":state.editor.caption_range(story,start,end).map_err(|e|e.to_string())?}))
    }),
    ("crossReferenceTargets", |_,s,_| {
        let state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        Ok(json!({"targets":state.editor.cross_reference_targets().map_err(|e|e.to_string())?}))
    }),
    ("mixedInkGroupPreview", |r,_,_| {
        let values = serde_json::from_value(r["values"].clone()).map_err(|e| e.to_string())?;
        Ok(json!({"swatches":folio_edit::mixedink_groups::preview(&values).map_err(|e|e.to_string())?}))
    }),
    ("textPathItems", |r,s,_| {
        let state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        Ok(json!({"ids":state.editor.text_path_items_for_story(r["story"].as_str()).map_err(|e|e.to_string())?}))
    }),
    ("indexes", |_,s,_| Ok(json!({"indexes":s.ok_or("no document open")?.doc.indexes}))),
    ("trapPresets", |_,s,_| Ok(json!({"traps":s.ok_or("no document open")?.doc.traps}))),
    ("pageRange", |r,s,_| Ok(json!({"pages":folio_pdf::printing::page_range(&s.ok_or("no document open")?.doc,r["range"].as_str().ok_or("range required")?)?}))),
    ("conditions", |_,s,_| {
        let session=s.ok_or("no document open")?;
        if let Some(edit)=session.edit.as_ref() {
            let state=edit.lock().expect("shared state must remain unpoisoned after each operation");
            let (conditions,indicators)=state.editor.condition_state().map_err(|e|e.to_string())?;
            // the condition sets and the set in use, and which conditions the text uses (build-c: sets, Select All Unused)
            let (sets,active)=state.editor.condition_sets().map_err(|e|e.to_string())?;
            return Ok(json!({"conditions":conditions,"indicators":indicators,"colors":&folio_model::UI_COLORS[..],"sets":sets,"activeSet":active,"usage":state.editor.condition_usage()}));
        }
        Ok(json!({"conditions":session.doc.conditions,"indicators":"SHOW_INDICATORS","colors":&folio_model::UI_COLORS[..],"sets":[],"activeSet":"n","usage":{}}))
    }),
    // Conditional Text panel ▸ Load Conditions (and Sets) (build-c): a project document's conditions and sets, and the part `loadConditions` takes
    ("conditionsFile", |r,s,_| conditions_file(r,s)),
    // Trap Presets panel ▸ Load Trap Presets (build-c): a project document's trap presets and the part `loadTrapPresets` takes
    ("trapPresetsFile", |r,s,_| trap_presets_file(r,s)),
    ("fontGlyphs", |r,s,_| crate::glyphs::query(r,s)),
    ("glyphFind", |r,s,_| crate::glyphs::find(r,s)),
    ("glyphAt", |r,s,_| crate::glyphs::at(r,s)),
    ("glyphAlternates", |r,s,_| crate::glyphs::alternates(r,s)),
    ("library", crate::library::run),
    ("book", crate::book::run),
    ("pdfComments", |r,s,_| crate::pdfcomments::query(r,s)),
    ("pdfCommentsDoc", |r,s,_| crate::pdfcomments::document(r,s)),
    ("pdfCommentMarks", |r,s,_| crate::pdfcomments::marks(r,s)),
    ("anchoredObject", |r,s,_| {
        let mut state=s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        let object=if let Some(id)=r["object"].as_str(){Some(state.editor.anchored_object(id).map_err(|e|e.to_string())?)}
        else {state.editor.anchored_at(r["story"].as_str().ok_or("story required")?,r["at"].as_u64().ok_or("at required")? as usize).map_err(|e|e.to_string())?};
        Ok(json!({"object":object}))
    }),
    ("anchoredDefaults", |_,s,_| {
        let state=s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        Ok(json!({"defaults":state.editor.anchored_defaults().map_err(|e|e.to_string())?}))
    }),
    ("hyperlinkTarget", |r,s,_| {
        let session = s.ok_or("no document open")?;
        let id = r["link"].as_str().filter(|id| !id.is_empty()).ok_or("link required")?;
        Ok(json!({"target":folio_idml::hyperlinks::target_in(&session.doc.hyperlinks, id, &|page| session.doc.spreads.iter().any(|s| s.pages.iter().any(|p| p.id == page)))}))
    }),
    ("textPathBrackets", |r,s,_| crate::textpath_handles::query(r,s)),
    ("textPathOptions", |r,s,_| {
        let state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        state.editor.text_path_options(r["target"].as_str().ok_or("target required")?).map_err(|e|e.to_string())
    }),
    ("opticalMargin", |r,s,_| {
        let state=s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        Ok(json!({"settings":state.editor.optical_margin(r["story"].as_str().ok_or("story required")?).map_err(|e|e.to_string())?}))
    }),
    ("spellCheck", |r,s,_| crate::spell::check(r,s)),
    ("spellSuggest", |r,_,_| crate::spell::suggestions(r)),
    ("spellLanguages", |_,_,_| Ok(crate::spell::languages())),
    ("spellUserDictionary", |r,_,_| crate::spell::user_dictionary(r)),
    ("spellAutoCorrect", |r,_,_| crate::spell::auto_correct(r)),
    ("dataMergeSource", |r,s,_| crate::datamerge::source(r,s)),
    ("dataMergeEvaluate", |r,s,_| crate::datamerge_expr::evaluate(r,s)),
    ("dataMergeUpdateSource", |r,s,_| crate::datamerge::update_source(r,s)),
    ("hyperlinkUrlStatus", |r,_,_| crate::urlcheck::run(r)),
    ("dataMergeLog", |_,s,_| crate::datamerge::update_log(s)),
    ("spellMarks", |r,s,_| crate::spell::marks(r,s)),
    ("spellTyped", |r,s,_| crate::spell::typed(r,s)),
    ("colorPick", |r,s,_| color_pick(r,s)),
    ("listPrinters", |_,_,_| Ok(crate::printing::list_printers())),
    ("printToDevice", |r,s,_| Ok(crate::printing::print_to_device(r,s))),
    ("cancelPrintJob", |r,_,_| Ok(crate::printing::cancel_print_job(r))),
    ("pageSizePresets", |r,_,_| Ok(page_size_presets(r))),
    ("uiColors", |_,_,_| Ok(ui_colors())),
    // Equations: the live preview of the Insert MathML dialog, and the Math Expressions panel's list (rules/mathml.ko.md)
    ("mathmlLayout", |r,_,_| crate::mathml::layout(r)),
    ("mathExpressions", |_,s,_| crate::mathml::expressions(s)),
    // File > Adjust Layout: the document's default margins, what Auto-adjust margins would use for a page size, the remembered setting (rules/layoutadj-c7.ko.md)
    ("layoutAdjustSettings", |r,s,_| {
        let state = s.and_then(|s|s.edit.as_ref()).ok_or("no document open")?.lock().expect("shared state must remain unpoisoned after each operation");
        serde_json::to_value(state.editor.layout_adjust_settings(r["width"].as_f64(), r["height"].as_f64()).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
    }),
    // Help > Known Differences and Limits (principle 2, item 3; limits-c1): what Folio does differently from InDesign, and with `document` what the open document uses
    ("knownLimits", |r,s,_| crate::known_limits::run(r,s)),
    ("describeCommands", |_,_,_| describe_commands()),
    ("userSettings", |r,s,_| crate::user_settings::command(r,s)),
    ("rendererPreferences", |r,_,_| crate::renderer_preferences::run(r)),
    // Glyphs panel ▸ glyph sets (build-c): the person's sets in the user settings, files in the project (glyph_sets.rs)
    ("glyphSets", |r,s,_| crate::glyph_sets::run(r,s)),
    ("viewState", |r,s,_| crate::view_state::read(r,s)),
    ("setViewState", |r,s,_| crate::view_state::patch(r,s)),
    ("setSelection", |r,s,_| crate::view_state::selection(r,s)),
    ("setTextSelection", |r,s,_| crate::view_state::text(r,s)),
    ("goToPage", |r,s,_| crate::view_state::page(r,s)),
    ("setZoom", |r,s,_| crate::view_state::zoom(r,s)),
    ("setTool", |r,s,_| crate::view_state::tool(r,s)),
    ("setScrollCenter", |r,s,_| crate::view_state::scroll(r,s)),
    // Data Merge from a database (dbmerge.rs; design artifacts/dbmerge-20261119): connections in the user settings, passwords in the OS credential store
    ("dbConnections", |_,_,_| Ok(crate::dbmerge::connections())),
    ("dbSaveConnection", |r,_,_| Ok(crate::dbmerge::save_connection(r))),
    ("dbRemoveConnection", |r,_,_| Ok(crate::dbmerge::remove_connection(r))),
    ("dbUnlockConnection", |r,s,_| Ok(crate::dbmerge::unlock_connection(r,s))),
    ("dbLockConnection", |r,_,_| Ok(crate::dbmerge::lock_connection(r))),
    ("dbTestConnection", |r,s,_| Ok(crate::dbmerge::test_connection(r,s))),
    ("dbTables", |r,s,_| Ok(crate::dbmerge::tables(r,s))),
    ("dbColumns", |r,s,_| Ok(crate::dbmerge::columns(r,s))),
    ("dbPreview", |r,s,_| Ok(crate::dbmerge::preview(r,s))),
    ("dataMergeSourceStatus", |r,s,_| crate::dbmerge::source_status(r,s)),
    // what this device can do with databases, and the credential store's self test (Help > Diagnostics; dbplatform-c1)
    ("dbCapabilities", |_,_,_| Ok(crate::dbmerge::capabilities())),
    ("dbCredentialStoreSelfTest", |_,_,_| Ok(crate::dbmerge::credential_store_self_test())),
    // File > Document Presets: the user's presets, saved and deleted (appdlg-c2)
    ("documentPresets", |r,_,_| crate::document_presets::run(r)),
    // the window's layout: Folio's sidebar or InDesign Classic, the sidebar open, its width and split (uix-c1, user decision 2026-11-20)
    ("workspaceLayout", |r,_,_| crate::workspace_layout::run(r)),
    ("setWorkspaceLayout", |r,_,_| crate::workspace_layout::run(r)),
    // the explorer's New Folder: an empty folder in the project (uix-c1)
    ("projectNewFolder", |r,_,_| crate::project::new_folder(r)),
    // a dock layout command for a tool (MCP): the screen applies it and answers (uix-d2, dock_action.rs)
    ("dockAction", |r,_,_| crate::dock_action::run(r)),
    ("dockActionDone", |r,_,_| crate::dock_action::run(r)),
];

fn describe_commands() -> Result<Value, String> {
    serde_json::from_str(include_str!("../../../commands.generated.json")).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "registered_queries_catalog_tests.rs"]
mod catalog_tests;

pub fn dispatch(req: &Value, session: Option<&crate::Session>, profiles: &crate::Profiles) -> Option<Value> {
    let (_, query) = QUERIES.iter().find(|(name, _)| Some(*name) == req["cmd"].as_str())?;
    Some(match query(req, session, profiles) {
        // a query that answers with its own `ok` (false for a structured failure: `errorKind` says which) keeps it
        Ok(mut result) => { if result.get("ok").is_none() { result["ok"] = json!(true); } result }
        Err(error) => json!({"ok":false,"error":error}),
    })
}

fn oracle_step(req: &Value, _: &crate::Profiles) -> Result<Value, String> {
    let command = folio_edit::commands::oracle::command(&req["step"], req["target"].as_str())
        .map_err(|e| e.to_string())?.ok_or("unregistered oracle step")?;
    Ok(json!({"command": command}))
}

fn color_profiles(_: &Value, profiles: &crate::Profiles) -> Result<Value, String> {
    Ok(json!({"profiles": profiles.get().entries().iter()
        .filter(|p| p.space == "RGB " || p.space == "CMYK" || p.space == "GRAY")
        .filter_map(|p| p.names.first().map(|n| json!({"name":n,"space":p.space.trim()})))
        .collect::<Vec<_>>()}))
}

/// The Swatches panel's Select All Unused and Find this Color: which page items (and how many
/// other places) name each swatch, and the swatches nothing names.
fn swatch_usage(session: Option<&crate::Session>) -> Result<Value, String> {
    let edit = session.and_then(|s| s.edit.as_ref()).ok_or("no document open for editing")?;
    let state = edit.lock().expect("shared state must remain unpoisoned after each operation");
    let parts = state.editor.parts();
    Ok(json!({"unused": folio_edit::unused_swatches(parts), "used": folio_edit::swatch_references(parts)}))
}

/// Load Conditions / Load Conditions and Sets…: the conditions and condition sets of another document of the project (a `.folio` or an
/// IDML, `path` relative to the project root), and the part `loadConditions` takes (`designmap.xml`). No path of it is kept.
/// `{"cmd":"conditionsFile","path":"chapters/other.folio"}`
fn conditions_file(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let relative = req["path"].as_str().ok_or("path required")?;
    let root = &session.and_then(|s| s.folio.as_ref()).ok_or("Open a project document")?.opened.project.root;
    let path = folio_project::paths::checked_project_path(root, relative)?;
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if extension != "idml" && extension != "folio" { return Err("conditions are loaded from a .folio or .idml document".into()); }
    let all = crate::edit::package_parts(&path)?;
    let designmap = all.get("designmap.xml").cloned().ok_or("the document has no designmap.xml")?;
    let conditions = folio_idml::conditions::read(&all).map_err(|e| e.to_string())?;
    let (sets, _) = folio_edit::Editor::new([("designmap.xml".to_owned(), designmap.clone())].into()).condition_sets().map_err(|e| e.to_string())?;
    Ok(json!({"parts": {"designmap.xml": designmap}, "conditions": conditions.iter().map(|c| &c.name).collect::<Vec<_>>(), "sets": sets.as_array().map(|a| a.iter().map(|s| s["name"].clone()).collect::<Vec<_>>()).unwrap_or_default()}))
}

/// Load Trap Presets…: the trap presets of another document of the project (`path` relative to the project root, a `.folio` or an IDML) and
/// the part `loadTrapPresets` takes (`Resources/Styles.xml`). No path of it is kept.
fn trap_presets_file(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let relative = req["path"].as_str().ok_or("path required")?;
    let root = &session.and_then(|s| s.folio.as_ref()).ok_or("Open a project document")?.opened.project.root;
    let path = folio_project::paths::checked_project_path(root, relative)?;
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if extension != "idml" && extension != "folio" { return Err("trap presets are loaded from a .folio or .idml document".into()); }
    let all = crate::edit::package_parts(&path)?;
    let styles = all.get("Resources/Styles.xml").cloned().ok_or("the document has no Resources/Styles.xml")?;
    let names: Vec<String> = styles.match_indices("<TrapPreset ").filter_map(|(at, _)| {
        let tag = &styles[at..styles[at..].find('>').map_or(styles.len(), |e| at + e)];
        tag.split(" Name=\"").nth(1).and_then(|r| r.split('"').next()).map(folio_edit::xml::unescape)
    }).collect();
    Ok(json!({"parts": {"Resources/Styles.xml": styles}, "presets": names}))
}

/// Load Paragraph / Character / All Text / Object Styles…: the styles of another document (an IDML or a `.folio`), which of them clash
/// with this document's, and the parts `loadStyles` takes. No path of the other document is kept: only these definitions come into the document.
/// `{"cmd":"stylesFile","path":…,"kinds":["paragraph"|"character"|"object"|"text"]}`
fn styles_file(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let path = std::path::Path::new(req["path"].as_str().ok_or("파일 경로가 없습니다")?);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    if extension != "idml" && extension != "folio" {
        return Err("IDML 또는 Folio 문서에서 스타일을 불러올 수 있습니다".into());
    }
    let kinds: Vec<String> = req["kinds"].as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_else(|| vec!["text".into(), "object".into()]);
    let all = crate::edit::package_parts(path)?;
    let edit = session.and_then(|s| s.edit.as_ref()).ok_or("no document open for editing")?;
    let state = edit.lock().expect("shared state must remain unpoisoned after each operation");
    let styles = folio_edit::describe_style_file(&kinds, &all, state.editor.parts()).map_err(|e| e.to_string())?;
    let parts: std::collections::BTreeMap<String, String> = all.into_iter().filter(|(n, _)| matches!(n.as_str(), "Resources/Styles.xml" | "Resources/Graphic.xml")).collect();
    Ok(json!({"parts": parts, "styles": styles}))
}


/// The Color Picker's conversions: a colour given in `space` (`RGB`, `CMYK` or `LAB`) with its `value` shown in all three through the document's
/// profiles (an unmanaged conversion without a document), whether the document CMYK can print it and what the gamut warning offers instead, and
/// the id the colour gets when it is applied (`unnamedColorId`, the Color panel's unnamed colour).
/// `{"cmd":"colorPick","space":"CMYK","value":[0,100,100,0]}`
fn color_pick(req: &Value, session: Option<&crate::Session>) -> Result<Value, String> {
    let name = req["space"].as_str().ok_or("space required")?;
    let space = folio_color::PickSpace::parse(name).ok_or("space must be RGB, CMYK or LAB")?;
    let value: Vec<f64> = req["value"].as_array().ok_or("value required")?.iter().map(|v| v.as_f64().ok_or("value must be numbers")).collect::<Result<_, _>>()?;
    let unmanaged = folio_color::Colors::unmanaged();
    let colors = session.map_or(&unmanaged, |s| &s.colors);
    let picked = folio_color::pick_color(colors, space, &value).ok_or("the value does not fit the space")?;
    let id = folio_edit::unnamed_color_id(name, &value).map_err(|e| e.to_string())?;
    Ok(json!({
        "rgb": picked.rgb, "cmyk": picked.cmyk, "lab": picked.lab, "display": picked.display, "inGamut": picked.in_gamut,
        "fixed": picked.fixed.map(|f| json!({"cmyk": f.cmyk, "display": f.display})), "unnamedColorId": id,
    }))
}

/// The Page tool's size list (control bar) and, given a `width` and `height`, which preset a page of that size is (`preset`: null for Custom).
/// `{"cmd":"pageSizePresets","width":595.275590551,"height":841.889763778}`
fn page_size_presets(req: &Value) -> Value {
    let found = match (req["width"].as_f64(), req["height"].as_f64()) {
        (Some(w), Some(h)) => folio_edit::page_size_preset_of(w, h),
        _ => None,
    };
    json!({
        "presets": folio_edit::PAGE_SIZE_PRESETS.iter().map(|(name, width, height)| json!({"name": name, "width": width, "height": height})).collect::<Vec<_>>(),
        "preset": found,
    })
}

/// InDesign's interface colours (`UIColors`) in the order its colour menus list them, each with the screen colour Folio draws it in (`rgb`: the
/// model's `ui_color_rgb`) and the colour InDesign 2025 itself puts on the screen for it (`indesign`: the model's `conditions::indicator_rgb`, the one
/// owner of that table; the Smart Guides are drawn in it, measured pixel by pixel for all 37 in rules/smartguides-c3.ko.md. Gray is 0.5, which the
/// screen shows as 127 and a PDF as 128).
/// `{"cmd":"uiColors"}` answers `{"colors":[{"name":"LightBlue","rgb":[79,153,255],"indesign":[79,153,255]},…]}`
fn ui_colors() -> Value {
    let screen = |name: &str| if name == "Gray" { Some([127, 127, 127]) } else { folio_model::conditions::indicator_rgb(name) };
    json!({"colors": folio_model::UI_COLORS.iter().map(|name| json!({"name": name, "rgb": folio_model::ui_color_rgb(name), "indesign": screen(name)})).collect::<Vec<_>>()})
}

#[cfg(test)]
mod ui_colors_tests {
    #[test]
    fn every_interface_colour_has_a_screen_colour() {
        let answer = super::ui_colors();
        let colors = answer["colors"].as_array().unwrap();
        assert_eq!(colors.len(), folio_model::UI_COLORS.len());
        assert!(colors.iter().all(|c| c["rgb"].as_array().is_some_and(|rgb| rgb.len() == 3)), "{answer}");
        assert_eq!(colors[35]["name"], "GridGreen");
        // the colours InDesign draws the Smart Guides in (smartguides-c3, every one of the 37 measured on the screen)
        assert!(colors.iter().all(|c| c["indesign"].as_array().is_some_and(|rgb| rgb.len() == 3)), "{answer}");
        let of = |name: &str| colors.iter().find(|c| c["name"] == name).unwrap()["indesign"].clone();
        assert_eq!((of("GridGreen"), of("Cyan"), of("Gray"), of("Charcoal")), (serde_json::json!([156, 222, 156]), serde_json::json!([0, 255, 255]), serde_json::json!([127, 127, 127]), serde_json::json!([171, 163, 181])));
    }
}
