//! Han characters in index order. InDesign orders every Han character by Unihan radical and additional strokes (the
//! CLDR "radical-stroke" order; ties by code point) and puts the characters of the extension blocks (A: U+3400, B..: U+20000..,
//! the additions to the main block U+9FA6.. and the unified ideographs of the compatibility block U+FA0E..) among the characters
//! of the main block (measured: `index-c5-han-a`, `index6-han-words`, `index6-han-bmp`; the order of the characters follows
//! Unihan's `kRSUnicode` in all of them).
//!
//! ICU4X's compiled root data has the original main block (U+4E00..U+9FA5) in that order and the rest after it in code point
//! order, so the characters it misplaces are rewritten for the comparison: each becomes two characters, the main block character
//! just before it in the radical-stroke order (its anchor) and a private use character that stands for its distance from the
//! anchor. A private use character has a primary weight above every Han character and the private use characters are ordered by
//! code point, so the pair is after the anchor and whatever follows it, and before the next anchor, and the extension characters
//! behind one anchor keep their order, also inside a longer name (a mark on the anchor would be a secondary difference: the next
//! character of the name would decide before it). The characters of the main block that ICU4X's data orders differently from this
//! Unihan version (about 2 percent: the data are of different Unicode versions) stay where the collation data puts them and are no
//! anchors.
//!
//! The order is `data/unihan-rs.bin` (made by `next/tools/gen-unihan-rs.py` from the Unihan database, Unicode License v3,
//! `data/UNICODE-LICENSE`).
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

static TABLE: &[u8] = include_bytes!("../../data/unihan-rs.bin");

/// The original main block, which the collation data orders by radical and strokes itself.
const MAIN: std::ops::RangeInclusive<u32> = 0x4E00..=0x9FA5;
/// The private use character for a distance from the anchor (planes 15 and 16 have 131068 of them: more than the characters there are).
fn distance(steps: usize) -> char {
    let n = if steps < 0xFFFE { 0xF0000 + steps } else { 0x100000 + steps - 0xFFFE };
    char::from_u32(n as u32).unwrap_or('\u{10FFFD}')
}

/// Code points in radical-stroke order.
fn order() -> impl Iterator<Item = u32> {
    assert!(TABLE.len() >= 8 && &TABLE[..4] == b"UHRS", "unihan-rs.bin");
    let count = u32::from_le_bytes([TABLE[4], TABLE[5], TABLE[6], TABLE[7]]) as usize;
    TABLE[8..8 + count * 3].chunks_exact(3).map(|b| u32::from_le_bytes([b[0], b[1], b[2], 0]))
}

/// The main block characters whose order the collation data and Unihan agree on (the longest increasing subsequence of the
/// main block in Unihan order under the collation): the characters the others are placed after.
fn anchors(order: &[u32]) -> Vec<bool> {
    let collator = super::order::collator();
    let one = |cp: u32| char::from_u32(cp).unwrap_or('\u{4E00}').to_string();
    let main: Vec<usize> = (0..order.len()).filter(|&i| MAIN.contains(&order[i])).collect();
    let text: Vec<String> = main.iter().map(|&i| one(order[i])).collect();
    let (mut tails, mut prev): (Vec<usize>, Vec<usize>) = (Vec::new(), vec![usize::MAX; main.len()]);
    for i in 0..main.len() {
        let at = tails.partition_point(|&t| collator.compare(&text[t], &text[i]) == std::cmp::Ordering::Less);
        if at > 0 {
            prev[i] = tails[at - 1];
        }
        if at == tails.len() { tails.push(i) } else { tails[at] = i }
    }
    let mut keep = vec![false; order.len()];
    let mut at = tails.last().copied().unwrap_or(usize::MAX);
    while at != usize::MAX {
        keep[main[at]] = true;
        at = prev[at];
    }
    keep
}

/// Every character the collation data misplaces: the anchor before it and its distance from that anchor in the order.
fn moved() -> &'static HashMap<u32, (char, usize)> {
    static MOVED: OnceLock<HashMap<u32, (char, usize)>> = OnceLock::new();
    MOVED.get_or_init(|| {
        let order: Vec<u32> = order().collect();
        let keep = anchors(&order);
        let mut moved = HashMap::new();
        let (mut anchor, mut steps) = ('\u{4E00}', 0);
        for (i, &cp) in order.iter().enumerate() {
            if keep[i] {
                (anchor, steps) = (char::from_u32(cp).unwrap_or('\u{4E00}'), 0);
            } else if !MAIN.contains(&cp) {
                steps += 1;
                moved.insert(cp, (anchor, steps));
            }
        }
        moved
    })
}

/// Whether the character may be one the collation data misplaces (a cheap test that leaves out the scripts around the Han).
fn candidate(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x9FA6..=0x9FFF | 0xFA0E..=0xFA29 | 0x20000..=0x3FFFF)
}

/// The text with every misplaced Han character rewritten (unchanged, and not copied, when there is none).
pub(super) fn fold(s: &str) -> Cow<'_, str> {
    if !s.chars().any(candidate) {
        return Cow::Borrowed(s);
    }
    let table = moved();
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match table.get(&(c as u32)).filter(|_| candidate(c)) {
            Some(&(anchor, steps)) => {
                out.push(anchor);
                out.push(distance(steps));
            }
            None => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn the_table_is_in_radical_stroke_order() {
        let order: Vec<u32> = order().collect();
        assert!(order.len() > 100_000, "{}", order.len());
        let pos = |c: char| order.iter().position(|&n| n == c as u32).unwrap_or_else(|| panic!("{c} not in the table"));
        // measured (index-c5-han-a): U+20000 and U+3400 come before 中, which has the second radical; 鿿 is between 海 and 豈
        let measured = ['一', '丁', '𠀀', '𠀁', '𠀂', '𰀀', '𪜀', '𫝀', '𠀋', '㐀', '㐁', '㐂', '中', '丿', '乂', '亅', '二', '亠', '人', '侮', '\u{FA0E}',
            '𫠠', '更', '海', '\u{9FFF}', '龍', '䶵', '𪛖'];
        assert!(measured.windows(2).all(|w| pos(w[0]) < pos(w[1])), "{measured:?}");
        // a compatibility ideograph with a canonical decomposition is not in the table (豈 is U+F900 for U+8C48)
        assert!(!order.contains(&0xF900) && order.contains(&0xFA0E));
    }

    #[test]
    fn only_the_characters_the_collation_misplaces_are_rewritten() {
        for plain in ["abc", "가나다", "一丁中", "日本語", "ひらがな", "ＡＢ", "\u{2F00}\u{2E80}"] {
            assert!(matches!(fold(plain), Cow::Borrowed(_)), "{plain}");
        }
        let (anchor, steps) = moved()[&0x20000];
        assert_eq!(anchor as u32, 0x4E06); // the last main block character of radical 1 and one stroke (code point order within it)
        assert_eq!(fold("a一𠀀"), format!("a一{anchor}{}", distance(steps)));
        assert!(moved().values().all(|&(_, steps)| steps < 0x10000), "the longest distance");
        // an extension character sorts right behind the main block character before it in the order and before the next one
        let c = super::super::order::collator();
        let less = |a: &str, b: &str| c.compare(&fold(a), &fold(b)) == std::cmp::Ordering::Less;
        assert!(less("丁", "\u{20000}") && less("\u{20000}", "\u{20001}") && less("\u{20001}", "\u{3400}") && less("\u{3400}", "中"));
        assert!(less("海", "\u{9FFF}") && less("\u{9FFF}", "豈") && less("侮", "\u{FA0E}") && less("\u{FA0E}", "更"));
    }

    fn names(path: &str) -> Vec<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/index-c3").join(path).join("indesign.json");
        let g: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap();
        g["extension"]["document"]["indexes"][0]["topics"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
    }

    /// InDesign's topic order (the DOM order is the generated order) of Han characters and words that mix the main block, the extensions,
    /// U+9FFD..9FFF and the compatibility ideographs. The tail of InDesign's generated text is cut short after supplementary-plane
    /// characters, so the first two are compared by the topic order only.
    #[test]
    fn matches_the_han_order_indesign_measured() {
        for scenario in ["index-c5-han-a", "index6-han-words", "index6-han-bmp"] {
            let topics = names(scenario);
            assert!(topics.len() > 30, "{scenario}");
            let dom: Vec<&String> = topics.iter().collect();
            let mut shuffled = dom.clone();
            shuffled.reverse();
            shuffled.sort_by(|a, b| super::super::order::compare(true, a, b));
            assert_eq!(shuffled, dom, "{scenario}");
        }
    }
}
