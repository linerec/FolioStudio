//! Layout > Table of Contents and Table of Contents Styles on the shared dialog frame (`toc`, `tocStyles`): the style the picker names
//! (none = a new one) as the boxes, the entries as rows, a row added taking the first paragraph style not listed yet, and Load… from an
//! IDML file of the project. Saving and making the TOC are the screen's commands (`saveTOCStyle`, then `createTOC` with the id the saved
//! style has: the dialog's `$then`).
use super::{Refusal, refuse};
use serde_json::{Map, Value, json};

/// The TOC style InDesign names `$ID/DefaultTOCStyleName` (shown as a screen text).
const DEFAULT_NAME: &str = "$ID/DefaultTOCStyleName";

/// `tocStyles`: the document's TOC styles by their ids.
pub(super) fn choices(s: &crate::Session) -> Vec<Value> {
    s.doc.toc_styles.iter().map(|t| if t.name == DEFAULT_NAME { json!({"value": t.id, "label": "기본 목차 스타일", "translate": true}) } else { json!({"value": t.id, "label": t.name}) }).collect()
}

/// `tocStyle`: the boxes of the style the picker (`style`) names; none, a new style named `namePrefix` (the first free of it, "… 2",
/// "… 3", as the screen named one) with the title `titleDefault`. A style the lists do not offer is the default style (`""`).
pub(super) fn defaults(s: &crate::Session, requested: &Value) -> Result<(Map<String, Value>, Value), Refusal> {
    let info = super::document_info(s)?;
    let ids = |list: &str| -> Vec<String> { info[list].as_array().into_iter().flatten().filter_map(|x| x["id"].as_str().map(str::to_owned)).collect() };
    let (paragraphs, characters) = (ids("paragraphStyles"), ids("characterStyles"));
    // an entry names its paragraph style by name (as saveTOCStyle writes it) or id: the id the list offers
    let paragraph = |name: &str| -> String {
        if paragraphs.iter().any(|p| p == name) { return name.to_owned(); }
        s.doc.styles.paragraph.iter().find(|p| p.name == name).map(|p| p.id.clone()).filter(|id| paragraphs.contains(id)).unwrap_or_default()
    };
    let known = |id: &str, list: &[String]| if list.iter().any(|x| x == id) { id.to_owned() } else { String::new() };
    let word = |key: &str, fallback: &str| requested[key].as_str().filter(|t| !t.trim().is_empty()).unwrap_or(fallback).to_owned();
    let id = requested["style"].as_str().unwrap_or("");
    let mut values = Map::new();
    let style = if id.is_empty() {
        let prefix = word("namePrefix", "New Table of Contents Style");
        let taken = |name: &str| s.doc.toc_styles.iter().any(|t| t.name == name);
        let name = std::iter::once(prefix.clone()).chain((2..).map(|n| format!("{prefix} {n}"))).find(|n| !taken(n)).unwrap_or_default();
        folio_model::toc::TocStyle { name, title: word("titleDefault", "Contents"), create_bookmarks: true, numbered_paragraphs: "IncludeFullParagraph".into(), ..Default::default() }
    } else {
        s.doc.toc_styles.iter().find(|t| t.id == id).cloned().ok_or_else(|| refuse("style", "unknownItem", format!("unknown TOC style: {id}")))?
    };
    values.insert("name".into(), json!(style.name));
    values.insert("title".into(), json!(style.title));
    values.insert("titleStyle".into(), json!(known(&style.title_style, &paragraphs)));
    let entries: Vec<Value> = style.entries.iter().map(|e| json!({
        "name": paragraph(&e.name), "formatStyle": known(&e.format_style, &paragraphs), "level": e.level.clamp(1, 10),
        "pageNumberPosition": e.page_number_position, "pageNumberStyle": known(&e.page_number_style, &characters), "separator": e.separator,
        "separatorStyle": known(&e.separator_style, &characters), "sortAlphabet": e.sort_alphabet,
    })).collect();
    values.insert("entries".into(), json!(entries));
    for (key, on) in [("includeHidden", style.include_hidden), ("includeBookDocuments", style.include_book_documents), ("createBookmarks", style.create_bookmarks),
        ("makeAnchor", style.make_anchor), ("runIn", style.run_in), ("removeForcedLineBreak", style.remove_forced_line_break)] {
        values.insert(key.into(), json!(on));
    }
    let numbered = if style.numbered_paragraphs.is_empty() { "IncludeFullParagraph" } else { style.numbered_paragraphs.as_str() };
    values.insert("numberedParagraphs".into(), json!(numbered));
    // a style a TOC story uses cannot be deleted (deleteTOCStyle refuses it too)
    values.insert("inUse".into(), json!(!id.is_empty() && s.doc.toc_stories.values().any(|used| used == id)));
    Ok((values, json!({})))
}

/// The entries kept in step (`link: "tocEntries"`): a row added without a style takes the first paragraph style the rows do not list
/// (as picking it from "Include Paragraph Styles" did), its entry style that one too; a row whose style changes takes the new one as its
/// entry style while the entry style was the old one.
pub(super) fn linked(before: &Value, values: &mut Value, changed: &str, choices: &Value) {
    if changed != "entries" { return; }
    let was = before["entries"].as_array().cloned().unwrap_or_default();
    let Some(rows) = values["entries"].as_array_mut() else { return };
    let listed: Vec<String> = rows.iter().filter_map(|r| r["name"].as_str().filter(|n| !n.is_empty()).map(str::to_owned)).collect();
    let free = choices["paragraphStyles"].as_array().into_iter().flatten().filter_map(|c| c["value"].as_str()).find(|id| !listed.iter().any(|l| l == id)).map(str::to_owned);
    if rows.len() == was.len() + 1 {
        if let Some(row) = rows.iter_mut().find(|r| r["name"] == "") {
            if let Some(id) = &free { row["name"] = json!(id); if row["formatStyle"] == "" { row["formatStyle"] = json!(id); } }
        }
    } else if rows.len() == was.len() {
        for (row, old) in rows.iter_mut().zip(&was) {
            if row["name"] != old["name"] && old["formatStyle"] == old["name"] && row["formatStyle"] == old["formatStyle"] { row["formatStyle"] = row["name"].clone(); }
        }
    }
}

/// `tocStylesFile` (Load…): the style parts of an IDML file of the project (`path`, relative to the project root), as `tocImportStyles`
/// gives them for `importTOCStyles`.
pub(super) fn styles_file(args: &Value, s: &crate::Session) -> Result<Value, Refusal> {
    let text = args["path"].as_str().unwrap_or("").trim();
    if text.is_empty() { return Err(refuse("importPath", "required", "importPath is required".into())); }
    let f = s.folio.as_ref().ok_or("open a project document first")?;
    let root = f.opened.project.root.canonicalize().map_err(|e| e.to_string())?;
    let rel = folio_project::RelPath::parse(text).map_err(|e| refuse("importPath", "badPath", format!("importPath: {e}")))?;
    let path = rel.to_native(&root).canonicalize().map_err(|_| refuse("importPath", "notFound", format!("no file {text} in this project")))?;
    if !path.starts_with(&root) || path.extension().and_then(|e| e.to_str()).is_none_or(|e| !e.eq_ignore_ascii_case("idml")) {
        return Err(refuse("importPath", "notIdml", "choose an IDML file inside this project".into()));
    }
    Ok(crate::toc::import_styles(&json!({"path": path.to_string_lossy()}))?)
}
