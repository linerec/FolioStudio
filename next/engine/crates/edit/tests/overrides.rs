//! Parent items on document pages (oracle pages-override measures the rectangle InDesign's scripting can override; the rest of
//! the kinds, Override All and the refusals are checked here on the same fixture): copies, the page's OverrideList, text copied,
//! stacking, Remove All Local Overrides, Detach, undo.
use folio_edit::{Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

struct Doc {
    ed: Editor,
    /// `oracle.id` label → Self
    ids: BTreeMap<String, String>,
    /// document pages, in order
    pages: Vec<String>,
}

fn fixture() -> Doc {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/pages-parent.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    let mut d = Doc { ed: Editor::new(parts), ids: BTreeMap::new(), pages: Vec::new() };
    d.scan();
    d
}

impl Doc {
    fn scan(&mut self) {
        self.ids.clear();
        self.pages.clear();
        for (name, t) in self.ed.parts() {
            if !name.starts_with("Spreads/") && !name.starts_with("MasterSpreads/") {
                continue;
            }
            let ix = Index::scan(t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some("oracle.id") {
                    let owner = ix.ancestors(i).find(|&a| ix.attr(t, a, "Self").is_some()).unwrap();
                    // a copy of a parent item carries its label: the parent's one is the one in MasterSpreads
                    if name.starts_with("MasterSpreads/") || !self.ids.contains_key(&ix.value(t, i, "Value").unwrap()) {
                        self.ids.insert(ix.value(t, i, "Value").unwrap(), ix.attr(t, owner, "Self").unwrap().to_owned());
                    }
                }
            }
        }
        let mut spreads: Vec<(String, String)> = self.ed.parts().iter().filter(|(n, _)| n.starts_with("Spreads/")).map(|(n, t)| (n.clone(), t.clone())).collect();
        // designmap order
        let dm = self.ed.part("designmap.xml").unwrap().to_owned();
        let dx = Index::scan(&dm).unwrap();
        let order: Vec<String> = (0..dx.nodes.len()).filter(|&i| dx.local(&dm, i) == "Spread").filter_map(|i| dx.attr(&dm, i, "src").map(str::to_owned)).collect();
        spreads.sort_by_key(|(n, _)| order.iter().position(|o| o == n));
        for (_, t) in spreads {
            let ix = Index::scan(&t).unwrap();
            self.pages.extend((0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "Page").filter_map(|i| ix.attr(&t, i, "Self").map(str::to_owned)));
        }
    }

    fn id(&self, label: &str) -> String {
        self.ids[label].clone()
    }

    fn apply(&mut self, c: Command) -> Result<Vec<String>, String> {
        let r = self.ed.apply("test", &[c]).map(|o| o.created).map_err(|e| e.to_string());
        self.scan();
        r
    }

    /// The page's OverrideList pairs.
    fn pairs(&self, page: &str) -> Vec<(String, String)> {
        for (name, t) in self.ed.parts() {
            if !name.starts_with("Spreads/") {
                continue;
            }
            let ix = Index::scan(t).unwrap();
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page" && ix.attr(t, i, "Self") == Some(page)) {
                let v: Vec<&str> = ix.attr(t, i, "OverrideList").unwrap_or("").split_whitespace().collect();
                return v.chunks(2).map(|c| (c[0].to_owned(), c[1].to_owned())).collect();
            }
        }
        panic!("no page {page}");
    }

    /// The ids of the items directly in the spread that holds `page`, in stacking order.
    fn spread_items(&self, page: &str) -> Vec<(String, String)> {
        for (name, t) in self.ed.parts() {
            if !name.starts_with("Spreads/") || !t.contains(&format!("Self=\"{page}\"")) {
                continue;
            }
            let ix = Index::scan(t).unwrap();
            let spread = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Spread" && ix.attr(t, i, "Self").is_some()).unwrap();
            return ix.children(spread).iter().filter(|&&c| matches!(ix.local(t, c), "Rectangle" | "Oval" | "TextFrame" | "GraphicLine" | "Group" | "Polygon")).map(|&c| (ix.local(t, c).to_owned(), ix.attr(t, c, "Self").unwrap().to_owned())).collect();
        }
        panic!("no page {page}");
    }

    fn attr(&self, id: &str, name: &str) -> Option<String> {
        for t in self.ed.parts().values() {
            if !t.contains(&format!(" Self=\"{id}\"")) {
                continue;
            }
            let ix = Index::scan(t).unwrap();
            if let Some(i) = ix.by_self(t, id) {
                return ix.value(t, i, name);
            }
        }
        panic!("no item {id}");
    }
}

fn over(d: &Doc, page: usize, ids: &[&str]) -> Command {
    Command::OverrideParentItems { page: d.pages[page].clone(), ids: ids.iter().map(|l| d.id(l)).collect() }
}

#[test]
fn override_all_copies_what_shows_on_the_page_in_the_parents_stacking_order() {
    let mut d = fixture();
    let created = d.apply(over(&d, 4, &[])).unwrap();
    // the right parent page: rect, rect2, oval, text, line, group (the fixed rectangle does not allow it)
    assert_eq!(created.len(), 6, "{created:?}");
    let pairs = d.pairs(&d.pages[4]);
    assert_eq!(pairs.len(), 6);
    let expected = ["parent-rect-R", "parent-rect2-R", "parent-oval-R", "parent-text-R", "parent-line-R", "parent-group-R"];
    for (k, label) in expected.iter().enumerate() {
        assert!(pairs.iter().any(|(m, _)| *m == d.ids[*label]), "{label} not overridden");
        // back to front like the parent, then the page's own box
        assert_eq!(d.spread_items(&d.pages[4])[k].1, created.iter().find(|c| pairs.iter().any(|(m, cc)| cc == *c && *m == d.ids[*label])).unwrap().clone(), "stacking of {label}");
    }
    assert_eq!(d.spread_items(&d.pages[4]).last().unwrap().1, d.ids["box-4"]);
    // the copies sit where the parent's items do on the page (the parents' and the page's geometry agree) and carry no AllowOverrides
    for (m, c) in &pairs {
        assert_eq!(d.attr(c, "ItemTransform"), d.attr(m, "ItemTransform"));
        assert_eq!(d.attr(c, "AllowOverrides"), None);
        assert_ne!(c, m);
    }
}

#[test]
fn a_left_hand_page_takes_the_left_parent_pages_items() {
    let mut d = fixture();
    let created = d.apply(over(&d, 1, &[])).unwrap();
    assert_eq!(created.len(), 6);
    let pairs = d.pairs(&d.pages[1]);
    let left: Vec<&String> = ["parent-rect-L", "parent-rect2-L", "parent-oval-L", "parent-text-L", "parent-line-L", "parent-group-L"].iter().map(|l| &d.ids[*l]).collect();
    assert!(pairs.iter().all(|(m, _)| left.contains(&m)), "left parent items");
    assert!(pairs.iter().any(|(m, _)| *m == d.ids["parent-text-L"]));
    assert!(!pairs.iter().any(|(m, _)| *m == d.ids["parent-text-R"]));
}

#[test]
fn the_text_of_a_text_frame_is_copied_with_its_own_story() {
    let mut d = fixture();
    let created = d.apply(over(&d, 0, &["parent-text-R"])).unwrap();
    let copy = &created[0];
    let (master_story, copy_story) = (d.attr(&d.ids["parent-text-R"], "ParentStory").unwrap(), d.attr(copy, "ParentStory").unwrap());
    assert_ne!(master_story, copy_story);
    let story = d.ed.part(&format!("Stories/Story_{copy_story}.xml")).expect("the copy's story is a part of its own");
    assert!(story.contains("Parent R"), "{story}");
    // the parent's story is untouched
    assert!(d.ed.part(&format!("Stories/Story_{master_story}.xml")).unwrap().contains("Parent R"));
}

#[test]
fn what_cannot_be_overridden_is_refused() {
    let mut d = fixture();
    let e = d.apply(over(&d, 0, &["parent-fixed-R"])).unwrap_err();
    assert!(e.contains("does not allow overrides"), "{e}");
    // an item of the other parent page is still a parent item of the page (the right one is chosen by the renderer, not here), but a page item is not
    let e = d.apply(Command::OverrideParentItems { page: d.pages[0].clone(), ids: vec![d.id("box-0")] }).unwrap_err();
    assert!(e.contains("not a parent item") || e.contains("not an item of the page's parent"), "{e}");
    d.apply(over(&d, 0, &["parent-rect-R"])).unwrap();
    let e = d.apply(over(&d, 0, &["parent-rect-R"])).unwrap_err();
    assert!(e.contains("already overridden"), "{e}");
    // nothing but the refused calls: the editor still works
    assert_eq!(d.pairs(&d.pages[0]).len(), 1);
    // a page without a parent
    d.apply(Command::ApplyMaster { pages: vec![d.pages[2].clone()], master: None }).unwrap();
    assert!(d.apply(over(&d, 2, &[])).unwrap_err().contains("no parent page"));
}

#[test]
fn override_all_skips_what_is_taken_off_the_page_or_already_overridden() {
    let mut d = fixture();
    d.apply(over(&d, 4, &["parent-rect-R"])).unwrap();
    let c = d.pairs(&d.pages[4])[0].1.clone();
    d.apply(Command::Delete { ids: vec![c] }).unwrap();
    assert_eq!(d.pairs(&d.pages[4])[0].1, "n", "a deleted copy leaves 'parent item, n'");
    let created = d.apply(over(&d, 4, &[])).unwrap();
    assert_eq!(created.len(), 5, "the deleted one stays off, the fixed one is not allowed");
    assert_eq!(d.pairs(&d.pages[4]).len(), 6);
}

#[test]
fn remove_local_overrides_brings_the_parent_items_back_but_not_the_deleted_ones() {
    let mut d = fixture();
    d.apply(over(&d, 4, &["parent-rect-R"])).unwrap();
    let deleted = d.pairs(&d.pages[4])[0].1.clone();
    d.apply(Command::Delete { ids: vec![deleted] }).unwrap();
    d.apply(over(&d, 4, &["parent-oval-R", "parent-text-R"])).unwrap();
    let before = d.spread_items(&d.pages[4]).len();
    d.apply(Command::RemoveLocalOverrides { page: d.pages[4].clone(), ids: vec![] }).unwrap();
    assert_eq!(d.spread_items(&d.pages[4]).len(), before - 2, "the two copies went");
    let pairs = d.pairs(&d.pages[4]);
    assert_eq!(pairs, vec![(d.ids["parent-rect-R"].clone(), "n".to_owned())], "the deleted one stays off the page");
    // the removed copy's story went with it
    assert!(!d.ed.parts().keys().any(|k| k.starts_with("Stories/") && d.ed.parts()[k].contains("Parent R") && k.as_str() != format!("Stories/Story_{}.xml", d.attr(&d.ids["parent-text-R"], "ParentStory").unwrap())), "a story of a removed copy is left");
    // and the parent's items can be overridden again
    d.apply(over(&d, 4, &["parent-oval-R"])).unwrap();
}

#[test]
fn detach_leaves_the_copy_standing_alone() {
    let mut d = fixture();
    d.apply(over(&d, 2, &["parent-rect-R", "parent-oval-R"])).unwrap();
    let pairs = d.pairs(&d.pages[2]);
    let (rect_copy, oval_copy) = (pairs.iter().find(|p| p.0 == d.ids["parent-rect-R"]).unwrap().1.clone(), pairs.iter().find(|p| p.0 == d.ids["parent-oval-R"]).unwrap().1.clone());
    d.apply(Command::DetachFromParent { page: d.pages[2].clone(), ids: vec![rect_copy.clone()] }).unwrap();
    let now = d.pairs(&d.pages[2]);
    assert!(now.contains(&(d.ids["parent-rect-R"].clone(), "n".to_owned())));
    assert!(now.contains(&(d.ids["parent-oval-R"].clone(), oval_copy.clone())));
    // the detached copy is still on the page
    assert!(d.spread_items(&d.pages[2]).iter().any(|(_, id)| *id == rect_copy));
    // not a copy on this page
    assert!(d.apply(Command::DetachFromParent { page: d.pages[2].clone(), ids: vec![d.id("box-2")] }).unwrap_err().contains("not a copy"));
    d.apply(Command::DetachFromParent { page: d.pages[2].clone(), ids: vec![] }).unwrap();
    assert!(d.pairs(&d.pages[2]).iter().all(|p| p.1 == "n"));
}

#[test]
fn allow_overrides_is_written_on_the_parent_item_and_undo_takes_everything_back() {
    let mut d = fixture();
    let before = d.ed.parts().clone();
    d.apply(Command::AllowParentOverrides { ids: vec![d.id("parent-rect-R")], allow: false }).unwrap();
    assert_eq!(d.attr(&d.ids["parent-rect-R"], "AllowOverrides").as_deref(), Some("false"));
    assert!(d.apply(over(&d, 0, &["parent-rect-R"])).is_err());
    d.apply(Command::AllowParentOverrides { ids: vec![d.id("parent-fixed-R")], allow: true }).unwrap();
    assert_eq!(d.attr(&d.ids["parent-fixed-R"], "AllowOverrides").as_deref(), Some("true"));
    d.apply(over(&d, 0, &["parent-fixed-R", "parent-text-R"])).unwrap();
    // not an item of a parent page
    assert!(d.apply(Command::AllowParentOverrides { ids: vec![d.id("box-0")], allow: false }).is_err());
    // undo all: the document is as it was
    while d.ed.undo().is_some() {}
    assert_eq!(d.ed.parts(), &before);
}

#[test]
fn override_all_is_one_undo_step() {
    let mut d = fixture();
    let before = d.ed.parts().clone();
    d.apply(over(&d, 3, &[])).unwrap();
    assert_ne!(d.ed.parts(), &before);
    d.ed.undo().unwrap();
    assert_eq!(d.ed.parts(), &before);
    d.ed.redo().unwrap();
    assert_eq!(d.pairs(&d.pages[3]).len(), 6);
}

// ---- found by the review of this feature ----

impl Doc {
    fn spread_of(&self, page: &str) -> String {
        for t in self.ed.parts().values() {
            if !t.contains(&format!("Self=\"{page}\"")) {
                continue;
            }
            let ix = Index::scan(t).unwrap();
            if let Some(i) = ix.by_self(t, page) {
                return ix.ancestors(i).find(|&a| ix.local(t, a) == "Spread").and_then(|a| ix.attr(t, a, "Self")).unwrap().to_owned();
            }
        }
        panic!("no page {page}");
    }

    fn color_of(&self, page: &str) -> String {
        for t in self.ed.parts().values() {
            let ix = Index::scan(t).unwrap();
            if let Some(i) = ix.by_self(t, page) {
                let c = ix.path(t, i, &["Properties", "PageColor"]).unwrap();
                return t[ix.nodes[c].open_end..ix.nodes[c].close_start].to_owned();
            }
        }
        panic!("no page {page}");
    }
}

#[test]
fn a_duplicated_spread_has_its_own_copies_in_its_override_list() {
    let mut d = fixture();
    d.apply(over(&d, 2, &["parent-rect-R"])).unwrap();
    let original = d.pairs(&d.pages[2]);
    assert_eq!(original.len(), 1);
    let spread = d.spread_of(&d.pages[2]);
    d.apply(Command::DuplicateSpread { spread }).unwrap();
    // the spread after the original: pages 3 and 4 now; the duplicate's pages 3, 4 (the right one is page 4)
    let copy_page = d.pages[4].clone();
    let dup = d.pairs(&copy_page);
    assert_eq!(dup.len(), 1, "{dup:?}");
    assert_eq!(dup[0].0, original[0].0, "the same parent item");
    assert_ne!(dup[0].1, original[0].1, "the duplicate's own copy, not the original's");
    assert!(d.spread_items(&copy_page).iter().any(|(_, id)| *id == dup[0].1), "the copy is on the duplicate");
    // removing the overrides of the duplicate leaves the original's copy alone
    d.apply(Command::RemoveLocalOverrides { page: copy_page.clone(), ids: vec![] }).unwrap();
    assert!(d.spread_items(&d.pages[2]).iter().any(|(_, id)| *id == original[0].1));
    assert!(d.pairs(&copy_page).is_empty());
}

#[test]
fn deleting_a_group_with_a_copy_inside_keeps_the_override_list_true() {
    let mut d = fixture();
    let created = d.apply(over(&d, 0, &["parent-rect-R", "parent-oval-R"])).unwrap();
    d.apply(Command::Group { ids: created.clone() }).unwrap();
    let group = d.spread_items(&d.pages[0]).iter().find(|(k, _)| k == "Group").unwrap().1.clone();
    d.apply(Command::Delete { ids: vec![group] }).unwrap();
    let pairs = d.pairs(&d.pages[0]);
    assert_eq!(pairs.len(), 2);
    assert!(pairs.iter().all(|(_, c)| c == "n"), "{pairs:?}");
    // so the page's commands still work
    d.apply(Command::RemoveLocalOverrides { page: d.pages[0].clone(), ids: vec![] }).unwrap();
    d.apply(Command::DetachFromParent { page: d.pages[0].clone(), ids: vec![] }).unwrap();
}

#[test]
fn a_new_page_does_not_take_the_colour_label_of_the_page_it_copies() {
    let mut d = fixture();
    d.apply(Command::SetPageColor { pages: vec![d.pages[4].clone()], masters: vec![], color: "Gold".into() }).unwrap();
    assert_eq!(d.color_of(&d.pages[4]), "Gold");
    let before = d.pages.len();
    d.apply(Command::InsertPages { count: 1, before: None, master: None }).unwrap();
    assert_eq!(d.pages.len(), before + 1);
    assert_eq!(d.color_of(&d.pages[before]), "UseMasterColor");
    assert_eq!(d.color_of(&d.pages[4]), "Gold");
}

#[test]
fn a_page_whose_spread_hides_parent_items_has_none_to_override() {
    let mut d = fixture();
    let spread = d.spread_of(&d.pages[2]);
    d.apply(Command::SetSpreadShowMaster { spreads: vec![spread], show: false }).unwrap();
    let e = d.apply(over(&d, 2, &[])).unwrap_err();
    assert!(e.contains("Show Parent Items is off"), "{e}");
    assert!(d.apply(over(&d, 2, &["parent-rect-R"])).is_err());
    // another spread is not affected
    d.apply(over(&d, 4, &["parent-rect-R"])).unwrap();
}

#[test]
fn a_parent_item_named_twice_is_overridden_once() {
    let mut d = fixture();
    let created = d.apply(over(&d, 0, &["parent-rect-R", "parent-rect-R"])).unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(d.pairs(&d.pages[0]).len(), 1);
}

#[test]
fn a_self_closing_properties_or_page_colour_is_written_out() {
    let mut d = fixture();
    // a page whose Properties and one whose PageColor are self-closing (other producers write them so)
    let (part, text) = d.ed.parts().iter().find(|(n, t)| n.starts_with("Spreads/") && t.contains("<PageColor")).map(|(n, t)| (n.clone(), t.clone())).unwrap();
    let ix = Index::scan(&text).unwrap();
    let pages: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(&text, i) == "Page").collect();
    let p0 = pages[0];
    let props = ix.named(&text, p0, "Properties")[0];
    let (s, e) = (ix.nodes[props].start, ix.nodes[props].end);
    let mut parts = d.ed.parts().clone();
    let mut t2 = text.clone();
    t2.replace_range(s..e, "<Properties/>");
    parts.insert(part.clone(), t2);
    let id0 = ix.attr(&text, p0, "Self").unwrap().to_owned();
    let mut ed = Editor::new(parts);
    ed.apply("color", &[Command::SetPageColor { pages: vec![id0.clone()], masters: vec![], color: "Pink".into() }]).unwrap();
    let t3 = ed.part(&part).unwrap();
    let ix3 = Index::scan(t3).unwrap();
    let i3 = ix3.by_self(t3, &id0).unwrap();
    let c = ix3.path(t3, i3, &["Properties", "PageColor"]).expect("PageColor inside Properties");
    assert_eq!(&t3[ix3.nodes[c].open_end..ix3.nodes[c].close_start], "Pink");
    // and the whole part still reads
    Index::scan(t3).unwrap();
    let _ = &mut d;
}

// ---- Apply Parent on a page with overrides (oracle pages-override parent-apply-*) ----

#[test]
fn applying_the_same_parent_again_takes_the_pages_overrides_away() {
    let mut d = fixture();
    let page = d.pages[4].clone();
    let parent = d.attr(&page, "AppliedMaster").unwrap();
    let items = d.spread_items(&page).len();
    d.apply(over(&d, 4, &["parent-rect-R", "parent-oval-R"])).unwrap();
    assert_eq!(d.spread_items(&page).len(), items + 2);
    d.apply(Command::ApplyMaster { pages: vec![page.clone()], master: Some(parent) }).unwrap();
    assert_eq!(d.spread_items(&page).len(), items, "the copies went");
    assert_eq!(d.attr(&page, "OverrideList").unwrap_or_default(), "");
    // one undo step brings the overrides back
    d.ed.undo().unwrap();
    d.scan();
    assert_eq!(d.spread_items(&page).len(), items + 2);
    assert_eq!(d.pairs(&page).len(), 2);
}

#[test]
fn applying_another_parent_or_none_leaves_the_copies_as_the_pages_own_objects() {
    let mut d = fixture();
    let page = d.pages[2].clone();
    d.apply(over(&d, 2, &["parent-rect-R"])).unwrap();
    let copy = d.pairs(&page)[0].1.clone();
    let items = d.spread_items(&page).len();
    d.apply(Command::ApplyMaster { pages: vec![page.clone()], master: None }).unwrap();
    assert_eq!(d.attr(&page, "OverrideList").unwrap_or_default(), "");
    assert_eq!(d.attr(&page, "AppliedMaster").unwrap(), "n");
    assert_eq!(d.spread_items(&page).len(), items);
    assert!(d.spread_items(&page).iter().any(|(_, id)| *id == copy), "the copy stays");
}
