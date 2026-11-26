use super::*;
use std::ops::Range;

/// Keep untouched element bytes, whitespace, and attribute order when only attributes change.
pub(super) fn attribute_edits(
    t: &str,
    ix: &Index,
    i: usize,
    a: &Attrs,
) -> Option<Vec<(Range<usize>, String)>> {
    let old = read_attrs(t, ix, i);
    let properties = |values: &Attrs| {
        values
            .iter()
            .filter(|(_, v)| v.xml.is_some())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Attrs>()
    };
    if properties(&old) != properties(a) {
        return None;
    }
    let mut edits = Vec::new();
    for at in ix.attrs(t, i) {
        let key = &t[at.name.0..at.name.1];
        match a.get(key) {
            Some(v) if old.get(key) != Some(v) => {
                edits.push((at.value.0..at.value.1, escape(&v.value)))
            }
            None => {
                let mut start = at.name.0;
                while start > ix.nodes[i].name.end && t.as_bytes()[start - 1].is_ascii_whitespace()
                {
                    start -= 1;
                }
                edits.push((start..at.value.1 + 1, String::new()));
            }
            _ => {}
        }
    }
    let added = a
        .iter()
        .filter(|(k, v)| v.xml.is_none() && !old.contains_key(*k))
        .map(|(k, v)| format!(" {k}=\"{}\"", escape(&v.value)))
        .collect::<String>();
    if !added.is_empty() {
        let at = ix.nodes[i].open_end - if ix.nodes[i].empty { 2 } else { 1 };
        edits.push((at..at, added));
    }
    Some(edits)
}
