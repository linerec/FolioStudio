//! Where a new topic goes among its siblings, and which section heading it gets. InDesign keeps
//! topics sorted when one is added (the DOM order is the IDML order).
//!
//! Sort order (measured in InDesign 2025, scenarios `index-c3-r3c/r3d`, `index-c4-*`): ICU root
//! collation with digit runs by value, punctuation and spaces shifted to the last level (`a b` <
//! `a_b` < `a-b` < `a.b` < `ab`; `Newark` < `New York`), tertiary case (lowercase first) and the
//! fourth level as tie-break. Every script follows that one order: symbols, currency, digits,
//! Latin, Greek, Cyrillic, Georgian, Armenian, Hebrew, Arabic, ... Thai, Hangul, Kana, Han. Two
//! names that are equal at all levels are the same topic (`a2` and `a02`: InDesign adds one).
//! The collation data is `icu_collator` (ICU4X, Unicode-3.0), compiled in: the same order on every OS. The Han characters outside the
//! original main block are placed by Unihan's radical and strokes (`han`; measured, index6-han-*).
//!
//! Section headings (top level only): a name that starts with a Latin letter (also with one
//! accent, `Ø`, `ł`, `ı`, fullwidth and circled letters) is under that letter, in A to Z. Every
//! other name (digits, symbols, other scripts, `Æ`, `ß`, `ð`, `þ`, `Œ`, ligatures, letters with two
//! accents) is under `Symbols`, which comes first. Names under a heading are ordered by the
//! collation above. First characters not measured give `None` and generation refuses them.
use icu_collator::options::{AlternateHandling, CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use std::cmp::Ordering;
use std::sync::OnceLock;

pub(super) fn collator() -> &'static CollatorBorrowed<'static> {
    static COLLATOR: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
    COLLATOR.get_or_init(|| {
        let mut prefs = CollatorPreferences::default();
        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Quaternary);
        options.alternate_handling = Some(AlternateHandling::Shifted);
        // compiled data: cannot fail for the root locale
        Collator::try_new(prefs, options).unwrap_or_else(|e| panic!("root collation data: {e}"))
    })
}

/// The section a name belongs to at the top level.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum Heading {
    Symbols,
    Letter(char),
}

/// The first character decides the heading. `None`: not measured.
fn heading_of(c: char) -> Option<Heading> {
    let letter = |l: char| Some(Heading::Letter(l.to_ascii_uppercase()));
    match c {
        'A'..='Z' | 'a'..='z' => letter(c),
        '0'..='9' => Some(Heading::Symbols),
        // measured ASCII symbols and punctuation (index-c3-r3c, index-c4-mix, index-c5-quote-a)
        '!' | '#' | '$' | '%' | '&' | '(' | ')' | '*' | '+' | '-' | '@' | '_' | '~' | '<' | '=' | '>' | '|'
        | ',' | '.' | ':' | ';' | '?' | '[' | '\\' | ']' | '^' | '`' | '{' | '}' => Some(Heading::Symbols),
        // measured punctuation of other kinds, none of it skipped like a quotation mark (index-c5-quote-a/b): CJK brackets and marks,
        // dashes, fullwidth and decorative quotes, dots
        '〈' | '〉' | '《' | '》' | '「' | '」' | '『' | '』' | '【' | '】' | '〔' | '〕' | '（' | '）' | '［' | '］' | '｛' | '｝'
        | '｡' | '､' | '、' | '。' | '·' | '・' | '‐' | '‑' | '–' | '—' | '―' | '‖' | '‥' | '‼' | '′' | '″' | '※' | '⁇' | '‰'
        | '❝' | '❞' | '〝' | '〞' | '＂' | '＇' | '❛' | '❜' => Some(Heading::Symbols),
        // Latin-1: letters with one accent are under their base letter; Æ Ð Þ ß are Symbols (æ ð þ as well)
        '\u{C0}'..='\u{C5}' | '\u{E0}'..='\u{E5}' => letter('a'),
        '\u{C7}' | '\u{E7}' => letter('c'),
        '\u{C8}'..='\u{CB}' | '\u{E8}'..='\u{EB}' => letter('e'),
        '\u{CC}'..='\u{CF}' | '\u{EC}'..='\u{EF}' => letter('i'),
        '\u{D1}' | '\u{F1}' => letter('n'),
        '\u{D2}'..='\u{D6}' | '\u{D8}' | '\u{F2}'..='\u{F6}' | '\u{F8}' => letter('o'),
        '\u{D9}'..='\u{DC}' | '\u{F9}'..='\u{FC}' => letter('u'),
        '\u{DD}' | '\u{FD}' | '\u{FF}' => letter('y'),
        '\u{C6}' | '\u{E6}' | '\u{D0}' | '\u{F0}' | '\u{DE}' | '\u{FE}' | '\u{DF}' => Some(Heading::Symbols),
        // Latin Extended-A letters measured under their base letter (index-c4-first)
        'Ā' | 'ă' | 'Ą' => letter('a'),
        'Ć' => letter('c'),
        'Ď' => letter('d'),
        'Ē' => letter('e'),
        'ğ' => letter('g'),
        'İ' | 'ı' => letter('i'),
        'ł' => letter('l'),
        'ƀ' => letter('b'),
        'ń' => letter('n'),
        'ő' => letter('o'),
        'ř' => letter('r'),
        'ś' | 'ş' | 'ſ' => letter('s'),
        'ţ' => letter('t'),
        'ů' => letter('u'),
        'ź' | 'ž' => letter('z'),
        'Ạ' => letter('a'),
        // letters with a stroke, ligatures, compatibility letters: Symbols (measured)
        'đ' | 'Œ' | 'ĳ' | 'ŉ' | 'ǆ' | 'ǅ' | 'ª' | 'º' | 'ẞ' | 'ᴀ' | 'ɐ' | 'ℂ' | 'ℓ' | 'ʰ' | 'ﬁ' | 'Ǻ' | 'Ⱥ' | 'Ȼ' | 'ᶏ' | 'Ɓ' => Some(Heading::Symbols),
        // fullwidth and circled Latin letters
        '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}' => fullwidth(c),
        '\u{24B6}'..='\u{24CF}' | '\u{24D0}'..='\u{24E9}' => circled(c),
        // measured symbols: ① ™ € and the Latin-1 / general punctuation and currency symbols
        '\u{A1}'..='\u{A5}' | '\u{A7}' | '\u{AC}' | '\u{AE}' | '\u{B0}' | '\u{B1}' | '\u{B5}' | '\u{B6}' | '\u{BC}'..='\u{BF}' | '\u{B2}' | '\u{A9}' | '\u{D7}' | '\u{F7}'
        | '\u{2020}' | '\u{2022}' | '\u{2026}' | '\u{20A9}' | '\u{20AC}' | '\u{20B9}' | '\u{E3F}' | '\u{2116}' | '\u{2122}' | '\u{2103}' | '\u{2460}' => Some(Heading::Symbols),
        // digits of other scripts
        '\u{FF10}'..='\u{FF19}' | '\u{660}'..='\u{669}' => Some(Heading::Symbols),
        // other scripts: all under Symbols (measured: Greek, Cyrillic, Georgian, Armenian, Hebrew, Arabic, Syriac, Thaana, Indic,
        // Thai, Lao, Tibetan, Myanmar, Khmer, Ethiopic, Cherokee, Canadian syllabics, Runic, Hangul, Kana, Bopomofo, Han)
        c if other_script(c) => Some(Heading::Symbols),
        _ => None,
    }
}

fn fullwidth(c: char) -> Option<Heading> {
    let n = c as u32;
    let offset = if n >= 0xFF41 { n - 0xFF41 } else { n - 0xFF21 };
    char::from_u32(b'A' as u32 + offset).map(Heading::Letter)
}

fn circled(c: char) -> Option<Heading> {
    let n = c as u32;
    let offset = if n >= 0x24D0 { n - 0x24D0 } else { n - 0x24B6 };
    char::from_u32(b'A' as u32 + offset).map(Heading::Letter)
}

fn other_script(c: char) -> bool {
    matches!(c as u32,
        0x370..=0x3FF | 0x1F00..=0x1FFF // Greek
        | 0x400..=0x52F // Cyrillic
        | 0x530..=0x58F | 0x10A0..=0x10FF // Armenian, Georgian
        | 0x590..=0x5FF | 0xFB1D..=0xFB4F // Hebrew
        | 0x600..=0x6FF | 0x700..=0x7BF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF // Arabic, Syriac, Thaana
        | 0x900..=0xDFF // Indic scripts, Sinhala
        | 0xE00..=0xEFF // Thai, Lao
        | 0xF00..=0xFFF | 0x1000..=0x109F | 0x1780..=0x17FF // Tibetan, Myanmar, Khmer
        | 0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF // Hangul
        | 0x1200..=0x139F | 0x13A0..=0x13FF | 0x1400..=0x167F | 0x16A0..=0x16FF // Ethiopic, Cherokee, Canadian syllabics, Runic
        | 0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F // Kana
        | 0x3100..=0x312F // Bopomofo
        | 0x3005 | 0x3007 | 0x2E80..=0x2FDF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF // Han (the extensions are placed by `han`)
    )
}

/// The heading of a topic with this sort key (its sort order, else its name): the first character that is not a quotation
/// mark decides (index-c5). `None`: not measured.
pub(super) fn heading_for(key: &str) -> Option<Heading> {
    key.chars().find(|&c| !super::quotes::is_quote(c)).and_then(heading_of)
}

/// Generation sorts and groups by the key. Below the top level only the order matters (any text);
/// at the top level the first character must be one whose heading is measured.
pub(super) fn generation_supported(s: &str, top: bool) -> bool {
    !s.is_empty() && (!top || heading_for(s).is_some())
}

pub(super) fn heading(s: &str) -> String {
    match heading_for(s) {
        Some(Heading::Letter(l)) => l.to_string(),
        _ => "Symbols".into(),
    }
}

/// Order of two sort keys (a topic's sort order, else its name) at the top level or below it.
pub(super) fn compare(top: bool, a: &str, b: &str) -> Ordering {
    // top level: the Symbols section first, then A to Z; each in collation order
    let section = |s: &str| matches!(heading_for(s), Some(Heading::Letter(_)));
    if top && section(a) != section(b) {
        return section(a).cmp(&section(b));
    }
    collator().compare(&super::han::fold(&super::quotes::fold(a)), &super::han::fold(&super::quotes::fold(b)))
}

/// InDesign adds no second topic whose name is equal to an existing one under the collation
/// (`a2` and `a02`, measured index-c4-mix; `a'b` and `a‘b`, measured index-c5-quote-c).
pub(super) fn same_topic(a: &str, b: &str) -> bool {
    compare(false, a, b) == Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::{compare, generation_supported, heading, same_topic};
    use serde_json::Value;

    fn sorted(top: bool, names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by(|a, b| compare(top, a, b));
        v
    }

    #[test]
    fn top_level_matches_the_measured_dom_order() {
        // index-sort-mixed: InDesign's own order for these topics
        let expected = ["2 two", "10 ten", "_under", "ㄱ글", "가", "가방", "나", "다", "출판", "apple", "Apple", "banana", "Éclair", "Zebra"];
        let mut shuffled = expected.to_vec();
        shuffled.reverse();
        assert_eq!(sorted(true, &shuffled), expected);
    }

    #[test]
    fn an_unaccented_letter_comes_before_the_accented_one() {
        assert_eq!(sorted(true, &["Über", "Uber", "Ñandu", "Nap", "Çedilla", "Cat"]), ["Cat", "Çedilla", "Ñandu", "Nap", "Uber", "Über"]);
    }

    #[test]
    fn digits_symbols_hangul_latin_at_the_top_and_latin_before_hangul_below() {
        assert_eq!(sorted(true, &["Zed", "Alpha", "가나", "Beta", "12", "출판"]), ["12", "가나", "출판", "Alpha", "Beta", "Zed"]);
        assert_eq!(sorted(false, &["Zed", "글꼴", "Alpha", "가나", "Beta"]), ["Alpha", "Beta", "Zed", "가나", "글꼴"]);
    }

    #[test]
    fn spaces_and_punctuation_are_the_last_level_and_equal_names_are_one_topic() {
        assert_eq!(sorted(false, &["ab", "a.b", "a-b", "a_b", "a b"]), ["a b", "a_b", "a-b", "a.b", "ab"]);
        assert_eq!(sorted(false, &["New York", "Newton", "Newark"]), ["Newark", "Newton", "New York"]);
        assert!(same_topic("a2", "a02") && same_topic("0", "00") && !same_topic("apple", "Apple") && !same_topic("a b", "ab"));
    }

    fn golden(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/index-c3").join(name).join("indesign.json");
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
    }

    fn names(topics: &Value) -> Vec<(String, String)> {
        topics.as_array().unwrap().iter().map(|t| (t["name"].as_str().unwrap().to_owned(), t["sortOrder"].as_str().unwrap().to_owned())).collect()
    }

    /// InDesign's topic order (DOM = generated order) and section headings, measured per scenario.
    /// Every scenario: the DOM order is the ICU order of the keys with Symbols first; the generated
    /// heading lines are the headings Folio gives.
    #[test]
    fn matches_every_measured_topic_order_and_heading() {
        let top = ["index-c4-first-a", "index-c4-first-b", "index-c4-mix-a", "index-c4-mix-b", "index-c4-mix-c", "index-c4-scripts-a",
            "index-c4-scripts-b", "index-c4-scripts-c", "index-c4-sortorder", "index-c5-quote-a", "index-c5-quote-b", "index-c5-quote-c", "index-c5-quote-d",
            "index6-han-bmp"];
        let mut checked = 0;
        for scenario in top {
            let g = golden(scenario);
            let key = |t: &(String, String)| if t.1.is_empty() { t.0.clone() } else { t.1.clone() };
            // names whose first character is not measured (generation refuses them) are left out
            // (a topic without a reference is not in the generated text)
            let referenced = Value::Array(g["extension"]["document"]["indexes"][0]["topics"].as_array().unwrap().iter().filter(|t| !t["references"].as_array().unwrap().is_empty()).cloned().collect());
            let all = names(&referenced);
            let topics: Vec<_> = all.iter().filter(|t| generation_supported(&key(t), true)).cloned().collect();
            for pair in topics.windows(2) {
                assert_ne!(compare(true, &key(&pair[0]), &key(&pair[1])), std::cmp::Ordering::Greater, "{scenario}: {:?} before {:?}", pair[0], pair[1]);
            }
            let text = g["extension"]["generated"][0]["text"].as_str().unwrap();
            let mut current = String::new();
            let mut entries = all.iter();
            for line in text.split('\r').skip(1).filter(|l| !l.is_empty()) {
                if let Some(name) = line.strip_suffix("  1") {
                    let topic = entries.next().unwrap();
                    if !generation_supported(&key(topic), true) {
                        continue;
                    }
                    assert_eq!(super::super::quotes::printed(&topic.0).as_deref(), Some(name), "{scenario}: generated order");
                    assert_eq!(heading(&key(topic)), current, "{scenario}: heading of {name}");
                    checked += 1;
                } else {
                    current = line.to_owned();
                }
            }
            // (the text of a scenario with characters of the supplementary planes is cut short at its end -- index-c5-han-a: the line of
            //  䶵 has no page number and an empty line follows; the text of index-c4-scripts-c does hold the line of 龙)
        }
        assert!(checked > 300, "{checked}");
        // below the top level: the same collation, no headings
        for scenario in ["index-c4-sub-a", "index-c4-sub-b"] {
            let g = golden(scenario);
            let parent = &g["extension"]["document"]["indexes"][0]["topics"][0]["topics"];
            let topics = names(parent);
            for pair in topics.windows(2) {
                assert_ne!(compare(false, &pair[0].0, &pair[1].0), std::cmp::Ordering::Greater, "{scenario}: {:?} before {:?}", pair[0], pair[1]);
            }
        }
    }

    /// A topic whose name is equal to an existing one for InDesign is that topic (`‑q` after `‐q`, `a‘b` after `a'b`): the topics
    /// InDesign kept are the ones Folio keeps, adding the names in the scenario's order.
    #[test]
    fn names_equal_for_indesign_are_one_topic() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/scenarios/index-c5.json");
        let scenarios: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut checked = 0;
        for s in scenarios["scenarios"].as_array().unwrap() {
            let name = s["name"].as_str().unwrap();
            if !std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/index-c3").join(name).join("indesign.json").exists() || name == "noop" {
                continue;
            }
            let mut kept: Vec<String> = Vec::new();
            for step in s["steps"].as_array().unwrap() {
                if step["op"] == "addIndexTopic" && step["parent"] == "" {
                    let n = step["name"].as_str().unwrap();
                    if !kept.iter().any(|k| k == n || same_topic(k, n)) {
                        kept.push(n.to_owned());
                    }
                }
            }
            let dom: Vec<String> = names(&golden(name)["extension"]["document"]["indexes"][0]["topics"]).into_iter().map(|t| t.0).collect();
            let (mut a, mut b) = (kept.clone(), dom.clone());
            a.sort();
            b.sort();
            assert_eq!(a, b, "{name}: topics kept");
            checked += kept.len();
        }
        assert!(checked > 100, "{checked}");
    }
}
