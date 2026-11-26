//! Placed or pasted text in another writing system shown as boxes: the characters the font
//! set on them lacks (installed, but without those glyphs: what preflight reports as
//! "Missing glyphs") take an installed font that has them, in the edit's own undo step.
//! The faces come from `folio_text::glyphfill` (the system's faces for that writing, then
//! any installed face); nothing is bundled.
use folio_edit::Command;
use folio_model::Document;
use folio_text::glyphfill::{GlyphFill, han_script};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The story ranges an edit put text into: a placed story whole, a paste its new characters.
pub type Targets = BTreeMap<String, Option<(usize, usize)>>;

/// What the fill changes: the formatting commands, the font records they need, and per font
/// the characters it took (for the import report: `{family, style, count, chars}`).
pub struct Fill {
    pub commands: Vec<Command>,
    pub records: Vec<BTreeMap<String, String>>,
    pub report: Vec<Value>,
}

/// The characters of story `s` as story positions count them (run text, one return between paragraphs).
fn story_chars(s: &folio_model::Story) -> Vec<char> {
    let mut out = Vec::new();
    for (i, p) in s.paragraphs.iter().enumerate() {
        if i > 0 {
            out.push('\r');
        }
        out.extend(p.runs.iter().flat_map(|r| r.text.chars()));
    }
    out
}

pub fn fill(doc: &Document, fonts: &folio_text::FontStore, targets: &Targets) -> Fill {
    let frames = folio_text::DocumentFrames::new(doc);
    let mut finder = GlyphFill::new(fonts);
    let mut commands = Vec::new();
    let mut used: BTreeMap<(String, String), (usize, String)> = BTreeMap::new();
    for (story, range) in targets {
        let Some(s) = doc.stories.get(story) else { continue };
        let Some(laid) = folio_text::layout_story(doc, &frames, story, fonts) else { continue };
        let chars = story_chars(s);
        let han = han_script(chars.iter().copied());
        // position → (family, style) for the boxes of installed fonts (a missing font's characters already take a substitute)
        let mut set: BTreeMap<usize, (String, String)> = BTreeMap::new();
        for m in laid.missing_glyphs.iter().filter(|m| m.installed) {
            for &(a, b) in &m.ranges {
                for at in a..b {
                    let (lo, hi) = range.unwrap_or((0, usize::MAX));
                    if at < lo || at >= hi {
                        continue;
                    }
                    // the box is this character of the story (nested text is reported at its anchor: left alone)
                    let Some(&c) = chars.get(at).filter(|c| m.chars.contains(**c)) else { continue };
                    let Some(font) = finder.face_for(c, han) else { continue };
                    let Some(names) = finder.names(font, &m.style) else { continue };
                    let e = used.entry(names.clone()).or_default();
                    e.0 += 1;
                    if e.1.chars().count() < 16 && !e.1.contains(c) {
                        e.1.push(c);
                    }
                    set.insert(at, names);
                }
            }
        }
        // one command per run of neighbouring characters taking the same font
        let mut run: Option<(usize, usize, (String, String))> = None;
        let flush = |run: Option<(usize, usize, (String, String))>, commands: &mut Vec<Command>| {
            if let Some((start, end, (family, style))) = run {
                let props = BTreeMap::from([("appliedFont".to_owned(), json!(family)), ("fontStyle".to_owned(), json!(style))]);
                commands.push(Command::TextFormat { story: story.clone(), start, end, props });
            }
        };
        for (at, names) in set {
            run = match run {
                Some((start, end, n)) if end == at && n == names => Some((start, at + 1, n)),
                other => {
                    flush(other, &mut commands);
                    Some((at, at + 1, names))
                }
            };
        }
        flush(run, &mut commands);
    }
    let records = used.keys().filter_map(|(family, style)| fonts.resource_record(family, style, &doc.font_table)).collect();
    let report = used.into_iter().map(|((family, style), (count, chars))| json!({"family": family, "style": style, "count": count, "chars": chars})).collect();
    Fill { commands, records, report }
}
