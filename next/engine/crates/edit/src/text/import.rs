//! A text file placed into a story (oracle `placetext-c1`): the text replaces the story's whole text, as
//! `frame.place(file)` does. The new paragraphs take the document's `[No Paragraph Style]` and no character
//! formatting (the old story's styles and overrides are gone); a text that ends with a return leaves one empty
//! paragraph after it, in the paragraph style the old story began with. A Word file brings its styles and local formatting
//! (oracle placetext-c2): the paragraphs and runs name their styles and carry the attributes InDesign would write.
use super::*;
use folio_textimport::{Inline, Paragraph, Table, TextSource};

const NORMAL_ID: &str = "ParagraphStyle/$ID/NormalParagraphStyle";
const NO_CHARACTER_STYLE_ID: &str = "CharacterStyle/$ID/[No character style]";

/// `ParagraphStyle/<name>`: a colon of a style group's path is written `%3a`.
fn style_id(tag: &str, name: &str) -> String {
    format!("{tag}/{}", name.replace(':', "%3a"))
}

/// The font a paragraph of the Normal paragraph style has (its `AppliedFont`); a run of an RTF file that names the same font needs no override of it.
pub(crate) fn normal_font(w: &Work<'_>) -> Result<Option<String>, EditError> {
    let t = w.text("Resources/Styles.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let Some(style) = ix.by_self(t, NORMAL_ID) else { return Ok(None) };
    for &props in ix.children(style).iter().filter(|&&c| ix.local(t, c) == "Properties") {
        if let Some(&c) = ix.children(props).iter().find(|&&c| ix.local(t, c) == "AppliedFont") {
            return Ok(Some(unescape(&t[ix.nodes[c].open_end..ix.nodes[c].close_start])));
        }
    }
    Ok(None)
}

/// The font of the document's `[No paragraph style]` (the document's default font): a style that is based on none and names this font needs no `AppliedFont`.
pub(crate) fn default_font(w: &Work<'_>) -> Result<Option<String>, EditError> {
    let t = w.text("Resources/Styles.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let Some(style) = ix.by_self(t, "ParagraphStyle/$ID/[No paragraph style]") else { return Ok(None) };
    for &props in ix.children(style).iter().filter(|&&c| ix.local(t, c) == "Properties") {
        if let Some(&c) = ix.children(props).iter().find(|&&c| ix.local(t, c) == "AppliedFont") {
            return Ok(Some(unescape(&t[ix.nodes[c].open_end..ix.nodes[c].close_start])));
        }
    }
    Ok(None)
}

/// A range style written like `like` (its indentation): `applied` style and the local `attrs`, then `properties` (verbatim child XML).
fn ranged(like: &Style, tag: &str, applied: (&str, String), attrs: &[(String, String)], properties: String) -> Style {
    let mut all = vec![(applied.0.to_owned(), applied.1)];
    all.extend(attrs.iter().cloned());
    let open = format!("<{tag}{}>", all.iter().map(|(k, v)| format!(" {k}=\"{}\"", crate::xml::escape(v))).collect::<String>());
    all.sort();
    let key = format!("{all:?}{properties}");
    Style { prefix: format!("{open}{properties}{}", like.separator), suffix: like.suffix.clone(), separator: like.separator.clone(), open, properties, key }
}

/// `<Properties>` of a paragraph range that has tab stops (the rest of InDesign's tab record as it writes it) or a list (`extra`: its
/// children as they are, after the tab list).
fn tab_properties(tabs: &[(String, f64)], extra: &str) -> String {
    if tabs.is_empty() && extra.is_empty() {
        return String::new();
    }
    let items: String = tabs
        .iter()
        .map(|(align, p)| format!("<ListItem type=\"record\"><Alignment type=\"enumeration\">{align}</Alignment><AlignmentCharacter type=\"string\">.</AlignmentCharacter><Leader type=\"string\"></Leader><Position type=\"unit\">{p}</Position></ListItem>"))
        .collect();
    let tab_list = if tabs.is_empty() { String::new() } else { format!("<TabList type=\"list\">{items}</TabList>") };
    format!("<Properties>{tab_list}{extra}</Properties>")
}

/// `<Properties>` of a character range: `(element, type, text)` children.
fn range_properties(props: &[(String, String, String)]) -> String {
    if props.is_empty() {
        return String::new();
    }
    let items: String = props.iter().map(|(tag, kind, text)| format!("<{tag} type=\"{kind}\">{}</{tag}>", crate::xml::escape(text))).collect();
    format!("<Properties>{items}</Properties>")
}

/// The index of the style with this key in `list`, added (made by `make`) when there is none.
fn index_of(list: &mut Vec<Style>, make: impl FnOnce() -> Style) -> usize {
    let style = make();
    list.iter().position(|o| o.key == style.key).unwrap_or_else(|| {
        list.push(style);
        list.len() - 1
    })
}

/// What `build` needs to know about the story the text goes into.
struct Ctx {
    story: String,
    like_p: Style,
    like_c: Style,
    base_font: Option<String>,
    /// A footnote marker's `Position` and character style (the document's footnote options), read when the file has a footnote.
    marker: Option<(Option<String>, Option<String>)>,
    /// An endnote reference's `Position` and character style (the document's endnote options), read when the file has an endnote.
    endnote_marker: Option<(Option<String>, Option<String>)>,
    /// The endnotes the text brought, `(reference, range, text)` in reading order: their bodies go in the endnote story after the text.
    endnotes: std::cell::RefCell<Vec<(String, String, String, Vec<crate::ops::endnotes::RichRun>)>>,
}

/// The atoms of some paragraphs (a story's or a table cell's) with the styles they use.
struct Built {
    atoms: Vec<Atom>,
    paragraphs: Vec<Style>,
    characters: Vec<Style>,
    plain: usize,
}

fn build(w: &mut Work<'_>, ctx: &Ctx, source: &[Paragraph], ends_with_return: bool) -> Result<Built, EditError> {
    let mut atoms = Vec::new();
    let mut paragraphs: Vec<Style> = Vec::new();
    let mut characters: Vec<Style> = Vec::new();
    let character = |characters: &mut Vec<Style>, style: Option<&str>, attrs: &[(String, String)], props: &[(String, String, String)], keep_font: bool| {
        let applied = style.map_or_else(|| NO_CHARACTER_STYLE_ID.to_owned(), |n| style_id("CharacterStyle", n));
        // the paragraph style's own font is no override (a run of a paragraph in a style of the file names a font that differs from the style's)
        let props: Vec<_> = props.iter().filter(|(tag, _, text)| keep_font || !(tag == "AppliedFont" && ctx.base_font.as_deref() == Some(text.as_str()))).cloned().collect();
        index_of(characters, || ranged(&ctx.like_c, "CharacterStyleRange", ("AppliedCharacterStyle", applied), attrs, range_properties(&props)))
    };
    let plain = character(&mut characters, None, &[], &[], false);
    let count = source.len();
    for (n, paragraph) in source.iter().enumerate() {
        let applied = paragraph.style.as_deref().map_or_else(|| NORMAL_ID.to_owned(), |name| style_id("ParagraphStyle", name));
        let p = index_of(&mut paragraphs, || ranged(&ctx.like_p, "ParagraphStyleRange", ("AppliedParagraphStyle", applied), &paragraph.attrs, tab_properties(&paragraph.tabs, &paragraph.properties)));
        // a paragraph in a style of an RTF file: its runs name a font when it differs from the style's
        let keep_font = paragraph.style.as_deref().is_some_and(|n| n != "$ID/[No paragraph style]") && !matches!(paragraph.runs.first().map(|r| &r.inline), Some(Some(_)));
        let mut last = plain;
        for run in &paragraph.runs {
            let c = character(&mut characters, run.char_style.as_deref(), &run.attrs, &run.props, keep_font);
            match &run.inline {
                Some(Inline::Endnote(text)) => {
                    // the reference has the document's endnote look and none of the run's character style (docxtext-c2-en); the run's style goes on to the return
                    let (position, marker_style) = ctx.endnote_marker.clone().unwrap_or_default();
                    let attrs: Vec<(String, String)> = position.into_iter().map(|p| ("Position".to_owned(), p)).collect();
                    let applied = marker_style.unwrap_or_else(|| NO_CHARACTER_STYLE_ID.to_owned());
                    let marker = index_of(&mut characters, || ranged(&ctx.like_c, "CharacterStyleRange", ("AppliedCharacterStyle", applied), &attrs, String::new()));
                    let (reference, range) = (w.new_id(), w.new_id());
                    let raw = format!("<Endnote Self=\"{reference}\" EndnoteTextRange=\"{range}\" />");
                    // the runs with character formatting (a Word endnote's own bold, italic ...): each with its character style and range attributes
                    let rich = run.note.iter().map(|r| (r.text.clone(), r.char_style.as_deref().map_or_else(|| NO_CHARACTER_STYLE_ID.to_owned(), |n| style_id("CharacterStyle", n)), r.attrs.clone())).collect();
                    ctx.endnotes.borrow_mut().push((reference, range, text.clone(), rich));
                    atoms.push(Atom { p, c: marker, ch: '\u{4}', content: false, raw });
                }
                None => {
                    for ch in run.text.chars() {
                        atoms.push(Atom { p, c, ch, content: ch != '\r', raw: raw_of(ch) });
                    }
                }
                Some(Inline::Table(table)) => {
                    // a table sits in the range of its run: the plain range for a Word table, the formatting of the character before it for an RTF one
                    // (oracle docxtext-c1-table, docxtext-c5-rtf-w13b)
                    let raw = table_xml(w, ctx, table)?;
                    atoms.push(Atom { p, c, ch: '\u{16}', content: false, raw });
                }
                Some(Inline::Picture(picture)) => {
                    // an inline frame in the range of the run that held the drawing (oracle docxtext-c3-pic)
                    let raw = crate::ops::place::inline::element(w, picture)?;
                    atoms.push(Atom { p, c, ch: '\u{fffc}', content: false, raw });
                }
                Some(Inline::Footnote(text)) => {
                    // the marker has the document's marker look and none of the run's character style (docxtext-c1-fn); the run's style goes on to the return
                    let (position, marker_style) = ctx.marker.clone().unwrap_or_default();
                    let attrs: Vec<(String, String)> = position.into_iter().map(|p| ("Position".to_owned(), p)).collect();
                    let applied = marker_style.unwrap_or_else(|| NO_CHARACTER_STYLE_ID.to_owned());
                    let marker = index_of(&mut characters, || ranged(&ctx.like_c, "CharacterStyleRange", ("AppliedCharacterStyle", applied), &attrs, String::new()));
                    let raw = note_xml(w, text, &run.note, &|r: &folio_textimport::Run| r.char_style.as_deref().map_or_else(|| NO_CHARACTER_STYLE_ID.to_owned(), |n| style_id("CharacterStyle", n)))?;
                    atoms.push(Atom { p, c: marker, ch: '\u{4}', content: false, raw });
                }
            }
            if !run.text.is_empty() {
                last = c;
            }
        }
        // the return that ends the paragraph (the last paragraph has none unless the text ends with one)
        if n + 1 < count || ends_with_return {
            let c = match (&paragraph.break_type, &paragraph.end) {
                (None, None) => last,
                (kind, end) => {
                    let (style, mut attrs, props) = end.as_ref().map_or((None, Vec::new(), Vec::new()), |e| (e.char_style.as_deref(), e.attrs.clone(), e.props.clone()));
                    attrs.extend(kind.iter().map(|k| ("ParagraphBreakType".to_owned(), k.clone())));
                    character(&mut characters, style, &attrs, &props, keep_font)
                }
            };
            atoms.push(Atom { p, c, ch: '\r', content: false, raw: raw_of('\r') });
        }
    }
    Ok(Built { atoms, paragraphs, characters, plain })
}

/// The id of the character style of the return that ends a Word file whose last paragraph ends with a note reference (an endnote or a
/// footnote), when the file has endnotes (measured: docxtext2-en3, docxtext3-n01 .. n07).
fn tail_note_style(source: &TextSource) -> Option<String> {
    if !source.ends_with_return {
        return None;
    }
    let last = source.paragraphs.last()?;
    let run = last.runs.last()?;
    matches!(run.inline, Some(Inline::Endnote(_) | Inline::Footnote(_))).then_some(())?;
    // the return has the style of the last run unless the file says its own
    let style = last.end.as_ref().map_or(run.char_style.as_deref(), |e| e.char_style.as_deref());
    style.map(|name| style_id("CharacterStyle", name))
}

/// `<Content>` elements for `text` with the returns in it as `<Br />`; `prefix` goes in front of the first and `suffix` after the last.
fn note_content(prefix: &str, text: &str, suffix: &str) -> String {
    let parts: Vec<&str> = text.split('\r').collect();
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str("<Br />");
        }
        let (before, after) = (if i == 0 { prefix } else { "" }, if i + 1 == parts.len() { suffix } else { "" });
        if !part.is_empty() || !before.is_empty() || !after.is_empty() {
            out.push_str(&format!("<Content>{before}{}{after}</Content>", crate::ops::endnotes::text_escape(part)));
        }
    }
    out
}

/// A footnote a Word file brought: one paragraph range in the document's footnote text style, a character range per run when the runs have
/// character formatting (oracle docxtext-c3-fn*; the paragraphs of the note are one range with returns).
fn note_xml(w: &Work<'_>, text: &str, runs: &[folio_textimport::Run], style: &dyn Fn(&folio_textimport::Run) -> String) -> Result<String, EditError> {
    let (ps, separator) = super::footnotes::footnote_wrapper(w)?;
    let lead = format!("<?ACE 4?>{}", crate::ops::endnotes::text_escape(&separator));
    let body = if runs.is_empty() {
        format!("<CharacterStyleRange AppliedCharacterStyle=\"{NO_CHARACTER_STYLE_ID}\">{}</CharacterStyleRange>", note_content(&lead, text, ""))
    } else {
        runs.iter()
            .enumerate()
            .map(|(i, r)| {
                let attrs: String = r.attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", crate::xml::escape(v))).collect();
                format!("<CharacterStyleRange AppliedCharacterStyle=\"{}\"{attrs}>{}</CharacterStyleRange>", crate::xml::escape(&style(r)), note_content(if i == 0 { &lead } else { "" }, &r.text, ""))
            })
            .collect()
    };
    Ok(format!("<Footnote><ParagraphStyleRange AppliedParagraphStyle=\"{}\">{body}</ParagraphStyleRange></Footnote>", crate::xml::escape(&ps)))
}

/// `Language` names the file's runs ask for, each once, in order of appearance (cells included).
fn languages_of(source: &[Paragraph], out: &mut Vec<String>) {
    for run in source.iter().flat_map(|p| p.runs.iter().chain(p.end.iter())) {
        for (key, value) in &run.attrs {
            if key == "AppliedLanguage" && let Some(name) = value.strip_prefix("$ID/") && !out.iter().any(|o| o == name) {
                out.push(name.to_owned());
            }
        }
        if let Some(Inline::Table(table)) = &run.inline {
            for cell in &table.cells {
                languages_of(&cell.paragraphs, out);
            }
        }
    }
}

fn has_inline(source: &[Paragraph], is: &dyn Fn(&Inline) -> bool) -> bool {
    source.iter().flat_map(|p| p.runs.iter()).any(|r| r.inline.as_ref().is_some_and(is))
        || source.iter().flat_map(|p| p.runs.iter()).any(|r| matches!(&r.inline, Some(Inline::Table(t)) if t.cells.iter().any(|c| has_inline(&c.paragraphs, is))))
}

fn has_footnote(source: &[Paragraph]) -> bool {
    has_inline(source, &|i| matches!(i, Inline::Footnote(_)))
}

fn has_endnote(source: &[Paragraph]) -> bool {
    has_inline(source, &|i| matches!(i, Inline::Endnote(_)))
}

const CELL_INSETS: [(&str, &str); 5] = [("TextTopInset", "4"), ("TextLeftInset", "4"), ("TextBottomInset", "4"), ("TextRightInset", "4"), ("ClipContentToTextCell", "false")];

/// ` Key="value"` for each attribute (a table's borders, a cell's fill).
fn extra_attrs(attrs: &[(String, String)]) -> String {
    attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", crate::xml::escape(v))).collect()
}

fn insets() -> String {
    CELL_INSETS.iter().map(|(k, v)| format!(" {k}=\"{v}\"")).collect()
}

/// The insets of a row, column or cell of the table: the table's 4 pt, or none (an RTF table: measured docxtext-c5-rtf-w01).
fn inner_insets(table: &Table) -> String {
    if !table.zero_insets {
        return insets();
    }
    CELL_INSETS.iter().map(|(k, v)| format!(" {k}=\"{}\"", if *k == "ClipContentToTextCell" { v } else { "0" })).collect()
}

/// The `<Table>` element of an imported table (oracle docxtext-c1-table*): the grid with its widths, the merged cells, the header rows;
/// a position a merged cell covers has no element but uses up an id.
fn table_xml(w: &mut Work<'_>, ctx: &Ctx, table: &Table) -> Result<String, EditError> {
    let columns = table.columns.len();
    let rows = table.rows;
    let header = table.header_rows.min(rows);
    let uid = w.new_id();
    let id = format!("{}i{}", ctx.story, &uid[1..]);
    let ind = "\t\t\t\t";
    let mut s = format!(
        "<Table Self=\"{id}\" HeaderRowCount=\"{header}\" FooterRowCount=\"0\"{} BodyRowCount=\"{}\" ColumnCount=\"{columns}\" AppliedTableStyle=\"TableStyle/$ID/[No table style]\" TableDirection=\"LeftToRightDirection\"{}>",
        insets(),
        rows - header,
        extra_attrs(&table.attrs)
    );
    let inset_attrs = inner_insets(table);
    for r in 0..rows {
        let height = table.row_heights.get(r).copied().filter(|h| *h > 0.0).map_or_else(|| "3".to_owned(), crate::geom::num);
        s += &format!("\n{ind}\t<Row Self=\"{id}Row{r}\" Name=\"{r}\"{inset_attrs} SingleRowHeight=\"{height}\" MinimumHeight=\"{height}\" AutoGrow=\"true\" />");
    }
    for (c, width) in table.columns.iter().enumerate() {
        s += &format!("\n{ind}\t<Column Self=\"{id}Column{c}\" Name=\"{c}\"{inset_attrs} SingleColumnWidth=\"{}\" />", crate::geom::num(*width));
    }
    // InDesign numbers the cells as it made them: a grid of rows by columns; with header rows the first body row, then the header rows, then the other body rows
    let order: Vec<usize> = if header == 0 {
        (0..rows).collect()
    } else {
        std::iter::once(header).chain(0..header).chain(header + 1..rows).collect()
    };
    let slot = |row: usize, column: usize| order.iter().position(|r| *r == row).unwrap_or(row) * columns + column;
    let mut cells: Vec<&folio_textimport::Cell> = table.cells.iter().collect();
    cells.sort_by_key(|c| (c.row, c.column));
    for cell in cells {
        let paragraphs: Vec<Paragraph> = if cell.paragraphs.is_empty() { vec![Paragraph::default()] } else { cell.paragraphs.clone() };
        let built = build(w, ctx, &paragraphs, false)?;
        let inner = Story { range: 0..0, separator: String::new(), paragraphs: built.paragraphs, characters: built.characters, atoms: built.atoms, empty: (0, built.plain), trailing: None, tail_range: false }.serialize();
        s += &format!(
            "\n{ind}\t<Cell Self=\"{id}i{:x}\" Name=\"{}:{}\" RowSpan=\"{}\" ColumnSpan=\"{}\" CellType=\"TextTypeCell\"{} AppliedCellStyle=\"CellStyle/$ID/[None]\" AppliedCellStylePriority=\"0\"{} FirstBaselineOffset=\"AscentOffset\" WritingDirection=\"true\">{inner}</Cell>",
            slot(cell.row, cell.column),
            cell.column,
            cell.row,
            cell.row_span,
            cell.column_span,
            inset_attrs,
            extra_attrs(&cell.attrs)
        );
    }
    s += &format!("\n{ind}</Table>");
    Ok(s)
}

pub(crate) fn place(w: &mut Work<'_>, id: &str, source: &TextSource) -> Result<(), EditError> {
    track_edit::require_untracked(w, id)?;
    let (part, ix, i, story_id) = locate(w, id)?;
    let old = Story::read(w.text(&part)?, &ix, i)?;
    // the paragraph the old story began with (an empty story: its only one)
    let first = old.paragraphs[old.atoms.first().map_or(old.empty.0, |a| a.p)].clone();
    if !old.atoms.is_empty() {
        // everything the old text held goes (hyperlinks, index markers, anchored objects … by the rules of Delete)
        delete(w, &story_id, 0, old.atoms.len())?;
    }
    w.ed.text_state.remove(&story_id);
    let (part, ix, i, story_id) = locate(w, &story_id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    let marker = if has_footnote(&source.paragraphs) { Some(super::footnotes::marker_look(w)?) } else { None };
    let endnote_marker = if has_endnote(&source.paragraphs) { Some(crate::ops::endnotes::marker_look(w)?) } else { None };
    let ctx = Ctx { story: story_id.clone(), like_p: s.paragraphs[s.empty.0].clone(), like_c: s.characters[s.empty.1].clone(), base_font: normal_font(w)?.or(default_font(w)?), marker, endnote_marker, endnotes: Default::default() };
    let mut built = build(w, &ctx, &source.paragraphs, source.ends_with_return)?;
    // no text at all (a file InDesign reads as nothing): the story is one empty paragraph in the style the old one began with
    let empty = built.atoms.is_empty();
    built.paragraphs.push(first);
    s.trailing = source.ends_with_return.then_some(built.paragraphs.len() - 1);
    // a file whose last paragraph ends with a note reference: the final empty paragraph holds an empty plain character range, and the endnote
    // bodies take the character style of the return that ends the last paragraph (oracle docxtext-c3-en3)
    let tail_style = tail_note_style(source);
    s.tail_range = tail_style.is_some();
    s.empty = (if empty { built.paragraphs.len() - 1 } else { 0 }, built.plain);
    s.paragraphs = built.paragraphs;
    s.characters = built.characters;
    s.atoms = built.atoms;
    write(w, &part, &story_id, &s)?;
    // a language the text uses is listed by the document from now on
    let mut names = Vec::new();
    languages_of(&source.paragraphs, &mut names);
    for name in names {
        super::languages::register_name(w, &name)?;
    }
    // the bodies of the endnotes the text brought: in the endnote story, which the first one makes
    let endnotes = ctx.endnotes.take();
    crate::ops::endnotes::add_bodies(w, &endnotes, tail_style.as_deref())?;
    Ok(())
}
