//! The supported paragraph GREP dialect, shared by reading and editing.
use crate::xml::Element;
use folio_model::grep::GrepStyle;

pub use crate::grep_engine::Regex;

pub(crate) struct Compiled {
    pub rule: GrepStyle,
    pub regex: Regex,
}

pub(crate) fn compiled(rules: Vec<GrepStyle>, w: &mut crate::warn::Warnings) -> Vec<Compiled> {
    rules.into_iter().filter_map(|rule| match compile(&rule.expression) {
        Ok(regex) => Some(Compiled {rule, regex}),
        Err(e) => { w.add(format!("GREP style not applied: {e}")); None }
    }).collect()
}

/// Compile one expression with the matcher that follows InDesign's dialect (`grep_engine`); the error says why not.
pub fn compile(expression: &str) -> Result<Regex, String> {
    if expression.is_empty() || expression.len() > 4096 { return Err("GREP expression must contain 1..4096 bytes".into()); }
    Regex::new(expression)
}

pub(crate) fn read(style: &Element<'_>) -> Option<Vec<GrepStyle>> {
    let list = style.child("Properties").and_then(|p|p.child("AllGREPStyles"));
    if let Some(list) = list {
        return Some(list.children_named("ListItem").enumerate().map(|(index, item)| GrepStyle {
            style: item.child("AppliedCharacterStyle").map(Element::text).unwrap_or_default(),
            expression: item.child("GrepExpression").map(Element::text).unwrap_or_default(),
            source: Some(index),
        }).collect());
    }
    (style.attr("EmptyGrepStyles") == Some("true")).then(Vec::new)
}

pub fn rules(source: &str, id: &str) -> Result<Vec<GrepStyle>, String> {
    let root = crate::xml::parse(source).map_err(|e|e.0)?;
    let mut styles = Vec::new(); root.descendants_named("ParagraphStyle", &mut styles);
    let style = styles.into_iter().find(|s|s.attr("Self") == Some(id)).ok_or("paragraph style not found")?;
    Ok(read(style).unwrap_or_default())
}


#[cfg(test)]
mod tests {
    use super::*;

    /// What InDesign colours when the expression is the paragraph's GREP style: the stored text is split into
    /// paragraphs (each keeps its CR except the story's last) and matched one by one.
    fn mask(rx: &Regex, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = vec!['0'; chars.len()];
        let mut start = 0;
        loop {
            let end = chars[start..].iter().position(|c| *c == '\r').map(|p| start + p + 1).unwrap_or(chars.len());
            for r in rx.find_all(&chars[start..end], start == 0) { for i in r { out[start + i] = '1'; } }
            if end >= chars.len() { break; }
            start = end;
        }
        out.into_iter().collect()
    }

    /// Cases where Folio and InDesign still differ (id -> why). Everything else in the table must agree.
    const KNOWN: &[(&str, &str)] = &[
        ("backref-168", r"InDesign takes (a)\2 (a group that is not there) as a second \1; Folio rejects the expression"),
        ("r2-361", "same as backref-168"),
        ("r2-385", r"a literal U+2028 is odd in InDesign (a$ does not match at the story's end after it); forced breaks are \n"),
    ];

    #[test]
    fn indesign_table() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/goldens/grep-syntax/indesign.json");
        let golden: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("golden")).unwrap();
        let (mut agree, mut rejected_nothing, mut differ, mut known) = (0, 0, Vec::new(), 0);
        for case in golden["cases"].as_array().unwrap() {
            let (id, pattern, text, want) = (case["id"].as_str().unwrap(), case["pattern"].as_str().unwrap(), case["text"].as_str().unwrap(), case["mask"].as_str().unwrap());
            let got = match compile(pattern) { Ok(rx) => mask(&rx, text), Err(e) => { if want.contains('1') { format!("rejected: {e}") } else { rejected_nothing += 1; want.to_owned() } } };
            if got == want { agree += 1; }
            else if KNOWN.iter().any(|(k, _)| id == *k) { known += 1; }
            else { differ.push(format!("{id} {pattern:?} on {text:?}\n   InDesign {want}\n   Folio    {got}")); }
        }
        eprintln!("GREP table: {agree} agree ({rejected_nothing} are expressions both reject), {known} known differences, {} unexpected", differ.len());
        assert!(differ.is_empty(), "{}", differ.join("\n"));
    }

    /// `\N{name}` (measured: any letter case; `\N{U+0041}` names nothing). The table is `next/tools/gen-unicode-names.py`'s.
    #[test]
    fn names_match_unicode_data() {
        use crate::grep_engine::character_named as named;
        assert_eq!(named("LATIN SMALL LETTER A"), Some('a'));
        assert_eq!(named("latin small letter a"), Some('a'));
        assert_eq!(named("GREEK SMALL LETTER ALPHA"), Some('\u{3b1}'));
        assert_eq!(named("SPACE"), Some(' '));
        assert_eq!(named("DIGIT ONE"), Some('1'));
        assert_eq!(named("EM DASH"), Some('\u{2014}'));
        assert_eq!(named("HYPHEN-MINUS"), Some('-'));
        assert_eq!(named("NO-BREAK SPACE"), Some('\u{a0}'));
        assert_eq!(named("HANGUL SYLLABLE GA"), Some('\u{ac00}'));
        assert_eq!(named("HANGUL SYLLABLE HIH"), Some('\u{d7a3}'));
        assert_eq!(named("HANGUL SYLLABLE SSANG"), Some('\u{c30d}'));
        assert_eq!(named("HANGUL SYLLABLE XYZ"), None);
        assert_eq!(named("CJK UNIFIED IDEOGRAPH-4E00"), Some('\u{4e00}'));
        assert_eq!(named("U+0041"), None);
        assert_eq!(named("NO SUCH CHARACTER"), None);
        let chars: Vec<char> = "aA\u{3b1}1 b".chars().collect();
        let find = |p: &str| compile(p).unwrap().find_all(&chars, true).into_iter().map(|r| r.start).collect::<Vec<_>>();
        assert_eq!((find(r"\N{LATIN SMALL LETTER A}"), find(r"\N{greek small letter alpha}"), find(r"\N{SPACE}")), (vec![0], vec![2], vec![4]));
        assert!(compile(r"\N{U+0041}").is_err() && compile(r"\N{nonsense}").is_err() && compile(r"\N").is_err());
    }

    #[test]
    fn dialect_rules_and_rejections() {
        let find = |p: &str, t: &str| -> Vec<String> {
            let chars: Vec<char> = t.chars().collect();
            compile(p).unwrap().find_all(&chars, true).into_iter().map(|r| chars[r].iter().collect()).collect()
        };
        assert_eq!(find("(?i)word", "WORD word"), ["WORD", "word"]);
        assert_eq!(find(r"\d+", "a123b 45"), ["123", "45"]);
        assert_eq!(find(r"(\w)\1", "aabbc"), ["aa", "bb"]);
        assert_eq!(find("(?<=a)b", "ab cb"), ["b"]);
        assert_eq!(find("A~SB", "A\u{a0}B"), ["A\u{a0}B"]);
        assert_eq!(find("a*", "baa"), Vec::<String>::new(), "the scan stops at an empty match");
        for p in ["", "(?<=a+)b", "(a", "a**", "*a", r"\p{Han}", "[~M]", "[[a-b]]", r"\k<n>"] {
            assert!(compile(p).is_err(), "{p}");
        }
        // markers (measured, rules/specialchars-c1.ko.md): the page number kinds, a variable instance only by its code, breaks by their paragraph's end
        use folio_model::{marker, ParagraphStart};
        let text = [marker::PAGE, marker::NEXT_PAGE, marker::PREVIOUS_PAGE, marker::VARIABLE, marker::SECTION, marker::NOTE, '\u{fffc}'];
        let hit = |p: &str| -> Vec<usize> { compile(p).unwrap().find_all(&text, true).into_iter().map(|r| r.start).collect() };
        assert_eq!((hit("~#"), hit("~N"), hit("~X"), hit("~V")), (vec![0, 1, 2], vec![0], vec![1], vec![2]));
        assert_eq!((hit("~x"), hit("~F"), hit("~a"), hit(r"\x{0018}"), hit(r"\x{0019}"), hit(r"\x{0004}")), (vec![4], vec![5], vec![6], vec![0, 1, 2, 3], vec![4], vec![5]));
        assert!(hit("~I").is_empty(), "no index marker in this text");
        let marked = ['a', marker::INDEX, 'b', '\u{7}', 'c'];
        let at = |p: &str| -> Vec<usize> { compile(p).unwrap().find_all(&marked, true).into_iter().map(|r| r.start).collect() };
        assert_eq!((at("~I"), at(r"\x{FEFF}")), (vec![1], vec![]), "only ~I finds the index marker");
        assert_eq!((at(r"\s"), at("."), at(r"\W")), (vec![3], vec![0, 2, 4], vec![]), "the indent-to-here tab is found by a white space class alone");
        assert!(compile("~I").unwrap().names_index() && compile(r"\s").unwrap().names_indent_tab() && compile("~i").unwrap().names_indent_tab());
        assert!(!compile(r"\S").unwrap().names_indent_tab() && !compile(".").unwrap().names_indent_tab());
        let para = ['a', '\r'];
        let rx = compile("~M").unwrap();
        assert!(rx.find_all_in(&para, true, ParagraphStart::Anywhere).is_empty());
        assert_eq!(rx.find_all_in(&para, true, ParagraphStart::NextColumn), vec![1..2]);
        assert!(compile("~P").unwrap().find_all_in(&para, true, ParagraphStart::NextColumn).is_empty());
        assert!(compile("~h").unwrap().names_end_nested() && !compile("a").unwrap().names_end_nested());
        assert_eq!(marker::PAGE.to_string() + &marker::END_NESTED.to_string(), "\u{10FF00}\u{10FF06}", "the codes in grep_engine::CODES are the marker characters");
    }

    #[test]
    fn truncated_and_invalid_numeric_syntax_returns_errors() {
        for pattern in ["[", "[a-", "[z-a]", "\\", "(?<", "\\x{", "a{4294967296}", "a{3,2}", "a{1001}"] {
            assert!(compile(pattern).is_err(), "{pattern:?}");
        }
        assert!(compile(&"a".repeat(4097)).is_err());
        assert_eq!(rules("<Root/>", "missing").unwrap_err(), "paragraph style not found");
    }
}
