//! Index panel ▸ Sort Options (rest-c1): the language groups of the index (designmap `IndexingSortOption`: `Name`, `Include`, `Priority`,
//! `HeaderType`) and what generation makes of them.
//!
//! The IDML values are written as they are (exact round trip). Generation: a top-level topic belongs to the group of its first character
//! (not a quotation mark); a group that is not included gives its topics to the Symbol group. The included groups come in priority order,
//! each sorted by the collation of `order`. The section heading follows the group's header type: Roman `BasicLatin` (and the other Latin
//! header types) the letter A to Z (as `order` measured), Korean `KoreanConsonant` the first consonant (ㄱ ... ㅎ, a double consonant under
//! its single one, as the collation puts it), `KoreanConsonantPlusVowel` that consonant with ㅏ (가 ... 하), Kana the first kana as a
//! plain hiragana or katakana (`*All`) or the kana of its row (`*ConsonantsOnly`), Cyrillic the capital first letter; `Nothing` the
//! group's name (`Symbols`, `Numbers`, `Greek`, `Chinese`, `Arabic`, `Hebrew`). Chinese has no reading data in Folio, so both of its header
//! types give the one `Chinese` heading (interop.tsv `IndexingSortOption`). InDesign 2025 (this PC, English) generates the same index
//! whatever these values are (measured: artifacts/rest-c1-20261023/isort).
//! The defaults (Symbol and Roman included) give the measured headings of `order` unchanged.
use super::order::{self, Heading};
use crate::xml::{escape, Index};
use crate::{EditError, Work};
use std::cmp::Ordering;

/// The groups (IDML name without `$ID/`), their default inclusion and header type, in the default priority order.
pub(crate) const GROUPS: [(&str, bool, &str); 10] = [
    ("kIndexGroup_Symbol", true, "Nothing"),
    ("kIndexGroup_Alphabet", true, "BasicLatin"),
    ("kIndexGroup_Numeric", false, "Nothing"),
    ("kWRIndexGroup_GreekAlphabet", false, "Nothing"),
    ("kWRIndexGroup_CyrillicAlphabet", false, "Russian"),
    ("kIndexGroup_Kana", false, "HiraganaAll"),
    ("kIndexGroup_Chinese", false, "ChinesePinyin"),
    ("kIndexGroup_Korean", false, "KoreanConsonant"),
    ("kWRIndexGroup_ArabicAlphabet", false, "Nothing"),
    ("kWRIndexGroup_HebrewAlphabet", false, "Nothing"),
];

/// The header types each group takes (InDesign 2025's `HeaderTypes`, IDML spelling).
pub(crate) fn header_types(group: &str) -> &'static [&'static str] {
    match group {
        "kIndexGroup_Alphabet" => &["BasicLatin", "FinnishSwedish", "DanishNorwegian", "Spanish", "Croatian", "Czech", "Estonian", "Hungarian",
            "Latvian", "Lithuanian", "Polish", "Romanian", "Slovak", "Slovenian", "Turkish"],
        "kWRIndexGroup_CyrillicAlphabet" => &["Russian", "Belarusian", "Bulgarian", "Ukrainian"],
        "kIndexGroup_Kana" => &["HiraganaAll", "HiraganaConsonantsOnly", "KatakanaAll", "KatakanaConsonantsOnly"],
        "kIndexGroup_Chinese" => &["ChinesePinyin", "ChineseStrokeCount"],
        "kIndexGroup_Korean" => &["KoreanConsonant", "KoreanConsonantPlusVowel"],
        _ => &["Nothing"],
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SortGroup {
    /// IDML `Name` without `$ID/` (`kIndexGroup_Korean`).
    pub name: String,
    pub include: bool,
    pub priority: i64,
    pub header_type: String,
}

/// A change of one group (Sort Options dialog, engine command setIndexSortOptions): unset values stay.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SortGroupChange {
    pub name: String,
    #[serde(default)]
    pub include: Option<bool>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub header_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SortOptions {
    pub groups: Vec<SortGroup>,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self { groups: GROUPS.iter().enumerate().map(|(i, &(n, inc, h))| SortGroup { name: n.into(), include: inc, priority: i as i64, header_type: h.into() }).collect() }
    }
}

/// The group a character belongs to by script (`kIndexGroup_Symbol` for the rest).
fn script_group(c: char) -> &'static str {
    if matches!(order::heading_for(&c.to_string()), Some(Heading::Letter(_))) {
        return "kIndexGroup_Alphabet";
    }
    match c as u32 {
        0x30..=0x39 | 0xFF10..=0xFF19 | 0x660..=0x669 | 0x6F0..=0x6F9 => "kIndexGroup_Numeric",
        0x370..=0x3FF | 0x1F00..=0x1FFF => "kWRIndexGroup_GreekAlphabet",
        0x400..=0x52F => "kWRIndexGroup_CyrillicAlphabet",
        0x3041..=0x3096 | 0x30A1..=0x30FA | 0x31F0..=0x31FF | 0xFF66..=0xFF9D => "kIndexGroup_Kana",
        0x3005 | 0x3007 | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF => "kIndexGroup_Chinese",
        0x1100..=0x1112 | 0x3131..=0x3163 | 0xAC00..=0xD7A3 => "kIndexGroup_Korean",
        0x600..=0x65F | 0x66A..=0x6EF | 0x6FA..=0x6FF | 0x750..=0x77F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => "kWRIndexGroup_ArabicAlphabet",
        0x591..=0x5FF | 0xFB1D..=0xFB4F => "kWRIndexGroup_HebrewAlphabet",
        _ => "kIndexGroup_Symbol",
    }
}

fn first(key: &str) -> Option<char> {
    key.chars().find(|&c| !super::quotes::is_quote(c))
}

impl SortOptions {
    /// The options the document keeps (the defaults for groups it does not list).
    pub(crate) fn read(w: &Work<'_>) -> Result<Self, EditError> {
        let t = w.text("designmap.xml")?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        let mut out = Self::default();
        for i in (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "IndexingSortOption") {
            let name = ix.attr(t, i, "Name").unwrap_or("").trim_start_matches("$ID/");
            let Some(g) = out.groups.iter_mut().find(|g| g.name == name) else { continue };
            if let Some(v) = ix.attr(t, i, "Include") { g.include = v == "true"; }
            if let Some(v) = ix.attr(t, i, "Priority").and_then(|v| v.parse().ok()) { g.priority = v; }
            if let Some(v) = ix.attr(t, i, "HeaderType") { g.header_type = v.to_owned(); }
        }
        Ok(out)
    }

    /// The group a top-level key goes in: its script's group when that is included, else Symbol.
    fn group(&self, key: &str) -> &SortGroup {
        let symbol = self.groups.iter().find(|g| g.name == "kIndexGroup_Symbol").unwrap_or(&self.groups[0]);
        let Some(c) = first(key) else { return symbol };
        let name = script_group(c);
        self.groups.iter().find(|g| g.name == name && g.include).unwrap_or(symbol)
    }

    /// The place of the key's group among the groups (priority, then the default order).
    fn rank(&self, key: &str) -> (i64, usize) {
        let g = self.group(key);
        (g.priority, GROUPS.iter().position(|d| d.0 == g.name).unwrap_or(0))
    }

    /// Generation sorts and groups by the key; at the top level a key in the Symbol or Roman group must start with a measured character.
    pub(crate) fn supported(&self, key: &str, top: bool) -> bool {
        if !top || key.is_empty() { return order::generation_supported(key, top); }
        match self.group(key).name.as_str() {
            "kIndexGroup_Symbol" | "kIndexGroup_Alphabet" => order::generation_supported(key, top),
            _ => true,
        }
    }

    pub(crate) fn heading(&self, key: &str) -> String {
        let g = self.group(key);
        let c = first(key).unwrap_or(' ');
        match g.name.as_str() {
            "kIndexGroup_Symbol" | "kIndexGroup_Alphabet" => order::heading(key),
            "kIndexGroup_Korean" => korean(c, g.header_type == "KoreanConsonantPlusVowel"),
            "kIndexGroup_Kana" => kana(c, &g.header_type),
            "kWRIndexGroup_CyrillicAlphabet" => cyrillic(c),
            "kIndexGroup_Numeric" => "Numbers".into(),
            "kWRIndexGroup_GreekAlphabet" => "Greek".into(),
            "kIndexGroup_Chinese" => "Chinese".into(),
            "kWRIndexGroup_ArabicAlphabet" => "Arabic".into(),
            "kWRIndexGroup_HebrewAlphabet" => "Hebrew".into(),
            _ => order::heading(key),
        }
    }

    /// Order of two keys: at the top level by group, then by the collation (`order::compare`, whose Symbols-before-letters split is
    /// the default groups' order).
    pub(crate) fn compare(&self, top: bool, a: &str, b: &str) -> Ordering {
        if top {
            let (ra, rb) = (self.rank(a), self.rank(b));
            if ra != rb { return ra.cmp(&rb); }
        }
        order::compare(false, a, b)
    }
}

/// The 19 first consonants of the Hangul syllables; a double one's heading is its single one.
const CHOSEONG: [char; 19] = ['ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'];
const SINGLE: [usize; 19] = [0, 0, 2, 3, 3, 5, 6, 7, 7, 9, 9, 11, 12, 12, 14, 15, 16, 17, 18];

fn korean(c: char, vowel: bool) -> String {
    let n = c as u32;
    let l = match n {
        0xAC00..=0xD7A3 => Some(((n - 0xAC00) / 588) as usize),
        0x1100..=0x1112 => Some((n - 0x1100) as usize),
        // compatibility jamo: the consonants by their first part; a vowel alone is under ㅇ
        0x3131..=0x314E => {
            let jamo = ['ㄱ', 'ㄲ', 'ㄱ', 'ㄴ', 'ㄴ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㄹ', 'ㄹ', 'ㄹ', 'ㄹ', 'ㄹ', 'ㄹ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅂ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'];
            CHOSEONG.iter().position(|&j| j == jamo[(n - 0x3131) as usize])
        }
        0x314F..=0x3163 => Some(11),
        _ => None,
    };
    let Some(l) = l.map(|l| SINGLE[l]) else { return "Symbols".into() };
    if vowel { char::from_u32(0xAC00 + l as u32 * 588).map(String::from).unwrap_or_default() } else { CHOSEONG[l].to_string() }
}

/// Hiragana U+3041..=U+3096 without voicing marks and small forms.
const KANA_PLAIN: &str = "ああいいううええおおかかききくくけけここささししすすせせそそたたちちつつつててととなにぬねのはははひひひふふふへへへほほほまみむめもややゆゆよよらりるれろわわゐゑをんうかけ";

fn kana(c: char, header: &str) -> String {
    let n = c as u32;
    // katakana (and halfwidth katakana) as hiragana
    let h = match n {
        0x30A1..=0x30F6 => n - 0x60,
        0x30F7 => 0x308F, 0x30F8 => 0x3090, 0x30F9 => 0x3091, 0x30FA => 0x3092,
        0x31F0..=0x31FF => {
            // small katakana for Ainu: く し す と ぬ は ひ ふ へ ほ む ら り る れ ろ
            ['く', 'し', 'す', 'と', 'ぬ', 'は', 'ひ', 'ふ', 'へ', 'ほ', 'む', 'ら', 'り', 'る', 'れ', 'ろ'][(n - 0x31F0) as usize] as u32
        }
        0xFF66..=0xFF9D => "をぁぃぅぇぉゃゅょっーあいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわん".chars().nth((n - 0xFF66) as usize).map_or(0x3042, |c| c as u32),
        _ => n,
    };
    let plain = if (0x3041..=0x3096).contains(&h) { KANA_PLAIN.chars().nth((h - 0x3041) as usize).unwrap_or('あ') } else { 'あ' };
    let row = |p: char| -> char {
        let rows = ["あいうえお", "かきくけこ", "さしすせそ", "たちつてと", "なにぬねの", "はひふへほ", "まみむめも", "やゆよ", "らりるれろ", "わゐゑをん"];
        rows.iter().find(|r| r.contains(p)).and_then(|r| r.chars().next()).unwrap_or(p)
    };
    let p = if header.ends_with("ConsonantsOnly") { row(plain) } else { plain };
    if header.starts_with("Katakana") { char::from_u32(p as u32 + 0x60).unwrap_or(p).to_string() } else { p.to_string() }
}

fn cyrillic(c: char) -> String {
    let upper: String = c.to_uppercase().collect();
    // a letter the collation sorts with its base letter is under it
    match upper.as_str() {
        "Ё" => "Е".into(),
        "Й" => "И".into(),
        "Ї" => "І".into(),
        "Ў" => "У".into(),
        _ => upper,
    }
}

/// Index panel ▸ Sort Options: the groups' values in designmap (an `IndexingSortOption` the document lacks is added with the defaults first).
pub(crate) fn set(w: &mut Work<'_>, options: &Vec<SortGroupChange>) -> Result<(), EditError> {
    if options.is_empty() { return Err(super::invalid("정렬 옵션을 고르십시오")); }
    for o in options {
        let Some(&(_, _, _)) = GROUPS.iter().find(|g| g.0 == o.name) else { return Err(super::invalid(format!("unknown sort group {}", o.name))) };
        if let Some(h) = &o.header_type && !header_types(&o.name).contains(&h.as_str()) {
            return Err(super::invalid(format!("header type of {}: one of {}", o.name, header_types(&o.name).join(", "))));
        }
        if o.priority.is_some_and(|p| !(0..=99).contains(&p)) { return Err(super::invalid("priority: 0 to 99")); }
        let t = w.text("designmap.xml")?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let all: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "IndexingSortOption").collect();
        let found = all.iter().copied().find(|&i| ix.attr(&t, i, "Name").is_some_and(|n| n.trim_start_matches("$ID/") == o.name));
        match found {
            Some(i) => {
                let mut edits = Vec::new();
                if let Some(v) = o.include { edits.push(crate::ops::attr_edit(&ix, &t, i, "Include", if v { "true" } else { "false" })); }
                if let Some(v) = o.priority { edits.push(crate::ops::attr_edit(&ix, &t, i, "Priority", &v.to_string())); }
                if let Some(v) = &o.header_type { edits.push(crate::ops::attr_edit(&ix, &t, i, "HeaderType", v)); }
                edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
                w.splice("designmap.xml", edits)?;
            }
            None => {
                let d = GROUPS.iter().position(|g| g.0 == o.name).unwrap_or(0);
                let (_, inc, h) = GROUPS[d];
                let el = format!("<IndexingSortOption Self=\"dIndexingSortOptionn{}\" Name=\"$ID/{}\" Include=\"{}\" Priority=\"{}\" HeaderType=\"{}\" />",
                    escape(&o.name), escape(&o.name), o.include.unwrap_or(inc), o.priority.unwrap_or(d as i64), escape(o.header_type.as_deref().unwrap_or(h)));
                let root = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document"))?;
                let at = all.last().map(|&i| ix.nodes[i].end).unwrap_or(ix.nodes[root].close_start);
                w.splice("designmap.xml", vec![(at..at, format!("\n\t{el}"))])?;
            }
        }
    }
    // InDesign writes the options in priority order (measured: oracle index-sort isort-set); a tie keeps the order they had
    let t = w.text("designmap.xml")?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let all: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "IndexingSortOption").collect();
    let mut sorted = all.clone();
    sorted.sort_by_key(|&i| ix.attr(&t, i, "Priority").and_then(|v| v.parse::<i64>().ok()).unwrap_or(i64::MAX));
    if sorted != all {
        let edits = all.iter().zip(&sorted).filter(|(a, b)| a != b)
            .map(|(&a, &b)| (ix.nodes[a].start..ix.nodes[a].end, t[ix.nodes[b].start..ix.nodes[b].end].to_owned())).collect();
        w.splice("designmap.xml", edits)?;
    }
    Ok(())
}

/// The document's sort options for the Sort Options dialog (query `indexSortOptions`).
pub fn read_options(parts: &std::collections::BTreeMap<String, String>) -> Vec<SortGroup> {
    let mut out = SortOptions::default();
    if let Some(t) = parts.get("designmap.xml") && let Ok(ix) = Index::scan(t) {
        for i in (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "IndexingSortOption") {
            let name = ix.attr(t, i, "Name").unwrap_or("").trim_start_matches("$ID/");
            let Some(g) = out.groups.iter_mut().find(|g| g.name == name) else { continue };
            if let Some(v) = ix.attr(t, i, "Include") { g.include = v == "true"; }
            if let Some(v) = ix.attr(t, i, "Priority").and_then(|v| v.parse().ok()) { g.priority = v; }
            if let Some(v) = ix.attr(t, i, "HeaderType") { g.header_type = v.to_owned(); }
        }
    }
    out.groups.sort_by_key(|g| (g.priority, GROUPS.iter().position(|d| d.0 == g.name)));
    out.groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(on: &[(&str, &str)], first: &[&str]) -> SortOptions {
        let mut o = SortOptions::default();
        for g in &mut o.groups {
            if let Some((_, h)) = on.iter().find(|(n, _)| *n == g.name) { g.include = true; g.header_type = (*h).into(); }
            if let Some(p) = first.iter().position(|n| *n == g.name) { g.priority = -10 + p as i64; }
        }
        o
    }

    fn generated(o: &SortOptions, names: &[&str]) -> Vec<String> {
        let mut v: Vec<&str> = names.to_vec();
        v.sort_by(|a, b| o.compare(true, a, b));
        let mut out = Vec::new();
        let mut heading = String::new();
        for n in v {
            let h = o.heading(n);
            if h != heading { out.push(format!("[{h}]")); heading = h; }
            out.push(n.to_owned());
        }
        out
    }

    const NAMES: [&str; 17] = ["apple", "Banana", "3 three", "12 twelve", "_under", "가방", "까치", "나무", "ㄱ글", "하늘", "αλφα", "арбуз", "ёлка", "がっこう", "カメラ", "さくら", "出版"];

    #[test]
    fn defaults_are_the_measured_symbols_then_latin_letters() {
        let o = SortOptions::default();
        let g = generated(&o, &NAMES);
        assert_eq!(g[0], "[Symbols]");
        assert_eq!(&g[g.len() - 4..], ["[A]", "apple", "[B]", "Banana"]);
        // the same as order's own measured order (isort-default: InDesign's generated text for these names)
        let mut v: Vec<&str> = NAMES.to_vec();
        v.sort_by(|a, b| order::compare(true, a, b));
        assert_eq!(g.iter().filter(|s| !s.starts_with('[')).cloned().collect::<Vec<_>>(), v);
    }

    #[test]
    fn included_groups_get_their_headings_in_priority_order() {
        let o = opts(&[("kIndexGroup_Korean", "KoreanConsonant"), ("kIndexGroup_Numeric", "Nothing"), ("kIndexGroup_Kana", "HiraganaAll"),
            ("kWRIndexGroup_CyrillicAlphabet", "Russian")], &["kIndexGroup_Korean"]);
        let g = generated(&o, &NAMES);
        assert_eq!(&g[..9], ["[ㄱ]", "ㄱ글", "가방", "까치", "[ㄴ]", "나무", "[ㅎ]", "하늘", "[Symbols]"]);
        assert!(g.windows(3).any(|w| w == ["[Numbers]", "3 three", "12 twelve"]), "{g:?}");
        assert!(g.windows(2).any(|w| w == ["[А]", "арбуз"]) && g.windows(2).any(|w| w == ["[Е]", "ёлка"]), "{g:?}");
        assert!(g.windows(3).any(|w| w == ["[か]", "がっこう", "カメラ"]) && g.windows(2).any(|w| w == ["[さ]", "さくら"]), "{g:?}");
        // Greek and Chinese are not included: under Symbols
        let symbols = g.iter().position(|s| s == "[Symbols]").unwrap();
        assert!(g[symbols..].iter().take_while(|s| !s.starts_with("[A")).any(|s| s == "αλφα") && g.contains(&"出版".to_string()));
    }

    #[test]
    fn header_kinds() {
        assert_eq!(korean('까', true), "가");
        assert_eq!(korean('뷁', false), "ㅂ");
        assert_eq!(korean('ㅎ', true), "하");
        assert_eq!(kana('ぱ', "HiraganaConsonantsOnly"), "は");
        assert_eq!(kana('ょ', "KatakanaAll"), "ヨ");
        assert_eq!(kana('ｶ', "HiraganaAll"), "か");
        assert_eq!(cyrillic('й'), "И");
    }
}
