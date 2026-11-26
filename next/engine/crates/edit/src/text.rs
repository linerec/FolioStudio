//! Story editing. Rules measured in `oracle/scenarios/text-edit.json` (InDesign 2025).
//! Characters count Unicode scalars, not UTF-8 bytes or UTF-16 units; a Table and an
//! anchored item each count one (text-special-index, text-table-delete). A range of
//! paragraph style can contain several actual paragraphs: only <Br/> ends a paragraph.
//!
//! Keep opaque inline objects and all style attributes/properties as original XML.
//! Unsupported wrappers are refused, never flattened or silently discarded.

use crate::{EditError, Work};
use crate::xml::{Index, unescape};
use std::ops::Range;
pub(crate) mod format;
pub(crate) mod footnotes;
pub(crate) mod endnote_ref;
mod note_navigation;
pub use note_navigation::context as note_context;
pub use note_navigation::endnote_target;
mod query;
pub(crate) mod notes;
pub(crate) mod changes;
pub(crate) mod track_edit;
pub(crate) mod typed;
mod author;
mod fonts;
pub mod languages;
pub(crate) mod note_nodes;
mod import;
pub(crate) mod smart_spacing;
pub(crate) use import::{default_font, normal_font, place as place_source};
pub(crate) use fonts::register_family;
pub use format::TextFragment;
pub use format::find::{FindQuery, FoundText};
pub use format::glyph::{GlyphAt, GlyphFont, GlyphQuery};
pub use format::spell::{SpellRun, SpellText};
pub(crate) use format::glyph::replace as replace_glyph;
pub(crate) use format::glyph::insert_glyph;

#[derive(Clone, Debug)]
struct Style {
    open: String,
    properties: String,
    key: String,
    prefix: String,
    suffix: String,
    separator: String,
}

/// InDesign keeps the removed range's first character style at its insertion point,
/// even when no surviving XML range has it (text-replace-multiline). Session state,
/// included in undo/redo and transaction rollback; never written as fake IDML text.
#[derive(Clone, Debug)]
pub(crate) struct InsertionStyle {
    at: usize,
    character: Style,
}
pub(crate) type InsertionState = std::collections::BTreeMap<String, InsertionStyle>;

impl Style {
    fn read(t: &str, ix: &Index, i: usize) -> Self {
        let n = &ix.nodes[i];
        let tag = ix.local(t, i).to_owned();
        let open = if n.empty { format!("{}>", t[n.start..n.open_end - 2].trim_end()) } else { t[n.start..n.open_end].to_owned() };
        let properties: String = ix.children(i).iter().filter(|&&c| ix.local(t, c) == "Properties")
            .map(|&c| &t[ix.nodes[c].start..ix.nodes[c].end]).collect();
        let mut attrs: Vec<_> = ix.attrs(t, i).iter().map(|a| (&t[a.name.0..a.name.1], unescape(&t[a.value.0..a.value.1]))).collect();
        attrs.sort();
        // Properties are copied verbatim. Matching keys only coalesces identical ranges.
        let key = format!("{attrs:?}{properties}");
        let data: Vec<_> = ix.children(i).iter().copied().filter(|&c| ix.local(t, c) != "Properties").collect();
        let indent = if tag == "ParagraphStyleRange" { "\t\t\t" } else { "\t\t\t\t" };
        let prefix = data.first().map(|&c| t[n.start..ix.nodes[c].start].to_owned())
            .unwrap_or_else(|| format!("{open}{properties}\n{indent}"));
        let suffix = data.last().map(|&c| t[ix.nodes[c].end..n.end].to_owned())
            .unwrap_or_else(|| format!("\n{}</{tag}>", &indent[..indent.len() - 1]));
        let separator = prefix.rsplit_once('>').map(|(_, s)| s).unwrap_or("").to_owned();
        Self { open, properties, key, prefix, suffix, separator }
    }

    fn wrap(&self, contents: &str) -> String {
        if contents.is_empty() && self.properties.is_empty() {
            format!("{} />", self.open[..self.open.len() - 1].trim_end())
        } else {
            format!("{}{}{}", self.prefix, contents, self.suffix)
        }
    }
}

#[derive(Clone)]
struct Atom {
    p: usize,
    c: usize,
    ch: char,
    /// Inside <Content> (including ACE processing instructions), or a whole inline node.
    content: bool,
    raw: String,
}

impl Atom {
    /// An index marker (`PageReference`), one U+FEFF.
    fn is_marker(&self) -> bool {
        !self.content && self.ch == '\u{FEFF}' && self.raw.starts_with("<PageReference")
    }
}

#[derive(Clone)]
struct Story {
    range: Range<usize>,
    separator: String,
    paragraphs: Vec<Style>,
    characters: Vec<Style>,
    atoms: Vec<Atom>,
    empty: (usize, usize),
    /// A differently styled empty paragraph after the story's final return.
    /// InDesign writes a self-closing PSR with no CSR (format-trailing-paragraph).
    trailing: Option<usize>,
    /// The trailing paragraph holds one empty character range in the `empty` style (a Word file that ends with a note reference: docxtext-c3-en3).
    tail_range: bool,
}

fn unsupported(message: impl Into<String>) -> EditError {
    EditError::new("unsupported", message)
}

fn check_container(t: &str, ix: &Index, i: usize) -> Result<(), EditError> {
    let n = &ix.nodes[i];
    let mut at = n.open_end;
    let mut data = false;
    for &child in ix.children(i) {
        let c = &ix.nodes[child];
        if !t[at..c.start].trim().is_empty() {
            return Err(unsupported("unindexed text metadata; original XML kept"));
        }
        if ix.local(t, child) == "Properties" {
            if data { return Err(unsupported("properties between text nodes; original XML kept")); }
        } else {
            data = true;
        }
        at = c.end;
    }
    if !t[at..n.close_start].trim().is_empty() {
        return Err(unsupported("unindexed text metadata; original XML kept"));
    }
    Ok(())
}

impl Story {
    fn read(t: &str, ix: &Index, story: usize) -> Result<Self, EditError> {
        Self::read_mode(t, ix, story, false)
    }

    fn read_mode(t: &str, ix: &Index, story: usize, editorial: bool) -> Result<Self, EditError> {
        let ranges: Vec<_> = ix.children(story).iter().copied().filter(|&i| ix.local(t, i) == "ParagraphStyleRange").collect();
        if ranges.is_empty() {
            return Err(unsupported("story has no paragraph range"));
        }
        let first = ix.nodes[ranges[0]].start;
        let last = ix.nodes[*ranges.last().unwrap()].end;
        if !editorial && ix.children(story).iter().any(|&c|ix.local(t,c)=="Change") {
            return Err(unsupported("story contains tracked changes; original XML kept"));
        }
        for pair in ranges.windows(2) {
            if !t[ix.nodes[pair[0]].end..ix.nodes[pair[1]].start].trim().is_empty() {
                return Err(unsupported("metadata between paragraph ranges; original XML kept"));
            }
        }
        for &i in ix.children(story) {
            if ix.nodes[i].start >= first && ix.nodes[i].end <= last && ix.local(t, i) != "ParagraphStyleRange" {
                return Err(unsupported(format!("story wrapper {}", ix.local(t, i))));
            }
        }
        let separator = t[..first].rsplit_once('>').map(|(_, s)| s).unwrap_or("").to_owned();
        let mut s = Self { range: first..last, separator, paragraphs: Vec::new(), characters: Vec::new(), atoms: Vec::new(), empty: (0, 0), trailing:None, tail_range: false };
        for pi in ranges.iter().copied() {
            check_container(t, ix, pi)?;
            let p = s.paragraphs.len();
            s.paragraphs.push(Style::read(t, ix, pi));
            if ix.named(t,pi,"CharacterStyleRange").is_empty() && s.atoms.last().is_some_and(|a|a.ch=='\r') {
                s.trailing=Some(p); s.empty.0=p;
            }
            for &ci in ix.children(pi) {
                if ix.local(t, ci) == "Properties" { continue; }
                if ix.local(t, ci) != "CharacterStyleRange" {
                    return Err(unsupported(format!("paragraph wrapper {}", ix.local(t, ci))));
                }
                let c = s.characters.len();
                check_container(t, ix, ci)?;
                s.characters.push(Style::read(t, ix, ci));
                s.empty = (p, c);
                s.trailing=None;
                for &node in ix.children(ci) {
                    let n = &ix.nodes[node];
                    let name = ix.local(t, node);
                    match name {
                        "Change" if editorial => s.atoms.push(Atom { p, c, ch:'\u{feff}', content:false, raw:t[n.start..n.end].to_owned() }),
                        "Properties" => {}
                        "Content" => s.content(&t[n.open_end..n.close_start], p, c)?,
                        "Br" => s.atoms.push(Atom { p, c, ch: '\r', content: false, raw: t[n.start..n.end].to_owned() }),
                        "Table" | "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "Group" | "TextFrame" |
                        "Footnote" | "Endnote" | "Note" | "TextVariableInstance" => {
                            let ch = match name { "Table" => '\u{16}', "Note" => '\u{FEFF}', "Footnote" | "Endnote" => '\u{4}', "TextVariableInstance" => '\u{18}', _ => '\u{fffc}' };
                            s.atoms.push(Atom { p, c, ch, content: false, raw: t[n.start..n.end].to_owned() });
                        }
                        // Index marker: one U+FEFF, kept or removed with the text around it (index-c1).
                        "PageReference" if ix.children(node).iter().all(|&c| ix.local(t, c) == "Properties") => s.atoms.push(Atom { p, c, ch: '\u{FEFF}', content: false, raw: t[n.start..n.end].to_owned() }),
                        _ => return Err(unsupported(format!("text wrapper {name}; original XML kept"))),
                    }
                }
            }
        }
        if s.characters.is_empty() { return Err(unsupported("story has no character range")); }
        if let Some(&pi) = ranges.last() {
            let runs = ix.named(t, pi, "CharacterStyleRange");
            if let [run] = runs.as_slice() && ix.children(*run).is_empty() && s.atoms.last().is_some_and(|a| a.ch == '\r') && s.paragraphs.len() > 1 {
                s.trailing = Some(s.paragraphs.len() - 1);
                s.tail_range = true;
            }
        }
        if editorial {
            for &node in ix.children(story) {
                let n=&ix.nodes[node];
                if n.start>=last && ix.local(t,node)=="Change" {
                    if ix.value(t,node,"ChangeType").as_deref()!=Some("DeletedText") {return Err(unsupported("unsupported story-level change"));}
                    let (p,c)=s.empty;
                    s.atoms.push(Atom{p,c,ch:'\u{feff}',content:false,raw:t[n.start..n.end].to_owned()});s.range.end=n.end;
                }
            }
        }
        Ok(s)
    }

    fn content(&mut self, mut raw: &str, p: usize, c: usize) -> Result<(), EditError> {
        while !raw.is_empty() {
            let (len, ch) = if raw.starts_with("<?ACE ") {
                let end = raw.find("?>").ok_or_else(|| EditError::new("xml", "unterminated ACE"))? + 2;
                (end, '\u{fffc}')
            } else if raw.starts_with('&') {
                let end = raw.find(';').ok_or_else(|| EditError::new("xml", "unterminated entity"))? + 1;
                let decoded = unescape(&raw[..end]);
                if decoded.chars().count() != 1 { return Err(unsupported("unknown text entity")); }
                (end, decoded.chars().next().unwrap())
            } else if raw.starts_with('<') {
                return Err(unsupported("markup inside Content"));
            } else {
                let ch = raw.chars().next().unwrap();
                (ch.len_utf8(), ch)
            };
            self.atoms.push(Atom { p, c, ch, content: true, raw: raw[..len].to_owned() });
            raw = &raw[len..];
        }
        Ok(())
    }

    fn serialize(&self) -> String {
        self.serialize_ranges(false)
    }

    fn serialize_ranges(&self, footnote: bool) -> String {
        if self.atoms.last().is_some_and(|a|a.raw.starts_with("<Change ")&&a.raw.contains("ChangeType=\"DeletedText\"")) {
            let mut visible=self.clone();let mut tail=Vec::new();
            while visible.atoms.last().is_some_and(|a|a.raw.starts_with("<Change ")&&a.raw.contains("ChangeType=\"DeletedText\"")) {
                tail.push(visible.atoms.pop().unwrap().raw);
            }
            let mut result=visible.serialize_ranges(footnote);
            for raw in tail.into_iter().rev(){result.push_str(&self.separator);result.push_str(&raw);}
            return result;
        }
        if self.atoms.is_empty() {
            let (p, c) = self.empty;
            return self.paragraphs[p].wrap(&self.characters[c].wrap(""));
        }
        let mut out = String::new();
        let mut start = 0;
        // Exported footnotes start a fresh style range before paragraphs 32, 64, ...
        let mut paragraph_number = 1usize;
        while start < self.atoms.len() {
            let p = self.atoms[start].p;
            let mut end = start;
            while end < self.atoms.len() && self.paragraphs[self.atoms[end].p].key == self.paragraphs[p].key {
                let atom = &self.atoms[end];
                end += 1;
                if footnote && atom.ch == '\r' {
                    paragraph_number += 1;
                    if paragraph_number.is_multiple_of(32) { break; }
                }
            }
            let mut para = String::new();
            let mut cs = start;
            while cs < end {
                let c = self.atoms[cs].c;
                let mut ce = cs + 1;
                // InDesign puts index markers in character ranges of their own (index-c1, measured)
                while ce < end && self.characters[self.atoms[ce].c].key == self.characters[c].key && self.atoms[ce].is_marker() == self.atoms[cs].is_marker() { ce += 1; }
                let mut pieces = Vec::new();
                let mut content = String::new();
                for a in &self.atoms[cs..ce] {
                    if a.content {
                        content.push_str(&a.raw);
                    } else {
                        if !content.is_empty() {
                            pieces.push(format!("<Content>{content}</Content>"));
                            content.clear();
                        }
                        pieces.push(a.raw.clone());
                    }
                }
                if !content.is_empty() { pieces.push(format!("<Content>{content}</Content>")); }
                let body = pieces.join(&self.characters[c].separator);
                if !para.is_empty() { para.push_str(&self.paragraphs[p].separator); }
                para.push_str(&self.characters[c].wrap(&body));
                cs = ce;
            }
            if !out.is_empty() { out.push_str(&self.separator); }
            out.push_str(&self.paragraphs[p].wrap(&para));
            start = end;
        }
        if let Some(p)=self.trailing
            && self.paragraphs[p].key!=self.paragraphs[self.atoms.last().unwrap().p].key {
            let inner = if self.tail_range { self.characters[self.empty.1].wrap("") } else { String::new() };
            out.push_str(&self.separator);out.push_str(&self.paragraphs[p].wrap(&inner));
        }
        out
    }
}

/// A text by its owner: a story (or a frame of it) or a table cell, whose paragraphs are a
/// story's (InDesign writes a cell's text as a story's: tables cell-type, cell-return).
fn locate(w: &mut Work<'_>, id: &str) -> Result<(String, Index, usize, String), EditError> {
    if let Some((owner, index)) = folio_idml::footnotes::address(id) {
        let (part, ix, i, _) = locate(w, owner)?;
        let n = footnotes::node(w.text(&part)?, &ix, i, index)?;
        return Ok((part, ix, n, id.to_owned()));
    }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    match ix.local(t, i) {
        "Story" | "Cell" => Ok((part, ix, i, id.to_owned())),
        "TextFrame" => {
            let story = ix.value(t, i, "ParentStory").ok_or_else(|| unsupported("frame without story"))?;
            let (p, index, n) = w.find(&story)?;
            Ok((p, index, n, story))
        }
        _ => Err(EditError::new("invalid", format!("{id} is not a story or text frame"))),
    }
}

/// The story of a story or text frame id.
pub(crate) fn story_of(w: &mut Work<'_>, id: &str) -> Result<String, EditError> {
    Ok(locate(w, id)?.3)
}

/// Plain story characters in InDesign's index space; table cells/footnotes are not
/// recursively counted in their containing story. Useful for cursor/selection messages.
pub fn contents(t: &str, id: &str) -> Result<String, EditError> {
    if (t.contains("<Hyperlink") || t.contains("<CrossReferenceSource")) && !id.contains('#') {
        return folio_idml::editorial::contents(t,id).map_err(|e|EditError::new("xml",e));
    }
    if t.contains("<Change ") && !id.contains('#') {
        return folio_idml::editorial::contents(t, id).map_err(|e| EditError::new("xml",e));
    }
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let (owner, index) = folio_idml::footnotes::address(id).map_or((id, None), |(owner, index)| (owner, Some(index)));
    let mut i = ix.by_self(t, owner).ok_or_else(|| EditError::new("notFound", id))?;
    if let Some(index) = index { i = footnotes::node(t, &ix, i, index)?; }
    if !matches!(ix.local(t, i), "Story" | "Cell" | "Footnote") { return Err(EditError::new("invalid", "expected text owner")); }
    Ok(query::read(t, &ix, i, false)?.atoms.iter().map(|a| a.ch).collect())
}

fn write(w: &mut Work<'_>, part: &str, id: &str, story: &Story) -> Result<(), EditError> {
    let new = story.serialize_ranges(folio_idml::footnotes::address(id).is_some());
    let old = &w.text(part)?[story.range.clone()];
    // Record the smallest changed byte slice, including Unicode boundary checks.
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) { prefix -= 1; }
    let mut suffix = old[prefix..].bytes().rev().zip(new[prefix..].bytes().rev()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) { suffix -= 1; }
    let range = story.range.start + prefix..story.range.end - suffix;
    w.splice(part, vec![(range, new[prefix..new.len() - suffix].to_owned())])?;
    // a cell's text is composed with its story (each story is a part of its own)
    w.recompose(part.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).unwrap_or(id));
    Ok(())
}

pub(crate) fn insert(w: &mut Work<'_>, id: &str, at: usize, text: &str) -> Result<(), EditError> {
    track_edit::require_untracked(w,id)?;
    if crate::hyperlinks::text_edit::edit(w,id,at,at,Some(text))? {return Ok(());}
    if text.is_empty() {
        let (part, ix, i, _) = locate(w, id)?;
        let s = Story::read(w.text(&part)?, &ix, i)?;
        if at > s.atoms.len() { return Err(EditError::new("invalid", "insertion point outside story")); }
        return Ok(());
    }
    insert_with(w, id, at, &|_, p, c| {
        let mut added = Vec::new();
        for mut ch in text.chars() {
            if ch == '\n' { ch = '\u{2028}'; } // text-insert-forced-break: U+2028 in Content
            if ch < ' ' && !matches!(ch, '\r' | '\t') {
                return Err(EditError::new("invalid", "unsupported input control character"));
            }
            added.push(Atom { p, c, ch, content: ch != '\r', raw: raw_of(ch) });
        }
        Ok(added)
    })
}

/// A typed character as InDesign writes it in `<Content>` (a return: `<Br />`); straight quotes
/// are entities there (oracle text-special special-quote-ds, special-quote-ss).
fn raw_of(ch: char) -> String {
    match ch {
        '\r' => "<Br />".to_owned(),
        '&' => "&amp;".to_owned(), '<' => "&lt;".to_owned(), '>' => "&gt;".to_owned(),
        '"' => "&quot;".to_owned(), '\'' => "&apos;".to_owned(),
        ch if ch as u32 > 0xffff => format!("&#x{:x};", ch as u32),
        ch => ch.to_string(),
    }
}

/// What Type ▸ Insert Special Character / White Space / Break Character puts in a story, by
/// InDesign's scripting name (oracle text-special: what InDesign 2025 wrote for each).
enum Special {
    /// A character of its own (U+2014, U+00A0…).
    Char(char),
    /// A marker: `<?ACE n?>` in `<Content>` (18 current page number, 19 section marker,
    /// 3 end nested style here, 7 indent to here, 8 right indent tab).
    Ace(u8),
    /// The next or previous page number: the page number marker in a range of its own
    /// with `PageNumberType`.
    PageNumber(&'static str),
    /// A column, frame or page break: a paragraph end in a range of its own with
    /// `ParagraphBreakType`.
    Break(&'static str),
}

fn special_of(name: &str) -> Option<Special> {
    use Special::*;
    Some(match name {
        "BULLET_CHARACTER" => Char('\u{2022}'),
        "COPYRIGHT_SYMBOL" => Char('\u{a9}'),
        "ELLIPSIS_CHARACTER" => Char('\u{2026}'),
        "PARAGRAPH_SYMBOL" => Char('\u{b6}'),
        "REGISTERED_TRADEMARK" => Char('\u{ae}'),
        "SECTION_SYMBOL" => Char('\u{a7}'),
        "TRADEMARK_SYMBOL" => Char('\u{2122}'),
        "EM_DASH" => Char('\u{2014}'),
        "EN_DASH" => Char('\u{2013}'),
        "DISCRETIONARY_HYPHEN" => Char('\u{ad}'),
        "NONBREAKING_HYPHEN" => Char('\u{2011}'),
        "DOUBLE_LEFT_QUOTE" => Char('\u{201c}'),
        "DOUBLE_RIGHT_QUOTE" => Char('\u{201d}'),
        "SINGLE_LEFT_QUOTE" => Char('\u{2018}'),
        "SINGLE_RIGHT_QUOTE" => Char('\u{2019}'),
        "DOUBLE_STRAIGHT_QUOTE" => Char('"'),
        "SINGLE_STRAIGHT_QUOTE" => Char('\''),
        "TAB" => Char('\t'),
        "ZERO_WIDTH_NONJOINER" => Char('\u{200c}'),
        "EM_SPACE" => Char('\u{2003}'),
        "EN_SPACE" => Char('\u{2002}'),
        "NONBREAKING_SPACE" => Char('\u{a0}'),
        "FIXED_WIDTH_NONBREAKING_SPACE" => Char('\u{202f}'),
        "HAIR_SPACE" => Char('\u{200a}'),
        "SIXTH_SPACE" => Char('\u{2006}'),
        "THIN_SPACE" => Char('\u{2009}'),
        "QUARTER_SPACE" => Char('\u{2005}'),
        "THIRD_SPACE" => Char('\u{2004}'),
        "PUNCTUATION_SPACE" => Char('\u{2008}'),
        "FIGURE_SPACE" => Char('\u{2007}'),
        "FLUSH_SPACE" => Char('\u{2001}'),
        "FORCED_LINE_BREAK" => Char('\u{2028}'),
        "DISCRETIONARY_LINE_BREAK" => Char('\u{200b}'),
        "PARAGRAPH_RETURN" => Char('\r'),
        "AUTO_PAGE_NUMBER" => Ace(18),
        "SECTION_MARKER" => Ace(19),
        "END_NESTED_STYLE" => Ace(3),
        "INDENT_HERE_TAB" => Ace(7),
        "RIGHT_INDENT_TAB" => Ace(8),
        "NEXT_PAGE_NUMBER" => PageNumber("NextPageNumber"),
        "PREVIOUS_PAGE_NUMBER" => PageNumber("PreviousPageNumber"),
        "COLUMN_BREAK" => Break("NextColumn"),
        "FRAME_BREAK" => Break("NextFrame"),
        "PAGE_BREAK" => Break("NextPage"),
        "ODD_PAGE_BREAK" => Break("NextOddPage"),
        "EVEN_PAGE_BREAK" => Break("NextEvenPage"),
        _ => return None,
    })
}

/// Type ▸ Insert Special Character / White Space / Break Character at `at`.
pub(crate) fn insert_special(w: &mut Work<'_>, id: &str, at: usize, name: &str) -> Result<(), EditError> {
    if name == "FOOTNOTE_SYMBOL" { return footnotes::restore_marker(w, id, at); }
    let special = special_of(name).ok_or_else(|| EditError::new("invalid", format!("unknown special character {name}")))?;
    if let Special::Char(ch) = special {
        return insert(w, id, at, &ch.to_string());
    }
    insert_with(w, id, at, &|s, p, c| {
        Ok(vec![match special {
            Special::Ace(n) => Atom { p, c, ch: '\u{fffc}', content: true, raw: format!("<?ACE {n}?>") },
            Special::PageNumber(kind) => {
                let style = format::with_attr(&s.characters[c], "PageNumberType", kind);
                s.characters.push(style);
                Atom { p, c: s.characters.len() - 1, ch: '\u{fffc}', content: true, raw: "<?ACE 18?>".to_owned() }
            }
            Special::Break(kind) => {
                let style = format::with_attr(&s.characters[c], "ParagraphBreakType", kind);
                s.characters.push(style);
                Atom { p, c: s.characters.len() - 1, ch: '\r', content: false, raw: "<Br />".to_owned() }
            }
            Special::Char(_) => unreachable!(),
        }])
    })
}

/// Type ▸ Text Variables ▸ Insert Variable (oracle variables var-insert-*): the instance sits in a range of its own, the typing
/// style with `PageNumberType="TextVariable"`; its `Name` is the variable's, `ResultText` the custom text's contents (the others
/// are set from the layout, `SetVariableResults`).
pub(crate) fn insert_variable(w: &mut Work<'_>, id: &str, at: usize, variable: &str) -> Result<(), EditError> {
    insert_variable_showing(w, id, at, variable, None)
}

/// [`insert_variable`] with the text the instance shows until it is worked out again (`ResultText`; a live caption's metadata).
pub(crate) fn insert_variable_showing(w: &mut Work<'_>, id: &str, at: usize, variable: &str, shown: Option<&str>) -> Result<(), EditError> {
    let dm = w.text("designmap.xml")?.to_owned();
    let dx = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let v = dx.by_self(&dm, variable).filter(|&v| dx.local(&dm, v) == "TextVariable").ok_or_else(|| EditError::new("notFound", format!("text variable {variable}")))?;
    let name = dx.attr(&dm, v, "Name").unwrap_or_default().to_owned();
    let result = if let Some(shown) = shown {
        shown.to_owned()
    } else if dx.attr(&dm, v, "VariableType") == Some("CustomTextType") {
        dx.path(&dm, v, &["CustomTextVariablePreference", "Properties", "Contents"]).map(|c| unescape(dm[dx.nodes[c].open_end..dx.nodes[c].close_start].trim())).unwrap_or_default()
    } else {
        String::new()
    };
    let instance = w.new_id();
    let raw = format!("<TextVariableInstance Self=\"{instance}\" Name=\"{name}\" ResultText=\"{}\" AssociatedTextVariable=\"{variable}\" />", crate::xml::escape(&result));
    insert_with(w, id, at, &|s, p, c| {
        let style = format::with_attr(&s.characters[c], "PageNumberType", "TextVariable");
        s.characters.push(style);
        Ok(vec![Atom { p, c: s.characters.len() - 1, ch: '\u{18}', content: false, raw: raw.clone() }])
    })
}

/// What a text variable instance in a story says (its `Name`, `ResultText` and `AssociatedTextVariable`).
pub(crate) struct Instance {
    pub result: String,
    pub variable: String,
}

fn instance_of(raw: &str) -> Option<Instance> {
    if !raw.starts_with("<TextVariableInstance ") {
        return None;
    }
    let ix = Index::scan(raw).ok()?;
    Some(Instance { result: ix.value(raw, 0, "ResultText").unwrap_or_default(), variable: ix.value(raw, 0, "AssociatedTextVariable").unwrap_or_default() })
}

/// Replaces the instances of text variables that `pick` (given the instance's index in the story and what it says) answers a text for by
/// that text: its characters have the instance's formatting without `PageNumberType` (InDesign's export merges them into the range
/// around them, oracle variables var-convert-*). How many were replaced.
pub(crate) fn convert_instances(w: &mut Work<'_>, id: &str, pick: &dyn Fn(usize, &Instance) -> Option<String>) -> Result<usize, EditError> {
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    let found: Vec<(usize, String)> = s.atoms.iter().enumerate().filter_map(|(k, a)| instance_of(&a.raw).and_then(|inst| pick(k, &inst)).map(|t| (k, t))).collect();
    let count = found.len();
    if count == 0 {
        return Ok(0);
    }
    for (k, text) in found.into_iter().rev() {
        let p = s.atoms[k].p;
        let style = format::without_attr(&s.characters[s.atoms[k].c], "PageNumberType");
        s.characters.push(style);
        let c = s.characters.len() - 1;
        let atoms: Vec<Atom> = text.chars().map(|ch| Atom { p, c, ch, content: ch != '\r', raw: raw_of(ch) }).collect();
        s.atoms.splice(k..k + 1, atoms);
    }
    write(w, &part, &id, &s)?;
    Ok(count)
}

/// Type ▸ Text Variables ▸ Convert Variable to Text: the instances in [start, end) become the text they show.
pub(crate) fn convert_variables(w: &mut Work<'_>, id: &str, start: usize, end: usize) -> Result<(), EditError> {
    if convert_instances(w, id, &|k, inst| (k >= start && k < end).then(|| inst.result.clone()))? == 0 {
        return Err(EditError::new("invalid", "no text variable in the range"));
    }
    Ok(())
}

/// Type ▸ Change Case of the letters in [start, end) (oracle text-special case-*):
/// - title case: a letter starts a word when the character before it (inside the selection or
///   not) is not a letter, digit or apostrophe; such letters go upper case, the others lower;
/// - sentence case: a letter starts a sentence when, going back over what is neither letter nor
///   digit, a paragraph's start or `.`, `!`, `?` comes before any letter or digit.
pub(crate) fn change_case(w: &mut Work<'_>, id: &str, start: usize, end: usize, mode: &str) -> Result<(), EditError> {
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    if start > end || end > s.atoms.len() { return Err(EditError::new("invalid", "range outside story")); }
    if !matches!(mode, "UPPERCASE" | "LOWERCASE" | "TITLECASE" | "SENTENCECASE") {
        return Err(EditError::new("invalid", format!("unknown case {mode}")));
    }
    let chars: Vec<char> = s.atoms.iter().map(|a| a.ch).collect();
    let in_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let sentence_start = |k: usize| {
        for &c in chars[..k].iter().rev() {
            if c == '\r' || matches!(c, '.' | '!' | '?') {
                return true;
            }
            if c.is_alphanumeric() {
                return false;
            }
        }
        true
    };
    let one = |it: &mut dyn Iterator<Item = char>| -> Option<char> {
        let c = it.next()?;
        it.next().is_none().then_some(c)
    };
    let mut changed = false;
    for k in start..end {
        let a = &s.atoms[k];
        if !a.content || !a.ch.is_alphabetic() {
            continue;
        }
        let upper = match mode {
            "UPPERCASE" => true,
            "LOWERCASE" => false,
            "TITLECASE" => k == 0 || !in_word(chars[k - 1]),
            _ => sentence_start(k),
        };
        let new = if upper { one(&mut a.ch.to_uppercase()) } else { one(&mut a.ch.to_lowercase()) };
        if let Some(n) = new.filter(|&n| n != a.ch) {
            let a = &mut s.atoms[k];
            a.ch = n;
            a.raw = raw_of(n);
            changed = true;
        }
    }
    if changed {
        write(w, &part, &id, &s)?;
    }
    Ok(())
}

/// Inserts the atoms `build` makes (given the story, the paragraph and the typing style the
/// insertion point has) at `at`, as typing does.
fn insert_with(w: &mut Work<'_>, id: &str, at: usize, build: &dyn Fn(&mut Story, usize, usize) -> Result<Vec<Atom>, EditError>) -> Result<(), EditError> {
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    if at > s.atoms.len() { return Err(EditError::new("invalid", "insertion point outside story")); }
    let (p,c)=typing_at(w,&id,&mut s,at,at);
    let mut added = build(&mut s, p, c)?;
    // text-insert-paragraph-start: new complete paragraphs at a paragraph's beginning
    // take the previous paragraph's attributes; the final fragment joins the existing
    // paragraph. Character format stays that of the right-hand character (17pt here).
    if at > 0 && s.atoms[at - 1].ch == '\r'
        && let Some(last_return) = added.iter().rposition(|a| a.ch == '\r') {
        for a in &mut added[..=last_return] { a.p = s.atoms[at - 1].p; }
    }
    let next = at + added.len();
    s.atoms.splice(at..at, added);
    write(w, &part, &id, &s)?;
    w.ed.text_state.insert(id, InsertionStyle { at: next, character: s.characters[c].clone() });
    Ok(())
}

fn typing_at(w: &Work<'_>, id: &str, s: &mut Story, at: usize, visible_at: usize) -> (usize,usize) {
    // text-insert-*-boundary: inherit the left character. At a paragraph's beginning,
    // use its first character (text-insert-start, text-insert-paragraph-start).
    let inherit = if at > 0 && s.atoms[at - 1].ch != '\r' { s.atoms.get(at - 1) } else { s.atoms.get(at).or_else(|| s.atoms.last()) };
    let (mut p, mut c) = inherit.map(|a| (a.p, a.c)).unwrap_or(s.empty);
    if at==s.atoms.len()
        && let Some(tail)=s.trailing.take() {p=tail;}
    if let Some(style) = w.ed.text_state.get(id).filter(|style| style.at == visible_at) {
        c = s.characters.len();
        s.characters.push(style.character.clone());
    }
    let typing=format::typing_style(&s.characters[c]);
    c=s.characters.len(); s.characters.push(typing);
    (p,c)
}

pub(crate) fn delete(w: &mut Work<'_>, id: &str, start: usize, end: usize) -> Result<(), EditError> {
    track_edit::require_untracked(w,id)?;
    if crate::hyperlinks::text_edit::edit(w,id,start,end,None)? {return Ok(());}
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    if start > end || end > s.atoms.len() { return Err(EditError::new("invalid", "deletion range outside story")); }
    if start == end { return Ok(()); }
    let first = &s.atoms[start];
    // format-empty-{character,direct}, format-trailing-delete-all: the final
    // paragraph's formatting survives a whole-story deletion, including an empty
    // final PSR. The selected range's first character supplies the typing format.
    s.empty = (if start==0 && end==s.atoms.len() { s.trailing.unwrap_or_else(||s.atoms.last().unwrap().p) } else {first.p}, first.c);
    let style = InsertionStyle { at: start, character: s.characters[first.c].clone() };
    s.atoms.drain(start..end);
    if s.atoms.last().is_none_or(|a| a.ch != '\r') { s.trailing=None; }
    // text-delete-paragraph-{break,range}: first surviving paragraph's attributes win.
    // text-delete-first-paragraph: removing the entire paragraph keeps the next one's.
    let mut paragraph = None;
    for a in &mut s.atoms {
        let p = *paragraph.get_or_insert(a.p);
        a.p = p;
        if a.ch == '\r' { paragraph = None; }
    }
    write(w, &part, &id, &s)?;
    w.ed.text_state.insert(id, style);
    Ok(())
}

/// Replaces story characters [start, end) (at least one) with `text` set in the formatting of the first replaced character
/// (Data Merge ▸ Update Content in Data Fields: a field's new text keeps the field's formatting).
pub(crate) fn replace_keeping_format(w: &mut Work<'_>, id: &str, start: usize, end: usize, text: &str) -> Result<(), EditError> {
    track_edit::require_untracked(w, id)?;
    if crate::hyperlinks::text_edit::edit(w, id, start, end, Some(text))? { return Ok(()); }
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    if start >= end || end > s.atoms.len() { return Err(EditError::new("invalid", "range outside story")); }
    let (p, c) = (s.atoms[start].p, s.atoms[start].c);
    let mut added = Vec::new();
    for mut ch in text.chars() {
        if ch == '\n' { ch = '\u{2028}'; }
        if ch < ' ' && !matches!(ch, '\r' | '\t') { return Err(EditError::new("invalid", "unsupported input control character")); }
        added.push(Atom { p, c, ch, content: ch != '\r', raw: raw_of(ch) });
    }
    s.atoms.splice(start..end, added);
    if s.atoms.last().is_some_and(|a| a.ch != '\r') { s.trailing = None; }
    write(w, &part, &id, &s)
}

/// Replaces story characters [start, end) with one inline node `raw` (a table: `ch` U+0016),
/// set in the formatting of the first replaced character, or of an insertion point there (the
/// character to its left within the paragraph, else the one at it).
pub(crate) fn replace_with_node(w: &mut Work<'_>, id: &str, start: usize, end: usize, ch: char, raw: &str) -> Result<(), EditError> {
    replace_node(w, id, start, end, ch, raw, None)
}

/// `replace_with_node` for the object character Create Outlines leaves: its character range gets an explicit Leading
/// and the baseline shift `(leading, shift)` (see `format::object_style`).
pub(crate) fn replace_with_object(w: &mut Work<'_>, id: &str, start: usize, end: usize, raw: &str, leading: f64, shift: f64) -> Result<(), EditError> {
    replace_node(w, id, start, end, '\u{fffc}', raw, Some((leading, shift)))
}

fn replace_node(w: &mut Work<'_>, id: &str, start: usize, end: usize, ch: char, raw: &str, object: Option<(f64, f64)>) -> Result<(), EditError> {
    let (part, ix, i, id) = locate(w, id)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    if start > end || end > s.atoms.len() { return Err(EditError::new("invalid", "range outside story")); }
    let at = if start < end { s.atoms.get(start) } else if start > 0 && s.atoms[start - 1].ch != '\r' { s.atoms.get(start - 1) } else { s.atoms.get(start).or_else(|| s.atoms.last()) };
    let (p, mut c) = at.map(|a| (a.p, a.c)).unwrap_or(s.empty);
    if let Some((leading, shift)) = object {
        let style = format::object_style(&s.characters[c], leading, shift);
        s.characters.push(style);
        c = s.characters.len() - 1;
    }
    s.atoms.splice(start..end, [Atom { p, c, ch, content: false, raw: raw.to_owned() }]);
    if s.atoms.last().is_some_and(|a| a.ch != '\r') { s.trailing = None; }
    write(w, &part, &id, &s)
}

/// Story characters [start, end) as rows (its paragraphs) of cells (between tabs): each
/// cell's XML, paragraph and character ranges with their formatting; a final paragraph end
/// starts no row.
pub(crate) fn cells_of(w: &mut Work<'_>, id: &str, start: usize, end: usize) -> Result<Vec<Vec<String>>, EditError> {
    let (part, ix, i, _) = locate(w, id)?;
    let s = Story::read(w.text(&part)?, &ix, i)?;
    if start > end || end > s.atoms.len() { return Err(EditError::new("invalid", "range outside story")); }
    let first = s.atoms.get(start).map(|a| (a.p, a.c)).unwrap_or(s.empty);
    let mut rows: Vec<Vec<(Vec<Atom>, (usize, usize))>> = vec![vec![(Vec::new(), first)]];
    for a in &s.atoms[start..end] {
        match a.ch {
            '\r' => rows.push(vec![(Vec::new(), (a.p, a.c))]),
            '\t' => rows.last_mut().expect("a row").push((Vec::new(), (a.p, a.c))),
            _ => {
                let cell = rows.last_mut().expect("a row").last_mut().expect("a cell");
                if cell.0.is_empty() { cell.1 = (a.p, a.c); }
                cell.0.push(a.clone());
            }
        }
    }
    if end > start && s.atoms[end - 1].ch == '\r' {
        rows.pop();
    }
    Ok(rows.into_iter().map(|row| row.into_iter().map(|(atoms, empty)| Story {
        range: 0..0, separator: s.separator.clone(), paragraphs: s.paragraphs.clone(), characters: s.characters.clone(), atoms, empty, trailing: None, tail_range: false,
    }.serialize()).collect()).collect())
}

/// The number of characters of a story (InDesign's index space).
pub(crate) fn contents_len(w: &mut Work<'_>, id: &str) -> Result<usize, EditError> {
    let (part, ix, i, _) = locate(w, id)?;
    Ok(Story::read(w.text(&part)?, &ix, i)?.atoms.len())
}

/// Replaces the inline node at `at` (a table) with its cells' texts, `rows` of cell `Self`s:
/// cells joined by tabs and rows by paragraph ends, each cell's text with its own formatting
/// (a separator takes the formatting before it).
pub(crate) fn unnest(w: &mut Work<'_>, id: &str, at: usize, rows: &[Vec<String>]) -> Result<(), EditError> {
    let (part, ix, i, id) = locate(w, id)?;
    let t = w.text(&part)?;
    let mut s = Story::read(t, &ix, i)?;
    let node = s.atoms.get(at).filter(|a| !a.content).map(|a| (a.p, a.c)).ok_or_else(|| EditError::new("invalid", "no table there"))?;
    let mut out: Vec<Atom> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (k, cell) in row.iter().enumerate() {
            if k > 0 {
                let (p, c) = out.last().map_or(node, |a| (a.p, a.c));
                out.push(Atom { p, c, ch: '\t', content: true, raw: "\t".into() });
            }
            let ci = ix.by_self(t, cell).ok_or_else(|| EditError::new("notFound", cell.clone()))?;
            let cs = Story::read(t, &ix, ci)?;
            let (po, co) = (s.paragraphs.len(), s.characters.len());
            s.paragraphs.extend(cs.paragraphs);
            s.characters.extend(cs.characters);
            out.extend(cs.atoms.into_iter().map(|a| Atom { p: a.p + po, c: a.c + co, ..a }));
        }
        if r + 1 < rows.len() {
            let (p, c) = out.last().map_or(node, |a| (a.p, a.c));
            out.push(Atom { p, c, ch: '\r', content: false, raw: "<Br />".into() });
        }
    }
    s.atoms.splice(at..at + 1, out);
    write(w, &part, &id, &s)
}

/// The character position of an inline node (by its `Self`) in its story.
pub(crate) fn node_position(w: &mut Work<'_>, story: &str, node: &str) -> Result<usize, EditError> {
    let (part, ix, i, _) = locate(w, story)?;
    let s = Story::read(w.text(&part)?, &ix, i)?;
    let pat = format!(" Self=\"{node}\"");
    s.atoms.iter().position(|a| !a.content && a.raw.contains(&pat)).ok_or_else(|| EditError::new("notFound", format!("{node} is not in {story}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Editor};
    use std::collections::BTreeMap;

    const PART: &str = "Stories/Story_s.xml";
    const XML: &str = "<idPkg:Story><Story Self=\"s\" Unknown=\"keep\"><Properties><Private>opaque</Private></Properties><ParagraphStyleRange AppliedParagraphStyle=\"P\"><CharacterStyleRange AppliedCharacterStyle=\"Normal\"><Content>AB</Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"Bold\"><Properties><Custom type=\"string\">keep &amp; this</Custom></Properties><Content>CD</Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"Normal\"><Content>EF</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>";

    fn editor(xml: &str) -> Editor {
        Editor::new(BTreeMap::from([(PART.into(), xml.into()), ("Unrelated.xml".into(), "<unknown/>".into())]))
    }
    fn ins(at: usize, text: &str) -> Command { Command::InsertText { story: "s".into(), at, text: text.into() } }
    fn del(start: usize, end: usize) -> Command { Command::DeleteText { story: "s".into(), start, end } }

    #[test]
    fn insertion_changes_only_content_bytes_and_invalid_batch_rolls_back() {
        let mut e = editor(XML);
        let before = e.parts().clone();
        let o = e.apply("type", &[ins(1, "한😀&")]).unwrap();
        assert_eq!(e.part(PART).unwrap(), XML.replace(">AB<", ">A한&#x1f600;&amp;B<"));
        assert_eq!(o.recompose, ["s"]);
        assert_eq!(e.part("Unrelated.xml"), Some("<unknown/>"));
        e.undo().unwrap();
        assert_eq!(e.parts(), &before);
        assert!(e.apply("bad batch", &[del(2, 4), ins(999, "x")]).is_err());
        assert_eq!(e.parts(), &before);
        assert!(e.redo().is_some());
    }

    #[test]
    fn deletion_insertion_format_survives_undo_redo_and_failed_transactions() {
        let mut e = editor(XML);
        e.apply("delete", &[del(2, 4)]).unwrap();
        let deleted = e.parts().clone();
        // If the failed transaction leaked its temporary Normal insertion format, the
        // final X below would lose the removed range's Bold format.
        assert!(e.apply("bad", &[del(0, 1), ins(999, "bad")]).is_err());
        assert_eq!(e.parts(), &deleted);
        e.undo().unwrap();
        e.redo().unwrap();
        e.apply("type", &[ins(2, "X")]).unwrap();
        assert!(e.part(PART).unwrap().contains("<Content>X</Content></CharacterStyleRange>"));
        assert!(e.part(PART).unwrap().contains("AppliedCharacterStyle=\"Bold\""));
        e.undo().unwrap();
        assert_eq!(e.parts(), &deleted);
        e.apply("type again", &[ins(2, "Y")]).unwrap();
        assert!(e.part(PART).unwrap().contains("<Content>Y</Content></CharacterStyleRange>"));
    }

    #[test]
    fn scalar_and_inline_indices_do_not_include_table_contents() {
        let xml = XML.replace(">AB<", ">A&#x1f600;<").replace("<Content>CD</Content>",
            "<Content>C</Content><Table Self=\"table\"><Cell><Content>not outer text</Content></Cell></Table><Rectangle Self=\"anchor\"/><Br /><Content>D</Content>");
        let mut e = editor(&xml);
        assert_eq!(contents(&xml, "s").unwrap(), "A😀C\u{16}\u{fffc}\rDEF");
        e.apply("delete specials", &[del(3, 6)]).unwrap();
        assert_eq!(contents(e.part(PART).unwrap(), "s").unwrap(), "A😀CDEF");
        e.undo().unwrap();
        assert_eq!(e.part(PART), Some(xml.as_str()));
    }

    #[test]
    fn unknown_wrappers_and_metadata_are_refused_without_data_loss() {
        for xml in [
            XML.replace("<Content>AB</Content>", "<HiddenText><Content>AB</Content></HiddenText>"),
            XML.replace("<Content>AB</Content>", "<Content>AB</Content><!-- metadata -->"),
        ] {
            let mut e = editor(&xml);
            assert_eq!(e.apply("type", &[ins(1, "x")]).unwrap_err().code, "unsupported");
            assert_eq!(e.part(PART), Some(xml.as_str()));
            assert_eq!(e.rev(), 0);
        }
    }
}
