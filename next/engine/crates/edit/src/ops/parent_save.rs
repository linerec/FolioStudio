use super::*;

pub(crate) fn save(w: &mut Work<'_>, spread: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(spread)?;
    let t = w.text(&part)?;
    if ix.local(t, i) != "Spread" {
        return Err(EditError::new("invalid", "choose a document spread"));
    }
    let mut src = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
    let ix = Index::scan(&src).map_err(|e| EditError::new("xml", e))?;
    let pages: Vec<_> = ix.children(0).iter().copied().filter(|&k| ix.local(&src, k) == "Page").collect();
    // a spread has 1 to 10 pages; one page is centred on the spine, two or more already are (measured for 1, 2 and 3 pages)
    if pages.is_empty() || pages.len() > 10 {
        return Err(EditError::new("unsupported", "save as parent supports spreads of one to ten pages"));
    }
    // one page is centred on the origin; three or more pages (a document spread keeps its origin at the first spine) are centred
    // as a whole, as InDesign does (parent-save-three-page-spread: pages at -300, 0, 300 become -450, -150, 150)
    let dx = if pages.len() == 2 { 0.0 } else {
        let (mut left, mut right) = (f64::INFINITY, f64::NEG_INFINITY);
        for &p in &pages {
            let b: Vec<f64> = ix.attr(&src, p, "GeometricBounds").unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
            let m = Mat::parse(ix.attr(&src, p, "ItemTransform").unwrap_or("1 0 0 1 0 0")).ok_or_else(|| EditError::new("xml", "invalid page transform"))?;
            if b.len() != 4 { return Err(EditError::new("xml", "page has no bounds")); }
            left = left.min(m.0[4] + b[1]);
            right = right.max(m.0[4] + b[3]);
        }
        -(left + right) / 2.0
    };
    let mut edits = vec![
        (ix.nodes[0].name.clone(), "MasterSpread".into()),
        (ix.nodes[0].close_start + 2..ix.nodes[0].end - 1, "MasterSpread".into()),
    ];
    for key in ["FlattenerOverride", "SpreadHidden", "AllowPageShuffle", "BindingLocation", "PageTransitionType", "PageTransitionDirection", "PageTransitionDuration"] {
        if let Some(e) = attr_removal(&ix, &src, 0, key) { edits.push(e); }
    }
    for (key, value) in [("ItemTransform", "1 0 0 1 0 0"), ("Name", "Parent"), ("NamePrefix", "A"), ("BaseName", "Parent"), ("OverriddenPageItemProps", ""), ("PrimaryTextFrame", "n")] {
        edits.push(attr_edit(&ix, &src, 0, key, value));
    }
    for k in 1..ix.nodes.len() {
        let n = &ix.nodes[k];
        let tag = ix.local(&src, k);
        if is_item(&ix, &src, k) {
            edits.push(attr_edit(&ix, &src, k, "AllowOverrides", "true"));
        }
        if tag == "FlattenerPreference" && n.parent == Some(0) || tag == "Descriptor" && n.parent.and_then(|p| ix.nodes[p].parent).is_some_and(|p| pages.contains(&p)) {
            edits.push((n.start..n.end, String::new()));
        }
        if tag == "Page" {
            edits.push(attr_edit(&ix, &src, k, "AppliedAlternateLayout", "n"));
            edits.push(attr_edit(&ix, &src, k, "UseMasterGrid", "false"));
        }
        if n.parent == Some(0) && dx != 0.0 && let Some(value) = ix.attr(&src, k, "ItemTransform") {
            let mut m = Mat::parse(value).ok_or_else(|| EditError::new("xml", "invalid item transform"))?;
            m.0[4] += dx;
            edits.push(attr_edit(&ix, &src, k, "ItemTransform", &m.text()));
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    for (r, value) in edits { src.replace_range(r, &value); }
    let at = src.find('>').ok_or_else(|| EditError::new("xml", "spread has no opening tag"))? + 1;
    src.insert_str(at, "\n\t\t<Properties><PageColor type=\"enumeration\">UseMasterColor</PageColor></Properties>");
    parents::copy_parent(w, &src)
}
