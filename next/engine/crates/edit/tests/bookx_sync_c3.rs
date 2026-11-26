//! Book > Synchronize for the resource kinds bookx-c2 left open (InDesign 2025, probe `bookx-c3-sync-probe.jsx`, goldens
//! `oracle/goldens/bookx-c3/<group>/{before-0,before-1,after-1}`: the style source, the target, the target after).
//! Every element of the kind in InDesign's result must be in Folio's result with the same definition (ids and Swatches
//! panel slots aside), and Folio must have no other: so replace-by-name, add and keep-the-rest are all checked at once.
use folio_edit::{Editor, commands::RegisteredCommand, xml::Index};
use std::{collections::{BTreeMap, BTreeSet}, path::Path};

const PARTS: [&str; 3] = ["designmap.xml", "Resources/Styles.xml", "Resources/Graphic.xml"];

fn measured(group: &str, name: &str) -> BTreeMap<String, String> {
    PARTS.into_iter().map(|part| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/bookx-c3").join(group).join(name).join(part);
        (part.into(), std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }).collect()
}

/// Every element of `tag` by name, as written minus its ids and Swatches panel slot (several with one name are sorted).
fn definitions(t: &str, tag: &str) -> BTreeMap<String, Vec<String>> {
    let ix = Index::scan(t).unwrap();
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in 0..ix.nodes.len() {
        if ix.local(t, n) != tag { continue; }
        let Some(name) = ix.attr(t, n, "Name").or_else(|| ix.attr(t, n, "Self")) else { continue };
        let mut xml = t[ix.nodes[n].start..ix.nodes[n].end].to_owned();
        let inner = Index::scan(&xml).unwrap();
        let mut remove = Vec::new();
        for i in 0..inner.nodes.len() {
            for a in inner.attrs(&xml, i) {
                if ["Self", "SwatchColorGroupReference"].contains(&&xml[a.name.0..a.name.1]) { remove.push(a.name.0..a.value.1 + 1); }
            }
        }
        for range in remove.into_iter().rev() { xml.replace_range(range, ""); }
        out.entry(name.to_owned()).or_default().push(xml.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    for v in out.values_mut() { v.sort(); }
    out
}

fn assert_ids_unique(parts: &BTreeMap<String, String>) {
    let mut ids = BTreeSet::new();
    for (part, t) in parts {
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if let Some(id) = ix.attr(t, i, "Self") { assert!(ids.insert(id.to_owned()), "duplicate {id} in {part}"); }
        }
    }
}

/// Runs the synchronization of `kinds` on the golden target and compares each (part, tag) with InDesign's result.
fn check(group: &str, kinds: &[&str], what: &[(&str, &str)]) {
    let before = measured(group, "before-1");
    let mut ed = Editor::new(before.clone());
    let command = RegisteredCommand::SyncBookResources { kinds: kinds.iter().map(|k| k.to_string()).collect(), parts: measured(group, "before-0") };
    ed.apply("sync", &[command.clone().into()]).unwrap();
    let expected = measured(group, "after-1");
    for (part, tag) in what {
        let (folio, indesign) = (definitions(ed.part(part).unwrap(), tag), definitions(&expected[*part], tag));
        assert_eq!(folio.keys().collect::<Vec<_>>(), indesign.keys().collect::<Vec<_>>(), "{group}: the {tag} names");
        for (name, defs) in &indesign {
            assert_eq!(&folio[name], defs, "{group}: {tag}/{name}");
        }
    }
    let after = ed.parts().clone();
    assert_ids_unique(&after);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
    ed.apply("again", &[command.into()]).unwrap();
    assert_eq!(ed.parts(), &after, "{group}: a second synchronization changes nothing");
}

#[test]
fn every_swatch_kind_matches_indesign() {
    check("swatch", &["swatch"], &[
        ("Resources/Graphic.xml", "Color"), ("Resources/Graphic.xml", "Tint"), ("Resources/Graphic.xml", "Gradient"),
        ("Resources/Graphic.xml", "MixedInk"), ("Resources/Graphic.xml", "MixedInkGroup"), ("Resources/Graphic.xml", "Ink"),
    ]);
}

#[test]
fn the_paper_swatch_is_never_carried() {
    let source = measured("swatch", "before-0");
    let before = measured("swatch", "before-1");
    let paper = |parts: &BTreeMap<String, String>| definitions(&parts["Resources/Graphic.xml"], "Color")["Paper"].clone();
    assert_ne!(paper(&source), paper(&before), "the probe edited the source's [Paper]");
    let mut ed = Editor::new(before.clone());
    ed.apply("sync", &[RegisteredCommand::SyncBookResources { kinds: vec!["swatch".into()], parts: source }.into()]).unwrap();
    assert_eq!(paper(ed.parts()), paper(&before));
}

#[test]
fn all_twelve_text_variable_types_match_indesign() {
    check("var", &["textVariable"], &[("designmap.xml", "TextVariable")]);
}

#[test]
fn cross_reference_formats_with_every_building_block_match_indesign() {
    check("xref", &["crossReferenceFormat"], &[("designmap.xml", "CrossReferenceFormat")]);
}

/// name -> (method, underline appearance, visible, indicator colour as (type, text)) of every condition in a designmap.
fn conditions(t: &str) -> BTreeMap<String, (String, String, String, Option<(String, String)>)> {
    let ix = Index::scan(t).unwrap();
    let mut out = BTreeMap::new();
    for n in 0..ix.nodes.len() {
        if ix.local(t, n) != "Condition" { continue; }
        let attr = |k: &str| ix.attr(t, n, k).unwrap_or_default().to_owned();
        let color = (n + 1..ix.nodes.len()).take_while(|&k| ix.nodes[k].start < ix.nodes[n].end)
            .find(|&k| ix.local(t, k) == "IndicatorColor")
            .map(|k| (ix.attr(t, k, "type").unwrap_or_default().to_owned(), t[ix.nodes[k].open_end..ix.nodes[k].close_start].split_whitespace().collect::<Vec<_>>().join(" ")));
        out.insert(attr("Name"), (attr("IndicatorMethod"), attr("UnderlineIndicatorAppearance"), attr("Visible"), color));
    }
    out
}

/// Conditions: what InDesign carries from the source (method, appearance, visibility, a UI colour, the sets) is checked in each of
/// the measured groups, and the colour a source colour of its own leaves in the document: cond..cond4 (bookx-c3) and cond5..cond22 (bookx-c4,
/// the two documents making the same conditions in different orders, with UI colours in different places), rule in `condition_colors_kept`.
#[test]
fn conditions_and_condition_sets_match_indesign() {
    let groups = ["cond", "cond2", "cond3", "cond4", "cond5", "cond6", "cond7", "cond8", "cond9", "cond10", "cond11", "cond12", "cond13", "cond14", "cond15", "cond16", "cond17", "cond18", "cond19", "cond20", "cond21", "cond22", "idml-a", "idml-b"];
    for group in groups {
        let (mut source, mut target) = (measured(group, "before-0"), measured(group, "before-1"));
        // InDesign's IDML lists conditions by name; the order they were made in (probe code) is what decides whether one keeps its own
        // colour, and an IDML opened in InDesign is made in the order of the file (probe bookx-c4-idml): the documents go in that order
        let (made_source, made_target) = made_in_order(group);
        if let Some(order) = made_source { in_order(&mut source, order); }
        if let Some(order) = made_target { in_order(&mut target, order); }
        let mut ed = Editor::new(target.clone());
        let command = RegisteredCommand::SyncBookResources { kinds: vec!["conditionalText".into()], parts: source.clone() };
        ed.apply("sync", &[command.clone().into()]).unwrap();
        let (folio, indesign) = (conditions(ed.part("designmap.xml").unwrap()), conditions(&measured(group, "after-1")["designmap.xml"]));
        assert_eq!(folio.keys().collect::<Vec<_>>(), indesign.keys().collect::<Vec<_>>(), "{group}: condition names");
        for (name, want) in &indesign {
            let got = &folio[name];
            assert_eq!((&got.0, &got.1, &got.2), (&want.0, &want.1, &want.2), "{group}: {name} method, appearance, visible");
            assert_eq!(got.3, want.3, "{group}: {name} colour");
        }
        assert_eq!(definitions(ed.part("designmap.xml").unwrap(), "ConditionSet"), definitions(&measured(group, "after-1")["designmap.xml"], "ConditionSet"), "{group}: condition sets");
        let after = ed.parts().clone();
        assert_ids_unique(&after);
        // a synchronization that finds nothing to change (cond8: every colour kept) is no step to undo
        if after != target {
            ed.undo().unwrap();
            assert_eq!(ed.parts(), &target);
            ed.redo().unwrap();
        }
        ed.apply("again", &[command.into()]).unwrap();
        // InDesign's second Synchronize unsets the colour it gave a source-only condition of its own colour (a UI colour; probe bookx-c4-cond17 after-2):
        // there the second result is InDesign's own, elsewhere it changes nothing
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/bookx-c3").join(group).join("after-2");
        if golden.exists() {
            let (folio, indesign) = (conditions(ed.part("designmap.xml").unwrap()), conditions(&measured(group, "after-2")["designmap.xml"]));
            for (name, want) in &indesign { assert_eq!(folio[name].3, want.3, "{group}: {name} colour after a second synchronization"); }
        } else if group != "cond" {
            assert_eq!(ed.parts(), &after, "{group}: a second synchronization changes nothing");
        }
    }
}

/// The order the probe made the conditions in, (source, target); none: the order of the IDML (which is also the order they were made in).
fn made_in_order(group: &str) -> (Option<&'static [&'static str]>, Option<&'static [&'static str]>) {
    const A_REVERSED: &[&str] = &["A6", "A5", "A4", "A3", "A2", "A1"];
    match group {
        "cond" => (Some(&["CShared", "CSame", "CSourceOnly", "CEnum"]), Some(&["CShared", "CSame", "CTargetOnly"])),
        "cond5" => (Some(&["N5", "N4", "N3", "N2", "N1"]), None),
        "cond6" => (Some(&["R4", "R3", "R2", "R1"]), None),
        "cond12" | "cond13" => (Some(A_REVERSED), None),
        "cond15" => (None, Some(A_REVERSED)),
        "cond16" => (Some(&["A1", "A2", "A3", "A4", "A6", "A5"]), None),
        "cond17" => (Some(&["A1", "A2", "X", "A3", "A4"]), None),
        "cond18" => (None, Some(&["A1", "Z", "A2", "A3"])),
        "cond21" => (None, Some(&["A1", "A3", "A2", "A4"])),
        _ => (None, None),
    }
}

/// The document's `Condition` elements (they stand next to each other in the designmap) in the order given; any not named keep their order after.
fn in_order(parts: &mut BTreeMap<String, String>, order: &[&str]) {
    let t = parts["designmap.xml"].clone();
    let ix = Index::scan(&t).unwrap();
    let nodes: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "Condition").collect();
    let mut by_name: Vec<(String, String)> = nodes.iter().map(|&i| (ix.attr(&t, i, "Name").unwrap().to_owned(), t[ix.nodes[i].start..ix.nodes[i].end].to_owned())).collect();
    by_name.sort_by_key(|(name, _)| order.iter().position(|o| o == name).unwrap_or(usize::MAX));
    let joined = by_name.iter().map(|c| c.1.as_str()).collect::<Vec<_>>().join("
	");
    let (first, last) = (ix.nodes[nodes[0]].start, ix.nodes[*nodes.last().unwrap()].end);
    parts.insert("designmap.xml".into(), format!("{}{}{}", &t[..first], joined, &t[last..]));
}

#[test]
fn table_of_contents_styles_match_indesign() {
    check("toc", &["tableOfContentStyle"], &[("Resources/Styles.xml", "TOCStyle")]);
}

/// Every file of a golden (designmap, the parents, the spread, the stories…) by its part name.
fn measured_all(group: &str, name: &str) -> BTreeMap<String, String> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = format!("{prefix}{}", path.file_name().unwrap().to_string_lossy());
            if path.is_dir() { walk(&path, &format!("{name}/"), out); } else { out.insert(name, std::fs::read_to_string(&path).unwrap()); }
        }
    }
    let mut out = BTreeMap::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/bookx-c3").join(group).join(name), "", &mut out);
    out
}

/// The text with every generated id (`u15b`) replaced by the number of its first appearance, so two documents' copies compare.
fn without_ids(t: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut out = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String, seen: &mut Vec<String>| {
        let id = token.len() >= 3 && token.starts_with('u') && token[1..].chars().all(|c| c.is_ascii_hexdigit());
        if id {
            let at = seen.iter().position(|s| s == token).unwrap_or_else(|| { seen.push(token.clone()); seen.len() - 1 });
            out.push_str(&format!("#{at}"));
        } else { out.push_str(token); }
        token.clear();
    };
    for c in t.chars() {
        if c.is_ascii_alphanumeric() { token.push(c); } else { flush(&mut token, &mut out, &mut seen); out.push(c); }
    }
    flush(&mut token, &mut out, &mut seen);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The parents of a document by name: the text of each, ids aside.
fn parents_of(parts: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let dm = &parts["designmap.xml"];
    let ix = Index::scan(dm).unwrap();
    let mut out = BTreeMap::new();
    for i in (0..ix.nodes.len()).filter(|&i| ix.local(dm, i) == "MasterSpread") {
        let part = ix.attr(dm, i, "src").unwrap();
        let t = &parts[part];
        let px = Index::scan(t).unwrap();
        let m = (0..px.nodes.len()).find(|&k| px.local(t, k) == "MasterSpread" && px.attr(t, k, "Self").is_some()).unwrap();
        let xml = &t[px.nodes[m].start..px.nodes[m].end];
        // ids are numbered per parent (the stories of its text frames follow in the same numbering)
        let stories: String = (0..px.nodes.len()).filter(|&k| px.local(t, k) == "TextFrame").filter_map(|k| px.attr(t, k, "ParentStory"))
            .map(|s| parts.get(&format!("Stories/Story_{s}.xml")).cloned().unwrap_or_default()).collect::<Vec<_>>().join("\n");
        out.insert(px.attr(t, m, "Name").unwrap().to_owned(), without_ids(&format!("{xml}\n{stories}")));
    }
    out
}

#[test]
fn parent_pages_are_replaced_added_and_kept_as_indesign_does() {
    let (source, target) = (measured_all("master", "before-0"), measured_all("master", "before-1"));
    // the probe made the shared parent differ, and gave each side a parent of its own
    let (from, into) = (parents_of(&source), parents_of(&target));
    assert_ne!(from["A-Parent"], into["A-Parent"]);
    assert!(from.contains_key("C-SrcOnly") && !into.contains_key("C-SrcOnly") && into.contains_key("B-TgtOnly"));
    let mut ed = Editor::new(target.clone());
    let command = RegisteredCommand::SyncBookResources { kinds: vec!["masterPage".into()], parts: source };
    ed.apply("sync", &[command.into()]).unwrap();
    let (folio, indesign) = (parents_of(ed.parts()), parents_of(&measured_all("master", "after-1")));
    assert_eq!(folio.keys().collect::<Vec<_>>(), indesign.keys().collect::<Vec<_>>(), "parent names");
    for (name, want) in &indesign { assert_eq!(&folio[name], want, "parent {name}"); }
    assert_eq!(folio["A-Parent"], from["A-Parent"], "the shared parent is the source's");
    assert_eq!(folio["B-TgtOnly"], into["B-TgtOnly"], "the document's own parent stays");
    // the document's pages still have a parent that exists
    let after = ed.parts().clone();
    assert_ids_unique(&after.iter().filter(|(p, _)| p.as_str() == "designmap.xml" || p.starts_with("MasterSpreads/") || p.starts_with("Spreads/")).map(|(p, t)| (p.clone(), t.clone())).collect());
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &target);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
}

/// The element kinds of Graphic.xml in order, a run of one kind written once (InDesign: Color, Ink, PastedSmoothShade, Swatch, Gradient…).
fn kind_runs(t: &str) -> Vec<String> {
    let ix = Index::scan(t).unwrap();
    let root = ix.roots().next().unwrap();
    let mut out: Vec<String> = Vec::new();
    for &c in ix.children(root) {
        let kind = ix.local(t, c).to_owned();
        if out.last() != Some(&kind) { out.push(kind); }
    }
    out
}

/// A document File > New made has no Ink in its Graphic.xml (before 2026-11-20 it had none of the four process inks): Synchronize
/// with a style source that has spot colours stopped on it with "Graphic.xml has no Ink" (mcp-c6). The spot colours come with their
/// Inks all the same, and the four process inks go in first, after the last colour where InDesign has them (oracle
/// `newdoc/reference/inks-new-document.json`), so the spot colours are the fifth ink on, as in InDesign: here the very Inks of the
/// style source (the probe's source has the four process inks and four spot inks).
#[test]
fn a_document_without_any_ink_takes_spot_colours_with_their_inks() {
    let source = measured("swatch", "before-0");
    let mut target = measured("swatch", "before-1");
    let graphic = target.get_mut("Resources/Graphic.xml").unwrap();
    *graphic = graphic.lines().filter(|l| !l.contains("<Ink ") && !(l.contains("<Color ") && l.contains("Model=\"Spot\""))).collect::<Vec<_>>().join("\n") + "\n";
    assert!(!graphic.contains("<Ink ") && !graphic.contains("Model=\"Spot\""), "the target is a document with no ink and no spot colour");
    let mut ed = Editor::new(target.clone());
    let command = RegisteredCommand::SyncBookResources { kinds: vec!["swatch".into()], parts: source.clone() };
    ed.apply("sync", &[command.clone().into()]).unwrap();
    let after = ed.parts().clone();
    let inks = |parts: &BTreeMap<String, String>| definitions(&parts["Resources/Graphic.xml"], "Ink");
    assert_eq!(inks(&after), inks(&source), "the four process inks, then the source's spot inks in order");
    assert_eq!(kind_runs(&after["Resources/Graphic.xml"]).iter().position(|k| k == "Ink"), kind_runs(&after["Resources/Graphic.xml"]).iter().position(|k| k == "Color").map(|c| c + 1), "the Inks follow the Colors");
    assert_eq!(kind_runs(&after["Resources/Graphic.xml"]).iter().filter(|k| *k == "Ink").count(), 1, "one run of Inks");
    assert_ids_unique(&after);
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &target);
    ed.redo().unwrap();
    assert_eq!(ed.parts(), &after);
    ed.apply("again", &[command.into()]).unwrap();
    assert_eq!(ed.parts(), &after, "a second synchronization changes nothing");
}
