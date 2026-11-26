//! Text variables on the variables fixture (oracle `variables`: InDesign's own pages show, in the parent's header, the running headers
//! by paragraph style and by character style, the page number, the Last Page Number, the Chapter Number and a custom text). The pages
//! of the fixture hold: 1 "Alpha Heading" and "Zulu" in the Accent character style; 2 "Beta" and "Gamma Second" in Head and "nkee" in
//! Accent; 3 none of them; 4 "Delta Ending".
use folio_text::{FontStore, layout_document, text_variable_values};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> BTreeMap<String, String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/variables.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    parts
}

fn values(parts: &BTreeMap<String, String>) -> Vec<BTreeMap<String, String>> {
    let doc = folio_idml::read_parts(parts, Path::new("."), &folio_idml::ReaderData::default()).unwrap();
    let fonts = FontStore::empty();
    let layout = layout_document(&doc, &fonts);
    // by the variables' names
    text_variable_values(&doc, &fonts, &layout)
        .into_iter()
        .map(|m| m.into_iter().map(|(id, v)| (doc.text_variables[&id].name.clone(), v)).collect())
        .collect()
}

#[test]
fn running_headers_last_page_and_chapter_number_follow_the_page() {
    let v = values(&fixture());
    assert_eq!(v.len(), 4);
    let row = |p: usize| (v[p]["By Head"].as_str(), v[p]["By Accent"].as_str(), v[p]["Last Page Number"].as_str(), v[p]["Chapter Number"].as_str());
    assert_eq!(row(0), ("Alpha Heading", "Zulu", "4", "1"));
    // the first Head paragraph of the page counts ("Beta", not "Gamma Second")
    assert_eq!(row(1), ("Beta", "nkee", "4", "1"));
    // a page without one shows what the page before showed
    assert_eq!(row(2), ("Beta", "nkee", "4", "1"));
    assert_eq!(row(3), ("Delta Ending", "nkee", "4", "1"));
    // custom text, file name and the dates keep the stored result (no entry)
    assert!(!v[0].contains_key("Custom Note") && !v[0].contains_key("File Name"));
}

#[test]
fn the_search_the_case_the_text_around_and_the_end_punctuation_are_the_variables() {
    let mut parts = fixture();
    // By Head: the last Head paragraph on the page, upper case, "<" and ">" around it, no end punctuation
    let dm = parts.get_mut("designmap.xml").unwrap();
    let old = "TextBefore=\"\" TextAfter=\"\" AppliedParagraphStyle=\"ParagraphStyle/Head\" SearchStrategy=\"FirstOnPage\" ChangeCase=\"None\" DeleteEndPunctuation=\"false\"";
    assert!(dm.contains(old));
    *dm = dm.replace(old, "TextBefore=\"&lt;\" TextAfter=\"&gt;\" AppliedParagraphStyle=\"ParagraphStyle/Head\" SearchStrategy=\"LastOnPage\" ChangeCase=\"Uppercase\" DeleteEndPunctuation=\"true\"");
    // page 4's heading gets a full stop
    let story = parts.iter_mut().find(|(_, t)| t.contains("Delta Ending")).unwrap().1;
    *story = story.replace("Delta Ending", "Delta Ending.");
    let v = values(&parts);
    assert_eq!(v[0]["By Head"], "<ALPHA HEADING>");
    assert_eq!(v[1]["By Head"], "<GAMMA SECOND>", "the last one on the page");
    assert_eq!(v[2]["By Head"], "<GAMMA SECOND>");
    assert_eq!(v[3]["By Head"], "<DELTA ENDING>", "the full stop is dropped");
}

// ---- inserting an instance (oracle variables var-insert-*) ----

fn story_of(parts: &BTreeMap<String, String>, text: &str) -> String {
    parts.iter().find(|(n, t)| n.starts_with("Stories/") && t.contains(text)).map(|(n, _)| n.trim_start_matches("Stories/Story_").trim_end_matches(".xml").to_owned()).unwrap()
}

#[test]
fn an_inserted_custom_text_variable_carries_its_text_and_an_unknown_variable_is_refused() {
    let parts = fixture();
    let story = story_of(&parts, "Alpha Heading");
    let mut ed = folio_edit::Editor::new(parts);
    let count = |ed: &folio_edit::Editor| ed.parts().values().map(|t| t.matches("Name=\"Custom Note\" ResultText=\"Custom note\" AssociatedTextVariable=\"dTextVariablenCustom Note\"").count()).sum::<usize>();
    let before = count(&ed);
    ed.apply("v", &[folio_edit::Command::InsertVariable { story: story.clone(), at: 5, variable: "dTextVariablenCustom Note".into() }]).unwrap();
    assert_eq!(count(&ed), before + 1, "the instance with the custom text");
    // in the story's own part, in a range of its own marked as a variable
    let t = ed.parts().get(&format!("Stories/Story_{story}.xml")).unwrap();
    assert!(t.contains("PageNumberType=\"TextVariable\""));
    assert!(ed.apply("v", &[folio_edit::Command::InsertVariable { story, at: 0, variable: "dTextVariablenNo Such".into() }]).is_err());
    ed.undo().unwrap();
    assert_eq!(count(&ed), before);
}

#[test]
fn results_are_stored_for_the_instances_that_differ() {
    let parts = fixture();
    let story = story_of(&parts, "Beta");
    let mut ed = folio_edit::Editor::new(parts);
    ed.apply("v", &[folio_edit::Command::InsertVariable { story, at: 0, variable: "dTextVariablenLast Page Number".into() }]).unwrap();
    let instance = ed.parts().values().find_map(|t| {
        let at = t.find("Name=\"Last Page Number\" ResultText=\"\"")?;
        let from = t[..at].rfind("Self=\"")? + 6;
        Some(t[from..t[from..].find('"')? + from].to_owned())
    }).expect("the new instance (no result yet)");
    ed.apply("results", &[folio_edit::Command::SetVariableResults { results: vec![(instance.clone(), "4".into())] }]).unwrap();
    assert!(ed.parts().values().any(|t| t.contains(&format!("Self=\"{instance}\" Name=\"Last Page Number\" ResultText=\"4\""))));
    // not an instance
    assert!(ed.apply("results", &[folio_edit::Command::SetVariableResults { results: vec![("nope".into(), "4".into())] }]).is_err());
}

// ---- defining and editing a variable (oracle variables var-def-*, var-edit-*) ----

fn names(ed: &folio_edit::Editor) -> Vec<String> {
    let dm = &ed.parts()["designmap.xml"];
    dm.match_indices("<TextVariable Self=\"").map(|(i, _)| {
        let from = dm[i..].find(" Name=\"").unwrap() + i + 7;
        dm[from..dm[from..].find('"').unwrap() + from].to_owned()
    }).collect()
}

fn options(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

#[test]
fn a_new_variable_goes_where_its_name_sorts_with_its_settings_and_is_refused_twice() {
    let mut ed = folio_edit::Editor::new(fixture());
    let before = names(&ed);
    let define = |name: &str, kind: &str, o: &[(&str, &str)]| folio_edit::Command::DefineTextVariable { name: name.into(), kind: kind.into(), options: options(o) };
    ed.apply("v", &[define("Head Upper", "MatchParagraphStyleType", &[("TextBefore", "["), ("TextAfter", "]"), ("AppliedParagraphStyle", "ParagraphStyle/Head"), ("SearchStrategy", "LastOnPage"), ("ChangeCase", "Uppercase"), ("DeleteEndPunctuation", "true")])]).unwrap();
    ed.apply("v", &[define("Zed", "ChapterNumberType", &[])]).unwrap();
    ed.apply("v", &[define("Aardvark", "CustomTextType", &[("Contents", "A & B")])]).unwrap();
    let after = names(&ed);
    let mut sorted = after.clone();
    sorted.sort_by_key(|n| n.to_lowercase());
    assert_eq!(after, sorted, "the list stays sorted by name");
    assert_eq!(after.len(), before.len() + 3);
    let dm = &ed.parts()["designmap.xml"];
    assert!(dm.contains("<MatchParagraphStylePreference TextBefore=\"[\" TextAfter=\"]\" AppliedParagraphStyle=\"ParagraphStyle/Head\" SearchStrategy=\"LastOnPage\" ChangeCase=\"Uppercase\" DeleteEndPunctuation=\"true\" />"));
    assert!(dm.contains("<ChapterNumberVariablePreference TextBefore=\"\" Format=\"Current\" TextAfter=\"\" />"), "the defaults");
    assert!(dm.contains("<Contents type=\"string\">A &amp; B</Contents>"));
    // the same name, a setting the kind does not have, a token that is not InDesign's, a kind that is not built
    assert!(ed.apply("v", &[define("Zed", "ChapterNumberType", &[])]).is_err());
    assert!(ed.apply("v", &[define("Other", "ChapterNumberType", &[("SearchStrategy", "LastOnPage")])]).is_err());
    assert!(ed.apply("v", &[define("Other", "MatchParagraphStyleType", &[("SearchStrategy", "Sometimes")])]).is_err());
    assert!(ed.apply("v", &[define("Other", "XRefPageNumberType", &[])]).is_err());
    assert_eq!(names(&ed).len(), before.len() + 3, "a refused definition changes nothing");
    ed.undo().unwrap();
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(names(&ed), before);
}

#[test]
fn editing_changes_the_settings_in_place_and_a_new_name_moves_it_and_its_instances() {
    let parts = fixture();
    let mut ed = folio_edit::Editor::new(parts);
    let edit = |id: &str, rename: Option<&str>, o: &[(&str, &str)]| folio_edit::Command::EditTextVariable { id: id.into(), rename: rename.map(str::to_owned), options: options(o) };
    let before = names(&ed);
    ed.apply("v", &[edit("dTextVariablenBy Accent", None, &[("SearchStrategy", "LastOnPage")])]).unwrap();
    assert_eq!(names(&ed), before, "same place");
    assert!(ed.parts()["designmap.xml"].contains("AppliedCharacterStyle=\"CharacterStyle/Accent\" SearchStrategy=\"LastOnPage\""), "the other settings stay");
    // the rename: "By Head" → "Heading Text" sorts later; its instances and the parent's placeholder follow
    ed.apply("v", &[edit("dTextVariablenBy Head", Some("Heading Text"), &[("TextBefore", ">> ")])]).unwrap();
    let after = names(&ed);
    assert!(!after.contains(&"By Head".to_owned()) && after.contains(&"Heading Text".to_owned()));
    let mut sorted = after.clone();
    sorted.sort_by_key(|n| n.to_lowercase());
    assert_eq!(after, sorted);
    let stories: String = ed.parts().iter().filter(|(n, _)| n.starts_with("Stories/")).map(|(_, t)| t.as_str()).collect();
    assert!(!stories.contains("By Head") && !stories.contains("dTextVariablenBy Head"));
    assert!(stories.contains("Name=\"Heading Text\" ResultText=\"&gt;&gt; &lt;Heading Text&gt;\" AssociatedTextVariable=\"dTextVariablenHeading Text\""));
    // a name another variable has, a variable that is not there
    assert!(ed.apply("v", &[edit("dTextVariablenHeading Text", Some("By Accent"), &[])]).is_err());
    assert!(ed.apply("v", &[edit("dTextVariablenNo Such", None, &[])]).is_err());
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(names(&ed), before);
    assert!(ed.parts().values().all(|t| !t.contains("Heading Text")));
}

#[test]
fn a_custom_text_edit_reaches_its_instances() {
    let parts = fixture();
    let story = story_of(&parts, "Alpha Heading");
    let mut ed = folio_edit::Editor::new(parts);
    ed.apply("v", &[folio_edit::Command::InsertVariable { story, at: 5, variable: "dTextVariablenCustom Note".into() }]).unwrap();
    ed.apply("v", &[folio_edit::Command::EditTextVariable { id: "dTextVariablenCustom Note".into(), rename: None, options: options(&[("Contents", "New text")]) }]).unwrap();
    let stories: String = ed.parts().iter().filter(|(n, _)| n.starts_with("Stories/")).map(|(_, t)| t.as_str()).collect();
    assert_eq!(stories.matches("AssociatedTextVariable=\"dTextVariablenCustom Note\"").count(), stories.matches("Name=\"Custom Note\" ResultText=\"New text\"").count(), "every instance shows the new text");
    assert!(ed.parts()["designmap.xml"].contains("<Contents type=\"string\">New text</Contents>"));
}

// ---- converting to text and deleting (oracle var-convert-*, var-del-unused) ----

fn all_stories(ed: &folio_edit::Editor) -> String {
    ed.parts().iter().filter(|(n, _)| n.starts_with("Stories/")).map(|(_, t)| t.as_str()).collect()
}

#[test]
fn converting_an_instance_leaves_the_text_it_showed_and_one_undo_brings_it_back() {
    let parts = fixture();
    let story = story_of(&parts, "Alpha Heading");
    let mut ed = folio_edit::Editor::new(parts);
    ed.apply("v", &[folio_edit::Command::InsertVariable { story: story.clone(), at: 5, variable: "dTextVariablenCustom Note".into() }]).unwrap();
    let before = ed.parts().get(&format!("Stories/Story_{story}.xml")).unwrap().clone();
    // the range has no instance: refused; the instance's own character: converted
    assert!(ed.apply("v", &[folio_edit::Command::ConvertVariableToText { story: story.clone(), start: 0, end: 5 }]).is_err());
    ed.apply("v", &[folio_edit::Command::ConvertVariableToText { story: story.clone(), start: 5, end: 6 }]).unwrap();
    let t = ed.parts().get(&format!("Stories/Story_{story}.xml")).unwrap();
    assert!(t.contains("Alpha Custom note") || t.contains("Custom note"), "the text stays: {t}");
    assert!(!t.contains("<TextVariableInstance") && !t.contains("PageNumberType=\"TextVariable\""), "no instance, no variable mark left");
    assert_eq!(folio_edit::text::contents(t, &story).unwrap().chars().filter(|&c| c == '\u{18}').count(), 0);
    ed.undo().unwrap();
    assert_eq!(ed.parts().get(&format!("Stories/Story_{story}.xml")).unwrap(), &before);
}

#[test]
fn deleting_a_variable_removes_it_and_turns_its_instances_into_text() {
    let mut ed = folio_edit::Editor::new(fixture());
    let before = names(&ed);
    let used = all_stories(&ed).matches("AssociatedTextVariable=\"dTextVariablenCustom Note\"").count();
    assert!(used > 0, "the fixture's parents hold instances of it");
    ed.apply("v", &[folio_edit::Command::DeleteTextVariable { id: "dTextVariablenCustom Note".into() }]).unwrap();
    assert_eq!(names(&ed).len(), before.len() - 1);
    assert!(!names(&ed).contains(&"Custom Note".to_owned()));
    let stories = all_stories(&ed);
    assert!(!stories.contains("dTextVariablenCustom Note"), "no instance left");
    assert_eq!(stories.matches("Custom note").count(), used, "each one is the text it showed");
    // a running header's placeholder in a parent is no text: it just goes
    ed.apply("v", &[folio_edit::Command::DeleteTextVariable { id: "dTextVariablenBy Head".into() }]).unwrap();
    let stories = all_stories(&ed);
    assert!(!stories.contains("By Head") && !stories.contains("&lt;By Head&gt;"), "placeholders are not kept as text");
    assert!(ed.apply("v", &[folio_edit::Command::DeleteTextVariable { id: "dTextVariablenNo Such".into() }]).is_err());
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(names(&ed), before);
}
