//! What a cross-reference format shows for a destination (InDesign 2025, measured: `next/oracle/rules/xref.ko.md`).
//! Page and paragraph numbers come from the composed document (`locations`, supplied by the caller as for the TOC).
use super::*;
use folio_model::{hyperlinks::{BuildingBlock, CrossReferenceFormat, Destination}, toc::TocSource};

/// A piece of the reference's text: plain text, or the page number (a `TV XRefPageNumber` instance).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Piece {
    Text(String, String),
    Page(String, String),
    /// The chapter number (a `TV XRefChapterNumber` instance; measured, buildc2-xref-probe).
    Chapter(String, String),
}

const PAGE_VARIABLE: &str = "&lt;?AID 001b?&gt;TV XRefPageNumber";
const CHAPTER_VARIABLE: &str = "&lt;?AID 001b?&gt;TV XRefChapterNumber";
pub(super) const NO_STYLE: &str = "CharacterStyle/$ID/[No character style]";

fn unsupported(s: &str) -> EditError { EditError::new("unsupported", s) }
fn plain(style: &str) -> bool { matches!(style, "" | "n") }

/// The paragraph text of the destination up to the block's delimiter (measured with `:`, with and without it).
fn up_to(text: &str, b: &BuildingBlock) -> Result<String,EditError> {
    let d = b.delimiter.strip_prefix("$ID/").unwrap_or(&b.delimiter);
    let mut chars = d.chars();
    let Some(c) = chars.next() else { return Ok(text.to_owned()) };
    if chars.next().is_some() || c == '^' { return Err(unsupported("cross-reference delimiters other than one character require additional measurement")); }
    // a delimiter the paragraph does not have: the whole paragraph (measured, buildc2-xref-probe `absent`)
    let Some(at) = text.find(c) else { return Ok(text.to_owned()) };
    Ok(text[..at + if b.include_delimiter { c.len_utf8() } else { 0 }].to_owned())
}

/// The list label of the paragraph, or none: a bullet is not a number (measured: Paragraph Number is one space, Full Paragraph the text).
fn label(location: &TocSource) -> &str {
    if location.number.chars().any(char::is_alphanumeric) { &location.number } else { "" }
}

/// Paragraph Number: the label without its tab and trailing punctuation. Measured: `.` `:` `-` `,` `?` `、` `–` `*` `．` go,
/// `)` and `>` stay, a letter stays (`제11장`). A paragraph without number gives one space.
fn number(label: &str) -> Result<String,EditError> {
    let s = label.trim_end();
    let Some(last) = s.chars().last() else { return Ok(" ".to_owned()) };
    let strip = |c: char| matches!(c, '.' | ':' | '-' | ',' | '?' | '\u{3001}' | '\u{2013}' | '*' | '\u{ff0e}');
    if last.is_alphanumeric() || matches!(last, ')' | '>') { return Ok(s.to_owned()); }
    if strip(last) {
        let rest = &s[..s.len() - last.len_utf8()];
        if rest.chars().last().is_some_and(strip) { return Err(unsupported("paragraph number with repeated trailing punctuation requires additional measurement")); }
        return Ok(rest.to_owned());
    }
    Err(unsupported("paragraph number ending in this punctuation requires additional measurement"))
}

fn push_text(pieces: &mut Vec<Piece>, s: String, style: &str) {
    match pieces.last_mut() { Some(Piece::Text(t, cs)) if cs == style => t.push_str(&s), _ => pieces.push(Piece::Text(s, style.into())) }
}

/// The language `[No paragraph style]` applies (`Resources/Styles.xml`), the document's text language.
fn default_language(w: &Work<'_>) -> Option<String> {
    let t = w.text("Resources/Styles.xml").ok()?;
    let ix = Index::scan(t).ok()?;
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "ParagraphStyle" && ix.attr(t, i, "Self") == Some("ParagraphStyle/$ID/[No paragraph style]"))
        .and_then(|i| ix.value(t, i, "AppliedLanguage"))
}

/// Whether a straight double quote after `prev` (the character before it in the story; none at a paragraph's start) is an opening
/// one: InDesign's typographer's quotes look at the character before (measured, buildc2-xref-probe `same.text`: a reference right
/// after a non-space character gets a closing quote first).
fn opens_after(prev: Option<char>) -> bool {
    prev.is_none_or(|c| c.is_whitespace() || matches!(c, '\r' | '(' | '[' | '{' | '\u{201c}' | '\u{2018}' | '\u{ab}' | '\u{2014}' | '\u{2013}' | '\u{feff}'))
}

pub(super) fn render(w: &mut Work<'_>, dest: &Destination, fmt: &CrossReferenceFormat, locations: &[TocSource]) -> Result<Vec<Piece>,EditError> {
    render_after(w, dest, fmt, locations, None)
}

/// `render` for a reference whose story has `prev` just before it (the quotes of its custom text follow it).
pub(super) fn render_after(w: &mut Work<'_>, dest: &Destination, fmt: &CrossReferenceFormat, locations: &[TocSource], prev: Option<char>) -> Result<Vec<Piece>,EditError> {
    if dest.story.is_empty() { return Err(unsupported("cross-reference requires a text destination")); }
    let (p, _, _) = w.find(&dest.story)?;
    let text = folio_idml::editorial::contents(w.text(&p)?, &dest.story).map_err(|e| EditError::new("xml", e))?;
    let chars = text.chars().collect::<Vec<_>>();
    if dest.at > chars.len() { return Err(invalid("destination outside story")); }
    let start = chars[..dest.at].iter().rposition(|&c| c == '\r').map_or(0, |i| i + 1);
    let end = chars[dest.at..].iter().position(|&c| c == '\r').map_or(chars.len(), |i| dest.at + i);
    let paragraph = chars[start..end].iter().collect::<String>();
    let index = chars[..start].iter().filter(|&&c| c == '\r').count();
    let location = || locations.iter().find(|l| l.story == dest.story && l.paragraph == index)
        .ok_or_else(|| EditError::new("staleLayout", "the destination paragraph has no composed page; compose the document and try again"));
    let mut pieces: Vec<Piece> = Vec::new();
    let mut last = prev;
    for b in &fmt.blocks {
        let style = if !plain(&b.character_style) { &b.character_style } else if !plain(&fmt.character_style) { &fmt.character_style } else { NO_STYLE };
        if !b.delimiter.strip_prefix("$ID/").unwrap_or(&b.delimiter).is_empty() && !matches!(b.kind.as_str(), "ParagraphTextBuildingBlock" | "CustomStringBuildingBlock") {
            return Err(unsupported("cross-reference delimiters on this block require additional measurement"));
        }
        let value = match b.kind.as_str() {
            "CustomStringBuildingBlock" => if b.text == "$ID/" { String::new() } else { b.text.clone() },
            "ParagraphTextBuildingBlock" => up_to(&paragraph, b)?,
            "BookmarkNameBuildingBlock" => dest.name.clone(),
            "ParagraphNumberBuildingBlock" => number(label(location()?))?,
            "FullParagraphBuildingBlock" => format!("{}{paragraph}", label(location()?).replace('\t', " ")),
            "PageNumberBuildingBlock" => { last = Some('\u{18}'); pieces.push(Piece::Page(location()?.page.clone(), style.into())); continue; }
            // the document's chapter number, as the Chapter Number variable shows it (measured: a variable instance, `ChapterNumberPreference`)
            "ChapterNumberBuildingBlock" => { last = Some('\u{18}'); pieces.push(Piece::Chapter(chapter_number(w)?, style.into())); continue; }
            // the document's file name with its extension (measured: `xref-probe.indd`)
            "FileNameBuildingBlock" => w.ed.file_name().to_owned(),
            _ => return Err(unsupported(&format!("cross-reference block {} is not supported yet", b.kind))),
        };
        if b.kind == "CustomStringBuildingBlock" && value.contains('"') {
            let prefs = w.text("Resources/Preferences.xml")?;
            let pi = Index::scan(prefs).map_err(|e| EditError::new("xml", e))?;
            let smart = (0..pi.nodes.len()).find(|&i| pi.local(prefs, i) == "TextPreference")
                .and_then(|i| pi.value(prefs, i, "TypographersQuotes")).as_deref() != Some("false");
            if smart {
                let t = w.text("designmap.xml")?; let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
                let languages = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Language").collect::<Vec<_>>();
                // several languages (every new InDesign document lists them): the document's text language, the one
                // [No paragraph style] applies (buildc2-xref-probe: English: USA, \u{201c} \u{201d})
                let language = if languages.len() == 1 { Some(languages[0]) } else {
                    let base = default_language(w);
                    languages.iter().copied().find(|&i| base.as_deref().is_some_and(|b| ix.value(t, i, "Name").as_deref() == Some(b)))
                };
                let Some(language) = language else { return Err(unsupported("quoted cross-reference formats in multilingual documents require additional measurement")); };
                let quotes = ix.value(t, language, "DoubleQuotes").unwrap_or_default().chars().collect::<Vec<_>>();
                if quotes.len() != 2 { return Err(invalid("missing language quote pair")); }
                let mut out = String::new();
                for c in value.chars() { let q = if c == '"' { quotes[if opens_after(last) { 0 } else { 1 }] } else { c }; out.push(q); last = Some(q); }
                push_text(&mut pieces, out, style);
                continue;
            }
        }
        if let Some(c) = value.chars().last() { last = Some(c); }
        push_text(&mut pieces, value, style);
    }
    Ok(pieces)
}

/// The text of a reference that is only text (the shape every reference had before page numbers).
pub(super) fn only_text(pieces: &[Piece]) -> Option<String> {
    match pieces {
        [] => Some(String::new()),
        [Piece::Text(t, style)] if style == NO_STYLE => Some(t.clone()),
        _ => None,
    }
}

pub(super) fn styled_text(pieces: &[Piece]) -> Option<(&str,&str)> {
    match pieces { [Piece::Text(t,style)] if style!=NO_STYLE => Some((t,style)), _ => None }
}

fn instance(w: &mut Work<'_>, id: Option<String>, page: &str) -> String {
    variable_instance(w, id, PAGE_VARIABLE, page)
}

fn variable_instance(w: &mut Work<'_>, id: Option<String>, variable: &str, text: &str) -> String {
    let id = id.unwrap_or_else(|| w.new_id());
    format!("<TextVariableInstance Self=\"{}\" Name=\"{variable}\" ResultText=\"{}\" AssociatedTextVariable=\"dTextVariablen{variable}\" />", esc(&id), esc(text))
}

/// The document's chapter number (`ChapterNumberPreference/@ChapterNumber`, 1 when absent), as the Chapter Number variable shows it.
fn chapter_number(w: &Work<'_>) -> Result<String,EditError> {
    let prefs = w.text("Resources/Preferences.xml").unwrap_or("");
    Ok(folio_idml::chapter::read(prefs).map(|(n, _)| n).unwrap_or_else(|_| "1".into()))
}

/// A reference that is only the page number sits inside a range of its own; any other mix is a run per piece inside the source.
pub(super) fn only_page(pieces: &[Piece]) -> Option<(&str, &str)> {
    match pieces { [Piece::Page(p, style)] => Some((p, style)), _ => None }
}

pub(super) fn empty_leading_range(pieces: &[Piece]) -> String {
    match pieces.first() {
        Some(Piece::Text(_, style) | Piece::Page(_, style) | Piece::Chapter(_, style)) if style != NO_STYLE => format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\" />", esc(style)),
        _ => String::new(),
    }
}

pub(super) fn has_page(pieces: &[Piece]) -> bool { pieces.iter().any(|p| matches!(p, Piece::Page(..))) }
pub(super) fn has_chapter(pieces: &[Piece]) -> bool { pieces.iter().any(|p| matches!(p, Piece::Chapter(..))) }

/// The inside of a source made of several pieces, reusing the page number instances `ids` already has.
pub(super) fn runs(w: &mut Work<'_>, pieces: &[Piece], ids: &mut Vec<String>) -> String {
    pieces.iter().map(|p| match p {
        Piece::Text(t, style) => format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\"><Content>{}</Content></CharacterStyleRange>", esc(style), esc(t)),
        Piece::Page(page, style) => {
            let id = if ids.is_empty() { None } else { Some(ids.remove(0)) };
            format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\" PageNumberType=\"TextVariable\">{}</CharacterStyleRange>", esc(style), instance(w, id, page))
        }
        Piece::Chapter(number, style) => {
            let id = if ids.is_empty() { None } else { Some(ids.remove(0)) };
            format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\" PageNumberType=\"TextVariable\">{}</CharacterStyleRange>", esc(style), variable_instance(w, id, CHAPTER_VARIABLE, number))
        }
    }).collect()
}

pub(super) fn instance_xml(w: &mut Work<'_>, id: Option<String>, page: &str) -> String { instance(w, id, page) }

/// The document must define the variable the instances name (InDesign's documents do).
pub(super) fn check_variable(w: &Work<'_>) -> Result<(),EditError> {
    if w.text("designmap.xml")?.contains(&format!("Self=\"dTextVariablen{PAGE_VARIABLE}\"")) { Ok(()) }
    else { Err(unsupported("the document has no cross-reference page number variable")) }
}

/// The same for the chapter number variable (`XrefChapterNumberType`, which InDesign's documents define).
pub(super) fn check_chapter_variable(w: &Work<'_>) -> Result<(),EditError> {
    if w.text("designmap.xml")?.contains(&format!("Self=\"dTextVariablen{CHAPTER_VARIABLE}\"")) { Ok(()) }
    else { Err(unsupported("the document has no cross-reference chapter number variable")) }
}

/// Only a range without local formatting can hold a reference with page numbers (the character style InDesign wrote was measured there).
pub(super) fn check_range(open: &str) -> Result<(),EditError> {
    let plain_range = format!("<CharacterStyleRange AppliedCharacterStyle=\"{NO_STYLE}\">");
    if open == plain_range { Ok(()) } else { Err(unsupported("a page number needs the plain character range around the insertion point (local formatting requires additional measurement)")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_number_drops_the_tab_and_trailing_punctuation() {
        // measured in InDesign 2025: label (result) pairs of the numbering probes
        for (label, result) in [("1.\t", "1"), ("4.1.\t", "4.1"), ("(1)\t", "(1)"), ("Step 2:\t", "Step 2"), ("8)\t", "8)"), ("vii.\t", "vii"),
            ("F.\t", "F"), ("1-\t", "1"), ("3,\t", "3"), ("7?\t", "7"), ("5>\t", "5>"), ("9\u{3001}\t", "9"), ("19\u{2013}\t", "19"),
            ("21*\t", "21"), ("23\u{ff0e}\t", "23"), ("15. \t", "15"), ("\u{c81c}11\u{c7a5}\t", "\u{c81c}11\u{c7a5}"), ("", " ")] {
            assert_eq!(number(label).unwrap(), result, "{label:?}");
        }
        // not measured: refused instead of guessed
        assert!(number("12..\t").is_err());
        assert!(number("8\u{b7}\t").is_err());
    }

    #[test]
    fn a_bullet_is_not_a_number() {
        let at = |number: &str| TocSource { story: String::new(), paragraph: 0, page: String::new(), page_index: 0, hidden: false, number: number.into(), book: None };
        assert_eq!(label(&at("\u{2022}\t")), "");
        assert_eq!(label(&at("2.\t")), "2.\t");
    }

    #[test]
    fn delimiter_cuts_the_paragraph_text() {
        let block = |include| BuildingBlock { kind: "ParagraphTextBuildingBlock".into(), delimiter: "$ID/:".into(), include_delimiter: include, ..Default::default() };
        assert_eq!(up_to("Numbered two: with delimiter", &block(false)).unwrap(), "Numbered two");
        assert_eq!(up_to("Numbered two: with delimiter", &block(true)).unwrap(), "Numbered two:");
        // measured (buildc2-xref-probe `absent`): no delimiter in the paragraph, the whole paragraph
        assert_eq!(up_to("Plain heading without colon", &block(false)).unwrap(), "Plain heading without colon");
    }
}
