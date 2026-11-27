//! Word (.docx) the way InDesign's Place reads it (oracle placetext-c2; measurements in `artifacts/placetext-c2-20261023/measurement`).
//!
//! What was measured (InDesign 2025, `wordRTFImportPreferences` at its defaults):
//! - Every paragraph keeps its Word style by name; the styles the text uses (and those they are based on or continue with) come into the
//!   document as styles marked `Imported`, in the order of Word's `styles.xml`; a style the document already has is kept as it is. A style
//!   nothing uses is not imported.
//! - A style's own settings become the style's attributes: size (`sz`), bold and italic (`FontStyle`), colour (a new swatch
//!   `Word_R255_G0_B0`), underline, strikethrough, capitals; alignment, indents, spacing, line spacing (`AutoLeading` = `line / 240 * 100`),
//!   keep with next / keep lines. A value of zero is not written. Fonts are not imported (neither in a style nor in a run).
//! - Formatting on a run or a paragraph is a local override, written only where it differs from what its styles already give (the same size,
//!   or turning bold off where the style is bold, leaves nothing): bold and italic add up (`Bold Italic`), a size or colour replaces.
//! - A line break is U+2028, a tab a tab; a page or column break ends the paragraph with a return that carries `ParagraphBreakType`.

mod archive;
mod styles;
mod walk;

use archive::{part, read_pictures};
use crate::messages::Message;
use crate::numbering::Lists;
use crate::xmltree::{El, parse};
use crate::{Inline, Options, Paragraph, Run, TextSource, quotes};
use std::collections::BTreeSet;
use styles::{Colors, needed, read_styles, style_def};
use walk::Walk;

/// Warnings, each said once.
#[derive(Default)]
struct Notes(Vec<Message>);

impl Notes {
    fn add(&mut self, text: Message) {
        if !self.0.iter().any(|n| n.message == text.message) {
            self.0.push(text);
        }
    }
}

pub(crate) fn read(bytes: &[u8], options: &Options) -> Result<TextSource, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("Word 파일을 열 수 없습니다: {e}"))?;
    let document = part(&mut zip, "word/document.xml")?.ok_or("Word 파일이 아닙니다: word/document.xml 이 없습니다")?;
    let styles_xml = part(&mut zip, "word/styles.xml")?;
    let mut notes = Notes::default();
    let styles_root = match &styles_xml {
        Some(x) => Some(parse(x)?),
        None => None,
    };
    let styles = read_styles(styles_root.as_ref(), &mut notes);
    let root = parse(&document)?;
    let body = root.child("body").ok_or("Word 파일이 아닙니다: 본문(body)이 없습니다")?;
    let footnotes = match part(&mut zip, "word/footnotes.xml")? {
        Some(x) => parse(&x)?.elements().filter(|e| e.name == "footnote" && !matches!(e.attr("type"), Some("separator" | "continuationSeparator" | "continuationNotice"))).filter_map(|e| Some((e.attr("id")?.to_owned(), clone_el(e)))).collect(),
        None => Vec::new(),
    };
    let endnotes = match part(&mut zip, "word/endnotes.xml")? {
        Some(x) => parse(&x)?.elements().filter(|e| e.name == "endnote" && !matches!(e.attr("type"), Some("separator" | "continuationSeparator" | "continuationNotice"))).filter_map(|e| Some((e.attr("id")?.to_owned(), clone_el(e)))).collect(),
        None => Vec::new(),
    };
    let numbering_root = match part(&mut zip, "word/numbering.xml")? {
        Some(x) => Some(parse(&x)?),
        None => None,
    };
    let lists = Lists::read(numbering_root.as_ref());
    let media = read_pictures(&mut zip)?;
    let mut walk = Walk { styles: &styles, options, notes, colors: Colors(Vec::new()), paragraphs: Vec::new(), used_paragraph: BTreeSet::new(), used_character: BTreeSet::new(), footnotes, endnotes, endnote_count: 0, pending: Vec::new(), lists, media };
    walk.body(body)?;
    // a table that ends the text has no paragraph to go in front of: it makes the paragraph of its own
    if !walk.pending.is_empty() {
        let mut last = Paragraph::default();
        for table in std::mem::take(&mut walk.pending) {
            last.runs.push(Run { text: "\u{16}".into(), inline: Some(Inline::Table(table)), ..Run::default() });
        }
        walk.paragraphs.push(last);
    }
    let Walk { mut notes, mut colors, mut paragraphs, used_paragraph, used_character, lists, endnote_count, .. } = walk;
    for note in &lists.notes {
        notes.add(note.clone());
    }
    if options.use_typographers_quotes {
        quotes::story(&mut paragraphs);
    }
    let (mut paragraph_styles, mut character_styles) = (Vec::new(), Vec::new());
    if !options.remove_formatting {
        for id in needed(&styles, &used_paragraph, true) {
            let s = styles.find(&id, true).expect("needed returns IDs from the same paragraph style list");
            let root = s.based_on.as_deref().and_then(|b| styles.find(b, true)).is_none();
            paragraph_styles.push(style_def(s, &styles, &mut colors, root));
        }
        for id in needed(&styles, &used_character, false) {
            character_styles.push(style_def(styles.find(&id, false).expect("needed returns IDs from the same character style list"), &styles, &mut colors, false));
        }
    }
    // the character styles the bullets and numbers are set in come after the file's own styles, in a group of their own
    character_styles.extend(lists.style_defs());
    // a file with no paragraph at all makes one empty paragraph and brings no style (measured: placetext-empty.docx)
    if paragraphs.is_empty() {
        paragraphs.push(Paragraph::default());
    }
    Ok(TextSource { format: "docx".into(), paragraphs, paragraph_styles, character_styles, colors: colors.0, formatted: !options.remove_formatting, ends_with_return: endnote_count > 0, numbering_lists: lists.list_names(), warnings: notes.0.iter().map(|m| m.message.clone()).collect(), warning_messages: notes.0 })
}

fn clone_el(e: &El) -> El {
    El {
        name: e.name.clone(),
        attrs: e.attrs.clone(),
        kids: e.kids.iter().map(|n| match n {
            crate::xmltree::Node::El(c) => crate::xmltree::Node::El(clone_el(c)),
            crate::xmltree::Node::Text(t) => crate::xmltree::Node::Text(t.clone()),
        }).collect(),
    }
}
