//! The Content Placer's Create Link (`linkPlaced`, `updateContentLink`, `unlinkContentLink`; oracle conveyor-c2-*, the
//! IDML measured in InDesign 2025). The InDesign comparison is in `oracle.rs`; this tests what it cannot: the status of a
//! link as the source changes, update and unlink, and what is refused.
use folio_edit::commands::RegisteredCommand as R;
use folio_edit::{Command, ContentLink, Editor, Foreign, ForeignSource, link_marks, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, BTreeMap<String, String>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut labels = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        if name.starts_with("Spreads/") {
            let ix = Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") {
                    let owner = ix.ancestors(i).find(|&a| ix.attr(&t, a, "Self").is_some()).unwrap();
                    labels.insert(ix.value(&t, i, "Value").unwrap(), ix.attr(&t, owner, "Self").unwrap().into());
                }
            }
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), labels)
}

fn first_page(e: &Editor) -> String {
    for (name, t) in e.parts() {
        if name.starts_with("Spreads/") {
            let ix = Index::scan(t).unwrap();
            if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page") {
                return ix.attr(t, i, "Self").unwrap().to_owned();
            }
        }
    }
    panic!("no page");
}

const DATE: &str = "2026-11-06T12:00:00";

/// Places a copy of `ids` with a link; the created objects.
fn place_linked(e: &mut Editor, ids: &[&str], map_styles: bool) -> Vec<String> {
    let ids: Vec<String> = ids.iter().map(|s| (*s).to_owned()).collect();
    e.copy(&ids).unwrap();
    let page = first_page(e);
    let paste = Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false };
    let link = Command::Registered(R::LinkPlaced { map_styles, date: DATE.into() });
    e.apply("place", &[paste, link]).unwrap().created
}

fn element(e: &Editor, id: &str) -> String {
    for t in e.parts().values() {
        if let Ok(ix) = Index::scan(t)
            && let Some(i) = ix.by_self(t, id)
        {
            return t[ix.nodes[i].start..ix.nodes[i].end].to_owned();
        }
    }
    panic!("no element {id}");
}

fn attr(e: &Editor, id: &str, name: &str) -> Option<String> {
    let x = element(e, id);
    let ix = Index::scan(&x).unwrap();
    ix.attr(&x, 0, name).map(str::to_owned)
}

fn links(e: &Editor) -> Vec<ContentLink> {
    e.content_links()
}

#[test]
fn a_placed_frame_links_to_its_source_and_a_text_frame_also_its_story() {
    let (mut e, l) = fixture();
    let (src, tf) = (l["r-fill"].clone(), l["tf"].clone());
    let made = place_linked(&mut e, &[&src, &tf], true);
    assert_eq!(made.len(), 2);
    // the source frames carry the resource ids; the placed ones the Link and the option
    assert!(attr(&e, &src, "LinkResourceId").is_some());
    let rect = element(&e, &made[0]);
    assert!(rect.contains("<LinkedPageItemOption ") && rect.contains("idinternalSharedArticle:"), "{rect}");
    assert!(rect.contains(r#"LinkResourceFormat="$ID/Internal Linked Object""#) && rect.contains(r#"LinkClassID="138546""#));
    let all = links(&e);
    assert_eq!((all.iter().filter(|c| c.kind == "object").count(), all.iter().filter(|c| c.kind == "story").count()), (2, 1));
    assert!(all.iter().all(|c| c.status == "normal" && c.source.is_some()), "{all:?}");
    // the text frame's story is linked too, Map Styles is its ApplyStyleMappings
    let story = all.iter().find(|c| c.kind == "story").unwrap();
    assert_eq!(story.map_styles, Some(true));
    let story_xml = element(&e, &story.target);
    assert!(story_xml.contains(r#"<LinkedStoryOption UpdateWhileSaving="false" WarnOnUpdateOfEditedStory="true" RemoveForcedLineBreaks="false" ApplyStyleMappings="true" />"#), "{story_xml}");
    assert!(story_xml.contains("idinternalcontent:") && story_xml.contains(r#"LinkClassID="138499""#));
    assert!(attr(&e, story.source.as_deref().unwrap(), "LinkResourceId").is_some());
}

#[test]
fn placing_again_from_the_same_source_shares_its_resource_id() {
    let (mut e, l) = fixture();
    let src = l["r-fill"].clone();
    place_linked(&mut e, &[&src], false);
    let id = attr(&e, &src, "LinkResourceId").unwrap();
    place_linked(&mut e, &[&src], false);
    assert_eq!(attr(&e, &src, "LinkResourceId").unwrap(), id);
    let all = links(&e);
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|c| c.source.as_deref() == Some(src.as_str())));
}

#[test]
fn changing_the_source_makes_the_link_out_of_date_and_update_brings_the_copy_along() {
    let (mut e, l) = fixture();
    let (src, tf) = (l["r-fill"].clone(), l["tf"].clone());
    let made = place_linked(&mut e, &[&src, &tf], false);
    // the source frame moves: a move is not a change of the link (InDesign); turning it is, and the text of the text frame is its
    // story's business
    e.apply("edit", &[Command::Move { ids: vec![src.clone()], by: [30.0, 0.0] }]).unwrap();
    assert!(links(&e).iter().all(|c| c.status == "normal"), "a move leaves the links as they were");
    e.apply("edit", &[Command::Rotate { ids: vec![src.clone()], by: 15.0, anchor: folio_edit::Anchor::default() }]).unwrap();
    let all = links(&e);
    assert_eq!(made.len(), 2);
    let obj = all.iter().find(|c| c.source.as_deref() == Some(src.as_str())).unwrap();
    assert_eq!(obj.status, "outOfDate");
    assert!(all.iter().filter(|c| c.source.as_deref() != Some(src.as_str())).all(|c| c.status == "normal"), "{all:?}");
    let story_target = all.iter().find(|c| c.kind == "story").unwrap().target.clone();
    // update: the copy takes the source's state and keeps its place
    let was = e.bounds(&obj.target).unwrap();
    e.apply("update", &[Command::Registered(R::UpdateContentLink { link: obj.link.clone(), date: DATE.into(), overwrite: None })]).unwrap();
    let turned = |id: &str| attr(&e, id, "ItemTransform").unwrap().split_whitespace().take(4).collect::<Vec<_>>().join(" ");
    assert_eq!(turned(&obj.target), turned(&src), "the copy turns like its source");
    let now = e.bounds(&obj.target).unwrap();
    assert!((now[0] - was[0]).abs() < 1e-6 && (now[1] - was[1]).abs() < 1e-6, "the copy keeps its top left corner: {was:?} -> {now:?}");
    assert!(links(&e).iter().all(|c| c.status == "normal"));
    // the source's text changes: the story link goes out of date, the frame's does not
    let story_link = links(&e).into_iter().find(|c| c.kind == "story").unwrap();
    let src_story = story_link.source.clone().unwrap();
    e.apply("edit", &[Command::InsertText { story: src_story, at: 0, text: "NEW ".into() }]).unwrap();
    let after = links(&e);
    assert_eq!(after.iter().find(|c| c.kind == "story").unwrap().status, "outOfDate");
    assert!(after.iter().filter(|c| c.kind == "object").all(|c| c.status == "normal"), "{after:?}");
    e.apply("update", &[Command::Registered(R::UpdateContentLink { link: story_link.link.clone(), date: DATE.into(), overwrite: None })]).unwrap();
    assert!(links(&e).iter().all(|c| c.status == "normal"));
    assert!(element(&e, &story_target).contains("NEW "), "the copy's story now reads like the source's");
}

#[test]
fn unlink_removes_the_link_and_the_option_and_keeps_the_object() {
    let (mut e, l) = fixture();
    let made = place_linked(&mut e, &[&l["r-fill"]], false);
    let link = links(&e).remove(0);
    e.apply("unlink", &[Command::Registered(R::UnlinkContentLink { link: link.link })]).unwrap();
    assert!(links(&e).is_empty());
    let x = element(&e, &made[0]);
    assert!(!x.contains("LinkedPageItemOption") && !x.contains("idinternalSharedArticle:"), "{x}");
}

#[test]
fn a_deleted_source_leaves_the_link_missing() {
    let (mut e, l) = fixture();
    let src = l["r-fill"].clone();
    place_linked(&mut e, &[&src], false);
    e.apply("delete", &[Command::Delete { ids: vec![src] }]).unwrap();
    assert_eq!(links(&e)[0].status, "missing");
    assert_eq!(links(&e)[0].source, None);
}

#[test]
fn linking_is_undone_with_the_placing_and_what_cannot_be_linked_is_refused() {
    let (mut e, l) = fixture();
    let before = e.parts().clone();
    place_linked(&mut e, &[&l["r-fill"]], false);
    e.undo().unwrap();
    assert_eq!(e.parts(), &before, "one undo step puts back the copy, the link and the source's id");
    // a link to an object of another document is not built: the whole placing is refused, nothing is half done
    let (other, ol) = fixture();
    let other_clip = other.collect(&[ol["r-fill"].clone()]).unwrap();
    e.set_clipboard(Some(other_clip));
    let page = first_page(&e);
    let paste = Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false };
    let err = e.apply("place", &[paste, Command::Registered(R::LinkPlaced { map_styles: false, date: DATE.into() })]).unwrap_err();
    assert_eq!(err.code, "unsupported");
    assert_eq!(e.parts(), &before);
}

#[test]
fn the_placed_object_changed_is_edited_and_an_update_asks_before_it_replaces_the_change() {
    let (mut e, l) = fixture();
    let (src, copy) = (l["r-fill"].clone(), place_linked_one(&mut e, &l["r-fill"]));
    assert!(!link(&e, &copy).edited, "a link just made is not edited");
    // the copy moves: edited (a source that moves is not changed, a placed object that moves is), the status stays normal
    e.apply("edit", &[Command::Move { ids: vec![copy.clone()], by: [10.0, 0.0] }]).unwrap();
    assert!(link(&e, &copy).edited && link(&e, &copy).status == "normal");
    // update on an up-to-date link does nothing, also for an edited copy
    let parts = e.parts().clone();
    e.apply("update", &[Command::Registered(R::UpdateContentLink { link: link(&e, &copy).link, date: DATE.into(), overwrite: None })]).unwrap();
    assert_eq!(e.parts(), &parts, "Update Link on an up-to-date link changes nothing");
    // the source changes too: the update would lose the copy's change, so it asks; with overwrite it replaces and clears `edited`
    e.apply("edit", &[Command::Rotate { ids: vec![src.clone()], by: 15.0, anchor: folio_edit::Anchor::default() }]).unwrap();
    assert!(link(&e, &copy).edited && link(&e, &copy).status == "outOfDate");
    let refused = e.apply("update", &[Command::Registered(R::UpdateContentLink { link: link(&e, &copy).link, date: DATE.into(), overwrite: None })]).unwrap_err();
    assert_eq!(refused.code, "edited");
    e.apply("update", &[Command::Registered(R::UpdateContentLink { link: link(&e, &copy).link, date: DATE.into(), overwrite: Some(true) })]).unwrap();
    assert!(!link(&e, &copy).edited && link(&e, &copy).status == "normal");
    // the state is kept in the designmap, and unlinking forgets it
    assert!(e.part("designmap.xml").unwrap().contains("folio.contentLinkEdits"));
    e.apply("unlink", &[Command::Registered(R::UnlinkContentLink { link: link(&e, &copy).link })]).unwrap();
    assert!(links(&e).is_empty());
}

fn link(e: &Editor, copy: &str) -> ContentLink {
    links(e).into_iter().find(|c| c.target == copy).unwrap()
}

fn place_linked_one(e: &mut Editor, id: &str) -> String {
    place_linked(e, &[id], false).remove(0)
}

#[test]
fn a_link_is_named_like_its_source_in_the_links_panel() {
    let (mut e, l) = fixture();
    place_linked(&mut e, &[&l["r-fill"], &l["tf"]], false);
    let all = links(&e);
    let rect = all.iter().find(|c| c.kind == "object" && c.source.as_deref() == Some(l["r-fill"].as_str())).unwrap();
    assert_eq!((rect.name.as_str(), rect.generic), ("<rectangle>", Some("rectangle")));
    let frame = all.iter().find(|c| c.kind == "object" && c.source.as_deref() == Some(l["tf"].as_str())).unwrap();
    let story = all.iter().find(|c| c.kind == "story").unwrap();
    assert_eq!(frame.name, story.name, "a story link has the name of its frame");
    assert!(frame.name.starts_with('<') && frame.name.ends_with('>') && frame.generic.is_none(), "text: {}", frame.name);
    assert_eq!(story.frame.as_deref(), Some(frame.target.as_str()), "the story is shown at its frame");
}

#[test]
fn a_link_to_another_document_from_an_import_is_listed_unlinked_and_not_updated() {
    let (mut e, l) = fixture();
    let made = place_linked_one(&mut e, &l["r-fill"]);
    // what an import of InDesign's link to an object of another document leaves: the document is a project reference, the number stays
    let mut parts = e.parts().clone();
    for t in parts.values_mut() {
        *t = t.replace("idinternalSharedArticle:", "idexternalSharedArticle:folio:path-ref/r7?");
    }
    let mut e = Editor::new(parts);
    let link = links(&e).into_iter().find(|c| c.target == made).unwrap();
    assert!(link.external && link.status == "external" && link.source.is_none(), "{link:?}");
    assert!(link.uri.starts_with("idexternalSharedArticle:folio:path-ref/r7?"), "{}", link.uri);
    let refused = e.apply("update", &[Command::Registered(R::UpdateContentLink { link: link.link.clone(), date: DATE.into(), overwrite: Some(true) })]).unwrap_err();
    // the address of an import is a hint only: there is no document to take the object from (conveyor-c4 reads the documents of the project)
    assert_eq!(refused.code, "notFound");
    e.apply("unlink", &[Command::Registered(R::UnlinkContentLink { link: link.link })]).unwrap();
    assert!(links(&e).is_empty());
    let x = element(&e, &made);
    assert!(!x.contains("LinkedPageItemOption") && !x.contains("idexternal"), "{x}");
}

/// The union of the visible bounds (stroke included), as InDesign puts the top left corner at the click of Place and Link.
fn union(e: &mut Editor, ids: &[String]) -> [f64; 4] {
    let mut u: Option<[f64; 4]> = None;
    for id in ids {
        let b = e.outline_bounds(id, false).unwrap_or_else(|| panic!("no bounds for {id}"));
        u = Some(match u {
            None => b,
            Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
        });
    }
    u.unwrap()
}

/// Place and Link as the menu action does it (oracle `pal-*`, and measured with a real click: two objects keep their places to each other
/// and the set's top left corner is at the point): the objects the engine holds go onto the page linked, in one undo step.
#[test]
fn place_and_link_puts_the_set_top_left_at_the_point_keeps_the_places_and_links_each() {
    let (mut e, l) = fixture();
    let (src, tf) = (l["r-fill"].clone(), l["tf"].clone());
    let sources = vec![src.clone(), tf.clone()];
    let rel_src: Vec<[f64; 2]> = {
        let u = union(&mut e, &sources);
        sources.iter().map(|id| { let b = e.outline_bounds(id, false).unwrap(); [b[0] - u[0], b[1] - u[1]] }).collect()
    };
    let page = first_page(&e);
    let place = |e: &mut Editor, at: [f64; 2]| -> Vec<String> {
        e.copy(&sources).unwrap();
        e.apply("place", &[Command::Registered(R::PlaceAndLink { page: page.clone(), at, map_styles: None, date: DATE.into() })]).unwrap().created
    };
    let a = place(&mut e, [100.0, 200.0]);
    let b = place(&mut e, [130.0, 260.0]);
    assert_eq!((a.len(), b.len()), (2, 2));
    let (ua, ub) = (union(&mut e, &a), union(&mut e, &b));
    // a second point 30 right and 60 down puts the set exactly there: the corner is the point, not the middle
    assert!((ub[0] - ua[0] - 30.0).abs() < 1e-6 && (ub[1] - ua[1] - 60.0).abs() < 1e-6, "{ua:?} {ub:?}");
    // the places to each other stay (a copy keeps the stacking order of the set, so the pairs are matched as sets)
    let mut rel_placed: Vec<[f64; 2]> = a.iter().map(|id| { let bnd = e.outline_bounds(id, false).unwrap(); [bnd[0] - ua[0], bnd[1] - ua[1]] }).collect();
    let mut rel_src = rel_src;
    let key = |r: &[f64; 2]| ((r[0] * 1e6).round() as i64, (r[1] * 1e6).round() as i64);
    rel_placed.sort_by_key(key);
    rel_src.sort_by_key(key);
    assert_eq!(rel_placed.len(), rel_src.len());
    for (p, q) in rel_placed.iter().zip(&rel_src) {
        assert!((p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6, "{rel_placed:?} {rel_src:?}");
    }
    // each placed object links to its source, a text frame's story too, Map Styles on unless it is switched off
    let story_links: Vec<_> = links(&e).into_iter().filter(|c| c.kind == "story").collect();
    assert_eq!(story_links.len(), 2, "{story_links:?}");
    assert!(story_links.iter().all(|c| c.map_styles == Some(true)), "{story_links:?}");
    assert_eq!(links(&e).len(), 6, "{:?}", links(&e));
    // one undo step takes the second placing away, links and all
    e.undo().unwrap();
    assert_eq!(links(&e).len(), 3);
    assert!(e.outline_bounds(&b[0], false).is_none());
    // switched off: the story link does not map styles
    e.copy(&sources).unwrap();
    e.apply("place", &[Command::Registered(R::PlaceAndLink { page: page.clone(), at: [50.0, 50.0], map_styles: Some(false), date: DATE.into() })]).unwrap();
    assert!(links(&e).iter().any(|c| c.kind == "story" && c.map_styles == Some(false)));
}

/// Nothing held, or an object of another document (not built): refused, nothing changes.
#[test]
fn place_and_link_with_nothing_loaded_is_refused() {
    let (mut e, _) = fixture();
    let page = first_page(&e);
    let err = e.apply("place", &[Command::Registered(R::PlaceAndLink { page, at: [0.0, 0.0], map_styles: None, date: DATE.into() })]).unwrap_err();
    assert_eq!(err.code, "invalid");
}

/// A link to an object of another document of the project (conveyor-c4): the source document gets the link numbers, the placing one the
/// external `Link` naming the document by its reference, and the status follows the source document's parts.
#[test]
fn a_link_to_an_object_of_another_document_follows_that_documents_parts() {
    let (mut a, la) = fixture();
    let (mut b, _) = fixture();
    let (src, tf) = (la["r-fill"].clone(), la["tf"].clone());
    let ids = vec![src.clone(), tf.clone()];
    // the source side: the numbers are written into the source document (and kept when it is marked again)
    a.apply("mark", &[Command::Registered(R::MarkLinkSources { ids: ids.clone() })]).unwrap();
    let marks = link_marks(a.parts(), &ids).unwrap();
    assert!(marks.iter().all(|m| m.resource != 0), "{marks:?}");
    assert!(marks[1].story.is_some_and(|(n, _)| n != 0), "a text frame's story is numbered too: {marks:?}");
    a.apply("mark again", &[Command::Registered(R::MarkLinkSources { ids: ids.clone() })]).unwrap();
    assert_eq!(link_marks(a.parts(), &ids).unwrap(), marks, "a second mark keeps the numbers");
    // the placing document: what was copied in the source, the numbers from it, the document by its reference
    a.copy(&ids).unwrap();
    b.set_clipboard(a.clipboard());
    b.set_foreign_source(Some(ForeignSource { document: "folio:path-ref/r7".into(), marks: marks.clone() }));
    let page = first_page(&b);
    let made = b.apply("place", &[
        Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false },
        Command::Registered(R::LinkPlaced { map_styles: true, date: DATE.into() }),
    ]).unwrap().created;
    assert_eq!(made.len(), 2);
    // a copy keeps the stacking order of the set, so the rectangle is found by what it is
    let (rect_id, frame_id) = if element(&b, &made[0]).starts_with("<Rectangle") { (&made[0], &made[1]) } else { (&made[1], &made[0]) };
    let rect = element(&b, rect_id);
    let rect_mark = marks.iter().find(|m| m.id == src).unwrap();
    assert!(rect.contains(&format!(r#"LinkResourceURI="idexternalSharedArticle:folio:path-ref/r7?{}""#, rect_mark.resource)), "{rect}");
    assert!(rect.contains(r#"LinkResourceFormat="$ID/External Linked Object""#) && !rect.contains(" LinkResourceId="), "{rect}");
    assert!(attr(&b, frame_id, "LinkResourceId").is_none());
    let stories: Vec<_> = links(&b).into_iter().filter(|c| c.kind == "story").collect();
    assert_eq!(stories.len(), 1);
    assert_eq!(links(&b).len(), 3);
    assert!(links(&b).iter().all(|c| c.external && c.status == "external"), "no one reads the source: {:?}", links(&b));
    // the source document read: up to date, then the source object changes, then it is gone
    let shown = |a: &Editor| -> Foreign { Foreign::Doc { name: "a.folio".into(), parts: std::sync::Arc::new(a.parts().clone()) } };
    let state_of = |b: &Editor, a: &Editor| -> Vec<(String, &'static str, String)> {
        let foreign = shown(a);
        b.content_links_with(&|d: &str| { assert_eq!(d, "folio:path-ref/r7"); foreign.clone() }).into_iter().map(|c| (c.kind.to_owned(), c.status, c.name)).collect()
    };
    let now = state_of(&b, &a);
    assert!(now.iter().all(|(_, st, _)| *st == "normal"), "{now:?}");
    assert!(now.iter().any(|(k, _, n)| k == "object" && n.starts_with("a.folio#")), "{now:?}");
    a.apply("change", &[Command::Rotate { ids: vec![src.clone()], by: 15.0, anchor: folio_edit::Anchor::default() }]).unwrap();
    let now = state_of(&b, &a);
    assert_eq!(now.iter().filter(|(_, st, _)| *st == "outOfDate").count(), 1, "only the changed object: {now:?}");
    a.apply("delete", &[Command::Delete { ids: vec![src.clone()] }]).unwrap();
    assert!(state_of(&b, &a).iter().any(|(k, st, _)| k == "object" && *st == "missing"));
    // the document itself gone / not a document of the project
    assert!(b.content_links_with(&|_| Foreign::Missing).iter().all(|c| c.status == "missing"));
    assert!(b.content_links_with(&|_| Foreign::Unknown).iter().all(|c| c.status == "external"));
    // unlinking works as for any link
    let obj = links(&b).into_iter().find(|c| c.kind == "object").unwrap();
    b.apply("unlink", &[Command::Registered(R::UnlinkContentLink { link: obj.link })]).unwrap();
    assert_eq!(links(&b).len(), 2);
}

/// Without the numbers from the source document an object of another document is not linked (nothing half done).
#[test]
fn objects_of_another_document_cannot_be_linked_without_the_source_numbers() {
    let (mut a, la) = fixture();
    let (mut b, _) = fixture();
    a.copy(&[la["r-fill"].clone()]).unwrap();
    b.set_clipboard(a.clipboard());
    let page = first_page(&b);
    let before = b.parts().clone();
    let err = b.apply("place", &[
        Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false },
        Command::Registered(R::LinkPlaced { map_styles: false, date: DATE.into() }),
    ]).unwrap_err();
    assert_eq!(err.code, "unsupported");
    assert_eq!(b.parts(), &before, "the paste is undone with it");
}

/// Update Link on a link to another document takes the object from that document (InDesign 2025 probe b5: also with the source closed,
/// from its file); the swatches and styles it uses come along, the copy keeps its top left corner, and the link matches the source again.
#[test]
fn updating_a_link_to_another_document_takes_the_object_from_its_parts() {
    let (mut a, la) = fixture();
    let (mut b, _) = fixture();
    let (src, tf) = (la["r-fill"].clone(), la["tf"].clone());
    let ids = vec![src.clone(), tf.clone()];
    a.apply("mark", &[Command::Registered(R::MarkLinkSources { ids: ids.clone() })]).unwrap();
    let marks = link_marks(a.parts(), &ids).unwrap();
    a.copy(&ids).unwrap();
    b.set_clipboard(a.clipboard());
    b.set_foreign_source(Some(ForeignSource { document: "folio:path-ref/r7".into(), marks }));
    let page = first_page(&b);
    b.apply("place", &[
        Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false },
        Command::Registered(R::LinkPlaced { map_styles: true, date: DATE.into() }),
    ]).unwrap();
    let read = |a: &Editor| -> Foreign { Foreign::Doc { name: "a.folio".into(), parts: std::sync::Arc::new(a.parts().clone()) } };
    let list = |b: &Editor, a: &Editor| { let f = read(a); b.content_links_with(&|_| f.clone()) };
    // the source turns and its text changes: both links are out of date
    a.apply("turn", &[Command::Rotate { ids: vec![src.clone()], by: 15.0, anchor: folio_edit::Anchor::default() }]).unwrap();
    let story_src = list(&b, &a).into_iter().find(|c| c.kind == "story").unwrap().source.unwrap();
    a.apply("text", &[Command::InsertText { story: story_src, at: 0, text: "NEW ".into() }]).unwrap();
    let before = list(&b, &a);
    assert_eq!(before.iter().filter(|c| c.status == "outOfDate").count(), 2, "{before:?}");
    let obj = before.iter().find(|c| c.kind == "object" && c.status == "outOfDate").unwrap().clone();
    let story = before.iter().find(|c| c.kind == "story").unwrap().clone();
    // without the other document's parts the editor says so and changes nothing
    let was = b.parts().clone();
    let err = b.apply("update", &[Command::Registered(R::UpdateContentLink { link: obj.link.clone(), date: DATE.into(), overwrite: None })]).unwrap_err();
    assert_eq!(err.code, "notFound", "{err:?}");
    assert_eq!(b.parts(), &was);
    // with them: the copy turns like the source and keeps its corner, the story reads like the source's
    b.set_foreign_documents(std::collections::BTreeMap::from([("folio:path-ref/r7".to_owned(), std::sync::Arc::new(a.parts().clone()))]));
    let corner = b.bounds(&obj.target).unwrap();
    b.apply("update", &[
        Command::Registered(R::UpdateContentLink { link: obj.link.clone(), date: DATE.into(), overwrite: None }),
        Command::Registered(R::UpdateContentLink { link: story.link.clone(), date: DATE.into(), overwrite: None }),
    ]).unwrap();
    let turned = |e: &Editor, id: &str| attr(e, id, "ItemTransform").unwrap().split_whitespace().take(4).collect::<Vec<_>>().join(" ");
    assert_eq!(turned(&b, &obj.target), turned(&a, &src), "the copy turns like its source");
    let now = b.bounds(&obj.target).unwrap();
    assert!((now[0] - corner[0]).abs() < 1e-6 && (now[1] - corner[1]).abs() < 1e-6, "the copy keeps its top left corner: {corner:?} -> {now:?}");
    assert!(element(&b, &story.target).contains("NEW "), "the copy's story now reads like the source's");
    let after = list(&b, &a);
    assert!(after.iter().all(|c| c.status == "normal"), "{after:?}");
    assert!(!element(&b, &obj.target).contains(" LinkResourceId="), "an updated copy is not a source");
}

/// A group is linked as one object (oracle `pal-group`): its members are numbered too, the copy's members name them, and the state
/// does not count the numbers; updating takes the group again (a group with text frames is refused).
#[test]
fn a_group_is_linked_as_one_object_and_its_members_name_their_sources() {
    let (mut e, l) = fixture();
    let grp = l["grp"].clone();
    let made = place_linked(&mut e, &[&grp], false);
    assert_eq!(made.len(), 1);
    let all = links(&e);
    assert_eq!(all.len(), 1, "one object link for the group: {all:?}");
    assert_eq!((all[0].kind, all[0].status, all[0].generic), ("object", "normal", Some("group")), "the numbers written into the members are not a change: {all:?}");
    let source = element(&e, &grp);
    assert_eq!(source.matches(" LinkResourceId=").count(), 3, "the group and its two members are numbered: {source}");
    let copy = element(&e, &made[0]);
    assert!(!copy.contains(" LinkResourceId="), "a copy is not a source");
    assert_eq!(copy.matches(" LinkedSourceTableId=").count(), 2, "the copy's members name the source's: {copy}");
    // placed again: the same numbers, nothing new on the source
    let again = place_linked(&mut e, &[&grp], false);
    assert_eq!(element(&e, &grp).matches(" LinkResourceId=").count(), 3);
    assert_eq!(element(&e, &again[0]).matches(" LinkedSourceTableId=").count(), 2);
    // the source turns: out of date; update takes it and keeps the names of the sources
    e.apply("turn", &[Command::Rotate { ids: vec![grp.clone()], by: 15.0, anchor: folio_edit::Anchor::default() }]).unwrap();
    let stale = links(&e).into_iter().find(|c| c.target == made[0]).unwrap();
    assert_eq!(stale.status, "outOfDate");
    e.apply("update", &[Command::Registered(R::UpdateContentLink { link: stale.link.clone(), date: DATE.into(), overwrite: None })]).unwrap();
    assert_eq!(links(&e).into_iter().find(|c| c.target == made[0]).unwrap().status, "normal");
    let updated = element(&e, &made[0]);
    assert_eq!(updated.matches(" LinkedSourceTableId=").count(), 2, "{updated}");
    assert!(!updated.contains(" LinkResourceId="));
    // the members of a copy are not sources: unlinking leaves the group and its members as objects
    e.apply("unlink", &[Command::Registered(R::UnlinkContentLink { link: stale.link })]).unwrap();
    assert!(!element(&e, &made[0]).contains("<Link "));
}

/// A group of another document: one object link, the copy's members name the source members (their numbers come with the marks).
#[test]
fn a_group_of_another_document_is_linked_with_its_members() {
    let (mut a, la) = fixture();
    let (mut b, _) = fixture();
    let grp = la["grp"].clone();
    a.apply("mark", &[Command::Registered(R::MarkLinkSources { ids: vec![grp.clone()] })]).unwrap();
    let marks = link_marks(a.parts(), std::slice::from_ref(&grp)).unwrap();
    assert_eq!(marks[0].members.len(), 2);
    assert!(marks[0].members.iter().all(|&n| n != 0), "{marks:?}");
    a.copy(std::slice::from_ref(&grp)).unwrap();
    b.set_clipboard(a.clipboard());
    b.set_foreign_source(Some(ForeignSource { document: "folio:path-ref/r9".into(), marks: marks.clone() }));
    let page = first_page(&b);
    let made = b.apply("place", &[
        Command::Paste { page: Some(page), at: None, layer: None, into: None, remember_layers: false },
        Command::Registered(R::LinkPlaced { map_styles: false, date: DATE.into() }),
    ]).unwrap().created;
    let copy = element(&b, &made[0]);
    assert_eq!(copy.matches(" LinkedSourceTableId=").count(), 2, "{copy}");
    assert!(copy.contains(&format!(r#"LinkedSourceTableId="{}""#, marks[0].members[0])));
    let foreign = Foreign::Doc { name: "a.folio".into(), parts: std::sync::Arc::new(a.parts().clone()) };
    let rows = b.content_links_with(&|_| foreign.clone());
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].status, rows[0].name.as_str()), ("normal", "a.folio#<group>"));
}

fn idml_parts(rel: &str) -> BTreeMap<String, String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(rel);
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
    parts
}

/// Links InDesign made to another document (conveyor-c5; the files are InDesign 2025's own IDML of the conveyor-c4 probe b3: a document with
/// a text frame, a rectangle and a story placed from `xsrc.indd` with Create Link, saved after the source's rectangle was turned and saved).
/// InDesign's DOM said then: the text frame NORMAL, the rectangle LINK_OUT_OF_DATE, the story NORMAL (`LinkResourceModified` false / true /
/// false in the saved file). An import turns the address of that `.indd` into a reference; joining it to the project's document of that file
/// gives the same states, the stamp being Folio's (InDesign's own is a number Folio cannot compute).
#[test]
fn links_indesign_made_to_another_document_are_joined_to_the_project_document() {
    let address = "C:/Examples/Folio/artifacts/conveyor-c4-20261023/out/xsrc.indd";
    let mut target = idml_parts("conveyor-c5/indesign-target.idml");
    for t in target.values_mut().filter(|t| t.contains(address)) {
        // what the import does to the address (project/idmlparts.rs): a reference, the number stays
        *t = t.replace(address, "folio:path-ref/r4");
    }
    let mut b = Editor::new(target);
    let source = std::sync::Arc::new(idml_parts("conveyor-c5/indesign-source.idml"));
    let doc = "folio:path-ref/r4";
    let read = |b: &Editor, found: bool| -> Vec<(String, &'static str)> {
        let f = if found { Foreign::Doc { name: "xsrc.folio".into(), parts: source.clone() } } else { Foreign::Unknown };
        let mut rows: Vec<_> = b.content_links_with(&move |_| f.clone()).into_iter().map(|c| (format!("{}:{}", c.kind, c.name), c.status)).collect();
        rows.sort();
        rows
    };
    // the document is only an address: the state is not known
    assert!(read(&b, false).iter().all(|(_, st)| *st == "external"), "{:?}", read(&b, false));
    // read from the document, an InDesign stamp is nothing Folio can compare: every link would show out of date
    assert!(read(&b, true).iter().all(|(_, st)| *st == "outOfDate"), "{:?}", read(&b, true));
    // without the document's parts the command says so and changes nothing
    let was = b.parts().clone();
    let err = b.apply("join", &[Command::Registered(R::ReconnectContentLinks { document: doc.into(), target: None, to: Some("folio:path-ref/r5".into()) })]).unwrap_err();
    assert_eq!(err.code, "notFound");
    assert_eq!(b.parts(), &was);
    b.set_foreign_documents(BTreeMap::from([(doc.to_owned(), source.clone())]));
    // a document no link names: nothing to join
    b.set_foreign_documents(BTreeMap::from([(doc.to_owned(), source.clone()), ("folio:path-ref/r9".to_owned(), source.clone())]));
    let err = b.apply("join", &[Command::Registered(R::ReconnectContentLinks { document: "folio:path-ref/r9".into(), target: None, to: Some("folio:path-ref/r5".into()) })]).unwrap_err();
    assert_eq!(err.code, "notFound");
    // joined: InDesign's verdict of that save
    b.apply("join", &[Command::Registered(R::ReconnectContentLinks { document: doc.into(), target: None, to: Some("folio:path-ref/r5".into()) })]).unwrap();
    let after = read(&b, true);
    let find = |name: &str| after.iter().find(|(n, _)| n.ends_with(name)).map(|(_, st)| *st);
    assert_eq!(find("<rectangle>"), Some("outOfDate"), "{after:?}");
    assert_eq!(after.iter().filter(|(_, st)| *st == "normal").count(), 2, "{after:?}");
    assert!(after.iter().any(|(n, st)| n.starts_with("object:xsrc.folio#") && *st == "normal"), "{after:?}");
    // the links name the reference of the project document now; one undo step gives the address (and InDesign's stamps) back
    assert!(b.content_links().iter().all(|c| c.uri.contains("folio:path-ref/r5?")), "{:?}", b.content_links().iter().map(|c| &c.uri).collect::<Vec<_>>());
    b.undo().unwrap();
    assert_eq!(b.parts(), &was);
    b.redo().unwrap();
    // Update Link takes the changed rectangle from the document and the link matches it
    b.set_foreign_documents(BTreeMap::from([("folio:path-ref/r5".to_owned(), source.clone())]));
    let stale = b.content_links_with(&|_| Foreign::Doc { name: "xsrc.folio".into(), parts: source.clone() }).into_iter().find(|c| c.status == "outOfDate").unwrap();
    b.apply("update", &[Command::Registered(R::UpdateContentLink { link: stale.link.clone(), date: DATE.into(), overwrite: Some(true) })]).unwrap();
    assert!(read(&b, true).iter().all(|(_, st)| *st == "normal"), "{:?}", read(&b, true));
}
