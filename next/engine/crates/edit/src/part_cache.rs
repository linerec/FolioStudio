//! Revision-bound XML positions. Only edits wholly inside an attribute value can keep
//! the index: the element tree is then identical and only the byte positions move.
use crate::xml::Index;
use std::collections::BTreeSet;
use std::ops::Range;

pub(crate) struct Cached {
    pub rev: u64,
    pub index: Index,
    /// All changes since this revision are confined to the listed spread children.
    since: u64,
    items: BTreeSet<usize>,
}

impl Cached {
    pub fn new(rev: u64, index: Index) -> Self {
        Self { rev, index, since: rev, items: BTreeSet::new() }
    }

    /// Called before replacing the bytes. False means the index must be discarded.
    pub fn splice(&mut self, text: &str, range: Range<usize>, new: &str, rev: u64) -> bool {
        let ix = &self.index;
        let Some(i) = ix.nodes.iter().position(|n| n.name.end < range.start && range.end < n.open_end) else { return false };
        let Some(a) = ix.attrs(text, i).into_iter().find(|a| a.value.0 <= range.start && range.end <= a.value.1) else { return false };
        let quote = text.as_bytes()[a.value.1];
        if new.bytes().any(|b| b == quote || b == b'<') {
            return false;
        }
        // The host may reread these complete subtrees, but never a page or spread setting.
        // Self values are kept: they also define TextPath links across parts.
        let top = std::iter::once(i).chain(ix.ancestors(i)).find(|&j| {
            ix.nodes[j].parent.is_some_and(|p| {
                matches!(ix.local(text, p), "Spread" | "MasterSpread")
                    && ix.nodes[p].parent.is_some_and(|root| ix.nodes[root].parent.is_none())
            })
        });
        let item = top.filter(|&j| {
            &text[a.name.0..a.name.1] != "Self"
                && ix.attr(text, j, "Self").is_some()
                && !ix.nodes.iter().enumerate().any(|(k, n)| {
                    n.start >= ix.nodes[j].start && n.end <= ix.nodes[j].end
                        && matches!(ix.local(text, k), "Page" | "Spread" | "MasterSpread")
                })
        });
        if let Some(item) = item {
            self.items.insert(item);
        } else {
            self.since = rev;
            self.items.clear();
        }
        let delta = new.len() as isize - range.len() as isize;
        let shift = |n: &mut usize| {
            if *n >= range.end { *n = n.checked_add_signed(delta).expect("attribute byte position"); }
        };
        for n in &mut self.index.nodes {
            shift(&mut n.name.start);
            shift(&mut n.name.end);
            shift(&mut n.start);
            shift(&mut n.open_end);
            shift(&mut n.close_start);
            shift(&mut n.end);
        }
        self.rev = rev;
        true
    }

    pub fn fragments<'a>(&self, text: &'a str, since: u64) -> Option<Vec<&'a str>> {
        if since < self.since || self.items.is_empty() { return None; }
        Some(self.items.iter().map(|&i| {
            let n = &self.index.nodes[i];
            &text[n.start..n.end]
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_index(a: &Index, b: &Index) {
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn attribute_edits_keep_exact_positions_and_child_order() {
        let mut text = r#"<idPkg:Spread><Spread Self="s"><Page Self="p"/><Group Self="g"><Rectangle Self="r" ItemTransform="1 0 0 1 5 6"><Image Self="im"><Properties><Contents><![CDATA[abc<ignored>]]></Contents></Properties></Image></Rectangle></Group><Oval Self="o"/></Spread></idPkg:Spread>"#.to_owned();
        let mut cache = Cached::new(0, Index::scan(&text).unwrap());
        for (rev, new) in [(1, "1 0 0 1 10000 6"), (2, ""), (3, "1 0 0 1 5 6")] {
            let i = cache.index.by_self(&text, "r").unwrap();
            let a = cache.index.attrs(&text, i).into_iter().find(|a| &text[a.name.0..a.name.1] == "ItemTransform").unwrap();
            let r = a.value.0..a.value.1;
            assert!(cache.splice(&text, r.clone(), new, rev));
            text.replace_range(r, new);
            same_index(&cache.index, &Index::scan(&text).unwrap());
            let fragments = cache.fragments(&text, 0).unwrap();
            assert_eq!(fragments.len(), 1);
            assert!(fragments[0].starts_with("<Group"));
        }
        let i = cache.index.by_self(&text, "p").unwrap();
        let a = cache.index.attrs(&text, i)[0];
        assert!(cache.splice(&text, a.value.0..a.value.1, "page", 4));
        assert!(cache.fragments(&text, 0).is_none());
    }

    #[test]
    fn structural_edits_and_quotes_drop_the_index() {
        let text = r#"<idPkg:Spread><Spread><Rectangle Self="r" Name="old"/></Spread></idPkg:Spread>"#;
        let at = text.find("old").unwrap();
        for (range, new) in [(at..at + 3, "new\" Bad=\"value"), (at..at + 3, "<bad>"), (0..0, "<!--comment-->"), (at - 1..at + 4, "'new'")] {
            assert!(!Cached::new(0, Index::scan(text).unwrap()).splice(text, range, new, 1));
        }
    }
}
