//! Applied-parent sources, from the oldest ancestor to the immediate parent.
use crate::{Affine, Document, Page, Rect, Spread};

pub struct Source<'a> {
    pub spread: &'a Spread,
    pub page: &'a Page,
    pub transform: Affine,
    pub range: Rect,
    pub hidden: Vec<&'a str>,
}

pub fn sources<'a>(doc: &'a Document, spread: &'a Spread, page: &'a Page) -> Vec<Source<'a>> {
    let mut out = Vec::new();
    let (mut spread, mut page) = (spread, page);
    let mut transform = Affine::IDENTITY;
    let mut hidden = Vec::new();
    let mut visited = std::collections::HashSet::new();
    visited.insert(spread.id.as_str());
    while !spread.hide_master_items {
        let Some(master) = page.master.as_ref().and_then(|id| doc.master_spreads.iter().find(|m| &m.id == id)) else { break };
        if !visited.insert(master.id.as_str()) { break; }
        let rect = page.transform.apply_rect(&page.bounds);
        let right = if spread.pages.len() == 1 { rect.left >= -1.0 } else { rect.left + rect.right > 0.0 };
        let Some(mp) = spread.master_page(master, page, right) else { break };
        let Some(inv) = mp.transform.invert() else { break };
        transform = transform.then_inner(&page.transform).then_inner(&page.master_transform).then_inner(&inv);
        hidden.extend(page.master_overrides.iter().map(String::as_str));
        let mut range = mp.transform.apply_rect(&mp.bounds);
        if master.pages.len() == 1 { range.left = f64::MIN / 4.0; range.right = f64::MAX / 4.0; }
        out.push(Source { spread: master, page: mp, transform, range, hidden: hidden.clone() });
        (spread, page) = (master, mp);
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_transforms_overrides_and_cycles() {
        let page = |id: &str, master: Option<&str>, dx: f64| Page {
            id: id.into(), master: master.map(str::to_owned),
            master_transform: Affine::translate(dx, 0.0),
            master_overrides: vec![format!("{id}-override")], ..Page::default()
        };
        let mut doc = Document::default();
        doc.master_spreads = vec![
            Spread { id: "a".into(), pages: vec![page("ap", None, 0.0)], ..Spread::default() },
            Spread { id: "b".into(), pages: vec![page("bp", Some("a"), 7.0)], ..Spread::default() },
        ];
        let spread = Spread { id: "document".into(), pages: vec![page("p", Some("b"), 11.0)], ..Spread::default() };
        let got = sources(&doc, &spread, &spread.pages[0]);
        assert_eq!(got.iter().map(|s| s.spread.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(got[0].transform.tx, 18.0);
        assert_eq!(got[0].hidden, ["p-override", "bp-override"]);
        assert_eq!(got[1].hidden, ["p-override"]);
        doc.master_spreads[0].pages[0].master = Some("b".into());
        assert_eq!(sources(&doc, &spread, &spread.pages[0]).len(), 2);
        doc.master_spreads[1].hide_master_items = true;
        assert_eq!(sources(&doc, &spread, &spread.pages[0]).len(), 1);
    }
}
