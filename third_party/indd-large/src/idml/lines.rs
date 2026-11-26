//! The saved composition of the document's stories as a JSON sidecar of the
//! IDML package (Folio fork, folio.6). IDML has no place for InDesign's
//! composed lines; a host that lays the package out can use them to show the
//! lines InDesign showed (`model::composition`).
//!
//! Format `folio-saved-lines`, version 1:
//!
//! ```text
//! {"format":"folio-saved-lines","version":1,"units":"idml-text-v1",
//!  "stories":[{"story":"u109","units":67,"digest":"<16 hex>",
//!              "frames":["u11b"],
//!              "lines":[[start,frame,column,baseline,[[x,width],[x,width,start],...]],...]}],
//!  "skipped":[{"story":"u177","reason":"..."}]}
//! ```
//!
//! Offsets count the story's own text as the IDML package writes it
//! (`idml-text-v1`): one unit per character of `Content`, per `Br`, per
//! element that stands for a character (table, footnote, endnote, anchored
//! page item, text variable) and per `<?ACE n?>`. U+FEFF markers (XML
//! elements, notes, hidden text, index markers and destinations), the
//! U+0017 markers after a table, characters XML cannot hold and the story's
//! final return (which IDML does not write) count nothing. `units` is the
//! story's count and `digest` the 64-bit FNV-1a of the UTF-8 of one code per
//! unit: the character, `\n` for a forced line break (IDML U+2028), U+0016
//! for a table, U+0004 for a footnote or endnote, U+FFFC for an anchored
//! item, U+0018 for a text variable and the code of `<?ACE n?>`. A host that
//! counts another number of units or digest has another text and must not
//! use the lines.
//!
//! A line is the start offset of its first character, the index of its text
//! frame in `frames` (-1: overset), the column index in that frame, the
//! baseline below the top of the frame's text area with vertical
//! justification (`null` when the record's fields are not all known) and
//! its segments: x and width in the frame's text area (points, paragraph
//! indents not subtracted), and for every segment after the first the start
//! offset of its first character (-1 when it holds none). A line has more
//! than one segment where a text wrap or a drop cap splits it. Lines of
//! table cells, footnotes, notes and hidden text are not listed; a line that
//! holds only uncounted characters is merged into the next.

use super::format::uref;
use super::xml::xml_char;
use crate::model::{Document, Story};

/// Format name and version of the sidecar.
pub const FORMAT: &str = "folio-saved-lines";
pub const VERSION: u32 = 1;

/// What the sidecar holds, for the host's manifest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SavedLinesSummary {
    pub stories: usize,
    pub lines: usize,
    pub skipped: usize,
}

/// The story's own text as IDML units: (UTF-16 offset in the story text,
/// digest code) of every unit (see the module documentation).
fn units(story: &Story) -> Vec<(usize, char)> {
    let mut out = Vec::new();
    for run in &story.runs {
        let mut at = run.start;
        for ch in run.text.chars() {
            if ch != '\u{FEFF}' && ch != '\u{17}' && xml_char(ch) {
                let code = if ch == '\u{2028}' { '\n' } else { ch };
                out.push((at, code));
            }
            at += ch.len_utf16();
        }
    }
    // The final return is not written (story.rs, `ranges`).
    if story.runs.last().is_some_and(|r| r.text.ends_with('\r')) && out.last().is_some_and(|u| u.1 == '\r') {
        out.pop();
    }
    out
}

/// 64-bit FNV-1a of the UTF-8 of the codes.
pub fn digest(codes: impl IntoIterator<Item = char>) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut buf = [0u8; 4];
    for c in codes {
        for b in c.encode_utf8(&mut buf).bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A coordinate with at most 4 decimals (0.0001 pt), as JSON.
fn coord(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// One story's entry, or why it has none.
fn story_entry(story: &Story) -> Result<Option<(String, usize)>, String> {
    let Some(comp) = &story.composition else {
        return Ok(None);
    };
    if let Some(why) = &comp.unusable {
        return Err(why.clone());
    }
    let units = units(story);
    // The story's own text: its runs' UTF-16 ranges, in order.
    let spans: Vec<(usize, usize)> = story.runs.iter().map(|r| (r.start, r.start + r.text.encode_utf16().count())).collect();
    let own = |at: usize| {
        let i = spans.partition_point(|s| s.0 <= at);
        i > 0 && at < spans[i - 1].1
    };
    let mut frames: Vec<u32> = Vec::new();
    // (start, frame index or -1, column, baseline, segments (x, width, start))
    type Line = (usize, i64, u16, Option<f64>, Vec<(f64, f64, i64)>);
    let mut lines: Vec<Line> = Vec::new();
    for l in &comp.lines {
        if !own(l.start) {
            continue;
        }
        let start = units.partition_point(|u| u.0 < l.start);
        let (frame, column) = match l.place {
            Some(p) => {
                let i = match frames.iter().position(|&f| f == p.frame) {
                    Some(i) => i,
                    None => {
                        frames.push(p.frame);
                        frames.len() - 1
                    }
                };
                (i as i64, p.index)
            }
            None => (-1, 0),
        };
        let at = |o: usize| units.partition_point(|u| u.0 < o);
        let segments = l
            .segments
            .iter()
            .map(|g| (g.x, g.width, g.start.map(|o| at(o) as i64).filter(|&u| u > start as i64).unwrap_or(-1)))
            .collect();
        let line = (start, frame, column, l.baseline, segments);
        match lines.last_mut() {
            // A line of uncounted characters only: the next line takes its start.
            Some(last) if last.0 == start => *last = line,
            Some(last) if last.0 > start => return Err("lines out of order".into()),
            _ => lines.push(line),
        }
    }
    if lines.is_empty() {
        return Err("no lines of the story's own text".into());
    }
    if lines[0].0 != 0 {
        return Err("first line does not start the story".into());
    }
    let mut out = String::new();
    out.push_str("{\"story\":");
    json_str(&mut out, &uref(Some(story.uid)));
    out.push_str(&format!(",\"units\":{},\"digest\":\"{:016x}\",\"frames\":[", units.len(), digest(units.iter().map(|u| u.1))));
    for (i, f) in frames.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json_str(&mut out, &uref(Some(*f)));
    }
    out.push_str("],\"lines\":[");
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let baseline = l.3.map_or_else(|| "null".to_string(), coord);
        let segments: Vec<String> = l
            .4
            .iter()
            .enumerate()
            .map(|(k, g)| if k == 0 { format!("[{},{}]", coord(g.0), coord(g.1)) } else { format!("[{},{},{}]", coord(g.0), coord(g.1), g.2) })
            .collect();
        out.push_str(&format!("[{},{},{},{},[{}]]", l.0, l.1, l.2, baseline, segments.join(",")));
    }
    out.push_str("]}");
    Ok(Some((out, lines.len())))
}

/// The sidecar for `doc` (see the module documentation) and its summary.
pub fn saved_lines(doc: &Document) -> (Vec<u8>, SavedLinesSummary) {
    let mut summary = SavedLinesSummary::default();
    let mut stories = Vec::new();
    let mut skipped = Vec::new();
    for s in &doc.stories {
        match story_entry(s) {
            Ok(Some((e, n))) => {
                summary.stories += 1;
                summary.lines += n;
                stories.push(e);
            }
            Ok(None) => {}
            Err(why) => {
                let mut e = String::from("{\"story\":");
                json_str(&mut e, &uref(Some(s.uid)));
                e.push_str(",\"reason\":");
                json_str(&mut e, &why);
                e.push('}');
                skipped.push(e);
            }
        }
    }
    summary.skipped = skipped.len();
    let out = format!(
        "{{\"format\":\"{FORMAT}\",\"version\":{VERSION},\"units\":\"idml-text-v1\",\"stories\":[{}],\"skipped\":[{}]}}",
        stories.join(","),
        skipped.join(",")
    );
    (out.into_bytes(), summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Attrs, Composition, LinePlace, SavedLine, Segment, Story, TextRun};

    fn tests_story(runs: Vec<TextRun>) -> Story {
        Story {
            uid: 9,
            runs,
            anchors: Default::default(),
            tables: Default::default(),
            text_variables: Default::default(),
            sources: Vec::new(),
            xml_markers: Default::default(),
            footnotes: Default::default(),
            notes: Default::default(),
            hidden_texts: Default::default(),
            deletions: Default::default(),
            changes: Vec::new(),
            index_markers: Default::default(),
            endnotes: Default::default(),
            is_endnote: false,
            endnote_ranges: Vec::new(),
            text_destinations: Default::default(),
            xml_element: None,
            orientation: None,
            toc_style: None,
            settings: Default::default(),
            composition: None,
        }
    }

    fn run(start: usize, text: &str) -> TextRun {
        TextRun {
            start,
            text: text.into(),
            paragraph_style: None,
            character_style: None,
            paragraph_attrs: Attrs::default(),
            character_attrs: Attrs::default(),
            paragraph_run: 0,
        }
    }

    fn line(start: usize, place: Option<(u32, u16)>) -> SavedLine {
        SavedLine {
            start,
            len: 0,
            owner: 9,
            place: place.map(|(frame, index)| LinePlace { column: 100 + frame, frame, index }),
            baseline: Some(7.25),
            ascent: None,
            segments: vec![Segment { x: 0.0, width: 120.5, start: Some(start) }],
            table: false,
        }
    }

    /// A line at 5 split by a wrap: its second segment starts at 7 ("\r" of the table line).
    fn split_line() -> SavedLine {
        let mut l = line(5, Some((0x20, 1)));
        l.baseline = None;
        l.segments.push(Segment { x: 60.0, width: 40.0, start: Some(7) });
        l.segments.push(Segment { x: 110.0, width: 10.0, start: None });
        l
    }

    #[test]
    fn units_lines_and_digest() {
        let mut story = tests_story(vec![run(0, "ab\u{FEFF}c\r"), run(5, "\u{16}\u{17}\rde\r")]);
        story.composition = Some(Composition {
            lines: vec![line(0, Some((0x20, 0))), split_line(), line(8, Some((0x21, 0))), line(10, None)],
            unusable: None,
        });
        let u = units(&story);
        let codes: String = u.iter().map(|u| u.1).collect();
        // FEFF, U+0017 and the final return count nothing.
        assert_eq!(codes, "abc\r\u{16}\rde");
        let (json, summary) = saved_lines(&crate::model::Document { stories: vec![story], ..Default::default() });
        let json = String::from_utf8(json).unwrap();
        assert_eq!(summary, SavedLinesSummary { stories: 1, lines: 4, skipped: 0 });
        let d = format!("{:016x}", digest(codes.chars()));
        assert!(json.contains(&format!("\"story\":\"u9\",\"units\":8,\"digest\":\"{d}\",\"frames\":[\"u20\",\"u21\"]")), "{json}");
        assert!(
            json.contains("\"lines\":[[0,0,0,7.25,[[0,120.5]]],[4,0,1,null,[[0,120.5],[60,40,5],[110,10,-1]]],[6,1,0,7.25,[[0,120.5]]],[8,-1,0,7.25,[[0,120.5]]]]"),
            "{json}"
        );
    }

    #[test]
    fn unusable_and_missing_compositions() {
        let mut story = tests_story(vec![run(0, "ab\r")]);
        story.composition = Some(Composition { lines: Vec::new(), unusable: Some("spanning or split columns".into()) });
        let other = tests_story(vec![run(0, "x\r")]);
        let (json, summary) = saved_lines(&crate::model::Document { stories: vec![story, other], ..Default::default() });
        assert_eq!(summary, SavedLinesSummary { stories: 0, lines: 0, skipped: 1 });
        assert!(String::from_utf8(json).unwrap().contains("\"skipped\":[{\"story\":\"u9\",\"reason\":\"spanning or split columns\"}]"));
    }
}
