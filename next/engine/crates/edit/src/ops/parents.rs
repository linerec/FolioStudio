//! Parent pages in the Pages panel (next/oracle/scenarios/pages-parents.json):
//! - New Parent: the next free prefix letter, base name "Parent", as many pages as a spread of
//!   the document (two with facing pages) at the document's page size and default margins;
//! - Duplicate: a copy with new ids (its text frames' stories copied) and the next free prefix;
//! - Delete: the parent's part, designmap entry and stories go; pages and parents that used it
//!   get [None] (`AppliedMaster="n"`, an identity `MasterPageTransform`, `UseMasterGrid="false"`);
//! - Parent Page Options: prefix and name (the pages carry the prefix as their name) and Based
//!   On: the pages take the parent's margins and layout grid, as applying a parent does.
use super::*;

const DM: &str = "designmap.xml";

/// The document's parents: (part, Self, prefix), in designmap order.
pub(super) fn parents(w: &Work<'_>) -> Result<Vec<(String, String, String)>, EditError> {
    let dm = w.text(DM)?;
    let ix = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
    let mut out = Vec::new();
    for i in (0..ix.nodes.len()).filter(|&i| ix.local(dm, i) == "MasterSpread") {
        let Some(part) = ix.value(dm, i, "src") else { continue };
        let t = w.text(&part)?;
        let px = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        if let Some(m) = (0..px.nodes.len()).find(|&k| px.local(t, k) == "MasterSpread" && px.attr(t, k, "Self").is_some()) {
            out.push((part.clone(), px.attr(t, m, "Self").unwrap().to_owned(), px.value(t, m, "NamePrefix").unwrap_or_default()));
        }
    }
    Ok(out)
}

/// The first prefix no parent has: A to Z, then AA, AB…
pub(super) fn free_prefix(taken: &[String]) -> String {
    let letters: Vec<char> = ('A'..='Z').collect();
    let singles = letters.iter().map(|a| a.to_string());
    let pairs = letters.iter().flat_map(|a| letters.iter().map(move |b| format!("{a}{b}")));
    singles.chain(pairs).find(|p| !taken.contains(p)).unwrap_or_else(|| "A".into())
}

pub(super) fn doc_pref(w: &Work<'_>, element: &str, name: &str) -> Option<String> {
    let t = w.text("Resources/Preferences.xml").ok()?;
    let ix = Index::scan(t).ok()?;
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == element).and_then(|i| ix.value(t, i, name))
}

/// `ColumnsPositions` of `count` columns in a live area `width` wide.
pub(super) fn column_positions(width: f64, count: u32, gutter: f64) -> String {
    let n = count.max(1) as f64;
    let col = (width - gutter * (n - 1.0)) / n;
    (0..count.max(1)).flat_map(|k| { let x = k as f64 * (col + gutter); [x, x + col] }).map(geom::num).collect::<Vec<_>>().join(" ")
}

/// `based_on`: a parent the new one is based on (New Parent's Based On).
pub(super) fn new_parent(w: &mut Work<'_>, pages: Option<u32>, prefix: Option<&str>, name: Option<&str>, based_on: Option<&str>) -> Result<(), EditError> {
    let existing = parents(w)?;
    let facing = doc_pref(w, "DocumentPreference", "FacingPages").as_deref() == Some("true");
    let count = pages.unwrap_or(if facing { 2 } else { 1 });
    // InDesign (Pages panel > New Parent): "Number of Pages" is 1 to 10; masterSpreads.add(11) and add(0) fail ("Data is out of range").
    if !(1..=10).contains(&count) {
        return Err(EditError::new("invalid", "a parent has 1 to 10 pages"));
    }
    let prefix = match prefix {
        Some(p) if !p.trim().is_empty() && p.chars().count() <= 4 => p.trim().to_owned(),
        Some(_) => return Err(EditError::new("invalid", "a prefix has 1 to 4 characters")),
        None => free_prefix(&existing.iter().map(|e| e.2.clone()).collect::<Vec<_>>()),
    };
    let base = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Parent").to_owned();
    let num = |e: &str, k: &str| doc_pref(w, e, k).and_then(|v| v.parse::<f64>().ok());
    let (width, height) = (num("DocumentPreference", "PageWidth").unwrap_or(612.0), num("DocumentPreference", "PageHeight").unwrap_or(792.0));
    let m = |k: &str, d: f64| num("MarginPreference", k).unwrap_or(d);
    let (top, bottom, left, right, gutter) = (m("Top", 36.0), m("Bottom", 36.0), m("Left", 36.0), m("Right", 36.0), m("ColumnGutter", 12.0));
    let columns = num("MarginPreference", "ColumnCount").unwrap_or(1.0).max(1.0) as u32;
    let direction = doc_pref(w, "MarginPreference", "ColumnDirection").unwrap_or_else(|| "Horizontal".into());
    // the layout grid of the document's parents (InDesign's named-grid default otherwise)
    let grid = grid_of(w, &existing).unwrap_or_else(|| "<GridDataInformation FontStyle=\"Regular\" PointSize=\"12\" CharacterAki=\"0\" LineAki=\"9\" HorizontalScale=\"100\" VerticalScale=\"100\" LineAlignment=\"LeftOrTopLineJustify\" GridAlignment=\"AlignEmCenter\" CharacterAlignment=\"AlignEmCenter\">\n\t<Properties>\n\t\t<AppliedFont type=\"string\">Minion Pro</AppliedFont>\n\t</Properties>\n</GridDataInformation>".into());
    let id = w.new_id();
    let mut pages_xml = String::new();
    for k in 0..count {
        let pid = w.new_id();
        // the pages are centred on the spine at x = 0 (measured for 1, 2, 3, 4, 5 and 10 pages)
        let tx = (k as f64 - count as f64 / 2.0) * width;
        let grid = reindent(&grid, "\t\t\t");
        pages_xml.push_str(&format!(
            "\t\t<Page Self=\"{pid}\" AppliedAlternateLayout=\"n\" LayoutRule=\"Off\" SnapshotBlendingMode=\"IgnoreLayoutSnapshots\" OptionalPage=\"false\" GeometricBounds=\"0 0 {h} {wd}\" ItemTransform=\"1 0 0 1 {tx} {ty}\" Name=\"{p}\" AppliedTrapPreset=\"TrapPreset/$ID/kDefaultTrapStyleName\" OverrideList=\"\" AppliedMaster=\"n\" MasterPageTransform=\"1 0 0 1 0 0\" TabOrder=\"\" GridStartingPoint=\"TopOutside\" UseMasterGrid=\"true\">\n\t\t\t<Properties>\n\t\t\t\t<PageColor type=\"enumeration\">UseMasterColor</PageColor>\n\t\t\t</Properties>\n\t\t\t<MarginPreference ColumnCount=\"{columns}\" ColumnGutter=\"{g}\" Top=\"{t}\" Bottom=\"{b}\" Left=\"{l}\" Right=\"{r}\" ColumnDirection=\"{direction}\" ColumnsPositions=\"{cp}\" />\n\t\t\t{grid}\n\t\t</Page>\n",
            h = geom::num(height), wd = geom::num(width), tx = geom::num(tx), ty = geom::num(-height / 2.0), p = escape(&prefix),
            g = geom::num(gutter), t = geom::num(top), b = geom::num(bottom), l = geom::num(left), r = geom::num(right),
            cp = column_positions(width - left - right, columns, gutter)));
    }
    let spread = format!(
        "\t<MasterSpread Self=\"{id}\" ItemTransform=\"1 0 0 1 0 0\" OverriddenPageItemProps=\"\" Name=\"{n}\" NamePrefix=\"{p}\" BaseName=\"{b}\" ShowMasterItems=\"true\" PageCount=\"{count}\" PrimaryTextFrame=\"n\">\n\t\t<Properties>\n\t\t\t<PageColor type=\"enumeration\">UseMasterColor</PageColor>\n\t\t</Properties>\n{pages_xml}\t</MasterSpread>\n",
        n = escape(&format!("{prefix}-{base}")), p = escape(&prefix), b = escape(&base));
    add(w, &id, &spread)?;
    match based_on {
        Some(b) if b != "n" => parent_options(w, &id, None, None, Some(Some(b))),
        _ => Ok(()),
    }
}

/// The part for parent `id` with `spread` (its MasterSpread element), listed after the last
/// parent in the designmap.
fn add(w: &mut Work<'_>, id: &str, spread: &str) -> Result<(), EditError> {
    let version = {
        let dm = w.text(DM)?;
        let ix = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
        (0..ix.nodes.len()).find(|&i| ix.local(dm, i) == "Document").and_then(|i| ix.value(dm, i, "DOMVersion")).unwrap_or_else(|| "20.4".into())
    };
    let part = format!("MasterSpreads/MasterSpread_{id}.xml");
    w.add_part(part.clone(), format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:MasterSpread xmlns:idPkg=\"http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging\" DOMVersion=\"{version}\">\n{spread}</idPkg:MasterSpread>\n"))?;
    let dm = w.text(DM)?.to_owned();
    let ix = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let entries: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(&dm, i) == "MasterSpread" && ix.attr(&dm, i, "src").is_some()).collect();
    let entry = format!("<idPkg:MasterSpread src=\"{}\" />", escape(&part));
    let edit = match entries.last() {
        Some(&last) => { let n = &ix.nodes[last]; (n.end..n.end, format!("\n{}{entry}", indent_before(&dm, n.start))) }
        None => {
            let root = ix.roots().find(|&r| ix.local(&dm, r) == "Document").ok_or_else(|| EditError::new("xml", "designmap has no Document"))?;
            let at = ix.nodes[root].close_start;
            (at..at, format!("\t{entry}\n"))
        }
    };
    w.splice(DM, vec![edit])
}

/// A parent page's layout grid, as written (indented from column 0).
fn grid_of(w: &Work<'_>, parents: &[(String, String, String)]) -> Option<String> {
    let (part, _, _) = parents.first()?;
    let t = w.text(part).ok()?;
    let ix = Index::scan(t).ok()?;
    let g = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "GridDataInformation")?;
    let n = &ix.nodes[g];
    let ind = indent_before(t, n.start);
    Some(t[n.start..n.end].lines().map(|l| l.strip_prefix(ind).unwrap_or(l)).collect::<Vec<_>>().join("\n"))
}

/// `text` (lines indented from column 0) with every line after the first indented by `ind`.
fn reindent(text: &str, ind: &str) -> String {
    text.lines().enumerate().map(|(k, l)| if k == 0 { l.to_owned() } else { format!("{ind}{l}") }).collect::<Vec<_>>().join("\n")
}

/// The parent `id`'s part, its text and index, and its MasterSpread node.
fn parent(w: &Work<'_>, id: &str) -> Result<(String, String, Index, usize), EditError> {
    let (part, _, _) = parents(w)?.into_iter().find(|p| p.1 == id).ok_or_else(|| EditError::new("notFound", format!("parent {id}")))?;
    let t = w.text(&part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let m = (0..ix.nodes.len()).find(|&k| ix.local(&t, k) == "MasterSpread" && ix.attr(&t, k, "Self") == Some(id)).ok_or_else(|| EditError::new("xml", "parent spread missing"))?;
    Ok((part, t, ix, m))
}

/// Object ▸ Primary Text Frame (measured in InDesign 2025, menu action 6167): a text frame on a parent whose story is empty can be that
/// parent's primary text frame (the menu is off for a page, a frame with text, a rectangle). The attribute holds the first frame of the
/// frame's thread (`MasterSpread@PrimaryTextFrame`, "n" for none; the earlier primary frame stops being one). Any frame of the thread
/// shows it checked; without `primary` the attribute goes back to "n" when the thread is the primary one (any other thread is left alone).
pub(super) fn set_primary_text_frame(w: &mut Work<'_>, id: &str, primary: bool) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "TextFrame" {
        return Err(EditError::new("invalid", format!("{id} is not a text frame")));
    }
    let mut at = ix.nodes[i].parent;
    while let Some(k) = at.filter(|&k| ix.local(&t, k) != "MasterSpread") {
        at = ix.nodes[k].parent;
    }
    let m = at.ok_or_else(|| EditError::new("invalid", "only a text frame on a parent can be the primary text frame"))?;
    // the first frame of the thread
    let mut head = i;
    while let Some(prev) = ix.attr(&t, head, "PreviousTextFrame").filter(|p| *p != "n") {
        head = ix.by_self(&t, prev).ok_or_else(|| EditError::new("notFound", format!("frame {prev}")))?;
    }
    let head_id = ix.attr(&t, head, "Self").unwrap_or_default().to_owned();
    let current = ix.attr(&t, m, "PrimaryTextFrame");
    if primary {
        let story = ix.value(&t, i, "ParentStory").ok_or_else(|| EditError::new("invalid", "frame without story"))?;
        let (sp, _, _) = w.find(&story)?;
        if !crate::text::contents(w.text(&sp)?, &story)?.is_empty() {
            return Err(EditError::new("invalid", "a frame with text cannot be the primary text frame"));
        }
    }
    let value = if primary { head_id.as_str() } else { "n" };
    if current == Some(value) || (!primary && current != Some(head_id.as_str())) {
        return Ok(());
    }
    w.splice(&part, vec![attr_edit(&ix, &t, m, "PrimaryTextFrame", value)])
}

pub(super) fn duplicate_parent(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (_, t, ix, m) = parent(w, id)?;
    let n = &ix.nodes[m];
    copy_parent(w, &t[n.start..n.end])
}

pub(super) fn copy_parent(w: &mut Work<'_>, src: &str) -> Result<(), EditError> {
    let taken: Vec<String> = parents(w)?.into_iter().map(|p| p.2).collect();
    let stories = super::clipboard::Source::of(w.ed);
    copy_parent_from(w, src, &stories, Vec::new(), &free_prefix(&taken))
}

pub(super) fn copy_parent_from(w: &mut Work<'_>, src: &str, stories: &super::clipboard::Source, mut map: Vec<(String, String)>, prefix: &str) -> Result<(), EditError> {
    let cx = Index::scan(src).map_err(|e| EditError::new("xml", e))?;
    for k in 0..cx.nodes.len() {
        if let Some(s) = cx.attr(src, k, "Self") && !map.iter().any(|(old,_)|old==s) {
            let new = w.new_id();
            map.push((s.to_owned(), new));
        }
    }
    let new_id = cx.attr(src,0,"Self").and_then(|id|map.iter().find(|(old,_)|old==id)).map(|m|m.1.clone()).ok_or_else(|| EditError::new("xml", "parent without id"))?;
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    story_copy::copy_stories(w, stories, &cx, src, &mut map, &mut edits)?;
    let base = cx.value(src, 0, "BaseName").unwrap_or_else(|| "Parent".into());
    for k in 0..cx.nodes.len() {
        if let Some(edit) = clipboard::copied_profile(&cx, src, k) { edits.push(edit); }
        for a in cx.attrs(src, k) {
            let name = &src[a.name.0..a.name.1];
            let v = &src[a.value.0..a.value.1];
            if k == 0 && name == "NamePrefix" {
                edits.push((a.value.0..a.value.1, escape(prefix)));
            } else if k == 0 && name == "Name" {
                edits.push((a.value.0..a.value.1, escape(&format!("{prefix}-{base}"))));
            } else if cx.local(src, k) == "Page" && name == "Name" {
                edits.push((a.value.0..a.value.1, escape(prefix)));
            } else if let Some((_, new)) = map.iter().find(|(old, _)| old == v) {
                edits.push((a.value.0..a.value.1, new.clone()));
            }
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut copy = src.to_owned();
    for (r, s) in edits {
        copy.replace_range(r, &s);
    }
    add(w, &new_id, &format!("\t{copy}\n"))?;
    w.created.push(new_id);
    Ok(())
}

pub(super) fn delete_parent(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (part, t, ix, m) = parent(w, id)?;
    let stories: Vec<String> = (0..ix.nodes.len()).filter(|&k| ix.local(&t, k) == "TextFrame")
        .filter_map(|k| ix.attr(&t, k, "ParentStory").map(str::to_owned)).collect();
    drop((t, ix, m));
    // pages (document and parent) that used it: [None]
    for p in w.parts_named("Spreads/").into_iter().chain(w.parts_named("MasterSpreads/")).filter(|p| *p != part) {
        let t = w.text(&p)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for k in (0..ix.nodes.len()).filter(|&k| ix.local(&t, k) == "Page" && ix.attr(&t, k, "AppliedMaster") == Some(id)) {
            edits.push(attr_edit(&ix, &t, k, "AppliedMaster", "n"));
            edits.push(attr_edit(&ix, &t, k, "MasterPageTransform", &Mat::ID.text()));
            edits.push(attr_edit(&ix, &t, k, "UseMasterGrid", "false"));
        }
        if !edits.is_empty() {
            w.splice(&p, edits)?;
        }
    }
    let dm = w.text(DM)?.to_owned();
    let ix = Index::scan(&dm).map_err(|e| EditError::new("xml", e))?;
    let entry = (0..ix.nodes.len()).find(|&i| ix.local(&dm, i) == "MasterSpread" && ix.value(&dm, i, "src").as_deref() == Some(part.as_str()));
    if let Some(e) = entry {
        w.splice(DM, vec![(element_span(&ix, &dm, e), String::new())])?;
    }
    w.remove_part(&part);
    for s in stories {
        delete_story(w, &s)?;
    }
    Ok(())
}

/// `based_on`: None leaves it, Some(None) is [None], Some(Some(parent)).
pub(super) fn parent_options(w: &mut Work<'_>, id: &str, prefix: Option<&str>, name: Option<&str>, based_on: Option<Option<&str>>) -> Result<(), EditError> {
    if let Some(Some(b)) = based_on {
        if b == id {
            return Err(EditError::new("invalid", "a parent cannot be based on itself"));
        }
        let mut cur = Some(b.to_owned());
        // no loops: the new base must not be based on this parent
        for _ in 0..64 {
            let Some(c) = cur.take() else { break };
            if c == id {
                return Err(EditError::new("invalid", "that parent is based on this one"));
            }
            let (_, t, ix, m) = parent(w, &c)?;
            cur = ix.children(m).iter().find(|&&k| ix.local(&t, k) == "Page").and_then(|&k| ix.attr(&t, k, "AppliedMaster")).filter(|v| *v != "n").map(str::to_owned);
        }
    }
    if let Some(p) = prefix && (p.trim().is_empty() || p.chars().count() > 4) {
        return Err(EditError::new("invalid", "a prefix has 1 to 4 characters"));
    }
    let base_pages: Option<Vec<String>> = match based_on {
        Some(Some(b)) => {
            let (_, t, ix, m) = parent(w, b)?;
            Some(ix.children(m).iter().copied().filter(|&k| ix.local(&t, k) == "Page").map(|k| t[ix.nodes[k].start..ix.nodes[k].end].to_owned()).collect())
        }
        _ => None,
    };
    let (part, t, ix, m) = parent(w, id)?;
    let mut edits = Vec::new();
    let new_prefix = prefix.map(|p| p.trim().to_owned()).or_else(|| ix.value(&t, m, "NamePrefix")).unwrap_or_default();
    let new_base = name.map(|n| n.trim().to_owned()).or_else(|| ix.value(&t, m, "BaseName")).unwrap_or_default();
    if prefix.is_some() || name.is_some() {
        edits.push(attr_edit(&ix, &t, m, "Name", &format!("{new_prefix}-{new_base}")));
        edits.push(attr_edit(&ix, &t, m, "NamePrefix", &new_prefix));
        edits.push(attr_edit(&ix, &t, m, "BaseName", &new_base));
    }
    let pages: Vec<usize> = ix.children(m).iter().copied().filter(|&k| ix.local(&t, k) == "Page").collect();
    for (k, &pg) in pages.iter().enumerate() {
        let n = &ix.nodes[pg];
        let mut xml = t[n.start..n.end].to_owned();
        let set = |xml: &mut String, key: &str, v: &str| {
            if let Ok(px) = Index::scan(xml) { let (r, s) = attr_edit(&px, xml, 0, key, v); xml.replace_range(r, &s); }
        };
        if prefix.is_some() {
            set(&mut xml, "Name", &new_prefix);
        }
        match (based_on, &base_pages) {
            (Some(Some(b)), Some(bp)) if !bp.is_empty() => {
                // the parent page on the same side: the first for a left page, else the last
                // the parent page on the same side of the spine (as `Book::master_properties`)
                let (b0, m0) = (nums_of(&xml, "GeometricBounds"), nums_of(&xml, "ItemTransform"));
                let left = b0.len() == 4 && m0.len() == 6 && m0[4] + b0[1] < -0.001;
                let _ = k;
                let mp = if left { &bp[0] } else { &bp[bp.len() - 1] };
                let (pb, mb) = (nums_of(&xml, "GeometricBounds"), nums_of(mp, "GeometricBounds"));
                let mut tr = Mat::ID;
                if pb.len() == 4 && mb.len() == 4 {
                    tr.0[4] = pb[1] - mb[1];
                    tr.0[5] = pb[0] - mb[0];
                }
                let same = pb.len() == 4 && mb.len() == 4 && (pb[2] - pb[0] - (mb[2] - mb[0])).abs() < 1e-6 && (pb[3] - pb[1] - (mb[3] - mb[1])).abs() < 1e-6;
                set(&mut xml, "AppliedMaster", b);
                set(&mut xml, "MasterPageTransform", &tr.text());
                set(&mut xml, "UseMasterGrid", bool_text(same).as_str());
                super::pages::copy_layout(&mut xml, mp);
            }
            (Some(None), _) => {
                set(&mut xml, "AppliedMaster", "n");
                set(&mut xml, "MasterPageTransform", &Mat::ID.text());
                set(&mut xml, "UseMasterGrid", "false");
            }
            _ => {}
        }
        if xml != t[n.start..n.end] {
            edits.push((n.start..n.end, xml));
        }
    }
    if edits.is_empty() {
        return Ok(());
    }
    w.splice(&part, edits)
}

fn nums_of(xml: &str, key: &str) -> Vec<f64> {
    Index::scan(xml).ok().and_then(|ix| ix.attr(xml, 0, key).map(|v| v.split_whitespace().filter_map(|n| n.parse().ok()).collect())).unwrap_or_default()
}
