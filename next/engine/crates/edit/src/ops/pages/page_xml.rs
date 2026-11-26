//! Read and rewrite page and spread XML fragments.

use super::*;

pub(super) fn scan(t: &str) -> Result<Index, EditError> {
    Index::scan(t).map_err(|e| EditError::new("xml", e))
}
pub(super) fn get(t: &str, key: &str) -> String {
    let ix = Index::scan(t).unwrap();
    ix.value(t, 0, key).unwrap_or_default()
}
pub(super) fn set(t: &mut String, key: &str, val: &str) {
    let ix = Index::scan(t).unwrap();
    if let Some(old) = ix.attr(t, 0, key) {
        if old == val {
            return;
        }
        if [
            "ItemTransform",
            "GeometricBounds",
            "MasterPageTransform",
            "PageWidth",
            "PageHeight",
            "ColumnsPositions",
        ]
        .contains(&key)
        {
            let numbers = |s: &str| {
                s.split_whitespace()
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>()
            };
            if let (Ok(a), Ok(b)) = (numbers(old), numbers(val))
                && a.len() == b.len()
                && !a.is_empty()
                && a.iter()
                    .zip(&b)
                    .all(|(a, b)| (a - b).abs() < 1e-9 * a.abs().max(1.))
            {
                return;
            }
        }
    }
    let (r, v) = attr_edit(&ix, t, 0, key, val);
    t.replace_range(r, &v);
}
pub(super) fn nums(t: &str, key: &str) -> Vec<f64> {
    get(t, key)
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect()
}
pub(super) fn bounds(t: &str) -> [f64; 4] {
    let n = nums(t, "GeometricBounds");
    [n[1], n[0], n[3], n[2]]
}
pub(super) fn mat(t: &str) -> Mat {
    let n = nums(t, "ItemTransform");
    Mat(n.try_into().unwrap_or([1., 0., 0., 1., 0., 0.]))
}
pub(super) fn shift(t: &mut String, dx: f64, dy: f64) {
    let mut m = mat(t);
    m.0[4] += dx;
    m.0[5] += dy;
    set(t, "ItemTransform", &m.text());
}
pub(super) fn element(t: &str, ix: &Index, i: usize) -> String {
    t[ix.nodes[i].start..ix.nodes[i].end].into()
}
pub(super) fn rewrite(w: &mut Work<'_>, part: &str, t: String) -> Result<(), EditError> {
    if let Ok(old) = w.text(part) {
        if xml_equal(old, &t) {
            return Ok(());
        }
        let len = old.len();
        w.splice(part, vec![(0..len, t)])
    } else {
        w.add_part(part.into(), t)
    }
}
// Preserve untouched parts byte-for-byte, including their indentation and attribute order.
// Leaf contents (including whitespace in stories) are never normalized.
fn xml_equal(a: &str, b: &str) -> bool {
    let (Ok(ax), Ok(bx)) = (scan(a), scan(b)) else {
        return a == b;
    };
    if ax.nodes.len() != bx.nodes.len() {
        return false;
    }
    for i in 0..ax.nodes.len() {
        if a[ax.nodes[i].name.clone()] != b[bx.nodes[i].name.clone()]
            || ax.nodes[i].parent != bx.nodes[i].parent
        {
            return false;
        }
        let attrs = |ix: &Index, t: &str| {
            ix.attrs(t, i)
                .iter()
                .map(|v| {
                    (
                        t[v.name.0..v.name.1].to_owned(),
                        crate::xml::unescape(&t[v.value.0..v.value.1]),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        if attrs(&ax, a) != attrs(&bx, b) {
            return false;
        }
        if ax.children(i).is_empty() {
            let text = |ix: &Index, t: &str| {
                let n = &ix.nodes[i];
                if n.empty {
                    String::new()
                } else {
                    t[n.open_end..n.close_start].to_owned()
                }
            };
            if text(&ax, a) != text(&bx, b) {
                return false;
            }
        }
    }
    true
}
