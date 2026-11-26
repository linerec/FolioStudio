//! Conformance check against InDesign's own composition.
//!
//! Input: a `composition.json` captured from InDesign 2025 by
//! `artifacts/composition-20261018/native/capture-composition.jsx` (per-character x offsets
//! of every line, paragraph settings, style runs). Each justified paragraph is rebuilt from
//! InDesign's measured widths, composed with this crate, and compared with InDesign's
//! line breaks. Glyph widths come from the measurement, so this checks the composer alone,
//! not font metrics.
//!
//! Reconstruction rules (all from the measurements, README.txt):
//! - Special characters arrive as enumeration names (`<EM_DASH>`) and are mapped to their
//!   Unicode characters. Tabs, page-number variables and other unsupported specials skip
//!   the paragraph (their widths depend on tab stops or variables the capture lacks).
//! - Tracking is `tracking × size / 1000 × horizontal scale` (a line's last glyph is wider
//!   than the same glyph mid-line by exactly that amount).
//! - The natural word space of a style is the median of the densest 0.002 pt cluster of
//!   spaces on lines set at desired spacing (ragged lines, complete paragraphs' last lines).
//!   Kerning of a space next to punctuation, digits or Latin letters is measured against
//!   the line's spaces between Hangul syllables and added per neighbour.
//! - A line's measure is its own width when InDesign justified it to the column edge. For
//!   a paragraph's last line and for a line ending in a drawn hyphen, the measure comes
//!   from the nearest justified line of the same frame with the same left edge.
//! - Latin words get soft hyphens at dictionary points (`hyphenate`); the drawn hyphen is
//!   the font's hyphen advance without tracking (measured at InDesign's hyphenated line
//!   ends, otherwise from mid-line '-' glyphs), and the letter before it keeps its tracking.
//! - Lines that a text-wrap object splits into two slots are skipped (slot widths are not
//!   captured).
//! - InDesign shows the composition stored in the document until a story is edited. Where
//!   that was composed with other fonts (another machine's font versions, missing fonts),
//!   its breaks do not follow from the widths measured here. `--recomposed` gives InDesign's
//!   line starts after every paragraph was composed again on the measuring machine
//!   (`artifacts/next-20261018/fit/fresh-composition-347.json`); paragraphs whose stored and
//!   fresh breaks differ are compared with the fresh breaks (same line count required; their
//!   positions are not checked, being those of the stored lines).
//!
//! Usage:
//!   compose-conformance <composition.json> [--exclude-texts-from <other.json>]
//!                       [--recomposed <fresh.json>] [--min-exact <fraction>] [--report <out.json>]
//! Environment (experiments and diagnostics): CONFORMANCE_SEARCH=exhaustive,
//! CONFORMANCE_HYPHEN_WEIGHT=<f>, CONFORMANCE_HYPHEN_Q=<q>, CONFORMANCE_DICT=us|uk,
//! CONFORMANCE_DEBUG_PARA=<story>:<paragraph index>, CONFORMANCE_DEBUG_LINE=<line prefix>,
//! CONFORMANCE_SHOW_SKIPS=1, CONFORMANCE_DUMP_WS=1, CONFORMANCE_BENCH=<repetitions>.
//! Exit code 1 when the share of exactly matching paragraphs is below --min-exact.

use folio_compose::breaks::{can_break_before, hyphen_break, is_latin_letter};
use folio_compose::compose::{LineReport, explain, path_cost};
use folio_compose::hyphenate::{self, Dictionary};
use folio_compose::{Alignment, Composer, HyphenSettings, Item, Range, SOFT_HYPHEN, Search, Settings, compose, is_trailing_blank, place_line};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::ExitCode;

fn num(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN)
}
fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}
fn floats(v: &Value, k: &str) -> Option<Vec<f64>> {
    match v.get(k)? {
        Value::Array(a) => a.iter().map(Value::as_f64).collect(),
        Value::Number(n) => n.as_f64().map(|x| vec![x]),
        _ => None,
    }
}
fn strings(v: &Value, k: &str) -> Option<Vec<String>> {
    match v.get(k)? {
        Value::Array(a) => Some(a.iter().map(|x| x.as_str().unwrap_or("<?>").to_string()).collect()),
        Value::String(s) => Some(vec![s.clone()]),
        _ => None,
    }
}

/// A captured character, decoded.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Decoded {
    Char(char),
    /// Paragraph end; not an item.
    End,
    Tab,
    Unsupported,
}

/// InDesign `SpecialCharacters` enumeration names as captured (`<NAME>`), and plain text.
fn decode(c: &str) -> Decoded {
    let mut it = c.chars();
    if let (Some(ch), None) = (it.next(), it.next()) {
        return match ch {
            '\r' => Decoded::End,
            '\t' => Decoded::Tab,
            '\u{FEFF}' => Decoded::Unsupported,
            '\u{2028}' => Decoded::Char('\n'),
            ch => Decoded::Char(ch),
        };
    }
    let ch = match c.trim_start_matches('<').trim_end_matches('>') {
        "SINGLE_LEFT_QUOTE" => '\u{2018}',
        "SINGLE_RIGHT_QUOTE" => '\u{2019}',
        "DOUBLE_LEFT_QUOTE" => '\u{201C}',
        "DOUBLE_RIGHT_QUOTE" => '\u{201D}',
        "SINGLE_STRAIGHT_QUOTE" => '\'',
        "DOUBLE_STRAIGHT_QUOTE" => '"',
        "ELLIPSIS_CHARACTER" => '\u{2026}',
        "BULLET_CHARACTER" => '\u{2022}',
        "EM_DASH" => '\u{2014}',
        "EN_DASH" => '\u{2013}',
        "NONBREAKING_HYPHEN" => '\u{2011}',
        "DISCRETIONARY_HYPHEN" => SOFT_HYPHEN,
        "NONBREAKING_SPACE" => '\u{00A0}',
        "FIXED_WIDTH_NONBREAKING_SPACE" => '\u{202F}',
        "EM_SPACE" => '\u{2003}',
        "EN_SPACE" => '\u{2002}',
        "THIRD_SPACE" => '\u{2004}',
        "QUARTER_SPACE" => '\u{2005}',
        "SIXTH_SPACE" => '\u{2006}',
        "FIGURE_SPACE" => '\u{2007}',
        "PUNCTUATION_SPACE" => '\u{2008}',
        "THIN_SPACE" => '\u{2009}',
        "HAIR_SPACE" => '\u{200A}',
        "FORCED_LINE_BREAK" => '\n',
        "DISCRETIONARY_LINE_BREAK" => '\u{200B}',
        "ZERO_WIDTH_NONJOINER" => '\u{200C}',
        "ZERO_WIDTH_JOINER" => '\u{200D}',
        "COPYRIGHT_SYMBOL" => '\u{00A9}',
        "REGISTERED_TRADEMARK" => '\u{00AE}',
        "TRADEMARK_SYMBOL" => '\u{2122}',
        "SECTION_SYMBOL" => '\u{00A7}',
        "PARAGRAPH_SYMBOL" => '\u{00B6}',
        "DEGREE_SYMBOL" => '\u{00B0}',
        // Page numbers, text variables, flush space, indent-to-here and right-indent tabs,
        // nested-style markers, column/frame/page breaks: not modelled.
        _ => return Decoded::Unsupported,
    };
    Decoded::Char(ch)
}

struct Run {
    index: usize,
    font: String,
    style: String,
    installed: bool,
    point_size: f64,
    tracking: f64,
    hscale: f64,
    language: String,
}

impl Run {
    /// Tracking in points: InDesign scales it with the horizontal scale (measured: a line's
    /// last glyph is wider than mid-line by tracking × size/1000 × scale/100).
    fn tracking_pt(&self) -> f64 {
        self.tracking * self.point_size / 1000.0 * self.hscale / 100.0
    }
    /// Converts a width in this run to em units of the unscaled font size.
    fn em(&self) -> f64 {
        self.point_size * self.hscale / 100.0
    }
}

fn runs_of(story: &Value) -> Vec<Run> {
    let mut runs: Vec<Run> = story["runs"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| Run {
                    index: num(r, "index") as usize,
                    font: text(r, "font"),
                    style: format!(
                        "{}|{}|{}|{}|{}",
                        text(r, "font"),
                        num(r, "pointSize"),
                        num(r, "tracking"),
                        num(r, "horizontalScale"),
                        text(r, "kerningMethod")
                    ),
                    installed: text(r, "fontStatus") == "INSTALLED",
                    point_size: num(r, "pointSize"),
                    tracking: num(r, "tracking"),
                    hscale: num(r, "horizontalScale"),
                    language: text(r, "language"),
                })
                .collect()
        })
        .unwrap_or_default();
    runs.sort_by_key(|r| r.index);
    runs
}

fn run_at(runs: &[Run], i: usize) -> Option<&Run> {
    let k = runs.partition_point(|r| r.index <= i);
    if k == 0 { None } else { Some(&runs[k - 1]) }
}

fn alignment(s: &str) -> Alignment {
    match s {
        "LEFT_JUSTIFIED" => Alignment::LeftJustified,
        "CENTER_JUSTIFIED" => Alignment::CenterJustified,
        "RIGHT_JUSTIFIED" => Alignment::RightJustified,
        "FULLY_JUSTIFIED" => Alignment::FullyJustified,
        "CENTER_ALIGN" => Alignment::Center,
        "RIGHT_ALIGN" => Alignment::Right,
        _ => Alignment::Left,
    }
}

fn env_f64(name: &str) -> Option<f64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn settings_of(p: &Value) -> Settings {
    let defaults = HyphenSettings::default();
    let count = |k: &str, d: usize| p.get(k).and_then(Value::as_f64).map_or(d, |v| v.max(0.0) as usize);
    let flag = |k: &str, d: bool| p.get(k).and_then(Value::as_bool).unwrap_or(d);
    Settings {
        alignment: alignment(&text(p, "justification")),
        single_word: if p.get("singleWordJustification").is_some() { alignment(&text(p, "singleWordJustification")) } else { Alignment::FullyJustified },
        balance_ragged_lines: flag("balanceRaggedLines", false),
        last_line_indent: p.get("lastLineIndent").and_then(Value::as_f64).unwrap_or(0.0),
        word_spacing: Range::new(num(p, "minimumWordSpacing"), num(p, "desiredWordSpacing"), num(p, "maximumWordSpacing")),
        letter_spacing: Range::new(num(p, "minimumLetterSpacing"), num(p, "desiredLetterSpacing"), num(p, "maximumLetterSpacing")),
        glyph_scaling: Range::new(num(p, "minimumGlyphScaling"), num(p, "desiredGlyphScaling"), num(p, "maximumGlyphScaling")),
        composer: match text(p, "composer").as_str() {
            "Adobe Japanese Paragraph Composer" => Composer::JapaneseParagraph,
            "" | "Adobe Paragraph Composer" => Composer::RomanParagraph,
            _ => Composer::Other,
        },
        hyphenation: flag("hyphenation", true),
        hyphen: HyphenSettings {
            words_longer_than: count("hyphenateWordsLongerThan", defaults.words_longer_than),
            after_first: count("hyphenateAfterFirst", defaults.after_first),
            before_last: count("hyphenateBeforeLast", defaults.before_last),
            capitalized: flag("hyphenateCapitalizedWords", defaults.capitalized),
            last_word: flag("hyphenateLastWord", defaults.last_word),
            ladder_limit: count("hyphenateLadderLimit", defaults.ladder_limit as usize) as u32,
            zone: p.get("hyphenationZone").and_then(Value::as_f64).unwrap_or(defaults.zone),
            across_columns: defaults.across_columns,
            weight: env_f64("CONFORMANCE_HYPHEN_WEIGHT").unwrap_or(defaults.weight),
            quality: env_f64("CONFORMANCE_HYPHEN_Q").unwrap_or(defaults.quality),
        },
        search: match std::env::var("CONFORMANCE_SEARCH").as_deref() {
            Ok("exhaustive") => Search::Exhaustive,
            _ => Search::InDesign,
        },
    }
}

/// A captured line with its characters and offsets.
struct Line {
    index: usize,
    chars: Vec<String>,
    off: Vec<f64>,
    end: Vec<f64>,
    frame: i64,
    baseline: f64,
}

impl Line {
    fn parse(l: &Value) -> Option<Line> {
        let (chars, off, end) = (strings(l, "chars")?, floats(l, "off")?, floats(l, "end")?);
        let n = num(l, "n") as usize;
        if chars.len() != n || off.len() != n || end.len() != n || n == 0 {
            return None;
        }
        Some(Line {
            index: num(l, "index") as usize,
            chars,
            off,
            end,
            frame: l.get("frame").and_then(Value::as_i64).unwrap_or(-1),
            baseline: num(l, "baseline"),
        })
    }
    /// Index of the last glyph that is not a trailing blank or the paragraph end.
    fn last_visible(&self) -> usize {
        let mut j = self.chars.len() - 1;
        while j > 0 && matches!(decode(&self.chars[j]), Decoded::End | Decoded::Char(' ') | Decoded::Char('\n')) {
            j -= 1;
        }
        j
    }
    fn first_char(&self) -> Option<char> {
        match decode(&self.chars[0]) {
            Decoded::Char(c) => Some(c),
            _ => None,
        }
    }
    fn last_char(&self) -> Option<char> {
        match decode(self.chars.last()?) {
            Decoded::Char(c) => Some(c),
            _ => None,
        }
    }
}

fn lines_of<'a>(story: &'a Value, p: &Value) -> Vec<&'a Value> {
    let (start, len) = (num(p, "index") as usize, num(p, "length") as usize);
    story["lines"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|l| {
                    let i = num(l, "index") as usize;
                    num(l, "n") > 0.0 && i >= start && i < start + len
                })
                .collect()
        })
        .unwrap_or_default()
}

fn paragraph_text(lines: &[&Value]) -> String {
    lines.iter().filter_map(|l| strings(l, "chars")).flatten().collect()
}

/// The typical value of precise measurements with outliers: the median of the densest
/// cluster no wider than `window` (ties: the smaller values).
fn typical(samples: &[f64], window: f64) -> Option<f64> {
    let mut s: Vec<f64> = samples.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return None;
    }
    s.sort_by(f64::total_cmp);
    let (mut best, mut lo) = ((0, 0), 0);
    for hi in 0..s.len() {
        while s[hi] - s[lo] > window {
            lo += 1;
        }
        if hi - lo > best.1 - best.0 {
            best = (lo, hi);
        }
    }
    let cluster = &s[best.0..=best.1];
    Some(cluster[cluster.len() / 2])
}

/// Whether line `k` of a paragraph ends inside a Latin word (InDesign drew a hyphen).
fn ends_in_word(lines: &[Line], k: usize) -> bool {
    k + 1 < lines.len()
        && lines[k].last_char().is_some_and(is_latin_letter)
        && lines[k + 1].first_char().is_some_and(is_latin_letter)
}

/// A captured line: (frame, first character index).
type LineId = (i64, usize);
/// A word space with its style, neighbours and gap.
type SpaceSample = (String, char, char, f64, LineId);
/// Deviations from a reference space width, with the line they came from.
type Deltas = HashMap<String, Vec<(f64, LineId)>>;

/// A line InDesign justified to the column edge, for measures of other lines.
struct Slot {
    left: f64,
    right: f64,
    baseline: f64,
}

/// Document-wide measurements shared by all paragraphs.
struct Measurements {
    /// Natural word space per style and desired word spacing.
    spaces: HashMap<String, f64>,
    /// Natural word space in em per font and desired word spacing (fallback).
    space_em: HashMap<String, f64>,
    /// Drawn hyphen advance in em per font.
    hyphen_em: HashMap<String, f64>,
    /// Kerning added to a space between two characters, per style (measured on lines at
    /// desired spacing; justification adds the same amount to every space of a line).
    kerning: HashMap<String, f64>,
    slots: HashMap<i64, Vec<Slot>>,
}

fn indents(p: &Value) -> (f64, f64, f64) {
    let z = |v: f64| if v.is_finite() { v } else { 0.0 };
    (z(num(p, "leftIndent")), z(num(p, "firstLineIndent")), z(num(p, "rightIndent")))
}

impl Measurements {
    fn new(doc: &Value) -> Self {
        let mut space_samples: HashMap<String, Vec<f64>> = HashMap::new();
        let mut space_em_samples: HashMap<String, Vec<f64>> = HashMap::new();
        let mut dash_em: HashMap<String, Vec<f64>> = HashMap::new();
        let mut slots: HashMap<i64, Vec<Slot>> = HashMap::new();
        let mut context_samples: Vec<SpaceSample> = Vec::new();
        for story in doc["stories"].as_array().into_iter().flatten() {
            let runs = runs_of(story);
            for p in story["paragraphs"].as_array().into_iter().flatten() {
                let justified = alignment(&text(p, "justification")).is_justified();
                let desired = num(p, "desiredWordSpacing");
                let raw = lines_of(story, p);
                let complete = raw.iter().all(|l| Line::parse(l).is_some());
                let lines: Vec<Line> = raw.iter().filter_map(|l| Line::parse(l)).collect();
                let (left_indent, first_indent, right_indent) = indents(p);
                for (k, l) in lines.iter().enumerate() {
                    for i in 0..l.chars.len() - 1 {
                        let Some(r) = run_at(&runs, l.index + i) else { continue };
                        if l.chars[i] == "-" {
                            dash_em.entry(r.font.clone()).or_default().push((l.off[i + 1] - l.off[i] - r.tracking_pt()) / r.em());
                        }
                        // Spaces at desired spacing: ragged lines and justified last lines.
                        // (A paragraph with uncaptured lines has no known last line.)
                        if (!justified || (complete && k + 1 == lines.len()))
                            && l.chars[i] == " "
                            && matches!(decode(&l.chars[i + 1]), Decoded::Char(c) if c != ' ')
                        {
                            let g = l.off[i + 1] - l.off[i];
                            space_samples.entry(format!("{}|{desired}", r.style)).or_default().push(g);
                            space_em_samples.entry(format!("{}|{desired}", r.font)).or_default().push((g - r.tracking_pt()) / r.em());
                        }
                    }
                    // Every interior space with its neighbours, for kerning.
                    let ctx = |c: &str| match decode(c) {
                        Decoded::Char(ch) => ch,
                        _ => '\0',
                    };
                    let lv = l.last_visible();
                    for i in 0..lv {
                        if l.chars[i] != " " || l.chars[i + 1] == " " {
                            continue;
                        }
                        let Some(r) = run_at(&runs, l.index + i) else { continue };
                        let prev = if i > 0 { ctx(&l.chars[i - 1]) } else { '\0' };
                        let key = format!("{}|{desired}", r.style);
                        context_samples.push((key, prev, ctx(&l.chars[i + 1]), l.off[i + 1] - l.off[i], (l.frame, l.index)));
                    }
                    // Lines justified to the column edge (not the last, not hyphenated).
                    let last_of_paragraph = complete && k + 1 == lines.len();
                    if justified && complete && !last_of_paragraph && !ends_in_word(&lines, k) {
                        let lv = l.last_visible();
                        let first = k == 0 && l.index == num(p, "index") as usize;
                        slots.entry(l.frame).or_default().push(Slot {
                            left: l.off[0] - left_indent - if first { first_indent } else { 0.0 },
                            right: l.end[lv] + right_indent,
                            baseline: l.baseline,
                        });
                    }
                }
            }
        }
        let mut m = Measurements {
            spaces: space_samples.iter().filter(|(_, v)| v.len() >= 2).filter_map(|(k, v)| Some((k.clone(), typical(v, 0.002)?))).collect(),
            space_em: space_em_samples.iter().filter(|(_, v)| v.len() >= 2).filter_map(|(k, v)| Some((k.clone(), typical(v, 0.0005)?))).collect(),
            hyphen_em: HashMap::new(),
            kerning: HashMap::new(),
            slots,
        };
        // Kerning of a space against Latin letters, digits and punctuation. Within a line every space of a
        // style gets the same justification amount, so a space's deviation from the line's
        // spaces between two Hangul syllables is its kerning. Learned per (style,
        // neighbour); a value must be seen on two lines.
        let plain = |c: char| folio_compose::breaks::is_hangul(c);
        let mut per_line: HashMap<((i64, usize), &str), Vec<f64>> = HashMap::new();
        for (style, prev, next, g, line) in &context_samples {
            if plain(*prev) && plain(*next) {
                per_line.entry((*line, style.as_str())).or_default().push(*g);
            }
        }
        let reference: HashMap<((i64, usize), &str), f64> = per_line
            .iter()
            .filter_map(|(key, v)| {
                let t = typical(v, 0.002)?;
                let members = v.iter().filter(|&&g| (g - t).abs() <= 0.002).count();
                (members * 2 > v.len()).then_some((*key, t))
            })
            .collect();
        // Pair kerning: first (previous character, space), then (space, next character)
        // from what remains.
        let samples: Vec<(&str, char, char, f64, LineId)> = context_samples
            .iter()
            .filter_map(|(style, prev, next, g, line)| {
                let base = reference.get(&(*line, style.as_str()))?;
                Some((style.as_str(), *prev, *next, g - base, *line))
            })
            .collect();
        let learn = |groups: Deltas| -> HashMap<String, f64> {
            groups
                .into_iter()
                .filter_map(|(key, v)| {
                    let values: Vec<f64> = v.iter().map(|x| x.0).collect();
                    let t = typical(&values, 0.002).filter(|t| t.abs() > 0.002)?;
                    let lines: HashSet<(i64, usize)> = v.iter().filter(|x| (x.0 - t).abs() <= 0.002).map(|x| x.1).collect();
                    (lines.len() >= 2).then_some((key, t))
                })
                .collect()
        };
        let mut by_prev: Deltas = HashMap::new();
        for (style, prev, _, d, line) in samples.iter().filter(|s| !plain(s.1)) {
            by_prev.entry(format!("{style}|<{prev}")).or_default().push((*d, *line));
        }
        let before = learn(by_prev);
        let mut by_next: Deltas = HashMap::new();
        for (style, prev, next, d, line) in samples.iter().filter(|s| !plain(s.2)) {
            let k = before.get(&format!("{style}|<{prev}")).copied().unwrap_or(0.0);
            by_next.entry(format!("{style}|>{next}")).or_default().push((d - k, *line));
        }
        m.kerning = before;
        m.kerning.extend(learn(by_next));
        // Drawn hyphens: the gap between a hyphenated line's last letter and the measure.
        let mut gap_em: HashMap<String, Vec<f64>> = HashMap::new();
        for story in doc["stories"].as_array().into_iter().flatten() {
            let runs = runs_of(story);
            for p in story["paragraphs"].as_array().into_iter().flatten() {
                if !alignment(&text(p, "justification")).is_justified() {
                    continue;
                }
                let lines: Vec<Line> = lines_of(story, p).iter().filter_map(|l| Line::parse(l)).collect();
                for k in 0..lines.len() {
                    if !ends_in_word(&lines, k) {
                        continue;
                    }
                    let l = &lines[k];
                    let lv = l.last_visible();
                    let (Some(r), Some(right)) = (run_at(&runs, l.index + lv), m.slot_right(p, l, k == 0)) else { continue };
                    gap_em.entry(r.font.clone()).or_default().push((right - l.end[lv]) / r.em());
                }
            }
        }
        for (font, v) in dash_em.iter().chain(gap_em.iter()) {
            let measured = gap_em.get(font).filter(|g| !g.is_empty()).unwrap_or(v);
            if let Some(t) = typical(measured, 0.0005) {
                m.hyphen_em.insert(font.clone(), t);
            }
        }
        m
    }

    /// The right edge of the column at line `l` of paragraph `p` (before the right indent),
    /// from the nearest justified line of the same frame with the same left edge.
    fn slot_right(&self, p: &Value, l: &Line, first: bool) -> Option<f64> {
        let (left_indent, first_indent, right_indent) = indents(p);
        let left = l.off[0] - left_indent - if first { first_indent } else { 0.0 };
        self.slots
            .get(&l.frame)?
            .iter()
            .filter(|s| (s.left - left).abs() < 0.01 && (s.baseline - l.baseline).abs() > 1e-6)
            .min_by(|a, b| {
                let (da, db) = ((a.baseline - l.baseline).abs(), (b.baseline - l.baseline).abs());
                da.total_cmp(&db).then(a.baseline.total_cmp(&b.baseline))
            })
            .map(|s| s.right - right_indent)
    }

    fn space(&self, r: &Run, desired: f64, prev: char, next: char) -> Option<f64> {
        let pair = |key: String| self.kerning.get(&key).copied().unwrap_or(0.0);
        let kern = pair(format!("{}|{desired}|<{prev}", r.style)) + pair(format!("{}|{desired}|>{next}", r.style));
        Some(self.plain_space(r, desired)? + kern)
    }

    fn plain_space(&self, r: &Run, desired: f64) -> Option<f64> {
        self.spaces
            .get(&format!("{}|{desired}", r.style))
            .copied()
            .or_else(|| Some(self.space_em.get(&format!("{}|{desired}", r.font))? * r.em() + r.tracking_pt()))
    }
}

struct Model {
    items: Vec<Item>,
    /// Story character index of each item (`None` for an inserted soft hyphen).
    chars: Vec<Option<usize>>,
    widths: Vec<f64>,
    breaks: Vec<usize>,
    /// InDesign x offsets from each line's start, per item, for position checks.
    measured_x: Vec<Vec<f64>>,
    line_starts: Vec<usize>,
    /// Characters the capture marked as special that are now modelled.
    had_special: bool,
    hyphen_points: usize,
    hyphen_points_without_width: usize,
}

fn model(story: &Value, runs: &[Run], p: &Value, raw: &[&Value], m: &Measurements, settings: &Settings) -> Result<Model, String> {
    let lines: Vec<Line> = raw.iter().map(|l| Line::parse(l).ok_or("no offsets (line not captured)")).collect::<Result<_, _>>()?;
    let _ = story;
    let desired = settings.word_spacing.desired;
    let mut had_special = false;
    // Decode and check every character first.
    let mut decoded: Vec<Vec<Decoded>> = Vec::new();
    for l in &lines {
        let mut row = Vec::new();
        for (i, c) in l.chars.iter().enumerate() {
            let d = decode(c);
            match d {
                Decoded::Tab => return Err("tab".into()),
                Decoded::Unsupported => return Err(format!("special {c}")),
                Decoded::Char(ch) if c.chars().count() != 1 || ch == '\u{FFFC}' || ch == '\n' => had_special = true,
                _ => {}
            }
            if d != Decoded::End {
                let r = run_at(runs, l.index + i).ok_or("font")?;
                if !r.installed {
                    return Err("font".into());
                }
                // A text-wrap object inside the line: InDesign folds the jump into the
                // glyph before it (measured up to 173 pt). The line has two slots whose
                // widths the capture does not give.
                if l.end[i] - l.off[i] > 1.5 * r.em() {
                    return Err("text wrap splits a line".into());
                }
            }
            row.push(d);
        }
        decoded.push(row);
    }
    let hyphenated: Vec<bool> = (0..lines.len()).map(|k| ends_in_word(&lines, k)).collect();
    // Measures.
    let mut widths = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        let lv = l.last_visible();
        if k + 1 < lines.len() && !hyphenated[k] {
            widths.push(l.end[lv] - l.off[0]);
        } else {
            let right = m.slot_right(p, l, k == 0).ok_or("unknown measure")?;
            widths.push(right - l.off[0]);
        }
    }
    if lines.len() < 2 {
        return Err("one line".into());
    }
    // Base items (no soft hyphens) with their line and char index.
    let mut items = Vec::new();
    let mut place = Vec::new();
    let mut line_of_item = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        let lv = l.last_visible();
        for (i, d) in decoded[k].iter().enumerate() {
            let Decoded::Char(ch) = *d else { continue };
            let r = run_at(runs, l.index + i).ok_or("font")?;
            let tr = r.tracking_pt();
            let item = if ch == ' ' || ch == '\u{00A0}' {
                let neighbour = |j: Option<usize>| match j.and_then(|j| decoded[k].get(j)) {
                    Some(Decoded::Char(c)) => *c,
                    _ => '\0',
                };
                let s = m.space(r, desired, neighbour(i.checked_sub(1)), neighbour(Some(i + 1))).ok_or("no space sample")?;
                Item { ch, advance: s, end_advance: s - tr, point_size: r.point_size, unit: 0.0 }
            } else if i == lv && hyphenated[k] {
                // The letter before a drawn hyphen keeps its tracking.
                let a = l.end[i] - l.off[i];
                Item { ch, advance: a, end_advance: a - tr, point_size: r.point_size, unit: 0.0 }
            } else if i == lv || i + 1 == l.chars.len() {
                let a = l.end[i] - l.off[i];
                Item { ch, advance: a + tr, end_advance: a, point_size: r.point_size, unit: 0.0 }
            } else {
                let g = l.off[i + 1] - l.off[i];
                Item { ch, advance: g, end_advance: g - tr, point_size: r.point_size, unit: 0.0 }
            };
            items.push(item);
            place.push((k, i));
            line_of_item.push(k);
        }
    }
    // Dictionary hyphen points and drawn hyphen widths.
    let language = |i: usize| -> Option<&str> {
        let (k, c) = place[i];
        Some(run_at(runs, lines[k].index + c)?.language.as_str())
    };
    // Experiments: CONFORMANCE_DICT=us|uk uses one pattern set for every English run.
    let only = match std::env::var("CONFORMANCE_DICT").as_deref() {
        Ok("us") => Some(Dictionary::english_us()),
        Ok("uk") => Some(Dictionary::english_uk()),
        _ => None,
    };
    let all_points = hyphenate::points(&items, settings, &|i| {
        let d = Dictionary::for_language(language(i)?)?;
        Some(only.unwrap_or(d))
    });
    let width_after = |i: usize| -> Option<f64> {
        let (k, c) = place[i];
        let r = run_at(runs, lines[k].index + c)?;
        Some(m.hyphen_em.get(&r.font)? * r.em())
    };
    let points: Vec<usize> = all_points.iter().copied().filter(|&p| width_after(p - 1).is_some()).collect();
    let hyphen_points_without_width = all_points.len() - points.len();
    let h = hyphenate::insert(&items, &points, &|i| width_after(i).unwrap_or(0.0));
    // InDesign's breaks and measured positions in the hyphenated item list.
    let mut breaks = Vec::new();
    let mut line_starts = Vec::new();
    let mut measured_x: Vec<Vec<f64>> = vec![Vec::new(); lines.len()];
    let mut current = usize::MAX;
    for (j, origin) in h.origin.iter().enumerate() {
        let k = match origin {
            Some(o) => line_of_item[*o],
            // A soft hyphen belongs to the line of the letter before it.
            None => line_of_item[h.origin[j - 1].expect("inserted hyphen follows an original item")],
        };
        if k != current {
            if current != usize::MAX {
                breaks.push(j);
            }
            line_starts.push(j);
            current = k;
        }
        let l = &lines[k];
        let x = match origin {
            Some(o) => l.off[place[*o].1] - l.off[0],
            None => {
                let before = place[h.origin[j - 1].expect("inserted hyphen follows an original item")].1;
                if before == l.last_visible() { l.end[before] - l.off[0] } else { l.off[before + 1] - l.off[0] }
            }
        };
        measured_x[k].push(x);
    }
    let chars = h.origin.iter().map(|o| o.map(|o| lines[place[o].0].index + place[o].1)).collect();
    Ok(Model {
        items: h.items,
        chars,
        widths,
        breaks,
        measured_x,
        line_starts,
        had_special,
        hyphen_points: points.len(),
        hyphen_points_without_width,
    })
}

fn context(items: &[Item], at: Option<usize>) -> String {
    let Some(j) = at else { return "(end)".into() };
    let show = |it: &Item| if it.ch == SOFT_HYPHEN { '-' } else { it.ch };
    let before: String = items[j.saturating_sub(6)..j].iter().map(show).collect();
    let after: String = items[j..(j + 4).min(items.len())].iter().map(show).collect();
    format!("{before} | {after}")
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn describe(items: &[Item], lines: &[LineReport]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            let text: String = items[l.start..l.end].iter().map(|i| if i.ch == SOFT_HYPHEN { '-' } else { i.ch }).collect();
            let ws = if l.space_unit > 0.0 { 100.0 + (l.measure - l.natural) / l.space_unit * 100.0 } else { f64::NAN };
            format!(
                "cat {} cost {:9.2} (hyph {:6.1}) nat {:8.3} measure {:8.3} S {:6.3} ws {:6.1}% | {}",
                l.category, l.cost, l.hyphen_penalty, l.natural, l.measure, l.space_unit, ws, text.trim_end()
            )
        })
        .collect()
}

#[derive(Debug)]
struct InputError {
    kind: &'static str,
    path: String,
    message: String,
}

impl InputError {
    fn new(kind: &'static str, path: &str, error: impl std::fmt::Display) -> Self {
        Self { kind, path: path.into(), message: error.to_string() }
    }
}

fn load(path: &str) -> Result<Value, InputError> {
    let bytes = std::fs::read(path).map_err(|e| InputError::new("read", path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| InputError::new("json", path, e))
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", serde_json::json!({"error": {
                "kind": error.kind, "path": error.path, "message": error.message
            }}));
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, InputError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input = None;
    let mut exclude = None;
    let mut min_exact = None;
    let mut report = None;
    let mut recomposed = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--exclude-texts-from" => exclude = it.next().cloned(),
            "--min-exact" => min_exact = it.next().and_then(|v| v.parse::<f64>().ok()),
            "--report" => report = it.next().cloned(),
            "--recomposed" => recomposed = it.next().cloned(),
            _ if input.is_none() => input = Some(a.clone()),
            _ => {
                eprintln!("unexpected argument {a}");
                return Ok(ExitCode::from(2));
            }
        }
    }
    let Some(input) = input else {
        eprintln!("usage: compose-conformance <composition.json> [--exclude-texts-from <json>] [--recomposed <json>] [--min-exact <f>] [--report <json>]");
        return Ok(ExitCode::from(2));
    };
    let doc = load(&input)?;
    let excluded: HashSet<String> = exclude
        .map(|path| -> Result<_, InputError> {
            let other = load(&path)?;
            let mut set = HashSet::new();
            for story in other["stories"].as_array().into_iter().flatten() {
                for p in story["paragraphs"].as_array().into_iter().flatten() {
                    if alignment(&text(p, "justification")).is_justified() {
                        set.insert(paragraph_text(&lines_of(story, p)));
                    }
                }
            }
            Ok(set)
        })
        .transpose()?
        .unwrap_or_default();
    // Stories whose stored composition differs from InDesign's fresh one: fresh line starts.
    let fresh: HashMap<u64, HashSet<usize>> = recomposed
        .map(|path| -> Result<_, InputError> {
            let v = load(&path)?;
            let starts = |lines: &Value| -> Vec<usize> {
                let n = |l: &Value| l[1].as_u64().unwrap_or(0);
                lines.as_array().into_iter().flatten().filter(|l| n(l) > 0).map(|l| l[0].as_u64().unwrap_or(0) as usize).collect()
            };
            let mut out = HashMap::new();
            for (id, s) in v["stories"].as_object().into_iter().flatten() {
                let (opened, now) = (starts(&s["opened"]), starts(&s["fresh"]));
                if opened != now
                    && let Ok(id) = u64::from_str_radix(id.trim_start_matches('u'), 16)
                {
                    out.insert(id, now.into_iter().collect());
                }
            }
            Ok(out)
        })
        .transpose()?
        .unwrap_or_default();
    let mut compared_fresh = 0usize;
    let debug = std::env::var("CONFORMANCE_DEBUG_PARA").ok();
    // CONFORMANCE_BENCH=<n>: time compose and place_line, n repetitions per paragraph.
    let bench: usize = std::env::var("CONFORMANCE_BENCH").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let (mut compose_time, mut place_time, mut bench_items) = (std::time::Duration::ZERO, std::time::Duration::ZERO, 0usize);

    let measurements = Measurements::new(&doc);
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    let (mut used, mut exact, mut found, mut total) = (0usize, 0usize, 0usize, 0usize);
    let (mut special_used, mut special_exact) = (0usize, 0usize);
    let (mut hyphen_points, mut hyphen_points_without_width) = (0usize, 0usize);
    let (mut indesign_hyphens, mut our_hyphens, mut hyphens_matched) = (0usize, 0usize, 0usize);
    let (mut indesign_dashes, mut our_dashes, mut dashes_matched) = (0usize, 0usize, 0usize);
    let mut position_errors = Vec::new();
    let mut mismatches = Vec::new();
    let mut causes: BTreeMap<&str, usize> = BTreeMap::new();
    let mut line_errors = Vec::new();
    let started = std::time::Instant::now();

    for story in doc["stories"].as_array().into_iter().flatten() {
        let runs = runs_of(story);
        for p in story["paragraphs"].as_array().into_iter().flatten() {
            let settings = settings_of(p);
            if !settings.alignment.is_justified() {
                continue;
            }
            let lines = lines_of(story, p);
            if lines.len() < 2 {
                continue;
            }
            if !excluded.is_empty() && excluded.contains(&paragraph_text(&lines)) {
                *skipped.entry("same text as excluded document".into()).or_default() += 1;
                continue;
            }
            let m = match model(story, &runs, p, &lines, &measurements, &settings) {
                Ok(mut m) => {
                    // The stored composition came from other fonts: compare with InDesign's
                    // fresh breaks instead.
                    if let Some(starts) = story["id"].as_u64().and_then(|id| fresh.get(&id)) {
                        let breaks: Vec<usize> = (1..m.items.len())
                            .filter(|&j| m.chars[j].is_some_and(|c| starts.contains(&c)))
                            .collect();
                        if breaks != m.breaks {
                            if breaks.len() != m.breaks.len() {
                                *skipped.entry("stored composition differs, line count changed".into()).or_default() += 1;
                                continue;
                            }
                            m.breaks = breaks;
                            m.measured_x.clear();
                            compared_fresh += 1;
                        }
                    }
                    m
                }
                Err(reason) => {
                    if std::env::var("CONFORMANCE_SHOW_SKIPS").is_ok() {
                        eprintln!("skip {}:{} {reason}", story["id"], p["index"]);
                    }
                    *skipped.entry(reason).or_default() += 1;
                    continue;
                }
            };
            used += 1;
            if std::env::var("CONFORMANCE_DUMP_WS").is_ok() {
                let w2 = m.widths.clone();
                let wf = move |k: usize| w2[k.min(w2.len() - 1)];
                for l in explain(&m.items, &settings, &wf, &m.breaks) {
                    if l.category != 0 && l.space_unit > 0.0 {
                        let ws = 100.0 + (l.measure - l.natural) / l.space_unit * 100.0;
                        let spaces = m.items[l.start..l.end].iter().filter(|i| i.ch == ' ').count();
                        eprintln!("WS {:.3} {} {} {:.4} {:.4}", ws, story["id"], p["index"], l.natural - l.measure, l.space_unit / spaces.max(1) as f64);
                    }
                }
            }
            hyphen_points += m.hyphen_points;
            hyphen_points_without_width += m.hyphen_points_without_width;
            let widths = m.widths.clone();
            let width = move |k: usize| widths[k.min(widths.len() - 1)];
            let ours = compose(&m.items, &settings, &width);
            if bench > 0 {
                let t = std::time::Instant::now();
                for _ in 0..bench {
                    std::hint::black_box(compose(&m.items, &settings, &width));
                }
                compose_time += t.elapsed();
                let t = std::time::Instant::now();
                for _ in 0..bench {
                    let mut starts = vec![0];
                    starts.extend_from_slice(&ours);
                    for (k, &start) in starts.iter().enumerate() {
                        let end = starts.get(k + 1).copied().unwrap_or(m.items.len());
                        std::hint::black_box(place_line(&m.items, start, end, width(k), &settings, k + 1 == starts.len()));
                    }
                }
                place_time += t.elapsed();
                bench_items += m.items.len() * bench;
            }
            let theirs: HashSet<usize> = m.breaks.iter().copied().collect();
            total += m.breaks.len();
            found += ours.iter().filter(|b| theirs.contains(b)).count();
            let hyphen_at = |b: &usize| *b > 0 && m.items[b - 1].ch == SOFT_HYPHEN;
            let dash_at = |b: &usize| hyphen_break(&m.items, *b, &settings).is_some_and(|h| !h.discretionary);
            indesign_dashes += m.breaks.iter().filter(|b| dash_at(b)).count();
            our_dashes += ours.iter().filter(|b| dash_at(b)).count();
            dashes_matched += ours.iter().filter(|b| dash_at(b) && theirs.contains(b)).count();
            indesign_hyphens += m.breaks.iter().filter(|b| hyphen_at(b)).count();
            our_hyphens += ours.iter().filter(|b| hyphen_at(b)).count();
            hyphens_matched += ours.iter().filter(|b| hyphen_at(b) && theirs.contains(b)).count();
            if m.had_special {
                special_used += 1;
            }
            let id = format!("{}:{}", story["id"], p["index"]);
            if debug.as_deref() == Some(id.as_str()) {
                eprintln!("== {id} InDesign (cost {:.2})", path_cost(&m.items, &settings, &width, &m.breaks));
                for line in describe(&m.items, &explain(&m.items, &settings, &width, &m.breaks)) {
                    eprintln!("  {line}");
                }
                eprintln!("== {id} ours (cost {:.2})", path_cost(&m.items, &settings, &width, &ours));
                for line in describe(&m.items, &explain(&m.items, &settings, &width, &ours)) {
                    eprintln!("  {line}");
                }
            }
            if ours == m.breaks {
                exact += 1;
                if m.had_special {
                    special_exact += 1;
                }
                // Positions: our justified placement against InDesign's measured x.
                for (k, &start) in m.line_starts.iter().enumerate() {
                    let end = m.line_starts.get(k + 1).copied().unwrap_or(m.items.len());
                    let last = k + 1 == m.line_starts.len();
                    let placed = place_line(&m.items, start, end, width(k), &settings, last);
                    let mut worst = 0.0_f64;
                    // Only visible items: trailing blanks are not drawn.
                    let visible = (start..end).rev().find(|&i| !is_trailing_blank(m.items[i].ch)).map_or(0, |i| i + 1 - start);
                    for (i, x) in placed.x.iter().enumerate().take(visible) {
                        if let Some(measured) = m.measured_x.get(k).and_then(|v| v.get(i)) {
                            let e = (x - measured).abs();
                            position_errors.push(e);
                            worst = worst.max(e);
                        }
                    }
                    if let Ok(prefix) = std::env::var("CONFORMANCE_DEBUG_LINE") {
                        let text: String = m.items[start..end].iter().map(|i| i.ch).collect();
                        if text.starts_with(&prefix) {
                            eprintln!("line {} last={last} measure={:.3} ws={:.1}%", k + 1, width(k), placed.word_spacing);
                            for (i, x) in placed.x.iter().enumerate() {
                                let it = &m.items[start + i];
                                eprintln!("  {:?} adv={:.4} end={:.4} ours={:.4} indesign={:.4}", it.ch, it.advance, it.end_advance, x, m.measured_x.get(k).and_then(|v| v.get(i)).copied().unwrap_or(f64::NAN));
                            }
                        }
                    }
                    if worst > 0.05 {
                        let text: String = m.items[start..end].iter().map(|i| i.ch).collect();
                        line_errors.push(json!({"story": story["id"], "line": k + 1, "last": last, "maxErrorPt": worst, "text": text}));
                    }
                }
            } else {
                let first = (0..m.breaks.len().max(ours.len()))
                    .find(|&i| m.breaks.get(i) != ours.get(i))
                    .unwrap_or(0);
                // Why: an InDesign break we do not offer, or a different choice among the same
                // candidates (then compare both choices under our cost).
                let unsupported: Vec<String> = m
                    .breaks
                    .iter()
                    .filter(|&&b| !can_break_before(&m.items, b, &settings))
                    .map(|&b| context(&m.items, Some(b)))
                    .collect();
                let shift = match (m.breaks.get(first), ours.get(first)) {
                    (Some(&a), Some(&b)) => b as i64 - a as i64,
                    _ => 0,
                };
                let theirs_lines = explain(&m.items, &settings, &width, &m.breaks);
                let indesign_cost: f64 = theirs_lines.iter().map(|l| l.cost).sum();
                let our_cost = path_cost(&m.items, &settings, &width, &ours);
                let past_minimum = theirs_lines.iter().filter(|l| l.category == 5).count();
                let min_ws = theirs_lines
                    .iter()
                    .filter(|l| l.category != 0 && l.space_unit > 0.0)
                    .map(|l| 100.0 + (l.measure - l.natural) / l.space_unit * 100.0)
                    .fold(f64::INFINITY, f64::min);
                let cause = if !unsupported.is_empty() {
                    "InDesign break not offered (dictionary)"
                } else if past_minimum > 0 {
                    "InDesign line past the shrink minimum under our measurement"
                } else if m.breaks.len() != ours.len() {
                    "different line count"
                } else if our_cost < indesign_cost {
                    "our path cheaper under the model"
                } else {
                    "search missed a cheaper path"
                };
                *causes.entry(cause).or_default() += 1;
                mismatches.push(json!({
                    "story": story["id"], "paragraphIndex": p["index"], "firstDifferentLine": first + 1,
                    "indesignLines": m.breaks.len() + 1, "ourLines": ours.len() + 1,
                    "cause": cause,
                    "unsupportedIndesignBreaks": unsupported,
                    "shiftChars": shift,
                    "indesignCost": indesign_cost,
                    "ourCost": our_cost,
                    "indesignLinesPastMinimum": past_minimum,
                    "indesignMinWordSpacing": min_ws,
                    "indesign": context(&m.items, m.breaks.get(first).copied()),
                    "ours": context(&m.items, ours.get(first).copied()),
                }));
            }
        }
    }

    position_errors.sort_by(f64::total_cmp);
    let share = if used > 0 { exact as f64 / used as f64 } else { 0.0 };
    let summary = json!({
        "source": input,
        "search": format!("{:?}", settings_of(&json!({})).search),
        "paragraphsCompared": used,
        "paragraphsComparedWithFreshComposition": compared_fresh,
        "paragraphsExact": exact,
        "exactShare": share,
        "breaksFound": found,
        "breaksTotal": total,
        "paragraphsWithSpecialCharacters": {"compared": special_used, "exact": special_exact},
        "hyphenation": {
            "dictionaryPoints": hyphen_points,
            "pointsWithoutHyphenWidth": hyphen_points_without_width,
            "indesignHyphenatedLines": indesign_hyphens,
            "ourHyphenatedLines": our_hyphens,
            "matched": hyphens_matched,
            "indesignDashBreaks": indesign_dashes,
            "ourDashBreaks": our_dashes,
            "dashBreaksMatched": dashes_matched,
        },
        "skipped": skipped,
        "mismatchCauses": causes,
        "positionErrorPt": {
            "n": position_errors.len(),
            "median": quantile(&position_errors, 0.5),
            "p95": quantile(&position_errors, 0.95),
            "max": position_errors.last().copied().unwrap_or(f64::NAN),
        },
        "elapsedMs": started.elapsed().as_millis() as u64,
        "minExact": min_exact,
        "mismatches": mismatches,
        "linesOver005pt": line_errors,
    });
    if let Some(path) = report {
        let json = serde_json::to_string_pretty(&summary).map_err(|e| InputError::new("serialize", &path, e))?;
        std::fs::write(&path, json).map_err(|e| InputError::new("write", &path, e))?;
    }
    println!(
        "paragraphs {exact}/{used} exact ({:.1}%), breaks {found}/{total} ({:.1}%), position error p95 {:.4} pt, with special characters {special_exact}/{special_used}, hyphens InDesign {indesign_hyphens} ours {our_hyphens} matched {hyphens_matched}",
        share * 100.0,
        if total > 0 { found as f64 * 100.0 / total as f64 } else { 0.0 },
        quantile(&position_errors, 0.95),
    );
    println!("  dash breaks InDesign {indesign_dashes} ours {our_dashes} matched {dashes_matched}; skipped {skipped:?}");
    if compared_fresh > 0 {
        println!("  compared with InDesign's fresh composition (stored composition from other fonts): {compared_fresh} paragraphs");
    }
    println!("  mismatches {causes:?}");
    if bench > 0 {
        let per = |d: std::time::Duration| d.as_secs_f64() * 1e6 / (used * bench).max(1) as f64;
        println!(
            "  bench: compose {:.1} µs/paragraph, place_line {:.1} µs/paragraph ({:.0} items/paragraph)",
            per(compose_time),
            per(place_time),
            bench_items as f64 / (used * bench).max(1) as f64
        );
    }
    Ok(match min_exact {
        Some(min) if share < min => {
            eprintln!("FAIL: exact share {:.1}% is below the gate {:.1}%", share * 100.0, min * 100.0);
            ExitCode::from(1)
        }
        _ => ExitCode::SUCCESS,
    })
}
