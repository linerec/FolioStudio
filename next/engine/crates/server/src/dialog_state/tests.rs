//! Dialog state tests.

use super::*;
use super::actions::{apply, commands, numbers_ok};
use super::choices::{profile_choices, profile_space};
use super::fields::{checked_row, page_range_ok};

fn print_session(fixture: &str, dir: &std::path::Path) -> crate::Session {
    use std::sync::{Arc, OnceLock};

    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles {
        startup: Mutex::new(None),
        index: OnceLock::from(Arc::new(folio_color::ProfileIndex::default())),
    });
    let mut session = crate::load_session(fixture, true, &fonts, &profiles, &|_, _| {}).expect("opens");
    let profile = dir.join("test-cmyk.icc");
    test_support::write_test_cmyk_profile(&profile).unwrap();
    session.cmyk_profile = Some(profile);
    session.summary["color"]["cmyk"]["status"] = json!("used");
    session
}

fn defaults(def: &Value) -> Value {
    let mut values = json!({});
    for f in def["fields"].as_array().unwrap() { values[name_of(f)] = f["default"].clone(); }
    values
}
fn state(name: &str, ids: Value, set: &[(&str, Value)]) -> (Value, Value) {
    let def = definition(name).ok().unwrap();
    let mut values = defaults(&def);
    for (k, v) in set { values[*k] = checked(&field(&def, k).ok().unwrap(), v, &json!({})).ok().unwrap(); linked(&def, &mut values, k); }
    (def, json!({"dialog":name,"values":values,"drafts":{},"ids":ids}))
}
/// The lists a dialog's defaults pick from (choices of rows too), as if the document offered just those.
fn offered_defaults(def: &Value) -> Value {
    let mut offered = Map::new();
    let mut offer = |f: &Value, value: &Value| if let Some(source) = f["source"].as_str().filter(|_| f["type"] == "choice") {
        offered.entry(source.to_owned()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(json!({"value": value}));
    };
    for f in def["fields"].as_array().unwrap() {
        offer(f, &f["default"]);
        for sub in f["fields"].as_array().into_iter().flatten() {
            for row in f["default"].as_array().into_iter().flatten() { offer(sub, &row[name_of(sub)]); }
        }
    }
    Value::Object(offered)
}
fn built(name: &str, ids: Value, set: &[(&str, Value)], action: &str) -> Vec<Value> {
    let (def, state) = state(name, ids, set);
    apply(&def, &state, action).ok().unwrap().0
}

/// The one app dialog of the engine process (`APP`): the tests that open one take turns.
static APP_SLOT: Mutex<()> = Mutex::new(());

#[test]
fn schemas_have_valid_defaults_and_every_command_parses() {
    for (name, def) in schema().as_object().unwrap() {
        // (a choice without `none` starts from what the document has, which its list then offers)
        let offered_all = offered_defaults(def);
        for f in def["fields"].as_array().unwrap() {
            assert!(checked(f, &f["default"], &offered_all).is_ok(), "{name}.{}", name_of(f));
        }
        // a dialog with tabs puts every field it shows on one of them, or above every tab (`everyTab`)
        if let Some(tabs) = def["tabs"].as_array() {
            for f in def["fields"].as_array().unwrap().iter().filter(|f| f["everyTab"] != true && f["hidden"] != true) {
                assert!(tabs.iter().any(|t| t["id"] == f["tab"]), "{name}.{} is on no tab", name_of(f));
            }
        }
        assert!(field(def, "missing").is_err());
        let ids = json!(["a"]);
        let mut values = defaults(def);
        if let Some(fields) = def["fields"].as_array() { for f in fields.iter().filter(|f| f["required"] == true) {
            values[name_of(f)] = if f["path"] == "folder" { json!(env!("CARGO_MANIFEST_DIR")) } else if f["format"] == "numbers" { json!("1") } else { json!("x") };
        } }
        let actions: Vec<String> = def["actions"].as_object().map_or(vec!["ok".into()], |a| a.keys().cloned().collect());
        for action in actions {
            // a button that asks the open document (Load…) needs a session, which these tests do not have
            if def["actions"][&action].as_array().is_some_and(|l| l.iter().any(|t| t["$each"].get("query").is_some())) { continue; }
            // (the PDF dialogs' buttons are built from a project's presets and the document's pages: their own test builds each one,
            // pdf_presets_and_pdf_export_send_the_requests_the_screen_sent)
            if def["build"] == "pdfPresets" { continue; }
            let mut values = values.clone();
            for key in def["needs"][&action].as_array().into_iter().flatten().filter_map(Value::as_str) {
                // (a file to write is a whole path in a folder that exists)
                values[key] = if field(def, key).ok().is_some_and(|f| f["path"] == "saveFile") { json!(format!("{}/x.{}", env!("CARGO_MANIFEST_DIR"), field(def, key).ok().unwrap()["extension"].as_str().unwrap_or("pdf"))) } else { json!("x") };
            }
            // (a required choice is given "x" above, which its list then offers too)
            let mut offered = offered_defaults(def);
            for list in offered.as_object_mut().unwrap().values_mut() { list.as_array_mut().unwrap().push(json!({"value": "x"})); }
        let (list, _) = apply(def, &json!({"values":values,"drafts":{},"ids":ids,"choices":offered}), &action).ok().unwrap_or_else(|| panic!("{name}/{action} does not build"));
            for command in list {
                // (commands that take what the first ones made are built with a stand-in for it)
                if let Some(then) = command.get("$then") {
                    let mut values = command["values"].clone();
                    values[command["find"]["as"].as_str().unwrap()] = json!("x");
                    for c in substitute(then, &values, &ids, &Map::new()).ok().unwrap().as_array().unwrap() {
                        serde_json::from_value::<folio_edit::Command>(c.clone()).unwrap_or_else(|e| panic!("{name}/{action}: {c} is not an edit command: {e}"));
                    }
                    continue;
                }
                // an app dialog sends app requests (the screen's), a document dialog edit commands
                if def["scope"] == "app" { assert!(APP_COMMANDS.contains(&command["cmd"].as_str().unwrap_or("")), "{name}/{action}: {command} is not an app request"); continue; }
                // (a dialog of the document's requests sends those: Print Booklet)
                if def["requests"] == true { assert!(DOCUMENT_REQUESTS.contains(&command["cmd"].as_str().unwrap_or("")), "{name}/{action}: {command} is not a document request"); continue; }
                serde_json::from_value::<folio_edit::Command>(command.clone()).unwrap_or_else(|e| panic!("{name}/{action}: {command} is not an edit command: {e}"));
            }
        }
    }
    // the unit-less counts still take whole numbers only
    assert!(checked(&field(&definition("stepRepeat").ok().unwrap(), "count").ok().unwrap(), &json!(1.5), &json!({})).is_err());
}

#[test]
fn move_links_offset_distance_and_angle_and_sends_the_move_the_screen_sent() {
    let (def, mut state) = state("move", json!(["a"]), &[("h", json!(30)), ("v", json!(-30))]);
    assert_eq!(state["values"]["distance"], json!(42.426407));
    assert_eq!(state["values"]["angle"], json!(45.0));
    let mut values = state["values"].clone();
    values["distance"] = json!(20); values["angle"] = json!(90);
    linked(&def, &mut values, "angle");
    assert_eq!((values["h"].as_f64().unwrap(), values["v"].as_f64().unwrap()), (0.0, -20.0));
    state["values"] = json!({"h":10,"v":5,"distance":0,"angle":0});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"move","ids":["a"],"by":[10,5]})]);
    assert_eq!(apply(&def, &state, "copy").ok().unwrap().0, vec![json!({"op":"duplicate","ids":["a"],"by":[10,5]})]);
}

#[test]
fn scale_rotate_and_shear_send_the_screens_commands_and_refuse_bad_values() {
    let scale = built("scale", json!(["a"]), &[("sx", json!(150))], "ok");
    assert_eq!(scale, vec![json!({"op":"scale","id":"a","sx":1.5,"sy":1.5,"anchor":"topLeft","adjust":{"stroke":true,"effects":true}})]);
    let copy = built("scale", json!(["a"]), &[("sx", json!(50)), ("uniform", json!(false)), ("sy", json!(200))], "copy");
    assert_eq!(copy[0], json!({"op":"duplicate","ids":["a"],"by":[0,0]}));
    assert_eq!((copy[1]["id"].clone(), copy[1]["sx"].clone(), copy[1]["sy"].clone()), (json!("@copies"), json!(0.5), json!(2.0)));
    assert_eq!(built("rotate", json!(["a", "b"]), &[("angle", json!(30)), ("anchor", json!("center"))], "ok"),
        vec![json!({"op":"rotate","ids":["a","b"],"by":30,"anchor":"center"})]);
    assert_eq!(built("rotate", json!(["a"]), &[("angle", json!(30))], "copy")[1], json!({"op":"rotate","ids":["@copies"],"by":30,"anchor":"topLeft"}));
    assert_eq!(built("shear", json!(["a"]), &[("angle", json!(20))], "ok")[0]["by"], json!(20));
    let def = definition("shear").ok().unwrap();
    let refused = checked(&field(&def, "angle").ok().unwrap(), &json!(90), &json!({})).err().unwrap();
    assert_eq!((refused.field.as_deref(), refused.reason), (Some("angle"), "outOfRange"));
    assert!(checked(&field(&def, "anchor").ok().unwrap(), &json!("middle"), &json!({})).is_err());
    let (def, mut state) = state("scale", json!(["a"]), &[]);
    state["values"]["sx"] = json!(0);
    assert!(apply(&def, &state, "ok").is_err());
    assert!(apply(&def, &state, "nothing").is_err());
}

/// Alt+click with the Rotate, Scale or Shear tool (tools-c1): the window turns about the point clicked (spread coordinates), not the reference point.
#[test]
fn the_about_a_point_windows_send_the_tools_commands_about_that_point() {
    assert_eq!(built("rotateAbout", json!(["a", "b"]), &[("angle", json!(30)), ("point", json!([100, 200]))], "ok"),
        vec![json!({"op":"rotateAbout","ids":["a","b"],"by":30,"point":[100,200]})]);
    let copy = built("rotateAbout", json!(["a"]), &[("angle", json!(-15)), ("point", json!([5, 6]))], "copy");
    assert_eq!(copy, vec![json!({"op":"duplicate","ids":["a"],"by":[0,0]}), json!({"op":"rotateAbout","ids":["@copies"],"by":-15,"point":[5,6]})]);
    assert_eq!(built("scaleAbout", json!(["a", "b"]), &[("sx", json!(150)), ("point", json!([10, 20]))], "ok"),
        vec![json!({"op":"scaleAbout","ids":["a","b"],"sx":1.5,"sy":1.5,"point":[10,20],"stroke":true,"effects":true})]);
    assert_eq!(built("shearAbout", json!(["a"]), &[("angle", json!(20)), ("point", json!([1, 2]))], "ok"),
        vec![json!({"op":"shearAbout","ids":["a"],"by":20,"point":[1,2]})]);
    for name in ["rotateAbout", "scaleAbout", "shearAbout"] {
        let def = definition(name).ok().unwrap();
        assert!(checked(&field(&def, "point").ok().unwrap(), &Value::Null, &json!({})).is_err(), "{name} needs its point");
    }
}

#[test]
fn layout_dialogs_send_whole_numbers_and_the_pages() {
    let margins = built("margins", json!(["p1", "p2"]), &[("top", json!(50)), ("columns", json!(3)), ("gutter", json!(10))], "ok");
    assert_eq!(margins, vec![json!({"op":"setMargins","pages":["p1","p2"],"top":50,"bottom":36,"left":36,"right":36,"columns":3,"gutter":10})]);
    let guides = built("createGuides", json!(["p1"]), &[("rows", json!(2)), ("columns", json!(2)), ("columnGutter", json!(10))], "ok");
    assert_eq!(guides, vec![json!({"op":"createGuides","page":"p1","rows":2,"columns":2,"rowGutter":12,"columnGutter":10,"fitMargins":true,"removeExisting":false})]);
    let (def, mut state) = state("newTint", json!([]), &[]);
    assert_eq!(apply(&def, &state, "ok").err().unwrap().reason, "required");
    state["values"]["base"] = json!("Color/Red");
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"newTint","base":"Color/Red","tint":50})]);
    state["drafts"]["tint"] = json!("abc");
    assert_eq!(apply(&def, &state, "ok").err().unwrap().reason, "notNumber");
    state["drafts"]["tint"] = json!("75");
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0[0]["tint"], json!(75.0));
}

#[test]
fn rows_are_set_added_removed_and_moved_by_path_and_choices_take_only_the_offered_ids() {
    let def = definition("captionSetup").ok().unwrap();
    let choices = json!({"paragraphStyles":[{"value":"ParagraphStyle/Caption","label":"Caption"}],"layers":[{"value":"L1","label":"Layer 1"}]});
    let mut values = defaults(&def);
    let set = |values: &mut Value, req: Value| set_path(&def, values, &choices, &req);
    // a row's sub-field by its path; the field path is what a refusal names
    assert_eq!(set(&mut values, json!({"field":"rows/0/before","value":"Photo: "})).ok(), Some("rows".into()));
    assert_eq!(values["rows"][0], json!({"before":"Photo: ","metadata":"Name","after":""}));
    let bad = set(&mut values, json!({"field":"rows/0/metadata","value":"Nothing"})).err().unwrap();
    assert_eq!((bad.field.as_deref(), bad.reason), (Some("rows/0/metadata"), "notOption"));
    assert_eq!(set(&mut values, json!({"field":"rows/3/before","value":"x"})).err().unwrap().reason, "noRow");
    assert_eq!(set(&mut values, json!({"field":"rows/0/nothing","value":"x"})).err().unwrap().reason, "unknownField");
    // added at the end (with the values given), at a row, moved, removed; the last row stays (minRows 1)
    set(&mut values, json!({"field":"rows","row":"add","value":{"metadata":"Page"}})).ok().unwrap();
    set(&mut values, json!({"field":"rows/0","row":"add","value":{"before":"A"}})).ok().unwrap();
    assert_eq!(values["rows"].as_array().unwrap().iter().map(|r| r["before"].as_str().unwrap()).collect::<Vec<_>>(), ["A", "Photo: ", ""]);
    assert_eq!(values["rows"][2]["metadata"], json!("Page"));
    set(&mut values, json!({"field":"rows/2","row":"move","to":0})).ok().unwrap();
    assert_eq!(values["rows"][0]["metadata"], json!("Page"));
    assert_eq!(set(&mut values, json!({"field":"rows/0","row":"move","to":9})).err().unwrap().reason, "noRow");
    set(&mut values, json!({"field":"rows/0","row":"remove"})).ok().unwrap();
    set(&mut values, json!({"field":"rows/0","row":"remove"})).ok().unwrap();
    assert_eq!(set(&mut values, json!({"field":"rows/0","row":"remove"})).err().unwrap().reason, "rowCount");
    assert_eq!(set(&mut values, json!({"field":"rows","row":"shuffle"})).err().unwrap().reason, "badRowOperation");
    assert_eq!(values["rows"], json!([{"before":"Photo: ","metadata":"Name","after":""}]));
    // a whole value is checked row by row
    assert_eq!(set(&mut values, json!({"field":"rows","value":[{"before":1}]})).err().unwrap().field.as_deref(), Some("rows/0/before"));
    // a choice is the empty one (`none`) or one the dialog offers
    set(&mut values, json!({"field":"paragraphStyle","value":"ParagraphStyle/Caption"})).ok().unwrap();
    assert_eq!(set(&mut values, json!({"field":"layer","value":"L9"})).err().unwrap().reason, "notOption");
    set(&mut values, json!({"field":"layer","value":""})).ok().unwrap();
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":choices});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"setCaptionSetup","props":{"rows":[{"before":"Photo: ","metadata":"Name","after":""}],
        "position":"below","offset":1,"paragraphStyle":"ParagraphStyle/Caption","layer":"","group":false}})]);
    // a value the document no longer offers is refused when applied too
    let mut stale = state.clone();
    stale["choices"]["paragraphStyles"] = json!([]);
    assert_eq!(apply(&def, &stale, "ok").err().unwrap().field.as_deref(), Some("paragraphStyle"));
    // the fitting dialog sends every option to every frame, as the screen did
    let fitting = built("frameFitting", json!(["a", "b"]), &[("topCrop", json!(2)), ("autoFit", json!(true))], "ok");
    assert_eq!(fitting, vec![json!({"op":"setFrameFitting","ids":["a","b"],"props":{"autoFit":true,"topCrop":2,"bottomCrop":0,"leftCrop":0,"rightCrop":0,
        "fittingAlignment":"CenterAnchor","fittingOnEmptyFrame":"None"}})]);
}

#[test]
fn footnote_options_read_the_documents_text_as_the_fields_types_and_send_only_what_changed() {
    let def = definition("footnoteOptions").ok().unwrap();
    let tabs: Vec<&str> = def["tabs"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(tabs, ["numbering", "layout"]);
    assert_eq!(typed(&field(&def, "StartAt").ok().unwrap(), &json!("3")), json!(3));
    assert_eq!(typed(&field(&def, "SpaceBetween").ok().unwrap(), &json!("4.5")), json!(4.5));
    assert_eq!(typed(&field(&def, "RuleOn").ok().unwrap(), &json!("true")), json!(true));
    let mut values = defaults(&def);
    values["FootnoteTextStyle"] = json!("ParagraphStyle/Note");
    let initial = values.clone();
    let choices = json!({"paragraphStyles":[{"value":"ParagraphStyle/Note"}],"characterStyles":[{"value":""}]});
    let state = |values: &Value| json!({"values":values,"initial":initial,"drafts":{},"ids":[],"choices":choices});
    assert!(apply(&def, &state(&values), "ok").ok().unwrap().0.is_empty(), "nothing changed, nothing sent");
    // a field on the other tab is set as any other; dialogState holds both tabs' values
    set_path(&def, &mut values, &choices, &json!({"field":"SpaceBetween","value":6})).ok().unwrap();
    set_path(&def, &mut values, &choices, &json!({"field":"StartAt","value":7})).ok().unwrap();
    set_path(&def, &mut values, &choices, &json!({"field":"NoSplitting","value":true})).ok().unwrap();
    assert_eq!(apply(&def, &state(&values), "ok").ok().unwrap().0, vec![json!({"op":"setFootnoteOptions","props":{"SpaceBetween":6,"StartAt":7,"NoSplitting":true}})]);
    assert_eq!(set_path(&def, &mut values, &choices, &json!({"field":"StartAt","value":0})).err().unwrap().reason, "outOfRange");
}

#[test]
fn endnote_options_send_the_changed_ones_with_a_whole_start_number() {
    let def = definition("endnoteOptions").ok().unwrap();
    let mut values = defaults(&def);
    let initial = values.clone();
    // (the style fields offer the document's styles; the dialog opens with them)
    let choices = json!({"paragraphStyles":[{"value":""}],"characterStyles":[{"value":""}]});
    set_path(&def, &mut values, &choices, &json!({"field":"EndnoteTitle","value":"Notes"})).ok().unwrap();
    set_path(&def, &mut values, &choices, &json!({"field":"EndnoteNumberingStyle","value":"UpperRoman"})).ok().unwrap();
    let state = json!({"values":values,"initial":initial,"drafts":{"StartEndnoteNumberAt":"4"},"ids":[],"choices":choices});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"setEndnoteOptions","props":{"EndnoteNumberingStyle":"UpperRoman","EndnoteTitle":"Notes","StartEndnoteNumberAt":4}})]);
    assert_eq!(set_path(&def, &mut values, &choices, &json!({"field":"RestartEndnoteNumbering","value":"PageRestart"})).err().unwrap().reason, "notOption");
    let mut bad = state.clone();
    bad["drafts"]["StartEndnoteNumberAt"] = json!("0");
    assert_eq!(apply(&def, &bad, "ok").err().unwrap().reason, "outOfRange");
}

#[test]
fn qr_code_makes_its_square_from_the_size_and_needs_content() {
    let (def, mut state) = state("qrCode", json!(["p1"]), &[("kind", json!("url")), ("size", json!(100))]);
    assert_eq!(apply(&def, &state, "ok").err().unwrap().reason, "required");
    state["values"]["content"] = json!("example.com");
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0,
        vec![json!({"op":"createQrVector","page":"p1","kind":"url","content":"example.com","rect":[36,36,136.0,136.0]})]);
    assert_eq!(checked(&field(&def, "size").ok().unwrap(), &json!(11), &json!({})).err().unwrap().reason, "outOfRange");
    assert_eq!(checked(&field(&def, "content").ok().unwrap(), &json!("x".repeat(501)), &json!({})).err().unwrap().reason, "tooLong");
}

#[test]
fn document_setup_turns_with_its_orientation_and_anchored_options_send_only_what_changed() {
    let (def, state) = state("documentSetup", json!([]), &[("width", json!(300)), ("height", json!(200))]);
    assert_eq!(state["values"]["orientation"], json!("landscape"));
    let mut values = state["values"].clone();
    values["orientation"] = json!("portrait");
    linked(&def, &mut values, "orientation");
    assert_eq!((values["width"].as_f64(), values["height"].as_f64()), (Some(200.), Some(300.)));
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"documentSetup","width":300,"height":200})]);

    let def = definition("anchoredOptions").ok().unwrap();
    let initial = defaults(&def);
    let mut values = initial.clone();
    let state = |values: &Value| json!({"values":values,"initial":initial,"drafts":{},"ids":["obj"]});
    assert!(apply(&def, &state(&values), "ok").ok().unwrap().0.is_empty(), "nothing changed, nothing sent");
    values["anchorXoffset"] = json!(9);
    values["anchoredPosition"] = json!("Anchored");
    assert_eq!(apply(&def, &state(&values), "ok").ok().unwrap().0,
        vec![json!({"op":"setAnchoredObject","ids":["obj"],"props":{"anchorXoffset":9,"anchoredPosition":"Anchored"}})]);
    // a choice is one of the engine's own, a number one of its range
    let position = field(&def, "anchoredPosition").ok().unwrap();
    assert!(checked(&position, &json!("Anchored"), &json!({})).is_ok() && checked(&position, &json!("Floating"), &json!({})).is_err());
    assert!(checked(&field(&def, "anchorXoffset").ok().unwrap(), &json!(100001), &json!({})).is_err());
}

#[test]
fn app_dialogs_take_whole_folders_and_portable_names_and_send_the_screens_requests() {
    let here = env!("CARGO_MANIFEST_DIR");
    let def = definition("newProject").ok().unwrap();
    // opened without a document, from the values asked for
    let state = opened(&def, "newProject", &json!({"values":{"name":"Magazine"}}), None, None).ok().unwrap();
    assert_eq!((state["values"]["parent"].clone(), state["values"]["name"].clone(), state["ids"].clone()), (json!(""), json!("Magazine"), json!([])));
    // the place is a whole folder that exists; the name one name every system takes
    let set = |values: &mut Value, field: &str, value: Value| set_path(&def, values, &json!({}), &json!({"field":field,"value":value})).map_err(|r| r.reason);
    let mut values = state["values"].clone();
    assert_eq!(set(&mut values, "parent", json!("relative/folder")), Err("notAbsolute"));
    assert_eq!(set(&mut values, "parent", json!(std::path::Path::new(here).join("no such folder").to_string_lossy())), Err("notFolder"));
    for bad in ["a/b", "..", "CON", "x?", "end."] { assert_eq!(set(&mut values, "name", json!(bad)), Err("badName"), "{bad}"); }
    let none = json!({"values":values,"initial":values,"drafts":{},"ids":[]});
    assert_eq!(apply(&def, &none, "ok").err().unwrap().field.as_deref(), Some("parent"), "a place is needed");
    set(&mut values, "parent", json!(here)).unwrap();
    set(&mut values, "name", json!("  Magazine 2026 ")).unwrap();
    let state = json!({"values":values,"initial":values,"drafts":{},"ids":[]});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"cmd":"projectCreate","dir":std::path::Path::new(here).join("Magazine 2026").to_string_lossy(),"name":"Magazine 2026"})]);
    let mut blank = state.clone();
    blank["values"]["name"] = json!("   ");
    assert_eq!(apply(&def, &blank, "ok").err().unwrap().reason, "required");

    let def = definition("language").ok().unwrap();
    assert!(checked(&field(&def, "language").ok().unwrap(), &json!("fr"), &json!({})).is_err());
    assert_eq!(built("language", json!([]), &[("language", json!("ko"))], "ok"), vec![json!({"cmd":"userSettings","patch":{"language":"ko"}})]);
    // an app dialog sends only the app requests the screens send
    assert_eq!(run_app(&[json!({"cmd":"documentClose"})], None).err().unwrap()["errorKind"], "appCommand");
}

#[test]
fn an_app_dialog_is_held_without_a_document_and_makes_the_project_only_when_applied() {
    let _slot = APP_SLOT.lock().unwrap_or_else(|e| e.into_inner());
    let ask = |req: Value| run(&req, None).unwrap();
    // the dialog a request is for: as asked, else the one opening, else the open one
    assert_eq!(app_scoped(&json!({"cmd":"openDialog","dialog":"newProject"}), None).ok(), Some(true));
    assert!(app_scoped(&json!({"cmd":"openDialog","dialog":"newProject","scope":"document"}), None).is_err());
    assert!(app_scoped(&json!({"cmd":"dialogState","scope":"window"}), None).is_err());
    assert!(run(&json!({"cmd":"openDialog","dialog":"move","ids":["a"]}), None).is_err(), "a document dialog needs a document");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/appdlg-tests").join(format!("run-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::path::absolute(&root).unwrap();
    // cancelled: nothing is made
    ask(json!({"cmd":"openDialog","dialog":"newProject"}));
    assert!(run(&json!({"cmd":"openDialog","dialog":"newProject"}), None).is_err(), "one at a time");
    ask(json!({"cmd":"setDialogField","field":"parent","value":root.to_string_lossy()}));
    ask(json!({"cmd":"setDialogField","field":"name","value":"Kept"}));
    let refused = ask(json!({"cmd":"setDialogField","field":"name","value":"no/slash"}));
    assert_eq!((refused["errorKind"].clone(), refused["field"].clone(), refused["reason"].clone()), (json!("dialogField"), json!("name"), json!("badName")));
    assert_eq!(ask(json!({"cmd":"dialogState","scope":"app"}))["dialogState"]["values"]["name"], "Kept");
    assert!(ask(json!({"cmd":"cancelDialog"}))["dialogState"].is_null());
    assert!(!root.join("Kept").exists());
    // applied: the project of the values, answered as what was made
    ask(json!({"cmd":"openDialog","dialog":"newProject","values":{"name":"Made"}}));
    ask(json!({"cmd":"setDialogField","field":"parent","value":root.to_string_lossy()}));
    let done = ask(json!({"cmd":"applyDialog"}));
    assert_eq!((done["dialog"].clone(), done["result"]["type"].clone(), done["result"]["project"]["name"].clone()), (json!("newProject"), json!("project"), json!("Made")));
    assert!(root.join("Made/folio.project").is_file());
    assert_eq!(done["_dialogEvent"]["scope"], "app");
    assert!(ask(json!({"cmd":"dialogState"}))["dialogState"].is_null());
    // a second one in the same place is refused by projectCreate, and the dialog stays open with its values
    ask(json!({"cmd":"openDialog","dialog":"newProject","values":{"name":"Made","parent":root.to_string_lossy()}}));
    let again = ask(json!({"cmd":"applyDialog"}));
    assert_eq!((again["ok"].clone(), again["errorKind"].clone()), (json!(false), json!("appCommand")));
    assert_eq!(ask(json!({"cmd":"dialogState"}))["dialogState"]["values"]["name"], "Made");
    ask(json!({"cmd":"cancelDialog"}));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn hyperlink_dialogs_send_the_commands_the_screen_sent_and_a_paragraph_target_is_cut_at_its_last_colon() {
    let (def, _) = state("newHyperlink", json!([]), &[]);
    let mut values = defaults(&def);
    for (key, value) in [("story", json!("u1")), ("start", json!(3)), ("end", json!(8)), ("name", json!("Link 1")), ("url", json!("https://example.net"))] { values[key] = value; }
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":{}});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0,
        vec![json!({"op":"insertHyperlink","story":"u1","start":3,"end":8,"name":"Link 1","url":"https://example.net"})]);
    let mut empty = state.clone();
    empty["values"]["url"] = json!("  ");
    assert_eq!(apply(&def, &empty, "ok").ok().map(|_| ()), Some(()), "the engine takes a blank text; the screen holds OK back (`required` checks only the empty text)");
    empty["values"]["url"] = json!("");
    assert_eq!(apply(&def, &empty, "ok").err().unwrap().reason, "required");

    let def = definition("editHyperlink").ok().unwrap();
    let state = json!({"values":{"name":"Site","url":"https://a.b"},"drafts":{},"ids":["link1"],"choices":{}});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"setHyperlink","id":"link1","name":"Site","url":"https://a.b"})]);

    // a cross-reference to a paragraph (the other story's `story:at`) or to a named anchor (no paragraph then)
    let def = definition("newCrossReference").ok().unwrap();
    let choices = json!({"crossReferenceDestinations":[{"value":"d1"}],"crossReferenceFormats":[{"value":"f1"}],"crossReferenceParagraphs":[{"value":"u9:19","label":"Second"}]});
    let mut values = defaults(&def);
    for (key, value) in [("story", json!("u1")), ("start", json!(13)), ("name", json!("Reference 1")), ("format", json!("f1")), ("paragraph", json!("u9:19"))] { values[key] = value; }
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":choices});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"insertCrossReference","story":"u1","at":13,"target_story":"u9","target_at":19,
        "destination":"","format":"f1","name":"Reference 1","locations":[]})]);
    let mut anchored = state.clone();
    anchored["values"]["destination"] = json!("d1");
    anchored["values"]["paragraph"] = json!("");
    assert_eq!(apply(&def, &anchored, "ok").ok().unwrap().0, vec![json!({"op":"insertCrossReference","story":"u1","at":13,"target_story":"","target_at":0,
        "destination":"d1","format":"f1","name":"Reference 1","locations":[]})]);
    // the paragraph is asked for only while no anchor is chosen
    let mut none = state.clone();
    none["values"]["paragraph"] = json!("");
    assert_eq!(apply(&def, &none, "ok").err().unwrap().field.as_deref(), Some("paragraph"));
    // a choice must be one the dialog offers
    let mut other = state.clone();
    other["values"]["format"] = json!("f9");
    assert_eq!(apply(&def, &other, "ok").err().unwrap().reason, "notOption");
}

#[test]
fn a_conditional_command_is_left_out_when_its_fields_are_empty() {
    let def = json!({"fields":[{"name":"a"},{"name":"b"}],"actions":{"ok":[{"$if":["a","b"],"then":{"op":"x","a":"$a"}},{"op":"y"},{"$if":"b","then":{"op":"z"},"else":{"op":"w"}}]}});
    let built = |values: Value| commands(&def, "ok", &values, &json!([]), &json!({}), None).ok().unwrap();
    assert_eq!(built(json!({"a":"","b":0})), vec![json!({"op":"y"}), json!({"op":"w"})]);
    assert_eq!(built(json!({"a":"t","b":0}))[0], json!({"op":"x","a":"t"}));
    assert_eq!(built(json!({"a":"","b":true})), vec![json!({"op":"x","a":""}), json!({"op":"y"}), json!({"op":"z"})]);
}

#[test]
fn math_dialogs_send_the_color_as_three_numbers_and_the_preview_waits_for_the_mathml() {
    let def = definition("insertMath").ok().unwrap();
    let mut values = defaults(&def);
    values["mathml"] = json!("<math/>");
    values["color"] = json!("#115395");
    values["size"] = json!(14);
    let state = json!({"values":values,"drafts":{},"ids":["p1"],"choices":{}});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0,
        vec![json!({"op":"insertMathml","page":"p1","mathml":"<math/>","at":[36,36],"size":14,"color":[17.0,83.0,149.0]})]);
    let color = field(&def, "color").ok().unwrap();
    assert!(checked(&color, &json!("#12345"), &json!({})).is_err() && checked(&color, &json!("#1234zz"), &json!({})).is_err());
    assert!(checked(&color, &json!("#AbCdEf"), &json!({})).is_ok());
    // nothing is drawn while the MathML is empty, and a dialog that declares no preview has none
    let empty = json!({"values":defaults(&def),"ids":["p1"]});
    assert_eq!(preview(&def, &empty, None).ok().unwrap(), json!({"state":"empty"}));
    assert!(preview(&definition("move").ok().unwrap(), &empty, None).is_err());
}

#[test]
fn preflight_options_send_the_profiles_and_the_settings_and_check_the_page_range_on_apply() {
    let def = definition("preflightOptions").ok().unwrap();
    let mut values = defaults(&def);
    values["profiles"] = json!([{"name":"Press","disabled_rules":[],"parameters":{}}]);
    values["active"] = json!("Press");
    values["page_range"] = json!("1, 3-5");
    let choices = json!({"preflightProfiles":[{"value":"Press"}]});
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":choices});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![
        json!({"op":"setPreflightProfiles","state":{"profiles":[{"name":"Press","disabled_rules":[],"parameters":{}}],"active":"Press"}}),
        json!({"op":"setPreflightSettings","state":{"page_range":"1, 3-5","layers":"visiblePrintable","include_pasteboard":false,"include_nonprinting":false,"off":false}})]);
    for bad in ["0", "3-1", "1,", "a", "1-", "2 - x"] {
        let mut wrong = state.clone();
        wrong["values"]["page_range"] = json!(bad);
        assert_eq!(apply(&def, &wrong, "ok").err().unwrap().reason, "notPageRange", "{bad}");
    }
    assert!(page_range_ok("") && page_range_ok(" 2 - 3 , 7") && !page_range_ok("1,,2"));
    // the working profile is one the document has (or none)
    let mut other = state.clone();
    other["values"]["active"] = json!("Nothing");
    assert_eq!(apply(&def, &other, "ok").err().unwrap().reason, "notOption");
}

#[test]
fn a_hidden_prefix_is_not_counted_and_the_empty_choice_can_have_a_value_of_its_own() {
    let def = definition("crossReferenceFormat").ok().unwrap();
    let blocks = field(&def, "blocks").ok().unwrap();
    let row = |delimiter: &str| json!({"kind":"ParagraphTextBuildingBlock","text":"","characterStyle":"n","delimiter":delimiter,"includeDelimiter":true});
    assert!(checked_row(&blocks, &row("$ID/;"), &json!({}), "blocks/0").is_ok());
    assert!(checked_row(&blocks, &row("ab"), &json!({}), "blocks/0").is_err());
    // "n" is no character style; any other id must be one the document offers
    let style = field(&def, "characterStyle").ok().unwrap();
    let offered = json!({"characterStylesPlain":[{"value":"CharacterStyle/Bold"}]});
    assert!(checked(&style, &json!("n"), &json!({})).is_ok() && checked(&style, &json!(""), &offered).is_err());
    assert!(checked(&style, &json!("CharacterStyle/Bold"), &offered).is_ok() && checked(&style, &json!("CharacterStyle/Bold"), &json!({})).is_err());
    // Save sends the picked format's id (none = a new one); Duplicate and Delete send theirs
    let mut values = defaults(&def);
    values["format"] = json!("u1");
    values["name"] = json!("Renamed");
    values["duplicateName"] = json!("Renamed copy");
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":{"crossReferenceFormats":[{"value":"u1"}]}});
    assert_eq!(apply(&def, &state, "duplicate").ok().unwrap().0, vec![json!({"op":"duplicateCrossReferenceFormat","id":"u1","name":"Renamed copy"})]);
    assert_eq!(apply(&def, &state, "delete").ok().unwrap().0, vec![json!({"op":"deleteCrossReferenceFormat","id":"u1"})]);
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0[0]["format"]["id"], json!("u1"));
}

#[test]
fn a_typed_list_of_numbers_becomes_the_numbers_and_delete_and_load_do_not_wait_for_the_form() {
    let def = definition("strokeStyles").ok().unwrap();
    let mut values = defaults(&def);
    values["name"] = json!("Dash");
    values["values"] = json!("18, 6.5");
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":{}});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"saveStrokeStyle","style":{"id":"","name":"Dash","kind":"DashedStrokeStyle","values":[18.0,6.5],
        "adjustment":"DashesAndGaps","cap":"ButtEndCap"}})]);
    let mut wrong = state.clone();
    wrong["values"]["values"] = json!("x");
    assert_eq!(apply(&def, &wrong, "ok").err().unwrap().reason, "notNumbers");
    let mut blank = wrong.clone();
    blank["values"]["name"] = json!("");
    assert_eq!(apply(&def, &blank, "ok").err().unwrap().reason, "required");
    assert_eq!(apply(&def, &blank, "delete").ok().unwrap().0, vec![json!({"op":"deleteStrokeStyle","id":""})]);
    // Load… asks the open document for the file's styles; without one it cannot build
    assert!(apply(&def, &state, "import").is_err());
    assert!(numbers_ok("12 6") && numbers_ok("1,2") && !numbers_ok("") && !numbers_ok("1 x"));
    // a `when` takes a value or a list of values
    assert!(when_holds(&json!({"kind":["A","B"]}), &json!({"kind":"B"})) && !when_holds(&json!({"kind":"A"}), &json!({"kind":"B"})));
    // a new style starts the pattern of the kind it is changed to; an existing one does not
    let mut v = json!({"kind":"StripedStrokeStyle","values":"12 6","existing":false});
    linked(&def, &mut v, "kind");
    assert_eq!(v["values"], json!("0 20 50 100"));
    let mut existing = json!({"kind":"StripedStrokeStyle","values":"12 6","existing":true});
    linked(&def, &mut existing, "kind");
    assert_eq!(existing["values"], json!("12 6"));
}

#[test]
fn gradient_stops_keep_their_order_and_the_name_is_trimmed_and_not_taken() {
    let def = definition("newGradient").ok().unwrap();
    let choices = json!({"stopColors":[{"value":"Color/Paper"},{"value":"Color/Black"},{"value":"Color/Red"}],"swatches":[{"value":"Color/Red","label":"Red"}]});
    let mut values = defaults(&def);
    let set = |values: &mut Value, req: Value| { let key = set_path(&def, values, &choices, &req)?; linked(&def, values, &key); Ok::<_, Refusal>(()) };
    set(&mut values, json!({"field":"stops","row":"add","value":{"color":"Color/Red","location":30,"midpoint":40}})).ok().unwrap();
    assert_eq!(values["stops"][1]["color"], json!("Color/Red"), "an added stop takes its place on the ramp");
    set(&mut values, json!({"field":"stops/1/location","value":90})).ok().unwrap();
    assert_eq!(values["stops"].as_array().unwrap().iter().map(|s| s["location"].as_f64().unwrap()).collect::<Vec<_>>(), [0., 90., 100.]);
    assert_eq!(set(&mut values, json!({"field":"stops/0/color","value":"Gradient/G"})).err().unwrap().reason, "notOption");
    assert_eq!(set(&mut values, json!({"field":"stops/0/midpoint","value":5})).err().unwrap().reason, "outOfRange");
    values["name"] = json!(" Red ");
    let state = |values: &Value, ids: Value| json!({"values":values,"drafts":{},"ids":ids,"choices":choices});
    assert_eq!(apply(&def, &state(&values, json!([])), "ok").err().unwrap().reason, "notUnique");
    values["name"] = json!("  Ramp ");
    let sent = apply(&def, &state(&values, json!([])), "ok").ok().unwrap().0;
    assert_eq!((sent[0]["op"].clone(), sent[0]["name"].clone(), sent[0]["stops"][1]["location"].clone()), (json!("newGradient"), json!("Ramp"), json!(90)));
    serde_json::from_value::<folio_edit::Command>(sent[0].clone()).unwrap();
    // the swatch the options dialog edits keeps its own name
    let options = definition("gradientOptions").ok().unwrap();
    values["name"] = json!("Red");
    assert_eq!(apply(&options, &json!({"values":values,"drafts":{},"ids":["Color/Red"],"choices":choices}), "ok").ok().unwrap().0[0]["rename"], json!("Red"));
}

#[test]
fn text_frame_options_send_what_changed_to_every_frame_with_the_four_insets_together() {
    let def = definition("textFrameOptions").ok().unwrap();
    let tabs: Vec<&str> = def["tabs"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(tabs, ["general", "baseline", "autosize"]);
    let mut values = defaults(&def);
    values["same"] = json!(false);
    let initial = values.clone();
    let state = |values: &Value| json!({"values":values,"initial":initial,"drafts":{},"ids":["a","b"]});
    assert!(apply(&def, &state(&values), "ok").ok().unwrap().0.is_empty(), "nothing changed, nothing sent");
    // one inset changed: all four go as insets; a column width is sent only with a fixed width, a minimum only when used
    let set = |values: &mut Value, field: &str, value: Value| { set_path(&def, values, &json!({}), &json!({"field":field,"value":value})).ok().unwrap(); linked(&def, values, field); };
    set(&mut values, "insetLeft", json!(5));
    set(&mut values, "columns", json!(3));
    set(&mut values, "columnWidth", json!(80));
    set(&mut values, "minHeight", json!(40));
    set(&mut values, "vertical", json!("CenterAlign"));
    let options = json!({"insets":[0,5,0,0],"columns":3,"vertical":"CenterAlign"});
    assert_eq!(apply(&def, &state(&values), "ok").ok().unwrap().0, vec![json!({"op":"textFrameOptions","id":"a","options":options}),
        json!({"op":"textFrameOptions","id":"b","options":options})]);
    set(&mut values, "fixed", json!(true));
    set(&mut values, "useMinHeight", json!(true));
    let sent = &apply(&def, &state(&values), "ok").ok().unwrap().0[0]["options"];
    assert_eq!((sent["fixed"].clone(), sent["columnWidth"].clone(), sent["useMinHeight"].clone(), sent["minHeight"].clone()), (json!(true), json!(80), json!(true), json!(40)));
    assert!(sent.get("same").is_none() && sent.get("insetLeft").is_none(), "{sent}");
    for command in apply(&def, &state(&values), "ok").ok().unwrap().0 { serde_json::from_value::<folio_edit::Command>(command).unwrap(); }
    // "the same" sets the four together, and turning it on gives all four the top one; so does a shaped corner (one inset)
    set(&mut values, "insetTop", json!(2));
    assert_eq!((values["insetLeft"].clone(), values["insetRight"].clone()), (json!(5), json!(0)));
    set(&mut values, "same", json!(true));
    assert!(["insetLeft", "insetBottom", "insetRight"].iter().all(|k| values[*k] == json!(2)));
    set(&mut values, "insetBottom", json!(7));
    assert!(["insetTop", "insetLeft", "insetRight"].iter().all(|k| values[*k] == json!(7)));
    let mut one = defaults(&def);
    one["same"] = json!(false); one["shaped"] = json!(true);
    set(&mut one, "insetTop", json!(4));
    assert!(["insetLeft", "insetBottom", "insetRight"].iter().all(|k| one[*k] == json!(4)));
    // a whole number of columns, at least one; a length not below zero; one of the reference points
    assert_eq!(checked(&field(&def, "columns").ok().unwrap(), &json!(2.5), &json!({})).err().unwrap().reason, "notInteger");
    assert_eq!(checked(&field(&def, "columns").ok().unwrap(), &json!(0), &json!({})).err().unwrap().reason, "outOfRange");
    assert_eq!(checked(&field(&def, "gutter").ok().unwrap(), &json!(-3), &json!({})).err().unwrap().reason, "outOfRange");
    assert_eq!(checked(&field(&def, "autoSizePoint").ok().unwrap(), &json!("Middle"), &json!({})).err().unwrap().reason, "notOption");
}

#[test]
fn convert_notes_sends_the_direction_and_a_text_path_sends_its_seven_options_as_idml_text() {
    let def = definition("convertNotes").ok().unwrap();
    let mut values = defaults(&def);
    values["direction"] = json!("endnotesToFootnotes");
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":{}});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"op":"convertNotes","direction":"endnotesToFootnotes","scope":"document"})]);
    assert_eq!(checked(&field(&def, "scope").ok().unwrap(), &json!("selection"), &json!({})).err().unwrap().reason, "notOption");

    // the path's own values (IDML text) come in as numbers and go back as text, the flip as Flipped/NotFlipped
    let def = definition("textPathOptions").ok().unwrap();
    let mut values = defaults(&def);
    for (key, text) in [("PathSpacing", "-2.5"), ("StartBracket", "12"), ("EndBracket", "140.25")] { values[key] = typed(&field(&def, key).ok().unwrap(), &json!(text)); }
    values["FlipPathEffect"] = json!(true);
    let state = |values: &Value| json!({"values":values,"drafts":{},"ids":["tp"],"choices":{}});
    let sent = apply(&def, &state(&values), "ok").ok().unwrap().0;
    assert_eq!(sent, vec![json!({"op":"setTextPathOptions","ids":["tp"],"props":{"PathAlignment":"CenterPathAlignment","TextAlignment":"BaselineTextAlignment",
        "PathEffect":"RainbowPathEffect","FlipPathEffect":"Flipped","PathSpacing":"-2.5","StartBracket":"12","EndBracket":"140.25"}})]);
    serde_json::from_value::<folio_edit::Command>(sent[0].clone()).unwrap();
    values["FlipPathEffect"] = json!(false);
    assert_eq!(apply(&def, &state(&values), "ok").ok().unwrap().0[0]["props"]["FlipPathEffect"], json!("NotFlipped"));
    // the two brackets may not be equal, and a path Folio does not edit takes no OK
    let mut same = values.clone();
    same["EndBracket"] = json!(12);
    let refused = apply(&def, &state(&same), "ok").err().unwrap();
    assert_eq!((refused.field.as_deref(), refused.reason), (Some("EndBracket"), "same"));
    let mut off = values.clone();
    off["supported"] = json!(false);
    assert_eq!(apply(&def, &state(&off), "ok").err().unwrap().reason, "notApplicable");
}

#[test]
fn a_mixed_ink_sends_the_ticked_inks_in_order_and_needs_two_with_a_spot_and_its_rows_stay() {
    let def = definition("newMixedInk").ok().unwrap();
    let mut values = defaults(&def);
    values["name"] = json!(" Mix ");
    values["inks"] = json!([{"ink":"Ink/$ID/Process Cyan","kind":"process","name":"Cyan","on":true,"percent":20},
        {"ink":"Ink/$ID/Process Black","kind":"process","name":"Black","on":false,"percent":100},
        {"ink":"Ink/Gold","kind":"spot","name":"Gold","on":true,"percent":50}]);
    let state = |values: &Value| json!({"values":values,"drafts":{},"ids":[],"choices":{"swatches":[{"value":"Color/Mix 2","label":"Mix 2"}]}});
    let sent = apply(&def, &state(&values), "ok").ok().unwrap().0;
    assert_eq!(sent, vec![json!({"op":"newMixedInk","name":"Mix","inks":["Ink/$ID/Process Cyan","Ink/Gold"],"percentages":[20,50]})]);
    serde_json::from_value::<folio_edit::Command>(sent[0].clone()).unwrap();
    // one ink ticked, or no spot ink among them: refused on the list
    let mut one = values.clone();
    one["inks"][0]["on"] = json!(false);
    let refused = apply(&def, &state(&one), "ok").err().unwrap();
    assert_eq!((refused.field.as_deref(), refused.reason), (Some("inks"), "tooFew"));
    let mut processes = values.clone();
    processes["inks"][1]["on"] = json!(true);
    processes["inks"][2]["on"] = json!(false);
    assert_eq!(apply(&def, &state(&processes), "ok").err().unwrap().reason, "tooFew");
    let mut taken = values.clone();
    taken["name"] = json!("Mix 2");
    assert_eq!(apply(&def, &state(&taken), "ok").err().unwrap().reason, "notUnique");
    // the rows are the document's inks: a tick and a percentage change, the ink of a row, its count and order do not
    let set = |values: &mut Value, req: Value| set_path(&def, values, &json!({}), &req).map(|_| ());
    let mut v = values.clone();
    set(&mut v, json!({"field":"inks/1/on","value":true})).ok().unwrap();
    set(&mut v, json!({"field":"inks/1/percent","value":35})).ok().unwrap();
    assert_eq!((v["inks"][1]["on"].clone(), v["inks"][1]["percent"].clone()), (json!(true), json!(35)));
    assert_eq!(set(&mut v, json!({"field":"inks/1/percent","value":101})).err().unwrap().reason, "outOfRange");
    for bad in [json!({"field":"inks/0/ink","value":"Ink/Other"}), json!({"field":"inks","row":"add"}), json!({"field":"inks/0","row":"remove"}),
                json!({"field":"inks/0","row":"move","to":1}), json!({"field":"inks","value":[]})] {
        assert_eq!(set(&mut v, bad.clone()).err().unwrap().reason, "fixedRows", "{bad}");
    }
    let mut swapped = v["inks"].clone();
    swapped[0]["on"] = json!(false);
    set(&mut v, json!({"field":"inks","value":swapped})).ok().unwrap();
    assert_eq!(v["inks"][0]["on"], json!(false));
}

#[test]
fn object_layer_options_send_the_comp_then_the_layers_that_differ_from_it_and_a_hand_switch_makes_custom() {
    let def = definition("objectLayerOptions").ok().unwrap();
    let layers = json!([{"id":1,"name":"Base","original":true,"visible":true},{"id":2,"name":"A","original":true,"visible":true},
        {"id":3,"name":"B","original":true,"visible":false}]);
    let initial = json!({"layers":layers,"comp":"-1","hasComps":true,"compsShown":[[1,2,3],[1,2]],"policy":"KeepOverrides"});
    let state = |values: &Value| json!({"values":values,"initial":initial,"drafts":{},"ids":["f"],"choices":{"layerComps":[{"value":"-2"},{"value":"-1"},{"value":"0"},{"value":"1"}]}});
    assert!(apply(&def, &state(&initial), "ok").ok().unwrap().0.is_empty(), "nothing changed, nothing sent");
    // a comp shows its layers; a layer switched afterwards is sent after the comp
    let mut v = initial.clone();
    v["comp"] = json!("0");
    linked(&def, &mut v, "comp");
    assert_eq!(v["layers"][2]["visible"], json!(true));
    v["layers"][1]["visible"] = json!(false);
    assert_eq!(apply(&def, &state(&v), "ok").ok().unwrap().0, vec![json!({"op":"setGraphicLayerComp","ids":["f"],"comp":0,"comps":null,"layers":null}),
        json!({"op":"setGraphicLayerVisibility","ids":["f"],"layer":2,"visible":false,"layers":null})]);
    // a hand switch makes the comp [Custom]: only the layer goes, against the picture's layers
    let mut hand = initial.clone();
    hand["layers"][2]["visible"] = json!(true);
    linked(&def, &mut hand, "layers");
    assert_eq!(hand["comp"], json!("-2"));
    hand["policy"] = json!("ApplicationSettings");
    let sent = apply(&def, &state(&hand), "ok").ok().unwrap().0;
    assert_eq!(sent, vec![json!({"op":"setGraphicLayerVisibility","ids":["f"],"layer":3,"visible":true,"layers":null}),
        json!({"op":"setGraphicLayerUpdateLink","ids":["f"],"policy":"ApplicationSettings","layers":null})]);
    for command in sent { serde_json::from_value::<folio_edit::Command>(command).unwrap(); }
    // Last Document State: the layers as the file saved them
    let mut last = initial.clone();
    last["comp"] = json!("1");
    linked(&def, &mut last, "comp");
    assert_eq!(last["layers"][2]["visible"], json!(false));
    last["comp"] = json!("-1");
    linked(&def, &mut last, "comp");
    assert!(last["layers"].as_array().unwrap().iter().all(|l| l["visible"] == l["original"]));
}

/// File ▸ Print Booklet on the shared frame: the plan, the PDF and the print (the mock device only: no printer is reached) are what the
/// screen's queries gave for the same values; bad values are refused at their field; Close leaves nothing behind.
#[test]
fn booklet_plans_writes_its_pdf_and_prints_on_the_mock_device_as_the_screen_did() {
    // SAFETY: set before this test lists printers; it only ever chooses the mock device (no printer is reached)
    unsafe { std::env::set_var("FOLIO_PRINT_DEVICE", "mock"); }
    let dir = std::env::temp_dir().join(format!("folio-booklet-dialog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/printing.idml");
    let s = print_session(fixture, &dir);
    let ask = |req: Value| run(&req, Some(&s)).unwrap_or_else(|e| panic!("{req}: {e}"));
    let options = |binding: &str| json!({"bookletType":"saddle","binding":binding,"signatureSize":4,"gap":0,"bleedBetweenPages":0,"creep":0,
        "margins":[36,36,36,36],"autoMargins":true,"printBlankPrinterSpreads":true});
    let plan_of = |req: Value| crate::printing::booklet(&req, Some(&s)).unwrap()["sheets"].clone();

    // opened with the computer's default printer (the mock's first) and the screen's starting values; the plan is bookletPlan's
    let opened = ask(json!({"cmd":"openDialog","dialog":"booklet"}));
    assert_eq!(opened["dialogState"]["values"]["printer"], "Mock Laser PS");
    assert_eq!(opened["dialogState"]["choices"]["printers"].as_array().map(Vec::len), Some(3));
    let all = ask(json!({"cmd":"dialogPreview"}))["preview"]["sheets"].clone();
    assert_eq!(all, plan_of(json!({"selection":{"kind":"all"},"options":options("document")})));
    assert_eq!(all.as_array().map(Vec::len), Some(4), "5 pages saddle stitched: 4 sheets");

    // a range (typed with spaces, taken trimmed) and the binding: the plan follows
    ask(json!({"cmd":"setDialogField","field":"range","value":" +2-+5 "}));
    ask(json!({"cmd":"setDialogField","field":"binding","value":"right"}));
    let ranged = ask(json!({"cmd":"dialogPreview"}))["preview"]["sheets"].clone();
    assert_eq!(ranged, plan_of(json!({"selection":{"kind":"range","value":"+2-+5"},"options":options("right")})));
    assert_eq!(ranged.as_array().map(Vec::len), Some(2));

    // bad values: refused at the field with the reason, the dialog as it was
    for (field, value, reason) in [("copies", json!(0), "outOfRange"), ("signatureSize", json!("5"), "notOption"), ("bookletType", json!("fold"), "notOption"),
        ("path", json!("booklet.pdf"), "notAbsolute"), ("path", json!(format!("{}/x.txt", env!("CARGO_MANIFEST_DIR"))), "badExtension"),
        ("path", json!(format!("{}/no-such-folder/x.pdf", env!("CARGO_MANIFEST_DIR"))), "notFolder")] {
        let bad = ask(json!({"cmd":"setDialogField","field":field,"value":value}));
        assert_eq!((bad["errorKind"].as_str(), bad["field"].as_str(), bad["reason"].as_str()), (Some("dialogField"), Some(field), Some(reason)), "{field}");
    }
    // Save without a file: asked for at the path (the screen fills it from the system's save window first)
    let unsaved = ask(json!({"cmd":"applyDialog","action":"ok"}));
    assert_eq!((unsaved["field"].as_str(), unsaved["reason"].as_str()), (Some("path"), Some("required")));

    // Print: the request the screen sent (the same sheets, copies, collation and fit) reaches the mock; the dialog stays with the job
    ask(json!({"cmd":"setDialogField","field":"copies","value":3}));
    ask(json!({"cmd":"setDialogField","field":"collate","value":true}));
    let printed = ask(json!({"cmd":"applyDialog","action":"print"}));
    let screen = crate::printing::print_to_device(&json!({"printer":"Mock Laser PS","settings":{"copies":3,"collating":true,"scaleMode":"scaleWidthHeight"},
        "booklet":{"selection":{"kind":"range","value":"+2-+5"},"options":options("right")}}), Some(&s));
    assert_eq!(printed["result"]["request"], screen["request"]);
    assert_eq!((printed["result"]["copies"].as_u64(), printed["result"]["sheets"].as_u64()), (Some(3), Some(2)));
    let job = printed["dialogState"]["values"]["sent"]["job"]["id"].clone();
    assert!(job.as_str().is_some_and(|j| j.starts_with("mock-")), "{printed}");
    assert_eq!(printed["dialogState"]["values"]["canceled"], false);
    // the job is taken back (cancelPrintJob of that job)
    let canceled = ask(json!({"cmd":"applyDialog","action":"cancelJob"}));
    assert_eq!((canceled["result"]["canceled"].as_bool(), &canceled["result"]["job"]), (Some(true), &job));
    assert_eq!(canceled["dialogState"]["values"]["canceled"], true);
    // a printer the computer does not have: a structured refusal with the device's kind, the dialog still open
    ask(json!({"cmd":"setDialogField","field":"printer","value":""}));
    let refused = ask(json!({"cmd":"applyDialog","action":"print"}));
    assert_eq!((refused["errorKind"].as_str(), refused["code"].as_str()), (Some("dialogRequest"), Some("invalidRequest")), "{refused}");
    assert!(ask(json!({"cmd":"dialogState"}))["dialogState"].is_object());

    // what applying it would send is what an attached MCP connection is checked against (attach_paths.rs refuses these requests)
    ask(json!({"cmd":"setDialogField","field":"printer","value":"Mock Inkjet"}));
    let attached = attached_dialog(&json!({"cmd":"applyDialog","action":"print"}), Some(&s)).unwrap().unwrap();
    assert_eq!((attached.dialog.as_str(), attached.requests.len(), attached.requests[0]["cmd"].as_str()), ("booklet", 1, Some("printToDevice")));

    // Save: the PDF of the same plan, one page a sheet, and the dialog closes
    let out = dir.join("booklet.pdf");
    ask(json!({"cmd":"setDialogField","field":"path","value":out.to_string_lossy()}));
    let saved = ask(json!({"cmd":"applyDialog","action":"ok"}));
    assert!(saved["dialogState"].is_null() && saved["_dialogEvent"]["detail"]["dialog"] == "booklet", "{saved}");
    assert_eq!(saved["result"]["plan"]["sheets"], ranged);
    assert!(out.is_file());
    // Close: nothing is written
    ask(json!({"cmd":"openDialog","dialog":"booklet"}));
    let closed = ask(json!({"cmd":"cancelDialog"}));
    assert!(closed["dialogState"].is_null());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// File ▸ Print Presets ▸ Define and File ▸ Print on the shared frame: the fields are the engine's print settings (each labelled);
/// what does not apply is off and refused; the manager's list is its own until OK saves it (userSettings, what the screen saved);
/// the print's plan, PDF proof, print (the mock device only: no printer is reached), job cancel and Save Preset are what the
/// screen's requests gave for the same values.
#[test]
fn print_and_print_presets_send_the_requests_the_screen_sent() {
    use folio_pdf::printing::settings;
    let _slot = APP_SLOT.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: set before this test lists printers or reads the app settings; only the mock device is ever chosen
    unsafe { std::env::set_var("FOLIO_PRINT_DEVICE", "mock"); }
    let dir = std::env::temp_dir().join(format!("folio-printdlg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if std::env::var_os("FOLIO_USER_DATA").is_none() { unsafe { std::env::set_var("FOLIO_USER_DATA", dir.join("user-data")); } }
    let before = crate::user_settings::run(&json!({})).unwrap()["settings"]["printPresets"].clone();
    crate::user_settings::run(&json!({"patch": {"printPresets": []}})).unwrap();
    let user_list = || crate::user_settings::run(&json!({})).unwrap()["settings"]["printPresets"].clone();

    // every print setting is a field of both dialogs, labelled; its tab and choices are the engine's
    let all = schema();
    for name in ["printPresets", "printing"] {
        for f in settings::FIELDS {
            let field = field(&all[name], f.key).ok().unwrap_or_else(|| panic!("{name} has no field {}", f.key));
            assert!(field["label"].as_str().is_some_and(|l| !l.is_empty()), "{name}.{} has no label", f.key);
            assert_eq!(field["tab"], f.tab.id(), "{name}.{}", f.key);
        }
    }
    assert!(all["printing"]["fields"].as_array().unwrap().iter().all(|f| f.get("settingFields").is_none()));

    // ---- the manager (app scope): [Default] is built in, every field off with why
    let app = |req: Value| run(&req, None).unwrap_or_else(|e| panic!("{req}: {e}"));
    let opened = app(json!({"cmd":"openDialog","dialog":"printPresets","values":{"namePrefix":"Preset"}}));
    let st = &opened["dialogState"];
    assert_eq!((st["values"]["preset"].as_str(), st["values"]["builtIn"].as_bool()), (Some("[Default]"), Some(true)), "{st}");
    assert_eq!(st["choices"]["printPresets"].as_array().map(Vec::len), Some(1));
    assert!(st["off"]["copies"].as_str().is_some_and(|w| w.contains("[기본값]")), "{}", st["off"]);
    let refused = app(json!({"cmd":"setDialogField","field":"copies","value":2}));
    assert_eq!((refused["field"].as_str(), refused["reason"].as_str()), (Some("copies"), Some("notApplicable")));
    // New: a copy of the chosen one under a free name; one copy, so Collate does not apply
    let made = app(json!({"cmd":"setDialogField","field":"listAction","value":"new"}));
    let st = &made["dialogState"];
    assert_eq!((st["values"]["preset"].as_str(), st["values"]["name"].as_str(), st["values"]["builtIn"].as_bool()), (Some("Preset"), Some("Preset"), Some(false)));
    assert_eq!(st["values"]["saved"].as_array().map(Vec::len), Some(1));
    assert_eq!(st["off"]["collating"], "지금 설정에서는 적용되지 않습니다.");
    let two = app(json!({"cmd":"setDialogField","field":"copies","value":2}));
    assert!(two["dialogState"]["off"].get("collating").is_none(), "two copies: Collate applies");
    assert_eq!(two["dialogState"]["values"]["saved"][0]["copies"], 2);
    // a PostScript file can separate and has a PPD; the default printer cannot separate
    let ps = app(json!({"cmd":"setDialogField","field":"printer","value":"postscriptFile"}));
    assert!(ps["dialogState"]["off"].get("ppd").is_none() && ps["dialogState"]["offOptions"].get("colorOutput").is_none(), "{}", ps["dialogState"]["off"]);
    app(json!({"cmd":"setDialogField","field":"printer","value":"default"}));
    let sep = app(json!({"cmd":"setDialogField","field":"colorOutput","value":"separations"}));
    assert_eq!((sep["field"].as_str(), sep["reason"].as_str()), (Some("colorOutput"), Some("notAvailable")), "{sep}");
    // All Printer's Marks sets the five marks (the engine's whole set comes back)
    let marks = app(json!({"cmd":"setDialogField","field":"allPrinterMarks","value":true}));
    assert!(["cropMarks", "bleedMarks", "registrationMarks", "colorBars", "pageInformationMarks"].iter().all(|k| marks["dialogState"]["values"][k] == true));
    // the name: empty is held back (OK off), a free one renames the preset in the list
    let empty = app(json!({"cmd":"setDialogField","field":"name","value":" "}));
    assert_eq!(empty["dialogState"]["values"]["nameOk"], false);
    let blocked = app(json!({"cmd":"applyDialog","action":"ok"}));
    assert_eq!(blocked["errorKind"], "dialogField", "{blocked}");
    let renamed = app(json!({"cmd":"setDialogField","field":"name","value":"Proof"}));
    assert_eq!((renamed["dialogState"]["values"]["preset"].as_str(), renamed["dialogState"]["values"]["saved"][0]["name"].as_str()), (Some("Proof"), Some("Proof")));
    // Save… writes the chosen one to a file, Delete takes it out of the list, Load… puts the file's back
    let file = dir.join("print-presets.json");
    app(json!({"cmd":"setDialogField","field":"filePath","value":file.to_string_lossy()}));
    let written = app(json!({"cmd":"applyDialog","action":"export"}));
    assert_eq!(written["result"]["written"], 1, "{written}");
    assert!(file.is_file() && written["dialogState"].is_object());
    app(json!({"cmd":"setDialogField","field":"listAction","value":"delete"}));
    let gone = app(json!({"cmd":"dialogState","scope":"app"}));
    assert_eq!((gone["dialogState"]["values"]["preset"].as_str(), gone["dialogState"]["values"]["saved"].as_array().map(Vec::len)), (Some("[Default]"), Some(0)));
    for (value, reason) in [(json!("x.json"), "notAbsolute"), (json!(dir.join("none.json").to_string_lossy()), "notFile"), (json!(dir.to_string_lossy()), "badExtension")] {
        let bad = app(json!({"cmd":"setDialogField","field":"loadPath","value":value}));
        assert_eq!(bad["reason"].as_str(), Some(reason), "{bad}");
    }
    app(json!({"cmd":"setDialogField","field":"loadPath","value":file.to_string_lossy()}));
    let loaded = app(json!({"cmd":"applyDialog","action":"import"}));
    assert_eq!(loaded["dialogState"]["choices"]["printPresets"].as_array().map(Vec::len), Some(2), "{loaded}");
    let saved = loaded["dialogState"]["values"]["saved"].clone();
    assert_eq!((saved[0]["name"].as_str(), saved[0]["copies"].as_i64(), saved[0]["cropMarks"].as_bool()), (Some("Proof"), Some(2), Some(true)));
    // nothing is saved until OK; OK saves the list as the screen saved it (userSettings printPresets)
    assert_eq!(user_list(), json!([]));
    let ok = app(json!({"cmd":"applyDialog","action":"ok"}));
    assert!(ok["dialogState"].is_null(), "{ok}");
    assert_eq!(user_list(), saved);
    let screen = crate::printing::presets(&json!({"presets": [], "op": "define", "preset": saved[0]})).unwrap();
    assert_eq!(user_list(), screen["saved"]);
    // Cancel drops the dialog's list
    app(json!({"cmd":"openDialog","dialog":"printPresets"}));
    app(json!({"cmd":"setDialogField","field":"preset","value":"Proof"}));
    app(json!({"cmd":"setDialogField","field":"listAction","value":"delete"}));
    app(json!({"cmd":"cancelDialog","scope":"app"}));
    assert_eq!(user_list(), saved);

    // ---- File ▸ Print (the document's requests)
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/printing.idml");
    let s = print_session(fixture, &dir);
    let ask = |req: Value| run(&req, Some(&s)).unwrap_or_else(|e| panic!("{req}: {e}"));
    let plan_of = |job: Value, settings: Value| plan_sheets_of(&s, job, settings);
    fn plan_sheets_of(s: &crate::Session, job: Value, settings: Value) -> Value {
        printdlg::plan_sheets(&crate::printing::plan(&json!({"job": job, "settings": settings}), Some(s)).unwrap())
    }
    let opened = ask(json!({"cmd":"openDialog","dialog":"printing","values":{"newName":"New"}}));
    let st = &opened["dialogState"];
    assert_eq!((st["values"]["preset"].as_str(), st["values"]["device"].as_str(), st["values"]["range"].as_str()), (Some("[Default]"), Some("Mock Laser PS"), Some("all")));
    assert_eq!(st["choices"]["printPresets"].as_array().map(Vec::len), Some(2), "[Default] and the saved Proof");
    assert_eq!(st["offOptions"]["range"], json!(["pages"]), "no pages handed in");
    let defaults = json!(settings::defaults());
    let shown = ask(json!({"cmd":"dialogPreview"}))["preview"]["sheets"].clone();
    assert_eq!(shown, plan_of(json!({"selection":{"kind":"all"},"spreads":false,"printBlankPages":false}), defaults.clone()));
    assert_eq!(shown.as_array().map(Vec::len), Some(3), "Print Blank Pages off: A-12 and A-13 are left out");
    // a preset chosen fills the settings; a setting changed is [Custom]; the range stays the job's own
    ask(json!({"cmd":"setDialogField","field":"range","value":"range"}));
    ask(json!({"cmd":"setDialogField","field":"rangeText","value":" +2-+5 "}));
    let proof = ask(json!({"cmd":"setDialogField","field":"preset","value":"Proof"}));
    assert_eq!((proof["dialogState"]["values"]["copies"].as_i64(), proof["dialogState"]["values"]["rangeText"].as_str()), (Some(2), Some(" +2-+5 ")));
    let custom = ask(json!({"cmd":"setDialogField","field":"copies","value":3}));
    assert_eq!(custom["dialogState"]["values"]["preset"], newdoc::CUSTOM);
    ask(json!({"cmd":"setDialogField","field":"allPrinterMarks","value":false}));
    let mut settings_now = settings::defaults();
    settings_now.insert("copies".into(), json!(3));
    let ranged = ask(json!({"cmd":"dialogPreview"}))["preview"]["sheets"].clone();
    assert_eq!(ranged, plan_of(json!({"selection":{"kind":"range","value":"+2-+5"},"spreads":false,"printBlankPages":false}), json!(settings_now)));
    assert_eq!(ranged.as_array().map(Vec::len), Some(2));
    // a range that names no page: no plan (OK and Print wait for one)
    ask(json!({"cmd":"setDialogField","field":"rangeText","value":"nonsense"}));
    assert!(run(&json!({"cmd":"dialogPreview"}), Some(&s)).is_err());
    ask(json!({"cmd":"setDialogField","field":"rangeText","value":"+2-+5"}));
    // Print: the request the screen sent reaches the mock; the dialog stays with the job, which is taken back
    let printed = ask(json!({"cmd":"applyDialog","action":"print"}));
    let values = &printed["dialogState"]["values"];
    let job = json!({"selection":{"kind":"range","value":"+2-+5"},"spreads":false,"printBlankPages":false});
    let screen = crate::printing::print_to_device(&json!({"printer":"Mock Laser PS","settings":printdlg::settings_of(values).ok().unwrap(),"job":job}), Some(&s));
    assert_eq!(printed["result"]["request"], screen["request"], "{printed}");
    assert_eq!(printed["result"]["copies"], 3);
    let id = values["sent"]["job"]["id"].clone();
    let canceled = ask(json!({"cmd":"applyDialog","action":"cancelJob"}));
    assert_eq!((canceled["result"]["canceled"].as_bool(), &canceled["result"]["job"]), (Some(true), &id));
    assert_eq!(attached_dialog(&json!({"cmd":"applyDialog","action":"print"}), Some(&s)).unwrap().unwrap().requests[0]["cmd"], "printToDevice");
    // Save Preset: a name taken is refused at the field; a free one is saved with the settings, chosen, and listed
    ask(json!({"cmd":"setDialogField","field":"naming","value":true}));
    ask(json!({"cmd":"setDialogField","field":"newName","value":"Proof"}));
    let taken = ask(json!({"cmd":"applyDialog","action":"savePreset"}));
    assert_eq!((taken["field"].as_str(), taken["reason"].as_str()), (Some("newName"), Some("taken")), "{taken}");
    ask(json!({"cmd":"setDialogField","field":"newName","value":" Three "}));
    let kept = ask(json!({"cmd":"applyDialog","action":"savePreset"}));
    assert_eq!((kept["dialogState"]["values"]["preset"].as_str(), kept["dialogState"]["values"]["naming"].as_bool()), (Some("Three"), Some(false)), "{kept}");
    assert_eq!(kept["dialogState"]["choices"]["printPresets"].as_array().map(Vec::len), Some(3));
    let three = user_list().as_array().unwrap().iter().find(|p| p["name"] == "Three").cloned().unwrap();
    assert_eq!((three["copies"].as_i64(), three["cropMarks"].as_bool()), (Some(3), Some(false)));
    // OK: the PDF proof of the same plan, and the dialog closes
    let out = dir.join("proof.pdf");
    ask(json!({"cmd":"setDialogField","field":"path","value":out.to_string_lossy()}));
    let proofed = ask(json!({"cmd":"applyDialog","action":"ok"}));
    assert!(proofed["dialogState"].is_null() && out.is_file(), "{proofed}");
    assert_eq!(printdlg::plan_sheets(&proofed["result"]["plan"]), ranged);
    // Layout ▸ Pages ▸ Print Pages: the pages handed in are the range; Flattener Preview's settings stay over the preset asked for
    let page = s.doc.spreads[0].pages[0].id.clone();
    let pages = ask(json!({"cmd":"openDialog","dialog":"printing","values":{"pageIds":[page],"preset":"Proof","flattenerPresetName":"[High Resolution]"}}));
    let v = &pages["dialogState"]["values"];
    assert_eq!((v["range"].as_str(), v["copies"].as_i64(), v["flattenerPresetName"].as_str()), (Some("pages"), Some(2), Some("[High Resolution]")));
    assert!(pages["dialogState"]["offOptions"].get("range").is_none());
    assert_eq!(ask(json!({"cmd":"dialogPreview"}))["preview"]["sheets"].as_array().map(Vec::len), Some(1));
    ask(json!({"cmd":"cancelDialog"}));

    crate::user_settings::run(&json!({"patch": {"printPresets": before}})).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// File ▸ Package on the shared frame, in stages: the plan is packagePlan's; OK makes what the package request makes for the same
/// values (main.rs runs it on its own thread: here the same calls), with the progress in the dialog's values, and remembers the
/// choices; a folder that is there sends it back to the choices with the next free name; a failure is kept with its code.
#[test]
fn package_plans_makes_its_folder_in_stages_as_the_screen_did() {
    let _slot = APP_SLOT.lock().unwrap_or_else(|e| e.into_inner());
    // (one package at a time in this process: Package Book For Print's test makes some too)
    let _packages = crate::package::TEST_SLOT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("folio-packagedlg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("out")).unwrap();
    std::fs::create_dir_all(dir.join("direct")).unwrap();
    // SAFETY: set before this test reads the app settings (a folder of its own when none is configured)
    if std::env::var_os("FOLIO_USER_DATA").is_none() { unsafe { std::env::set_var("FOLIO_USER_DATA", dir.join("user-data")); } }
    let remembered = crate::user_settings::run(&json!({})).unwrap()["settings"].clone();
    crate::user_settings::run(&json!({"patch": {"packageFolder": dir.join("out"), "packageContact": {"contact": "Kim", "company": "", "phone": "", "email": ""}}})).unwrap();
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = std::sync::Arc::new(crate::Profiles::start());
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/printing.idml");
    let s = crate::load_session(fixture, true, &fonts, &profiles, &|_, _| {}).expect("opens");
    let ask = |req: Value| run(&req, Some(&s)).unwrap_or_else(|e| panic!("{req}: {e}"));

    // opened on the choices the settings remember, the document's folder name
    let opened = ask(json!({"cmd":"openDialog","dialog":"package","values":{"namePattern":"{name} Folder","preflightErrors":2,"appVersion":"test","utcOffsetMinutes":540}}));
    let v = &opened["dialogState"]["values"];
    assert_eq!((v["stage"].as_str(), v["name"].as_str(), v["contact"].as_str(), v["hasPreflightErrors"].as_bool()), (Some("choose"), Some("printing Folder"), Some("Kim"), Some(true)), "{v}");
    assert_eq!(v["folder"].as_str().map(std::path::PathBuf::from), Some(dir.join("out")));
    ask(json!({"cmd":"setDialogField","field":"pdf","value":false}));
    ask(json!({"cmd":"setDialogField","field":"fonts","value":false}));
    // the plan is packagePlan's for the same values, the folder can be made
    let shown = ask(json!({"cmd":"dialogPreview"}))["preview"].clone();
    let made = dir.join("out").join("printing Folder");
    let direct = crate::package::plan_command(&json!({"links":true,"fonts":false,"pdf":false,"cjkFonts":false,"roots":[],"dir":made.to_string_lossy()}), &s);
    let timeless = |mut plan: Value| { plan["counts"]["ms"] = Value::Null; plan };
    assert_eq!(timeless(shown["plan"].clone()), timeless(direct["plan"].clone()));
    assert_eq!((shown["okAllowed"].as_bool(), shown["pages"].as_u64()), (Some(true), Some(5)));
    // bad values: refused at the field; the stage moves only by itself
    for (field, value, reason) in [("name", json!("a/b"), "badName"), ("folder", json!("relative"), "notAbsolute"), ("stage", json!("running"), "notEditable")] {
        let bad = ask(json!({"cmd":"setDialogField","field":field,"value":value}));
        assert_eq!((bad["field"].as_str(), bad["reason"].as_str()), (Some(field), Some(reason)), "{bad}");
    }
    // OK: the running stage, the package request the screen sent (on its own thread in main.rs: the same calls here), the progress kept
    let applied = ask(json!({"cmd":"applyDialog","action":"ok"}));
    let job = applied["_documentRequest"].clone();
    assert_eq!(job["requests"][0]["cmd"], "package", "{applied}");
    assert_eq!(applied["_dialogEvent"]["detail"]["progress"], true);
    assert_eq!(applied["_dialogEvent"]["detail"]["dialogState"]["values"]["stage"], "running");
    let request = job["requests"][0].clone();
    assert_eq!((request["dir"].as_str().map(std::path::PathBuf::from), request["fonts"].as_bool(), request["pdf"].as_bool(), request["instructions"]["contact"].as_str()),
        (Some(made.clone()), Some(false), Some(false), Some("Kim")));
    assert_eq!(attached_dialog(&json!({"cmd":"applyDialog","action":"ok"}), Some(&s)).unwrap().unwrap().requests.len(), 0, "running: OK sends nothing");
    let heard = Mutex::new(0);
    let reply = crate::package::package_with(&request, &s, None, &|p: &Value| if document_progress(&s, p).is_some() { *heard.lock().unwrap() += 1; });
    assert!(*heard.lock().unwrap() >= 1, "the progress went into the dialog");
    let (answer, event) = document_finished(&s, &job, &reply);
    let v = &answer["dialogState"]["values"];
    assert_eq!((v["stage"].as_str(), v["result"]["dir"].as_str().map(std::path::PathBuf::from)), (Some("done"), Some(made.clone())), "{answer}");
    assert!(event.is_some() && made.is_dir());
    let theirs = crate::package::package_command(&json!({"dir": dir.join("direct").join("p"), "links":true,"fonts":false,"pdf":false,"cjkFonts":false,"roots":[]}), &s, None);
    assert_eq!(v["result"]["files"], theirs["result"]["files"]);
    let settings = crate::user_settings::run(&json!({})).unwrap()["settings"].clone();
    assert_eq!((settings["package"]["fonts"].as_bool(), settings["packageContact"]["contact"].as_str()), (Some(false), Some("Kim")));
    // back to the choices: the folder is there now, so the plan says it cannot be made, and a package of it comes back renamed
    ask(json!({"cmd":"setDialogField","field":"stage","value":"choose"}));
    assert_eq!(ask(json!({"cmd":"dialogPreview"}))["preview"]["okAllowed"], false);
    let again = ask(json!({"cmd":"applyDialog","action":"ok"}))["_documentRequest"].clone();
    let exists = crate::package::package_command(&again["requests"][0], &s, None);
    assert_eq!(exists["code"], "exists", "{exists}");
    let (answer, _) = document_finished(&s, &again, &exists);
    let v = &answer["dialogState"]["values"];
    assert_eq!((v["stage"].as_str(), v["renamed"].as_bool(), v["name"].as_str()), (Some("choose"), Some(true), exists["suggestion"].as_str()), "{v}");
    // a package that fails is kept with its code (Cancel while it runs: packageCancel, the dialog stays)
    let third = ask(json!({"cmd":"applyDialog","action":"ok"}))["_documentRequest"].clone();
    let canceled = ask(json!({"cmd":"applyDialog","action":"cancel"}));
    assert_eq!((canceled["result"]["type"].as_str(), canceled["dialogState"]["values"]["cancelling"].as_bool()), (Some("packageCancel"), Some(true)), "{canceled}");
    let (answer, _) = document_finished(&s, &third, &json!({"ok": false, "code": "cancelled", "error": "cancelled"}));
    assert_eq!((answer["dialogState"]["values"]["stage"].as_str(), answer["dialogState"]["values"]["failure"]["code"].as_str()), (Some("failed"), Some("cancelled")));
    assert!(!dir.join("out").join(v["name"].as_str().unwrap()).exists(), "nothing made by the failed one");
    ask(json!({"cmd":"cancelDialog"}));

    crate::user_settings::run(&json!({"patch": {"packageFolder": remembered["packageFolder"], "package": remembered["package"], "packageContact": remembered["packageContact"]}})).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toc_styles_save_the_boxes_and_toc_makes_the_toc_with_the_id_the_saved_style_gets() {
    let def = definition("toc").ok().unwrap();
    let mut values = defaults(&def);
    values["name"] = json!("  Contents A ");
    values["entries"] = json!([{"name":"ParagraphStyle/H1","formatStyle":"ParagraphStyle/TOC","level":2.0,"pageNumberPosition":"BeforeEntry","pageNumberStyle":"","separator":" — ","separatorStyle":"","sortAlphabet":false}]);
    let choices = json!({"paragraphStyles":[{"value":"ParagraphStyle/H1"},{"value":"ParagraphStyle/TOC"}],"characterStyles":[],"tocStyles":[]});
    let state = json!({"values":values,"drafts":{},"ids":[],"choices":choices});
    let list = apply(&def, &state, "ok").ok().unwrap().0;
    // the style first (as the screen saved it), then the TOC once the document has given the new style its id (`$then`)
    assert_eq!(list[0]["op"], "saveTOCStyle");
    assert_eq!(list[0]["style"]["name"], "Contents A");
    assert_eq!(list[0]["style"]["entries"][0]["level"], json!(2));
    assert!(serde_json::from_value::<folio_edit::Command>(list[0].clone()).is_ok());
    assert_eq!(list[1]["find"], json!({"in":"tocStyles","where":{"name":"Contents A"},"pick":"id","as":"savedStyle"}));
    assert_eq!(list[1]["$then"][0]["op"], "createTOC");
    // a row without its paragraph style is refused where it is
    let mut empty = state.clone();
    empty["values"]["entries"][0]["name"] = json!("");
    let refused = apply(&def, &empty, "ok").err().unwrap();
    assert_eq!((refused.field.as_deref(), refused.reason), (Some("entries/0/name"), "required"));
    // Table of Contents Styles saves only; Delete and Load are its other buttons
    let styles = definition("tocStyles").ok().unwrap();
    let mut sv = defaults(&styles);
    sv["name"] = json!("B");
    sv["style"] = json!("u1");
    let mut picked = choices.clone();
    picked["tocStyles"] = json!([{"value":"u1"}]);
    let state = json!({"values":sv,"drafts":{},"ids":[],"choices":picked});
    assert_eq!(apply(&styles, &state, "ok").ok().unwrap().0.len(), 1);
    assert_eq!(apply(&styles, &state, "delete").ok().unwrap().0, vec![json!({"op":"deleteTOCStyle","id":"u1"})]);
    // a row added takes the first paragraph style not listed, as its entry style too; a row's style changed carries an entry style that was the same
    let before = json!({"entries":[{"name":"ParagraphStyle/H1","formatStyle":"ParagraphStyle/H1"}]});
    let mut now = json!({"entries":[{"name":"ParagraphStyle/H1","formatStyle":"ParagraphStyle/H1"},{"name":"","formatStyle":""}]});
    tocdlg::linked(&before, &mut now, "entries", &choices);
    assert_eq!(now["entries"][1], json!({"name":"ParagraphStyle/TOC","formatStyle":"ParagraphStyle/TOC"}));
    let mut changed = json!({"entries":[{"name":"ParagraphStyle/TOC","formatStyle":"ParagraphStyle/H1"}]});
    tocdlg::linked(&before, &mut changed, "entries", &choices);
    assert_eq!(changed["entries"][0]["formatStyle"], "ParagraphStyle/TOC");
}

#[test]
fn define_profiles_keep_every_profile_and_send_the_list_with_the_working_one() {
    let def = definition("preflightProfiles").ok().unwrap();
    // the boxes are the engine's rules and numbers
    assert!(field(&def, "rule.LINK_MISSING").is_ok() && field(&def, "category.links").is_ok() && field(&def, "param.max_ink").is_ok());
    let mut values = defaults(&def);
    values["profiles"] = json!([{"name":"A","disabled_rules":["LINK_MISSING"],"parameters":{"max_ink":300.0}},{"name":"B","disabled_rules":[],"parameters":{}}]);
    values["active"] = json!("A");
    values["profile"] = json!("0");
    profdlg::load(&mut values, 0);
    assert_eq!((values["name"].clone(), values["useAsActive"].clone(), values["rule.LINK_MISSING"].clone(), values["category.links"].clone(), values["param.max_ink"].clone()),
        (json!("A"), json!(true), json!(false), json!(false), json!(300.0)));
    let mut choices = json!({});
    let mut set = |values: &mut Value, key: &str, value: Value| { let before = values.clone(); values[key] = value; profdlg::linked(&before, values, &mut choices, key, None).ok().unwrap(); };
    // the working profile follows its name; a rule turned on turns its category on when it was the last one off
    set(&mut values, "name", json!("A2"));
    assert_eq!(values["active"], "A2");
    set(&mut values, "rule.LINK_MISSING", json!(true));
    assert_eq!(values["category.links"], true);
    set(&mut values, "category.text", json!(false));
    assert_eq!(values["rule.FONT_MISSING"], false);
    // another profile: the boxes go back into the list first; none: a new one at the end
    set(&mut values, "profile", json!("1"));
    assert_eq!(values["name"], "B");
    set(&mut values, "profile", json!(""));
    assert_eq!((values["profile"].clone(), values["name"].clone()), (json!("2"), json!("New Preflight Profile")));
    let list = profdlg::commands(&values).ok().unwrap();
    assert_eq!(list[0]["op"], "setPreflightProfiles");
    assert_eq!(list[0]["state"]["active"], "A2");
    let profiles = list[0]["state"]["profiles"].as_array().unwrap();
    assert_eq!(profiles.iter().map(|p| p["name"].clone()).collect::<Vec<_>>(), vec![json!("A2"), json!("B"), json!("New Preflight Profile")]);
    assert!(!profiles[0]["disabled_rules"].as_array().unwrap().contains(&json!("LINK_MISSING")) && profiles[0]["disabled_rules"].as_array().unwrap().contains(&json!("FONT_MISSING")));
    assert_eq!(profiles[0]["parameters"]["max_ink"], json!(300.0));
    assert!(serde_json::from_value::<folio_edit::Command>(list[0].clone()).is_ok());
    // no two profiles of one name
    let mut twice = values.clone();
    twice["name"] = json!("B");
    assert_eq!(profdlg::commands(&twice).err().unwrap().reason, "notUnique");
}

/// The values a dialog starts from: its defaults with some set, and the lists its choices offer (each value set is offered).
fn opened_with(name: &str, set: &[(&str, Value)]) -> (Value, Value) {
    let def = definition(name).ok().unwrap();
    let mut values = defaults(&def);
    let initial = values.clone();
    let mut choices = offered_defaults(&def);
    for (k, v) in set {
        values[*k] = v.clone();
        if let Some(source) = field(&def, k).ok().and_then(|f| f["source"].as_str().map(str::to_owned)) {
            choices.as_object_mut().unwrap().entry(source).or_insert_with(|| json!([])).as_array_mut().unwrap().push(json!({"value": v}));
        }
    }
    (def, json!({"dialog":name,"values":values,"initial":initial,"drafts":{},"ids":[],"choices":choices}))
}

#[test]
fn assign_profiles_sends_what_changed_and_color_settings_saves_every_setting_of_the_app() {
    // Assign Profiles: only the changed settings, as the screen sent them (setColorPrefs props); nothing changed, nothing sent
    let (def, state) = opened_with("assignProfiles", &[("RGBProfile", json!("Adobe RGB (1998)")), ("RGBPolicy", json!("ConvertToWorkingSpace")), ("SolidColorIntent", json!("Perceptual"))]);
    let sent = apply(&def, &state, "ok").ok().unwrap().0;
    assert_eq!(sent, vec![json!({"op":"setColorPrefs","props":{"RGBProfile":"Adobe RGB (1998)","RGBPolicy":"ConvertToWorkingSpace","SolidColorIntent":"Perceptual"}})]);
    serde_json::from_value::<folio_edit::Command>(sent[0].clone()).unwrap();
    let (def, unchanged) = opened_with("assignProfiles", &[]);
    assert!(apply(&def, &unchanged, "ok").ok().unwrap().0.is_empty());
    // a profile the dialog does not offer is refused with the field and why
    let mut bad = state.clone();
    bad["values"]["CMYKProfile"] = json!("Not Installed");
    assert_eq!(apply(&def, &bad, "ok").err().unwrap().reason, "notOption");
    // Color Settings: every setting under colorSettings., as the screen saved them (userSettings values)
    let (def, state) = opened_with("colorSettings", &[("workingSpaceRGB", json!("Adobe RGB (1998)")), ("rgbPolicy", json!("CONVERT_TO_WORKING_SPACE")), ("useBPC", json!(false))]);
    let sent = apply(&def, &state, "ok").ok().unwrap().0;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["cmd"], json!("userSettings"));
    let saved = sent[0]["values"].as_object().unwrap();
    assert_eq!(saved.len(), 12);
    assert_eq!((&saved["colorSettings.workingSpaceRGB"], &saved["colorSettings.rgbPolicy"], &saved["colorSettings.useBPC"], &saved["colorSettings.intent"]),
        (&json!("Adobe RGB (1998)"), &json!("CONVERT_TO_WORKING_SPACE"), &json!(false), &json!("RELATIVE_COLORIMETRIC")));
    // the profile lists: the installed ones of the space and the defaults of the fields that pick from them (none bundled)
    let rgb = profile_choices(&def, "rgbProfiles", "RGB", None);
    assert_eq!(rgb, vec![json!({"value":"sRGB IEC61966-2.1","label":"sRGB IEC61966-2.1"})]);
    assert_eq!(profile_space("documentCmykProfiles"), Some("CMYK"));
}

#[test]
fn convert_to_profile_sends_the_conversion_with_the_commands_intent_and_nothing_to_the_same_profiles() {
    let (def, state) = opened_with("convertToProfile", &[]);
    // the destination starts where the source is: nothing to convert, and the plan says so without asking
    assert!(apply(&def, &state, "ok").ok().unwrap().0.is_empty());
    assert_eq!(preview(&def, &state, None).ok().unwrap(), json!({"ok":true,"values":{"rgb":0,"cmyk":0,"renamed":0}}));
    let (def, state) = opened_with("convertToProfile", &[("destRgb", json!("Adobe RGB (1998)")), ("intent", json!("PERCEPTUAL")), ("bpc", json!(false))]);
    let sent = apply(&def, &state, "ok").ok().unwrap().0;
    assert_eq!(sent, vec![json!({"op":"convertToProfile","rgb_profile":"Adobe RGB (1998)","cmyk_profile":"U.S. Web Coated (SWOP) v2","black_point_compensation":false,"intent":"Perceptual"})]);
    serde_json::from_value::<folio_edit::Command>(sent[0].clone()).unwrap();
    // a trial needs the open document; the preview of a changed destination asks it
    assert!(preview(&def, &state, None).is_err());
    // the source is shown, not edited; the preview box is off
    let mut values = state["values"].clone();
    for key in ["sourceRgb", "preview"] {
        let refused = set_path(&def, &mut values, &state["choices"], &json!({"field": key, "value": "x"})).err().unwrap();
        assert_eq!(refused.reason, "notEditable");
    }
}

#[test]
fn flattener_presets_show_one_preset_copy_and_delete_the_users_and_save_the_list_the_screen_saved() {
    let def = definition("flattenerPresets").ok().unwrap();
    // (no app settings in a test: no presets of the user's) High Resolution is shown, not to be changed
    let mut values = Value::Object(flattener::defaults());
    let mut choices = json!({"flattenerPresets": flattener::saved_choices()});
    assert_eq!((values["flattenerPreset"].clone(), values["builtIn"].clone(), values["lineArtAndTextResolution"].clone()), (json!("2"), json!(true), json!(1200.0)));
    assert_eq!(choices["flattenerPresets"].as_array().unwrap().len(), 3);
    values["lineArtAndTextResolution"] = json!(600);
    assert_eq!(flattener::linked(&mut values, &mut choices, "lineArtAndTextResolution").err().unwrap().reason, "notEditable");
    values["listAction"] = json!("delete");
    assert_eq!(flattener::linked(&mut values, &mut choices, "listAction").err().unwrap().reason, "notEditable");
    // New copies the one shown (Medium) under the first free name; a field set goes into it
    values["flattenerPreset"] = json!("1");
    flattener::linked(&mut values, &mut choices, "flattenerPreset").ok().unwrap();
    assert_eq!((values["lineArtAndTextResolution"].clone(), values["clipComplexRegions"].clone()), (json!(300.0), json!(true)));
    values["namePrefix"] = json!("New Preset {n}");
    for _ in 0..2 { values["listAction"] = json!("new"); flattener::linked(&mut values, &mut choices, "listAction").ok().unwrap(); }
    assert_eq!((values["flattenerPreset"].clone(), values["flattenerName"].clone(), values["builtIn"].clone(), values["listAction"].clone()), (json!("4"), json!("New Preset 2"), json!(false), json!("")));
    assert_eq!(choices["flattenerPresets"][4], json!({"value":"4","label":"New Preset 2"}));
    values["convertAllTextToOutlines"] = json!(true);
    flattener::linked(&mut values, &mut choices, "convertAllTextToOutlines").ok().unwrap();
    values["flattenerName"] = json!(" Folio test ");
    flattener::linked(&mut values, &mut choices, "flattenerName").ok().unwrap();
    assert_eq!(values["presets"][1]["convertAllTextToOutlines"], json!(true));
    // choosing the first shows its values again; Delete removes the one shown and shows High Resolution
    values["flattenerPreset"] = json!("3");
    flattener::linked(&mut values, &mut choices, "flattenerPreset").ok().unwrap();
    assert_eq!((values["flattenerName"].clone(), values["convertAllTextToOutlines"].clone()), (json!("New Preset 1"), json!(false)));
    values["listAction"] = json!("delete");
    flattener::linked(&mut values, &mut choices, "listAction").ok().unwrap();
    assert_eq!((values["flattenerPreset"].clone(), values["presets"].as_array().unwrap().len()), (json!("2"), 1));
    // OK: the list as the screen saved it (userSettings patch flattenerPresets), the names trimmed
    let state = json!({"dialog":"flattenerPresets","values":values,"initial":{},"drafts":{},"ids":[],"choices":choices});
    let sent = apply(&def, &state, "ok").ok().unwrap().0;
    assert_eq!(sent, vec![json!({"cmd":"userSettings","patch":{"flattenerPresets":[{"name":"Folio test","rasterVectorBalance":75.0,"lineArtAndTextResolution":300.0,
        "gradientAndMeshResolution":150.0,"convertAllTextToOutlines":true,"convertAllStrokesToOutlines":true,"clipComplexRegions":true}]}})]);
    // a name used twice, a built-in one's or none is refused at the preset's row
    for (name, reason) in [("[High Resolution]", "reserved"), ("  ", "required")] {
        let mut bad = state.clone();
        bad["values"]["presets"][0]["name"] = json!(name);
        let refused = apply(&def, &bad, "ok").err().unwrap();
        assert_eq!((refused.field.as_deref(), refused.reason), (Some("presets/0/name"), reason));
    }
    // the whole list set at once (a request may): the picker follows it, and a preset shown that is gone gives High Resolution
    let mut whole = state["values"].clone();
    let mut picked = choices.clone();
    whole["flattenerPreset"] = json!("3");
    whole["presets"] = json!([]);
    flattener::linked(&mut whole, &mut picked, "presets").ok().unwrap();
    assert_eq!((whole["flattenerPreset"].clone(), whole["builtIn"].clone(), picked["flattenerPresets"].as_array().unwrap().len()), (json!("2"), json!(true), 3));
    let mut twice = state.clone();
    let row = twice["values"]["presets"][0].clone();
    twice["values"]["presets"].as_array_mut().unwrap().push(row);
    assert_eq!(apply(&def, &twice, "ok").err().unwrap().reason, "duplicate");
}

#[test]
fn keyboard_shortcuts_take_a_normalized_key_refuse_a_conflict_and_save_the_users_keys() {
    assert_eq!(shortcuts::normalized("shift+ctrl+d").as_deref(), Some("Ctrl+Shift+D"));
    assert_eq!(shortcuts::normalized("Alt+Right").as_deref(), Some("Alt+Right"));
    assert_eq!(shortcuts::normalized("Ctrl+Alt+Shift+f10").as_deref(), Some("Ctrl+Alt+Shift+F10"));
    assert_eq!(shortcuts::normalized("Ctrl++").as_deref(), Some("Ctrl++"));
    for bad in ["D", "Shift+D", "Ctrl+Escape", "Ctrl+Space", "Meta+D", "Ctrl+Nope"] { assert_eq!(shortcuts::normalized(bad), None, "{bad}"); }
    let def = definition("shortcuts").ok().unwrap();
    assert_eq!(checked(&field(&def, "shortcutKey").ok().unwrap(), &json!("D"), &json!({})).err().unwrap().reason, "notShortcut");
    // three commands: Open has Ctrl+O; a contextual key (fallThrough) has Alt+Right and gives it up to a user's key
    let commands = json!([
        {"id":"file.open","label":"Open","keys":["Ctrl+O"],"fallThrough":false,"listed":true},
        {"id":"text.key.kernRight","label":"Kern right","keys":["Alt+Right"],"fallThrough":true,"listed":true},
        {"id":"edit.preferences","label":"Preferences","keys":["Ctrl+K"],"fallThrough":false,"listed":true},
        {"id":"recent.0","label":"a.folio","keys":[],"fallThrough":false,"listed":false}]);
    let mut values = json!({"commands": commands, "shortcuts": {}, "shortcutSearch": "", "shortcutCommand": "edit.preferences", "shortcutKey": "", "listAction": ""});
    let mut choices = json!({});
    shortcuts::linked(&mut values, &mut choices, "").ok().unwrap();
    assert_eq!((values["currentKeys"].clone(), values["hasKeys"].clone(), values["shortcutCommandLabel"].clone()), (json!("Ctrl+K"), json!(true), json!("Preferences")));
    assert_eq!(choices["shortcutCommands"].as_array().unwrap().iter().map(|r| r["value"].clone()).collect::<Vec<_>>(), vec![json!("file.open"), json!("text.key.kernRight"), json!("edit.preferences")]);
    assert_eq!(choices["shortcutCommands"][0]["label"], json!("Open · Ctrl+O"));
    // Ctrl+O is Open's: a conflict, and Assign is refused
    values["shortcutKey"] = json!("Ctrl+O");
    shortcuts::linked(&mut values, &mut choices, "shortcutKey").ok().unwrap();
    assert_eq!((values["hasConflict"].clone(), values["conflictWith"].clone()), (json!(true), json!("Open")));
    values["listAction"] = json!("assign");
    assert_eq!(shortcuts::linked(&mut values, &mut choices, "listAction").err().unwrap().reason, "conflict");
    // Alt+Right is a contextual default: no conflict; assigned it is the user's, and the contextual key gives it up
    values["listAction"] = json!(""); values["shortcutKey"] = json!("Alt+Right");
    shortcuts::linked(&mut values, &mut choices, "shortcutKey").ok().unwrap();
    assert_eq!(values["hasConflict"], json!(false));
    values["listAction"] = json!("assign");
    shortcuts::linked(&mut values, &mut choices, "listAction").ok().unwrap();
    assert_eq!((values["shortcuts"].clone(), values["shortcutKey"].clone(), values["currentKeys"].clone()), (json!({"edit.preferences":["Ctrl+K","Alt+Right"]}), json!(""), json!("Ctrl+K, Alt+Right")));
    assert_eq!(choices["shortcutCommands"][1]["label"], json!("Kern right"));
    // the search narrows the list (the command chosen stays); Remove leaves no key, Reset the defaults again
    values["shortcutSearch"] = json!("OPEN");
    shortcuts::linked(&mut values, &mut choices, "shortcutSearch").ok().unwrap();
    assert_eq!(choices["shortcutCommands"].as_array().unwrap().iter().map(|r| r["value"].clone()).collect::<Vec<_>>(), vec![json!("file.open"), json!("edit.preferences")]);
    values["listAction"] = json!("remove");
    shortcuts::linked(&mut values, &mut choices, "listAction").ok().unwrap();
    assert_eq!((values["shortcuts"]["edit.preferences"].clone(), values["hasKeys"].clone()), (json!([]), json!(false)));
    values["listAction"] = json!("reset");
    shortcuts::linked(&mut values, &mut choices, "listAction").ok().unwrap();
    assert_eq!((values["shortcuts"].clone(), values["currentKeys"].clone()), (json!({}), json!("Ctrl+K")));
    // choosing another command clears the key; OK saves the user's shortcuts as the screen saved them
    values["shortcutKey"] = json!("Ctrl+Alt+F12"); values["shortcutCommand"] = json!("file.open");
    shortcuts::linked(&mut values, &mut choices, "shortcutCommand").ok().unwrap();
    assert_eq!(values["shortcutKey"], json!(""));
    values["shortcuts"] = json!({"file.open":["Ctrl+Alt+F12"]});
    let state = json!({"dialog":"shortcuts","values":values,"initial":{},"drafts":{},"ids":[],"choices":choices});
    assert_eq!(apply(&def, &state, "ok").ok().unwrap().0, vec![json!({"cmd":"userSettings","patch":{"shortcuts":{"file.open":["Ctrl+Alt+F12"]}}})]);
}

#[test]
fn pdf_presets_and_pdf_export_send_the_requests_the_screen_sent() {
    let _slot = APP_SLOT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("folio-pdfdlg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: set before this test reads the app settings (a folder of its own when none is configured)
    if std::env::var_os("FOLIO_USER_DATA").is_none() { unsafe { std::env::set_var("FOLIO_USER_DATA", dir.join("user-data")); } }
    let made = crate::project::create(&json!({"cmd": "projectCreate", "dir": dir.join("project"), "name": "PDF"}));
    assert_eq!(made["ok"], true, "{made}");
    let root = made["project"]["root"].as_str().unwrap().to_owned();
    let list = |project: &str| crate::pdf_presets::run(&json!({"action": "list", "project": project})).unwrap();

    // File > PDF Presets > Define: the six built-in presets and an unsaved one listed only while shown; PDF/X-4 shown
    let app = |req: Value| { let mut req = req; req["scope"] = json!("app"); run(&req, None).unwrap_or_else(|e| panic!("{req}: {e}")) };
    let opened = app(json!({"cmd": "openDialog", "dialog": "pdfPresets", "values": {"project": root, "newName": "New PDF Preset"}}));
    let state = &opened["dialogState"];
    assert_eq!(state["values"]["preset"], "x4");
    assert_eq!(state["values"]["builtIn"], true);
    assert_eq!(state["choices"]["pdfPresets"].as_array().map(Vec::len), Some(7));
    // New starts an unsaved copy named by the screen's words; the fields are set as the screen's boxes set them
    app(json!({"cmd": "setDialogField", "field": "listAction", "value": "new"}));
    for (field, value) in [("name", json!("Printer A")), ("crop", json!(true)), ("bleedTop", json!(3)), ("bleedOutside", json!(11)), ("markOffset", json!(12)),
        ("markWeight", json!("0.5")), ("imageCompression", json!("jpeg")), ("jpegQuality", json!(80)), ("colorConversion", json!("documentCmyk"))] {
        let r = app(json!({"cmd": "setDialogField", "field": field, "value": value}));
        assert!(r.get("errorKind").is_none(), "{field}: {r}");
    }
    assert_eq!(app(json!({"cmd": "dialogState"}))["dialogState"]["values"]["preset"], "");
    // PDF/X-1a converts to CMYK: "do not convert" is refused, and choosing X-1a over it goes back to the preset's default
    app(json!({"cmd": "setDialogField", "field": "standard", "value": "X1a"}));
    let refused = app(json!({"cmd": "setDialogField", "field": "colorConversion", "value": "none"}));
    assert_eq!((refused["field"].as_str(), refused["reason"].as_str()), (Some("colorConversion"), Some("notAllowed")), "{refused}");
    app(json!({"cmd": "setDialogField", "field": "standard", "value": "X4"}));
    // a built-in preset's name is refused; a file outside the project too
    app(json!({"cmd": "setDialogField", "field": "name", "value": "[PDF/X-4:2008]"}));
    assert_eq!(app(json!({"cmd": "applyDialog"}))["reason"], "reserved");
    app(json!({"cmd": "setDialogField", "field": "name", "value": "Printer A"}));
    for bad in ["../a.json", "C:/a.json", "/a.json", "a.txt"] {
        app(json!({"cmd": "setDialogField", "field": "filePath", "value": bad}));
        let r = app(json!({"cmd": "applyDialog", "action": "export"}));
        assert_eq!(r["field"], "filePath", "{bad}: {r}");
    }
    // Save to Project = pdfPresets save of the fields as one preset; the dialog stays open showing it
    let saved = app(json!({"cmd": "applyDialog"}));
    assert_eq!(saved["dialogState"]["values"]["preset"], "project:Printer A", "{saved}");
    let entry = list(&root).as_array().unwrap().iter().find(|e| e["id"] == "project:Printer A").cloned().expect("saved");
    let mut expected = serde_json::to_value(folio_pdf::PdfPreset::builtin(folio_pdf::Preset::X4)).unwrap();
    expected["name"] = json!("Printer A"); expected["marks"]["crop"] = json!(true); expected["bleed"] = json!([3., 0., 0., 11.]);
    expected["markOffset"] = json!(12.); expected["markWeight"] = json!(0.5); expected["imageCompression"] = json!("jpeg");
    expected["jpegQuality"] = json!(80); expected["colorConversion"] = json!("documentCmyk");
    assert_eq!(entry["settings"], expected);
    // saving the same name again replaces it (the one shown); Save to File writes it in the project, Load reads it back unsaved
    app(json!({"cmd": "setDialogField", "field": "markOffset", "value": 18}));
    assert_eq!(app(json!({"cmd": "applyDialog"}))["dialogState"]["values"]["preset"], "project:Printer A");
    app(json!({"cmd": "setDialogField", "field": "filePath", "value": "shared.json"}));
    app(json!({"cmd": "applyDialog", "action": "export"}));
    let file: Value = serde_json::from_slice(&std::fs::read(std::path::Path::new(&root).join("shared.json")).unwrap()).unwrap();
    assert_eq!(file["markOffset"], 18.);
    app(json!({"cmd": "setDialogField", "field": "preset", "value": "high"}));
    let loaded = app(json!({"cmd": "applyDialog", "action": "import"}));
    assert_eq!((loaded["dialogState"]["values"]["preset"].as_str(), loaded["dialogState"]["values"]["name"].as_str()), (Some(""), Some("Printer A")), "{loaded}");
    assert_eq!(loaded["dialogState"]["values"]["markOffset"], 18.);
    // Delete removes the project preset shown, then the first one is shown
    app(json!({"cmd": "setDialogField", "field": "preset", "value": "project:Printer A"}));
    let deleted = app(json!({"cmd": "applyDialog", "action": "delete"}));
    assert_eq!(deleted["dialogState"]["values"]["preset"], "x4");
    assert!(!list(&root).as_array().unwrap().iter().any(|e| e["id"] == "project:Printer A"));
    app(json!({"cmd": "cancelDialog"}));

    // File > Export PDF: the document's dialog; a preset asked for fills the fields (File > PDF Presets > [High Quality Print])
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/pdfexport.idml");
    let s = print_session(fixture, &dir);
    let ask = |req: Value| run(&req, Some(&s)).unwrap_or_else(|e| panic!("{req}: {e}"));
    let opened = ask(json!({"cmd": "openDialog", "dialog": "pdfExport", "values": {"preset": "high", "project": root, "preflightErrors": 2}}));
    let values = &opened["dialogState"]["values"];
    assert_eq!((values["standard"].as_str(), values["hasPreflightErrors"].as_bool()), (Some("High"), Some(true)));
    assert_eq!(opened["dialogState"]["context"]["fileName"], "pdfexport");
    // a range that names no page holds OK back (and is refused)
    assert_eq!(ask(json!({"cmd": "setDialogField", "field": "range", "value": "1-2-3"}))["dialogState"]["values"]["canExport"], false);
    let path = dir.join("export.pdf");
    ask(json!({"cmd": "setDialogField", "field": "path", "value": path}));
    assert_eq!(ask(json!({"cmd": "applyDialog"}))["errorKind"], "dialogField");
    ask(json!({"cmd": "setDialogField", "field": "range", "value": "2"}));
    // OK: the export request the screen sent for these values, run on its own thread by main.rs, then the settings remembered
    let job = ask(json!({"cmd": "applyDialog"}))["_documentRequest"].clone();
    let preset = serde_json::to_value(folio_pdf::PdfPreset::builtin(folio_pdf::Preset::High)).unwrap();
    let choice = json!({"preset": "high", "bleed": preset["useDocumentBleed"], "marks": preset["marks"], "spreads": preset["spreads"], "settings": preset});
    let mut export = choice.clone();
    export["cmd"] = json!("export"); export["path"] = json!(path.to_string_lossy()); export["pages"] = json!([1]);
    assert_eq!(job["requests"][0], export);
    assert_eq!(job["requests"][1], json!({"cmd": "userSettings", "patch": {"export": choice, "exportFolder": dir.to_string_lossy()}}));
    let out: crate::Out = std::sync::Arc::new(Mutex::new(std::io::stdout()));
    let header = crate::print_command(&job["requests"][0], &s, &json!(1), &out).map(|mut v| { v["ok"] = json!(true); v }).unwrap();
    let (answer, event) = document_finished(&s, &job, &header);
    assert!(event.is_some() && answer["ok"] == true, "{answer}");
    assert_eq!(answer["dialogState"]["values"]["exported"]["path"], header["result"]["path"], "{answer}");
    assert_eq!(answer["dialogState"]["values"]["exported"]["pages"], 1);
    assert!(path.is_file());
    // the next Export PDF starts from what was exported
    ask(json!({"cmd": "cancelDialog"}));
    let again = ask(json!({"cmd": "openDialog", "dialog": "pdfExport", "values": {"project": root}}));
    assert_eq!(again["dialogState"]["values"]["preset"], "high");
    // Create Acrobat Layers with all layers on (build-c2): the settings carry them, the PDF has its optional content
    assert_eq!(again["dialogState"]["values"]["acrobatLayers"], false);
    assert_eq!(again["dialogState"]["values"]["exportLayers"], "visiblePrintable");
    ask(json!({"cmd": "setDialogField", "field": "acrobatLayers", "value": true}));
    ask(json!({"cmd": "setDialogField", "field": "exportLayers", "value": "all"}));
    let layered = dir.join("layers.pdf");
    ask(json!({"cmd": "setDialogField", "field": "path", "value": layered}));
    let job = ask(json!({"cmd": "applyDialog"}))["_documentRequest"].clone();
    assert_eq!((job["requests"][0]["settings"]["acrobatLayers"].as_bool(), job["requests"][0]["settings"]["exportLayers"].as_str()), (Some(true), Some("all")));
    crate::print_command(&job["requests"][0], &s, &json!(2), &out).unwrap();
    let bytes = std::fs::read(&layered).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.5") && bytes.windows(13).any(|w| w == b"/OCProperties"));
    // PDF/X-1a takes none: the setting is dropped with the standard
    ask(json!({"cmd": "setDialogField", "field": "standard", "value": "X1a"}));
    let job = ask(json!({"cmd": "applyDialog"}))["_documentRequest"].clone();
    assert_eq!(job["requests"][0]["settings"]["acrobatLayers"], false);
    ask(json!({"cmd": "cancelDialog"}));
    let _ = std::fs::remove_dir_all(&dir);
}
