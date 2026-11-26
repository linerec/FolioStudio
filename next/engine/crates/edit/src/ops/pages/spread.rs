//! Read and serialize spread pages and their owned items.

use super::*;

#[derive(Clone)]
pub(super) struct Page {
    pub(super) id: String,
    pub(super) xml: String,
    pub(super) binding: usize,
}
#[derive(Clone)]
pub(super) struct Item {
    pub(super) xml: String,
    pub(super) page: Option<String>,
}
#[derive(Clone)]
pub(super) struct Spread {
    pub(super) part: String,
    pub(super) xml: String,
    pub(super) prefix: String,
    pub(super) suffix: String,
    pub(super) pages: Vec<Page>,
    pub(super) items: Vec<Item>,
}

impl Spread {
    pub(super) fn id(&self) -> String {
        get(&self.xml, "Self")
    }
    pub(super) fn allowed(&self) -> bool {
        get(&self.xml, "AllowPageShuffle") != "false"
    }
    /// Layout ▸ Pages ▸ Hide Spread.
    pub(super) fn hidden(&self) -> bool {
        get(&self.xml, "SpreadHidden") == "true"
    }
    /// View ▸ Rotate Spread: the quarter turn (0, 90 clockwise, 180, 270) in the spread's `ItemTransform`.
    pub(super) fn rotation(&self) -> i32 {
        let m = mat(&self.xml).0;
        if m[0] > 0.5 { 0 } else if m[1] > 0.5 { 90 } else if m[0] < -0.5 { 180 } else if m[1] < -0.5 { 270 } else { 0 }
    }
    pub(super) fn read(part: String, t: &str) -> Result<Self, EditError> {
        let ix = scan(t)?;
        let root = (0..ix.nodes.len())
            .find(|&i| {
                matches!(ix.local(t, i), "Spread" | "MasterSpread")
                    && ix.attr(t, i, "Self").is_some()
            })
            .ok_or_else(|| EditError::new("xml", "spread missing"))?;
        let binding = ix
            .attr(t, root, "BindingLocation")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let pages: Vec<_> = ix
            .named(t, root, "Page")
            .iter()
            .map(|&i| Page {
                id: ix.attr(t, i, "Self").unwrap_or_default().into(),
                xml: element(t, &ix, i),
                binding,
            })
            .collect();
        for p in &pages {
            let r = nums(&p.xml, "GeometricBounds");
            let m = nums(&p.xml, "ItemTransform");
            if r.len() != 4
                || r.iter().any(|n| !n.is_finite())
                || r[2] <= r[0]
                || r[3] <= r[1]
                || m.len() != 6
                || m.iter().any(|n| !n.is_finite())
            {
                return Err(EditError::new("invalid", "page geometry is invalid"));
            }
            if m[..4] != [1., 0., 0., 1.] {
                return Err(EditError::new(
                    "unsupported",
                    "page-panel edits of rotated or scaled pages are not supported",
                ));
            }
        }
        let rects: Vec<_> = pages
            .iter()
            .map(|p| {
                let b = bounds(&p.xml);
                let m = mat(&p.xml);
                let (x, y) = m.apply((b[0], b[1]));
                [x, y, x + b[2] - b[0], y + b[3] - b[1]]
            })
            .collect();
        let items = ix
            .children(root)
            .iter()
            .filter(|&&i| is_item(&ix, t, i))
            .map(|&i| {
                let b = spread_bounds(&ix, t, i).unwrap_or([0.; 4]);
                let best = rects
                    .iter()
                    .enumerate()
                    .map(|(j, r)| {
                        let area = (b[2].min(r[2]) - b[0].max(r[0])).max(0.)
                            * (b[3].min(r[3]) - b[1].max(r[1])).max(0.);
                        (j, area)
                    })
                    .filter(|(_, a)| *a > 0.)
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                Item {
                    xml: element(t, &ix, i),
                    page: best.map(|(j, _)| pages[j].id.clone()),
                }
            })
            .collect();
        Ok(Self {
            part,
            xml: element(t, &ix, root),
            prefix: t[..ix.nodes[root].start].into(),
            suffix: t[ix.nodes[root].end..].into(),
            pages,
            items,
        })
    }
    pub(super) fn output(&self) -> Result<String, EditError> {
        let mut t = self.xml.clone();
        let ix = scan(&t)?;
        let mut edits = Vec::new();
        let mut at = None;
        for &i in ix.children(0) {
            if ix.local(&t, i) == "Page" || is_item(&ix, &t, i) {
                at.get_or_insert(ix.nodes[i].start);
                edits.push((element_span(&ix, &t, i), String::new()));
            }
        }
        let insert = self
            .pages
            .iter()
            .map(|p| p.xml.as_str())
            .chain(self.items.iter().map(|i| i.xml.as_str()))
            .collect::<Vec<_>>()
            .join("\n\t\t");
        // Insert at the first page's start; use its removal range to avoid overlap.
        if let Some((_, s)) = edits.first_mut() {
            *s = format!("\n\t\t{insert}");
        } else {
            edits.push((ix.nodes[0].close_start..ix.nodes[0].close_start, insert));
        }
        edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        for (r, s) in edits {
            t.replace_range(r, &s);
        }
        set(&mut t, "PageCount", &self.pages.len().to_string());
        Ok(format!("{}{t}{}", self.prefix, self.suffix))
    }
}
