//! Place ▸ a Word file: what the file defines comes into the document before its text does (oracle placetext-c2). Each colour is a
//! new swatch (`Word_R255_G0_B0`, RGB), each style a new style marked `Imported`, added at the end of the root style group in the order
//! of the file's styles. A style or swatch the document already has is kept as it is (the clash rule's default, "use existing").
use super::*;
use folio_textimport::{StyleDef, TextSource};

const STYLES: &str = "Resources/Styles.xml";
const GRAPHIC: &str = "Resources/Graphic.xml";

/// `ParagraphStyle/<name>`; a colon of a style group's path is written `%3a` (the story writer does the same: `text/import.rs`).
fn style_id(tag: &str, name: &str) -> String {
    format!("{tag}/{}", name.replace(':', "%3a"))
}

fn element(tag: &str, def: &StyleDef, indent: &str, base_font: Option<&str>) -> String {
    let attr = |k: &str, v: &str| format!(" {k}=\"{}\"", escape(v));
    let mut open = format!("<{tag}");
    open += &attr("Self", &style_id(tag, &def.name));
    open += &attr("Name", &def.name);
    open += &attr("Imported", "true");
    if tag == "ParagraphStyle" {
        open += &attr("NextStyle", &style_id(tag, def.next.as_deref().unwrap_or(&def.name)));
    }
    for (k, v) in [("SplitDocument", "false"), ("EmitCss", "true"), ("IncludeClass", "true"), ("ExtendedKeyboardShortcut", "0 0 0")] {
        open += &attr(k, v);
    }
    if tag == "ParagraphStyle" {
        for k in ["EmptyNestedStyles", "EmptyLineStyles", "EmptyGrepStyles"] {
            open += &attr(k, "true");
        }
    }
    open += &attr("KeyboardShortcut", "0 0");
    for (k, v) in &def.attrs {
        open += &attr(k, v);
    }
    let none = if tag == "ParagraphStyle" { "$ID/[No paragraph style]" } else { "$ID/[No character style]" };
    let mut lines = vec![match &def.based_on {
        Some(b) => format!("<BasedOn type=\"object\">{}</BasedOn>", escape(&style_id(tag, b))),
        None => format!("<BasedOn type=\"string\">{none}</BasedOn>"),
    }];
    lines.push("<PreviewColor type=\"enumeration\">Nothing</PreviewColor>".to_owned());
    for (name, kind, text) in &def.properties {
        // a style based on none names its font only when it is not the document's own (measured: docxtext-c4-rtf with the Arial fixture)
        if name == "AppliedFont" && def.based_on.is_none() && base_font == Some(text.as_str()) {
            continue;
        }
        // `raw`: the text is the element as it stands (an RTF style's tab list)
        lines.push(if kind == "raw" { text.clone() } else { format!("<{name} type=\"{}\">{}</{name}>", escape(kind), escape(text)) });
    }
    let body: Vec<String> = lines.iter().map(|l| format!("{indent}\t\t{l}")).collect();
    format!("{open}>\n{indent}\t<Properties>\n{}\n{indent}\t</Properties>\n{indent}</{tag}>", body.join("\n"))
}

fn bring_styles(w: &mut Work<'_>, tag: &str, root: &str, defs: &[StyleDef], base_font: Option<&str>) -> Result<(), EditError> {
    for def in defs {
        let id = style_id(tag, &def.name);
        let t = w.text(STYLES)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        if ix.by_self(&t, &id).is_some() {
            continue;
        }
        let top = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == root).ok_or_else(|| EditError::new("notFound", root))?;
        // a style named `Group:Name` stands in the group `Group` (one level: the lists of a Word file are the only ones), made when it is not there
        let edit = match def.name.rsplit_once(':') {
            Some((group, _)) => {
                let group_tag = format!("{tag}Group");
                let group_id = format!("{group_tag}/$ID/{}", group.replace(':', "%3a"));
                match ix.by_self(&t, &group_id) {
                    Some(g) => super::tablestylegroups::append(&t, &ix, g, &|inner| element(tag, def, inner, base_font), Some(tag)),
                    None => super::tablestylegroups::append(
                        &t,
                        &ix,
                        top,
                        &|inner| {
                            let deeper = format!("{inner}\t");
                            format!("<{group_tag} Self=\"{}\" Name=\"$ID/{}\">\n{deeper}{}\n{inner}</{group_tag}>", escape(&group_id), escape(group), element(tag, def, &deeper, base_font))
                        },
                        None,
                    ),
                }
            }
            None => super::tablestylegroups::append(&t, &ix, top, &|inner| element(tag, def, inner, base_font), Some(tag)),
        };
        w.splice(STYLES, vec![edit])?;
    }
    Ok(())
}

/// The numbering lists a Word file's numbered paragraphs belong to (`Word Imported List 1` ...): elements of the document map after the
/// lists it has (measured: docxtext-c1 lists, they do not continue across stories).
fn bring_numbering_lists(w: &mut Work<'_>, names: &[String]) -> Result<(), EditError> {
    const DM: &str = "designmap.xml";
    for name in names {
        let t = w.text(DM)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let id = format!("NumberingList/{name}");
        if ix.by_self(&t, &id).is_some() {
            continue;
        }
        let element = format!("<NumberingList Self=\"{}\" Name=\"{}\" ContinueNumbersAcrossStories=\"false\" ContinueNumbersAcrossDocuments=\"false\" />", escape(&id), escape(name));
        match (0..ix.nodes.len()).rfind(|&i| ix.local(&t, i) == "NumberingList") {
            Some(last) => {
                let start = ix.nodes[last].start;
                let line = t[..start].rfind('\n').map_or(0, |p| p + 1);
                let indent = t[line..start].to_owned();
                let at = ix.nodes[last].end;
                w.splice(DM, vec![(at..at, format!("\n{indent}{element}"))])?;
            }
            None => {
                let at = ix.nodes[0].close_start;
                w.splice(DM, vec![(at..at, format!("\t{element}\n"))])?;
            }
        }
    }
    Ok(())
}

/// The kinsoku tables a Word import leaves in the document's map: InDesign's five (empty, named by their `$ID` keys) and an empty
/// `Word_Kinsoku` of its own, after the Fonts part (measured: placetext-docx-*, the document had none before).
const KINSOKU: [&str; 6] = [
    "<KinsokuTable Self=\"KinsokuTable/$ID/kHardKinsokuName\" Name=\"$ID/kHardKinsokuName\" />",
    "<KinsokuTable Self=\"KinsokuTable/$ID/kSoftKinsokuName\" Name=\"$ID/kSoftKinsokuName\" />",
    "<KinsokuTable Self=\"KinsokuTable/$ID/kKoreanKinsokuName\" Name=\"$ID/kKoreanKinsokuName\" />",
    "<KinsokuTable Self=\"KinsokuTable/$ID/kSimpChineseKinsokuName\" Name=\"$ID/kSimpChineseKinsokuName\" />",
    "<KinsokuTable Self=\"KinsokuTable/$ID/kTradChineseKinsokuName\" Name=\"$ID/kTradChineseKinsokuName\" />",
    "<KinsokuTable Self=\"KinsokuTable/Word_Kinsoku\" Name=\"Word_Kinsoku\" CantBeginLineChars=\"\" CantEndLineChars=\"\" HangingPunctuationChars=\"\" CantBeSeparatedChars=\"\" />",
];

fn bring_kinsoku(w: &mut Work<'_>) -> Result<(), EditError> {
    const DM: &str = "designmap.xml";
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let fonts = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Fonts").ok_or_else(|| EditError::new("unsupported", "designmap has no Fonts part"))?;
    let indent = {
        let start = ix.nodes[fonts].start;
        let line = t[..start].rfind('\n').map_or(0, |p| p + 1);
        t[line..start].to_owned()
    };
    let missing: String = KINSOKU
        .iter()
        .filter(|e| ix.by_self(&t, e.split('"').nth(1).unwrap_or_default()).is_none())
        .map(|e| format!("\n{indent}{e}"))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let at = ix.nodes[fonts].end;
    w.splice(DM, vec![(at..at, missing)])
}

pub(super) fn bring_in(w: &mut Work<'_>, source: &TextSource) -> Result<(), EditError> {
    if source.formatted {
        bring_kinsoku(w)?;
    }
    for color in &source.colors {
        let id = swatches::self_of("Color", &color.name);
        let (t, ix) = swatches::scan(w, GRAPHIC)?;
        if ix.by_self(&t, &id).is_some() {
            continue;
        }
        swatches::new_swatch_in(w, &color.name, false, "RGB", &color.rgb.map(f64::from), color.grouped)?;
    }
    let base = crate::text::normal_font(w)?;
    for family in source.font_families().iter().filter(|f| base.as_deref() != Some(f.as_str())) {
        crate::text::register_family(w, family)?;
    }
    let doc_font = crate::text::default_font(w)?;
    bring_styles(w, "ParagraphStyle", "RootParagraphStyleGroup", &source.paragraph_styles, doc_font.as_deref())?;
    bring_styles(w, "CharacterStyle", "RootCharacterStyleGroup", &source.character_styles, doc_font.as_deref())?;
    bring_numbering_lists(w, &source.numbering_lists)
}
