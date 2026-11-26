//! Nested styles of a paragraph style (or of one paragraph): an ordered list that applies a character
//! style from the start of the paragraph up to or through a delimiter.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NestedStyle {
    /// `CharacterStyle/<name>`; `CharacterStyle/$ID/[No character style]` styles nothing.
    pub style: String,
    /// IDML enumeration name (`AnyWord`, `Sentence`, `Repeat`, ...), or the typed characters when `custom`.
    pub delimiter: String,
    /// The delimiter is the user's own character set (IDML `type="string"`), not a named one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom: bool,
    /// Count of delimiters; for `Repeat`, how many of the items before it are repeated.
    pub repetition: u32,
    /// "Through" (true) or "up to" (false).
    pub inclusive: bool,
    /// Original list position when editing an imported record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<usize>,
}

/// One item of a paragraph style's nested line styles (`AllLineStyles`): a character style for the paragraph's next
/// `lines` lines, or `[Repeat]` of the `lines` items before it (measured: rules/dropcap.ko.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NestedLineStyle {
    /// `CharacterStyle/<name>`; `CharacterStyle/$ID/[No character style]` styles nothing (the line still counts).
    pub style: String,
    /// Lines the item covers (0: the rest of the paragraph); for a repeat, how many of the items before it repeat.
    pub lines: u32,
    /// `[Repeat]`: repeat the `lines` items before this one to the end of the paragraph.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub repeat: bool,
    /// Original list position when editing an imported record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<usize>,
}

impl NestedLineStyle {
    /// The item that each line of a paragraph with `count` lines gets: index into `items` (`None`: a line without
    /// a character style: past the end of the list or an item with no style). A repeat runs the items before it
    /// again and again; its count is cut to the items there are. Lines count 0 = the rest of the paragraph.
    pub fn per_line(items: &[NestedLineStyle], count: usize) -> Vec<Option<usize>> {
        let mut out: Vec<Option<usize>> = Vec::with_capacity(count);
        let mut index = 0;
        while index < items.len() && out.len() < count {
            let item = &items[index];
            if item.repeat {
                let n = (item.lines as usize).min(index);
                if n == 0 { break; }
                'cycle: loop {
                    #[allow(clippy::needless_range_loop, reason = "emit the original style indexes from the preceding span")]
                    for j in index - n..index {
                        if items[j].repeat { break 'cycle; }
                        let take = if items[j].lines == 0 { count - out.len() } else { items[j].lines as usize };
                        for _ in 0..take { if out.len() >= count { break 'cycle; } out.push(Some(j)); }
                    }
                }
                break;
            }
            let take = if item.lines == 0 { count - out.len() } else { item.lines as usize };
            for _ in 0..take { if out.len() >= count { break; } out.push(Some(index)); }
            index += 1;
        }
        out.resize(count, None);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(lines: u32, repeat: bool) -> NestedLineStyle { NestedLineStyle { style: "CharacterStyle/A".into(), lines, repeat, source: None } }

    // InDesign 2025's per-character DOM answers (artifacts/dropcap-c2-20261023/measurement/nls1.json)
    #[test]
    fn items_cover_lines_in_order() {
        let ix = |v: Vec<Option<usize>>| v.into_iter().map(|x| x.map_or('-', |i| (b'a' + i as u8) as char)).collect::<String>();
        assert_eq!(ix(NestedLineStyle::per_line(&[item(2, false), item(1, false)], 6)), "aab---");
        assert_eq!(ix(NestedLineStyle::per_line(&[item(1, false)], 3)), "a--");
        assert_eq!(ix(NestedLineStyle::per_line(&[item(0, false)], 4)), "aaaa");
        assert_eq!(ix(NestedLineStyle::per_line(&[item(30, false)], 4)), "aaaa");
        // A(1) [Repeat]1 -> every line A; A(1) B(2) [Repeat]1 -> A then B for the rest; A(1) B(1) [Repeat]2 -> A B A B ...
        assert_eq!(ix(NestedLineStyle::per_line(&[item(1, false), item(1, true)], 4)), "aaaa");
        assert_eq!(ix(NestedLineStyle::per_line(&[item(1, false), item(2, false), item(1, true)], 6)), "abbbbb");
        assert_eq!(ix(NestedLineStyle::per_line(&[item(1, false), item(1, false), item(2, true)], 6)), "ababab");
        assert_eq!(ix(NestedLineStyle::per_line(&[item(1, false), item(1, false), item(5, true)], 5)), "ababa");
    }
}
