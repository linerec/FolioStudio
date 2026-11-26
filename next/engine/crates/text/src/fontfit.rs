//! File ▸ Adjust Layout with "Adjust font size": what the editor cannot read off an IDML story by itself, the composition of the text at its old size.
//!
//! InDesign does not compose the text again at the trial sizes (layoutadj-c7; `next/oracle/rules/layoutadj-c7.ko.md`): from the story composed
//! as it is it takes the number of lines of each paragraph, the width of the frame's column and the natural width of each paragraph's last line,
//! and counts the lines of a trial from those. The editor (`folio-edit` `ops/pages/fontfit.rs`) holds the search; this module hands it the facts.
//!
//! Supported (the measured case): a text frame of one column with no inset, not threaded, whose story is plain text in one font at one size
//! and, when the leading is fixed, one leading, and fits its frame. Anything else is refused with the reason, so the editor changes nothing.
//! Several paragraphs follow the same model paragraph by paragraph (not measured: approximate).
use crate::fonts::FontStore;
use crate::layout::columns_of;
use folio_model::layout::DocumentLayout;
use folio_model::{Document, ItemKind, Leading, PageItem};
use serde::Serialize;
use std::collections::BTreeMap;

/// The space of the line the model adds to the length of the last line ("e", layoutadj-c7: about the font's space width less 0.2 pt).
const SPACE_LESS: f64 = 0.2;

/// What the editor needs of one text frame: the story it sets, the width of its column, the size and fixed leading of its text, the model's
/// small constant `e`, and per paragraph the number of lines and the natural width of the last one.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FrameFit {
    pub story: String,
    pub width: f64,
    pub size: f64,
    pub leading: Option<f64>,
    pub e: f64,
    pub paragraphs: Vec<(usize, f64)>,
}

/// Every text frame of the document with its facts, or the reason it cannot be adjusted.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Fits {
    pub frames: BTreeMap<String, Result<FrameFit, String>>,
}

fn each<'a>(items: &'a [PageItem], out: &mut Vec<&'a PageItem>) {
    for it in items {
        out.push(it);
        if let ItemKind::Group { children } = &it.kind {
            each(children, out);
        }
    }
}

/// The facts of every text frame (the spreads' and the parent pages').
pub fn frame_fits(doc: &Document, layout: &DocumentLayout, fonts: &FontStore) -> Fits {
    let mut all = Vec::new();
    for s in doc.spreads.iter().chain(doc.master_spreads.iter()) {
        each(&s.items, &mut all);
    }
    let mut frames = BTreeMap::new();
    for it in all {
        if matches!(it.kind, ItemKind::TextFrame(_)) {
            frames.insert(it.id.clone(), frame_fit(doc, layout, fonts, it));
        }
    }
    Fits { frames }
}

fn frame_fit(doc: &Document, layout: &DocumentLayout, fonts: &FontStore, item: &PageItem) -> Result<FrameFit, String> {
    let ItemKind::TextFrame(f) = &item.kind else { return Err("not a text frame".into()) };
    // a frame whose story has no text (an empty frame, a parent page's text frame) has nothing to adjust, whatever else it is
    if !doc.stories.get(&f.story).is_some_and(|s| s.paragraphs.iter().any(|p| p.runs.iter().any(|r| !r.text.is_empty()))) {
        return Ok(FrameFit { story: f.story.clone(), width: 0.0, size: 0.0, leading: None, e: 0.0, paragraphs: Vec::new() });
    }
    if f.previous.is_some() || f.next.is_some() {
        return Err("a text frame threaded to others is not supported (one frame holds the story)".into());
    }
    if f.columns != 1 {
        return Err("a text frame of several columns is not supported (one column)".into());
    }
    if f.insets.iter().any(|v| v.abs() > 1e-9) {
        return Err("a text frame with an inset is not supported (no inset)".into());
    }
    if item.text_path.is_some() {
        return Err("text on a path is not supported".into());
    }
    let story = doc.stories.get(&f.story).ok_or("the frame has no story")?;
    if layout.overset.contains(&f.story) {
        return Err("text that does not fit its frame is not supported (fit the text first)".into());
    }
    let text = layout.frames.get(&item.id).ok_or("the frame has no composed text")?;
    let cols = columns_of(item, f);
    let [col] = cols.as_slice() else { return Err("the frame has no single column".into()) };
    // one font at one size, one fixed leading (or auto), plain text
    let mut face: Option<(&str, &str, f64, Leading, f64)> = None;
    let mut chars_of_paragraph = Vec::new();
    for p in &story.paragraphs {
        let mut n = 0usize;
        for r in &p.runs {
            if !r.inline.is_empty() || !r.tables.is_empty() || r.note.is_some() || r.variable.is_some() || r.named.is_some() {
                return Err("text with anchored objects, tables, notes or variables is not supported (plain text)".into());
            }
            n += r.text.chars().count();
            if r.text.is_empty() {
                continue;
            }
            let a = &r.attrs;
            let key = (a.font_family.as_str(), a.font_style.as_str(), a.point_size, a.leading, a.auto_leading);
            match face {
                None => face = Some(key),
                Some(k) if k == key => {}
                Some(_) => return Err("text in several fonts, sizes or leadings is not supported (one font, one size)".into()),
            }
            if a.horizontal_scale != 100.0 || a.vertical_scale != 100.0 {
                return Err("scaled text is not supported".into());
            }
        }
        chars_of_paragraph.push(n);
    }
    // a frame with no text has nothing to adjust
    let Some((_, _, size, leading, _)) = face else {
        return Ok(FrameFit { story: f.story.clone(), width: col.right - col.left, size: 0.0, leading: None, e: 0.0, paragraphs: Vec::new() });
    };
    // the lines of each paragraph: a line belongs to the paragraph its first character is in (a return counts as the paragraph's last character)
    let mut starts = Vec::new();
    let mut at = 0usize;
    for n in &chars_of_paragraph {
        starts.push(at);
        at += n + 1;
    }
    let mut paragraphs: Vec<(usize, f64)> = vec![(0, 0.0); starts.len()];
    let mut font = None;
    for line in &text.lines {
        if line.rotation != 0.0 || !line.objects.is_empty() || line.table.is_some() || line.cell.is_some() {
            return Err("text with anchored objects, tables or turned lines is not supported".into());
        }
        let p = starts.iter().rposition(|s| *s <= line.start).unwrap_or(0);
        paragraphs[p].0 += 1;
        if let (Some(first), Some(last)) = (line.glyphs.first(), line.glyphs.last()) {
            font = font.or(Some(last.font));
            if first.font != last.font {
                return Err("text in several fonts is not supported (one font)".into());
            }
            // the natural width of the paragraph's last line so far (the last line seen for it wins)
            let upem = fonts.with_face(last.font, |f| f64::from(f.units_per_em())).unwrap_or(1000.0).max(1.0);
            let adv = fonts.with_face(last.font, |f| f.glyph_hor_advance(ttf_parser::GlyphId(last.glyph)).map(f64::from)).flatten().unwrap_or(0.0) / upem * last.size * last.hscale;
            paragraphs[p].1 = (last.x + adv - first.x).max(0.0);
        }
    }
    // an empty paragraph is one line of no width
    for (p, n) in chars_of_paragraph.iter().enumerate() {
        if *n == 0 && paragraphs[p].0 == 0 {
            paragraphs[p].0 = 1;
        }
    }
    let font = font.ok_or("the story has no composed glyphs")?;
    let space = fonts
        .with_face(font, |f| {
            let upem = f64::from(f.units_per_em()).max(1.0);
            f.glyph_index(' ').and_then(|g| f.glyph_hor_advance(g)).map(|a| f64::from(a) / upem)
        })
        .flatten()
        .unwrap_or(0.25)
        * size;
    Ok(FrameFit {
        story: f.story.clone(),
        width: col.right - col.left,
        size,
        leading: match leading {
            Leading::Fixed(v) => Some(v),
            Leading::Auto => None,
        },
        e: (space - SPACE_LESS).max(0.0),
        paragraphs,
    })
}
