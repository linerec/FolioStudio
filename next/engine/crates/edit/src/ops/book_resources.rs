use super::{clipboard::Source, xdoc::Importer, *};
use std::collections::BTreeMap;

const KINDS: &[(&str, &str, &str)] = &[
    ("numberingList", "designmap.xml", "NumberingList"),
    ("conditionalText", "designmap.xml", "Condition"),
    ("conditionalText", "designmap.xml", "ConditionSet"),
    ("textVariable", "designmap.xml", "TextVariable"),
    ("trapStyle", "Resources/Styles.xml", "TrapPreset"),
    ("tableOfContentStyle", "Resources/Styles.xml", "TOCStyle"),
    ("crossReferenceFormat", "designmap.xml", "CrossReferenceFormat"),
];

fn scan(t: &str) -> Result<Index, EditError> { Index::scan(t).map_err(|e| EditError::new("xml", e)) }
fn attr(xml: &mut String, key: &str, value: &str) -> Result<(), EditError> {
    let ix = scan(xml)?;
    let (range, value) = attr_edit(&ix, xml, 0, key, value);
    xml.replace_range(range, &value);
    Ok(())
}

pub(crate) fn sync(w: &mut Work<'_>, kinds: &[String], parts: &BTreeMap<String, String>) -> Result<(), EditError> {
    if kinds.iter().any(|k| k != "swatch" && k != "masterPage" && !KINDS.iter().any(|r| r.0 == k)) {
        return Err(EditError::new("invalid", "unknown book resource kind"));
    }
    let source = Source::of(&crate::Editor::new(parts.clone()));
    let mut importer = Importer::new(&source);
    if kinds.iter().any(|k| k == "swatch") { importer.book_colors(w)?; }
    if kinds.iter().any(|k| k == "masterPage") { super::parent_sync::sync(w, parts)?; }
    // which documents' conditions keep a colour of their own, decided on the documents as they are before any condition changes
    let survives = if kinds.iter().any(|k| k == "conditionalText") {
        condition_colors_kept(parts.get("designmap.xml").map(String::as_str).unwrap_or_default(), w.text("designmap.xml")?)?
    } else { BTreeMap::new() };
    for &(key, part, tag) in KINDS {
        if !kinds.iter().any(|k| k == key) { continue; }
        let Some(t) = parts.get(part) else { continue };
        let ix = scan(t)?;
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) != tag { continue; }
            let Some(name) = ix.attr(t, i, "Name") else { continue };
            if name.starts_with("$ID/") { continue; }
            let target = w.text(part)?.to_owned();
            let tx = scan(&target)?;
            let existing = (0..tx.nodes.len()).find(|&j| tx.local(&target, j) == tag && tx.attr(&target, j, "Name") == Some(name));
            if tag == "Condition" {
                // Book > Synchronize, conditions (oracle bookx-c3-cond/cond2/cond3/cond4, InDesign 2025): the document's condition takes
                // the source's indicator method, underline appearance and visibility; a UI colour of the source comes too. A source
                // colour of its own (r,g,b) does not: InDesign unsets the document's colour when the method or appearance differ, and
                // otherwise keeps it or unsets it by where the conditions stand in the two documents (`condition_colors_kept`,
                // oracle bookx-c4-cond5..22).
                let plain = crate::xml::unescape(name);
                if existing.is_none() { conditions::create(w, &plain)?; }
                let mut options = BTreeMap::new();
                for (key, attribute) in [("method", "IndicatorMethod"), ("appearance", "UnderlineIndicatorAppearance")] {
                    if let Some(v) = ix.attr(t, i, attribute) { options.insert(key.to_owned(), v.to_owned()); }
                }
                let element = &t[ix.nodes[i].start..ix.nodes[i].end];
                if let Some(color) = ui_color(element) { options.insert("color".to_owned(), color); }
                // a colour of its own and a different method or appearance: the document's colour goes (6 of 6 measured)
                let differs = existing.is_some_and(|j| ["IndicatorMethod", "UnderlineIndicatorAppearance"].iter().any(|a| tx.attr(&target, j, a) != ix.attr(t, i, a)));
                if !options.is_empty() { conditions::set_options(w, &plain, &options)?; }
                if existing.is_some() && ui_color(element).is_none() && (differs || !survives.get(name).copied().unwrap_or(false)) { conditions::clear_color(w, &plain)?; }
                conditions::set_visible(w, &plain, &(ix.attr(t, i, "Visible") != Some("false")))?;
                continue;
            }
            let mut xml = t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
            if tag == "TOCStyle" { xml = styles_present_first(w, &xml)?; }
            let old_id = ix.attr(t, i, "Self").unwrap_or_default();
            let id = existing.and_then(|j| tx.attr(&target, j, "Self")).map(str::to_owned)
                .unwrap_or_else(|| if tag == "CrossReferenceFormat" { w.new_id() } else { old_id.to_owned() });
            // Keep existing references valid and allocate every nested object's identity locally. A replaced definition keeps its
            // nested ids when it has as many as the source's (so a second synchronization writes the same bytes).
            let ex = scan(&xml)?;
            let kept: Vec<String> = existing.map(|j| (j + 1..tx.nodes.len()).take_while(|&k| tx.nodes[k].start < tx.nodes[j].end)
                .filter_map(|k| tx.attr(&target, k, "Self").map(str::to_owned)).collect()).unwrap_or_default();
            let nested = (1..ex.nodes.len()).filter(|&j| ex.attr(&xml, j, "Self").is_some()).count();
            let reuse = existing.is_some() && kept.len() == nested;
            let mut edits = Vec::new();
            let mut at = 0;
            for j in 0..ex.nodes.len() {
                if let Some(old) = ex.attr(&xml, j, "Self") {
                    let new = if j == 0 { id.clone() } else if reuse { at += 1; kept[at - 1].clone() } else if let Some(suffix) = old.strip_prefix(old_id) { format!("{id}{suffix}") } else { w.new_id() };
                    edits.push(attr_edit(&ex, &xml, j, "Self", &crate::xml::unescape(&new)));
                }
            }
            for (range, value) in edits.into_iter().rev() { xml.replace_range(range, &value); }
            xml = importer.rewrite(w, &xml)?;
            let target = w.text(part)?.to_owned();
            let tx = scan(&target)?;
            if let Some(j) = (0..tx.nodes.len()).find(|&j| tx.local(&target, j) == tag && tx.attr(&target, j, "Name") == Some(name)) {
                w.splice(part, vec![(tx.nodes[j].start..tx.nodes[j].end, xml)])?;
            } else {
                let anchor = |wanted: &str| (0..tx.nodes.len()).rfind(|&j| tx.local(&target, j) == wanted).map(|j| tx.nodes[j].end);
                // the first condition set of a document goes after its conditions
                let at = anchor(tag).or_else(|| if tag == "ConditionSet" { anchor("Condition") } else { None }).unwrap_or(tx.nodes[0].close_start);
                w.splice(part, vec![(at..at, format!("\n{xml}"))])?;
            }
        }
    }
    let stories: Vec<_> = w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
    for story in stories { w.recompose(&story); }
    Ok(())
}

pub(super) fn keep_slot(xml: &mut String, target: &str, ix: &Index, node: usize) -> Result<(), EditError> {
    if let Some(slot) = ix.attr(target, node, "SwatchColorGroupReference") { attr(xml, "SwatchColorGroupReference", &crate::xml::unescape(slot))?; }
    Ok(())
}

/// Book > Synchronize, the condition of the document that has a same-named condition in the source whose indicator colour is an (r,g,b) of
/// its own (probes bookx-c4-cond5..22 and bookx-c3-cond..cond4, 40 InDesign 2025 cases, next/oracle/rules/bookx-c4.ko.md): the document's
/// colour stays only when it is one of its own too, the condition stands at the same place in both documents' lists (the order they were
/// made in, which the IDML keeps), and up to and including that place both documents hold as many conditions with a UI colour. Any
/// other case unsets it. Name => the colour stays.
fn condition_colors_kept(source: &str, target: &str) -> Result<BTreeMap<String, bool>, EditError> {
    let list = |xml: &str| -> Result<Vec<(String, bool)>, EditError> {
        let ix = scan(xml)?;
        Ok((0..ix.nodes.len()).filter(|&i| ix.local(xml, i) == "Condition").filter_map(|i| {
            Some((ix.attr(xml, i, "Name")?.to_owned(), ui_color(&xml[ix.nodes[i].start..ix.nodes[i].end]).is_some()))
        }).collect())
    };
    let (s, t) = (list(source)?, list(target)?);
    let ui_up_to = |l: &[(String, bool)], at: usize| l[..=at].iter().filter(|c| c.1).count();
    let mut out = BTreeMap::new();
    for (i, (name, _)) in s.iter().enumerate() {
        let Some(j) = t.iter().position(|c| &c.0 == name) else { continue };
        out.insert(name.clone(), i == j && !t[j].1 && ui_up_to(&s, i) == ui_up_to(&t, j));
    }
    Ok(out)
}

/// The UI colour (`Gold`) the condition element names, if its indicator colour is one (not an `r,g,b` list).
fn ui_color(element: &str) -> Option<String> {
    let start = element.find("<IndicatorColor type=\"enumeration\">")? + "<IndicatorColor type=\"enumeration\">".len();
    let end = element[start..].find("</IndicatorColor>")?;
    Some(element[start..start + end].trim().to_owned())
}

/// Book > Synchronize, table of contents styles (oracle bookx-c3-toc, InDesign 2025): an entry names a paragraph style, and one whose
/// style the document lacks when the TOC style comes in is put back after the others once the style arrives; so the entries whose
/// styles the document has keep the source's order and come first, the rest follow in the source's order.
fn styles_present_first(w: &mut Work<'_>, xml: &str) -> Result<String, EditError> {
    let styles = w.text("Resources/Styles.xml")?.to_owned();
    let sx = scan(&styles)?;
    let have: std::collections::BTreeSet<String> = (0..sx.nodes.len()).filter(|&i| sx.local(&styles, i) == "ParagraphStyle")
        .filter_map(|i| sx.attr(&styles, i, "Name").map(str::to_owned)).collect();
    let ix = scan(xml)?;
    let entries: Vec<usize> = ix.children(0).iter().copied().filter(|&c| ix.local(xml, c) == "TOCStyleEntry").collect();
    if entries.len() < 2 { return Ok(xml.to_owned()); }
    let separator = &xml[ix.nodes[entries[0]].end..ix.nodes[entries[1]].start];
    let text = |c: usize| &xml[ix.nodes[c].start..ix.nodes[c].end];
    let present = |c: usize| ix.attr(xml, c, "Name").is_some_and(|n| have.contains(n));
    let ordered: Vec<&str> = entries.iter().copied().filter(|&c| present(c)).chain(entries.iter().copied().filter(|&c| !present(c))).map(text).collect();
    let (first, last) = (ix.nodes[entries[0]].start, ix.nodes[*entries.last().unwrap_or(&entries[0])].end);
    Ok(format!("{}{}{}", &xml[..first], ordered.join(separator), &xml[last..]))
}
