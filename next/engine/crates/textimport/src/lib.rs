//! Text files read the way InDesign's Place reads them (oracle `placetext-c1`, measurements in
//! `artifacts/placetext-c1-20261023`). The file's bytes become paragraphs of runs; `folio-edit` writes them
//! into a story. Nothing here knows the document: the text goes into the story, no link is kept.
//!
//! The option names are InDesign's own (`app.textImportPreferences`, `app.wordRTFImportPreferences`), so a
//! script, the screen and the oracle all say the same thing.
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

use serde::{Deserialize, Serialize};
use std::path::Path;

mod cell_edges;
mod charset;
mod docx;
mod messages;
pub use messages::Message;
mod numbering;
mod picture;
mod rtf;
mod txt;
mod xmltree;

pub use charset::{CharacterSet, detect as detect_character_set};
pub use picture::{Picture, decode_base64};
pub use txt::smart_quote;

/// The import preferences that change what a text file becomes (`app.textImportPreferences`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    /// Used only when the file has no byte order mark (a mark always wins: placetext-c1 cs-utf8-on-utf16, cs-ansi-on-bom).
    #[serde(default)]
    pub character_set: CharacterSet,
    /// "Use typographer's quotes": straight quotes become curly (English: USA quotes: the dictionary is not an option here).
    #[serde(default = "yes")]
    pub use_typographers_quotes: bool,
    /// "Remove at the end of every line": a single return between lines becomes a space.
    #[serde(default)]
    pub strip_returns_between_lines: bool,
    /// "Remove at the end of every paragraph": a run of returns (blank lines) becomes one return.
    #[serde(default)]
    pub strip_returns_between_paragraphs: bool,
    #[serde(default)]
    pub convert_spaces_into_tabs: bool,
    /// A run of at least this many spaces becomes one tab.
    #[serde(default = "three")]
    pub spaces_into_tabs_count: u32,
    /// `ImportPlatform` (MACINTOSH, PC): it changed nothing in what the text became (placetext-c1 platform-mac); kept so a script's options are accepted.
    #[serde(default)]
    pub platform: Option<String>,
    /// The dictionary whose quotes "typographer's quotes" uses. Only the default is measured; another is refused (its quote marks would differ).
    #[serde(default)]
    pub dictionary: Option<String>,
    /// Word and RTF (`app.wordRTFImportPreferences.removeFormatting`): "Remove Styles and Formatting from Text and Tables": the text only.
    #[serde(default)]
    pub remove_formatting: bool,
    /// Word and RTF (`convertPageBreaks`): what a page or column break of the file becomes.
    #[serde(default)]
    pub convert_page_breaks: PageBreaks,
    /// Word and RTF (`preserveGraphics`, on by default): the file's pictures come in; off, they are left out (measured: docxtext-c3).
    #[serde(default = "yes")]
    pub preserve_graphics: bool,
    /// Folio: characters the font of the text lacks take an installed font that has them (on by default; the engine's
    /// edit does it after placing, server `glyphfill.rs`). Not an InDesign preference; it changes nothing this crate reads.
    #[serde(default = "yes")]
    pub fill_missing_glyphs: bool,
}

/// `ConvertPageBreaks`: a page break becomes a page break (a column break stays one), becomes a column break, or goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PageBreaks {
    #[default]
    PageBreak,
    ColumnBreak,
    None,
}

fn yes() -> bool {
    true
}
fn three() -> u32 {
    3
}

impl Options {
    /// The options as a script of InDesign names them: `app.textImportPreferences` properties, an enumerator
    /// written with its class (`TextImportCharacterSet.UTF8`) or without (`UTF8`).
    pub fn from_dom(properties: &serde_json::Value) -> Result<Self, String> {
        let mut map = serde_json::Map::new();
        for (key, value) in properties.as_object().into_iter().flatten() {
            let value = match value.as_str() {
                Some(name) => serde_json::Value::String(name.rsplit('.').next().unwrap_or(name).to_owned()),
                None => value.clone(),
            };
            map.insert(key.clone(), value);
        }
        let options: Self = serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| format!("가져오기 옵션: {e}"))?;
        if options.use_typographers_quotes && options.dictionary.as_deref().is_some_and(|d| d != "English: USA") {
            return Err("따옴표 사전은 아직 English: USA 만 지원합니다".into());
        }
        Ok(options)
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            character_set: CharacterSet::default(),
            use_typographers_quotes: true,
            strip_returns_between_lines: false,
            strip_returns_between_paragraphs: false,
            convert_spaces_into_tabs: false,
            spaces_into_tabs_count: 3,
            platform: None,
            dictionary: None,
            remove_formatting: false,
            convert_page_breaks: PageBreaks::PageBreak,
            preserve_graphics: true,
            fill_missing_glyphs: true,
        }
    }
}

/// One stretch of text. A tab is a tab character; a forced line break is U+2028. `char_style` and `attrs` are what the file asks
/// of it: a character style by name and the `CharacterStyleRange` attributes InDesign would write (local formatting).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub char_style: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
    /// `(element, type, text)` children of the range's `<Properties>` (the font an RTF file names, the colour of an underline).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<(String, String, String)>,
    /// The run is one object instead of text: `text` is its one character (U+0016 for a table, U+0004 for a footnote marker).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline: Option<Inline>,
    /// A footnote's formatted text (the runs after the number and the separator, paragraphs joined by U+000D) when its runs have character
    /// formatting; the footnote's `Inline::Footnote` text is then the same text without the formatting (oracle `docxtext-c3-fn*`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub note: Vec<Run>,
}

/// An object that is one character of the text (oracle `docxtext-c1`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Inline {
    /// A table the file holds (Word's `w:tbl`): InDesign puts it in the story where the table was.
    Table(Table),
    /// A footnote: the text after the number and the separator (a Word footnote's text part).
    Footnote(String),
    /// An endnote: the text after the number and the separator. The document gets an endnote story (and, the first time, a last page for it).
    Endnote(String),
    /// A picture the file holds (Word's `w:drawing`): InDesign embeds it as an inline frame (oracle `docxtext-c3-pic`).
    Picture(Picture),
}

/// A table as the file lays it out: the grid and the cells that start in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    /// Column widths in points.
    pub columns: Vec<f64>,
    /// Rows in all (header rows first).
    pub rows: usize,
    /// How many rows from the top repeat as header rows.
    pub header_rows: usize,
    /// The cells that start a position of the grid, row by row; a position covered by a merged cell has none.
    pub cells: Vec<Cell>,
    /// Attributes of the `<Table>` element besides the ones every imported table has: the borders (`LeftBorderStrokeWeight` ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
    /// The rows, columns and cells have no inset (an RTF table; a Word table's have the table's 4 pt). The table itself keeps its 4 pt.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub zero_insets: bool,
    /// The minimum height of each row in points (0: none), empty when no row has one (RTF `\trrh`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_heights: Vec<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    pub paragraphs: Vec<Paragraph>,
    /// Attributes of the `<Cell>` element besides the ones every imported cell has: the fill (`FillColor`, `FillTint`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
}

/// The fill of the cells of a table: a shaded cell has the colour and its tint; InDesign also gives every cell under a shaded one (down its
/// column) the tint of the cell above with no colour (measured: docxtext2-b1, b2, b13; RTF the same: docxtext-c5-rtf-w04). `fills` is
/// the fill of each cell of `table.cells`, `columns` the number of grid columns.
pub(crate) fn apply_fills(table: &mut Table, fills: &[Option<(String, u32)>], columns: usize) {
    let mut shaded: Vec<Option<u32>> = vec![None; columns.max(1)];
    let mut order: Vec<usize> = (0..table.cells.len()).collect();
    order.sort_by_key(|&i| (table.cells[i].row, table.cells[i].column));
    for i in order {
        let (column, span) = (table.cells[i].column, table.cells[i].column_span);
        if shaded.len() < column + span {
            shaded.resize(column + span, None);
        }
        if let Some(Some((color, tint))) = fills.get(i) {
            table.cells[i].attrs.extend([("FillColor".to_owned(), color.clone()), ("FillTint".to_owned(), tint.to_string())]);
            shaded[column..column + span].iter_mut().for_each(|s| *s = Some(*tint));
        } else if let Some(tint) = shaded[column] {
            table.cells[i].attrs.push(("FillTint".to_owned(), tint.to_string()));
        }
    }
}

impl Run {
    pub fn plain(text: &str) -> Self {
        Self { text: text.to_owned(), ..Self::default() }
    }
}

/// A paragraph without its return. `style` is the paragraph style name the file asked for (none: the document's
/// `[No Paragraph Style]`); `attrs` are the `ParagraphStyleRange` attributes of its local formatting, `tabs` the left tab stops
/// as (alignment, position in points), `break_type` the `ParagraphBreakType` ("NextPage", "NextColumn") its return carries and `end`
/// the formatting of the return itself (text empty) where it differs from that of the last run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    pub runs: Vec<Run>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<(String, f64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Run>,
    /// Children of the paragraph range's `<Properties>` besides the tab list, written as they are (the list a Word paragraph is in:
    /// `AppliedNumberingList`, `BulletChar` ... in the order InDesign writes them).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub properties: String,
}

impl Paragraph {
    pub fn plain(text: &str) -> Self {
        Self { runs: if text.is_empty() { Vec::new() } else { vec![Run::plain(text)] }, ..Self::default() }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// A paragraph or character style the file defines (Word's styles come into the document as styles, `Imported="true"`).
/// `attrs` are the IDML attributes of its definition; `properties` are `(element, type, text)` children of `<Properties>` besides
/// `BasedOn` and `PreviewColor`, which the editor writes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleDef {
    pub name: String,
    /// The style it is based on, by name (none: the document's no-style).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// The style the next paragraph takes, by name (paragraph styles).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<(String, String, String)>,
}

/// A colour the file uses that the document gets as a swatch (RGB, 0 to 255), named the way InDesign names it (`Word_R255_G0_B0`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SwatchDef {
    pub name: String,
    pub rgb: [u8; 3],
    /// The swatch is in the Swatches panel's colour group (the default). A colour that only a style's fill colour names is not (measured: docxtext-c4-rtf-r03, t07, t13).
    #[serde(default = "yes")]
    pub grouped: bool,
}

/// What a text file holds, before it is written into a story.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSource {
    /// "txt", "docx" or "rtf".
    pub format: String,
    pub paragraphs: Vec<Paragraph>,
    /// Styles the paragraphs and runs name that the file defines, in the order they come into the document (a style before the styles based on it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraph_styles: Vec<StyleDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub character_styles: Vec<StyleDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub colors: Vec<SwatchDef>,
    /// A Word or RTF file read with its formatting (not "remove styles and formatting"): InDesign also leaves its kinsoku tables in the
    /// document's map (measured: placetext-docx-*; nothing with `removeFormatting`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub formatted: bool,
    /// The text ends with a paragraph return: the story gets one more, empty paragraph after it.
    #[serde(default)]
    pub ends_with_return: bool,
    /// The numbering lists the file's numbered paragraphs belong to (`Word Imported List 1` ...), in order of appearance: the document gets them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub numbering_lists: Vec<String>,
    /// What was not carried over (an element this reader does not know is named here, never dropped silently).
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, rename = "warningMessages")]
    pub warning_messages: Vec<messages::Message>,
}

impl TextSource {
    /// The families of the fonts the runs name (`AppliedFont`), each once, in order of appearance: the fonts the document has to know.
    pub fn font_families(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let styles = self.paragraph_styles.iter().chain(&self.character_styles).map(|s| &s.properties);
        let runs = self.paragraphs.iter().flat_map(|p| p.runs.iter().chain(p.end.iter())).map(|r| &r.props);
        for props in styles.chain(runs) {
            for (tag, _, family) in props {
                if tag == "AppliedFont" && !out.contains(family) {
                    out.push(family.clone());
                }
            }
        }
        out
    }

    /// The characters of the story this makes: paragraphs joined by returns (U+000D).
    pub fn story_text(&self) -> String {
        let mut out = self.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join("\r");
        if self.ends_with_return {
            out.push('\r');
        }
        out
    }

    pub fn paragraph_count(&self) -> usize {
        self.paragraphs.len() + usize::from(self.ends_with_return)
    }
}

/// Reads `path` as the Place command does: by its content, not its name (a text file of any extension is plain text).
pub fn read(path: &Path, options: &Options) -> Result<TextSource, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read_bytes(&bytes, options)
}

pub fn read_bytes(bytes: &[u8], options: &Options) -> Result<TextSource, String> {
    if bytes.is_empty() {
        return Err("빈 파일은 가져올 수 없습니다".into());
    }
    // InDesign tells the formats apart by content, not by name.
    if bytes.starts_with(b"PK\x03\x04") {
        return docx::read(bytes, options);
    }
    if bytes.starts_with(br"{\rtf") {
        return rtf::read(bytes, options);
    }
    txt::read(bytes, options)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod robustness_tests;
#[cfg(test)]
mod tests_docx;
#[cfg(test)]
mod tests_docx_c1;
#[cfg(test)]
mod tests_docx_c2;
#[cfg(test)]
mod tests_docx_c3;
#[cfg(test)]
mod tests_rtf;
#[cfg(test)]
mod tests_screens;
