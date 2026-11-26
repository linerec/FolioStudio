//! screens-doc: the Pen tool on open paths (continue, join, close), a group child moved to another spread by itself, a copy made on another
//! spread in one step, and the copies the engine refuses without changing anything.
use folio_edit::{Command, Editor, PathPt, xml::Index};
use serde_json::json;
use std::{collections::BTreeMap, io::Read, path::Path};

type P = (f64, f64);

fn fixture() -> (Editor, String) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut page = String::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        if name.starts_with("Spreads/") && page.is_empty() {
            let ix = Index::scan(&t).unwrap();
            page = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Page").and_then(|i| ix.attr(&t, i, "Self")).unwrap().into();
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), page)
}

/// `(anchor, left, right)` of the points of an item's first path, and whether the path is open; None when the item is gone.
fn path(e: &Editor, id: &str) -> Option<(Vec<(P, P, P)>, bool)> {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) else { continue };
        let pair = |p: usize, k: &str| {
            let v: Vec<f64> = ix.attr(t, p, k).unwrap().split_whitespace().map(|x| x.parse().unwrap()).collect();
            (v[0], v[1])
        };
        let open = ix.own_descendants(t, i, "GeometryPathType").first().map(|&g| ix.attr(t, g, "PathOpen") == Some("true")).unwrap_or(false);
        let pts = ix.own_descendants(t, i, "PathPointType").into_iter().map(|p| (pair(p, "Anchor"), pair(p, "LeftDirection"), pair(p, "RightDirection"))).collect();
        return Some((pts, open));
    }
    None
}

fn kind(e: &Editor, id: &str) -> Option<String> {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) {
            return Some(ix.local(t, i).to_owned());
        }
    }
    None
}

fn open_path(e: &mut Editor, page: &str, pts: &[P]) -> String {
    let points = pts.iter().map(|&(x, y)| PathPt { anchor: [x, y], left: None, right: None }).collect();
    e.apply("path", &[Command::CreatePath { page: page.into(), points, closed: false, layer: None }]).unwrap().created[0].clone()
}

fn cmd(v: serde_json::Value) -> Command {
    serde_json::from_value(v).unwrap()
}

fn near(a: P, b: P) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

fn minus(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

#[test]
fn the_pen_continues_an_open_path_at_its_end() {
    let (mut e, page) = fixture();
    let id = open_path(&mut e, &page, &[(100., 100.), (200., 100.)]);
    let o = e.apply("pen", &[cmd(json!({"op": "penContinue", "id": id, "page": page, "at_start": false,
        "anchors": [[260., 160.], [330., 120.]], "lefts": [[260., 160.], [300., 130.]], "rights": [[260., 160.], [330., 120.]], "join": "", "join_at_start": false}))]).unwrap();
    assert_eq!(o.created, vec![id.clone()]);
    let (pts, open) = path(&e, &id).unwrap();
    assert!(open);
    assert_eq!(pts.len(), 4);
    // the old points are where they were, the new ones are where they were put (page coordinates; the item's own are the page's plus one shift)
    assert!(near(minus(pts[2].0, pts[1].0), (60., 60.)));
    assert!(near(minus(pts[3].0, pts[1].0), (130., 20.)));
    assert!(near(minus(pts[3].1, pts[3].0), (-30., 10.)), "a handle of the new point is kept");
    e.undo().unwrap();
    assert_eq!(path(&e, &id).unwrap().0.len(), 2);
}

#[test]
fn the_pen_continues_an_open_path_at_its_start_and_the_path_keeps_its_direction() {
    let (mut e, page) = fixture();
    let id = open_path(&mut e, &page, &[(100., 100.), (200., 100.)]);
    e.apply("pen", &[cmd(json!({"op": "penContinue", "id": id, "page": page, "at_start": true,
        "anchors": [[60., 60.]], "lefts": [[60., 60.]], "rights": [[60., 60.]], "join": "", "join_at_start": false}))]).unwrap();
    let (pts, _) = path(&e, &id).unwrap();
    assert_eq!(pts.len(), 3);
    // the new point is in front, the old end is still the last
    assert!(near(minus(pts[0].0, pts[1].0), (-40., -40.)));
    assert!(near(minus(pts[2].0, pts[1].0), (100., 0.)));
}

#[test]
fn the_pen_joins_two_open_paths_where_it_is_clicked() {
    let (mut e, page) = fixture();
    let a = open_path(&mut e, &page, &[(100., 100.), (200., 100.)]);
    let b = open_path(&mut e, &page, &[(300., 200.), (400., 220.)]);
    // from A's end to B's far end: B is turned round and goes on after the new point
    e.apply("pen", &[cmd(json!({"op": "penContinue", "id": a, "page": page, "at_start": false,
        "anchors": [[250., 150.]], "lefts": [[250., 150.]], "rights": [[250., 150.]], "join": b, "join_at_start": false}))]).unwrap();
    assert!(path(&e, &b).is_none(), "the joined path is gone");
    let (pts, open) = path(&e, &a).unwrap();
    assert!(open);
    assert_eq!(pts.len(), 5);
    assert!(near(minus(pts[3].0, pts[1].0), (200., 120.)), "B's far end first");
    assert!(near(minus(pts[4].0, pts[1].0), (100., 100.)));
    e.undo().unwrap();
    assert!(path(&e, &b).is_some() && path(&e, &a).unwrap().0.len() == 2);
}

#[test]
fn the_pen_closes_a_path_on_its_other_end() {
    let (mut e, page) = fixture();
    let id = open_path(&mut e, &page, &[(100., 100.), (200., 100.), (150., 200.)]);
    e.apply("pen", &[cmd(json!({"op": "penContinue", "id": id, "page": page, "at_start": false,
        "anchors": [], "lefts": [], "rights": [], "join": id, "join_at_start": true}))]).unwrap();
    let (pts, open) = path(&e, &id).unwrap();
    assert!(!open, "the path is closed");
    assert_eq!(pts.len(), 3);
    assert!(kind(&e, &id).is_some());
}

#[test]
fn the_pen_refuses_what_it_cannot_continue_and_changes_nothing() {
    let (mut e, page) = fixture();
    let id = open_path(&mut e, &page, &[(100., 100.), (200., 100.)]);
    let before = e.parts().clone();
    // nothing to add, a handle list of another length, a join with something that is no path
    for v in [
        json!({"op": "penContinue", "id": id, "page": page, "at_start": false, "anchors": [], "lefts": [], "rights": [], "join": "", "join_at_start": false}),
        json!({"op": "penContinue", "id": id, "page": page, "at_start": false, "anchors": [[1., 1.]], "lefts": [], "rights": [], "join": "", "join_at_start": false}),
        json!({"op": "penContinue", "id": id, "page": page, "at_start": false, "anchors": [[1., 1.]], "lefts": [[1., 1.]], "rights": [[1., 1.]], "join": "nothere", "join_at_start": false}),
    ] {
        assert!(e.apply("pen", &[cmd(v)]).is_err());
        assert_eq!(e.parts(), &before);
    }
}

/// The second spread of the fixture and the id of an object of the first spread that is inside a group.
fn spreads_and_group_child(e: &Editor) -> (String, String, String) {
    let (mut s1, mut s2, mut child) = (String::new(), String::new(), String::new());
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let spread = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Spread").find_map(|i| ix.attr(t, i, "Self")).unwrap().to_owned();
        if let Some(g) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Group") {
            s1 = spread;
            child = ix.children(g).iter().copied().find(|&c| ix.local(t, c) == "Rectangle").and_then(|c| ix.attr(t, c, "Self")).unwrap().to_owned();
        } else {
            s2 = spread;
        }
    }
    (s1, s2, child)
}

/// The spread element (its id) an item is in, and whether its parent is a group.
fn home(e: &Editor, id: &str) -> Option<(String, bool)> {
    for (name, t) in e.parts() {
        if !name.starts_with("Spreads/") {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        let Some(i) = (0..ix.nodes.len()).find(|&i| ix.attr(t, i, "Self") == Some(id)) else { continue };
        let spread = ix.ancestors(i).filter(|&a| ix.local(t, a) == "Spread").find_map(|a| ix.attr(t, a, "Self")).unwrap().to_owned();
        let in_group = ix.nodes[i].parent.is_some_and(|p| ix.local(t, p) == "Group");
        return Some((spread, in_group));
    }
    None
}

#[test]
fn a_group_child_moved_alone_to_another_spread_leaves_the_group() {
    let (mut e, _) = fixture();
    let (s1, s2, child) = spreads_and_group_child(&e);
    assert_eq!(home(&e, &child), Some((s1.clone(), true)));
    e.apply("move", &[Command::MoveToSpread { ids: vec![child.clone()], spread: s2.clone(), by: [10., 20.] }]).unwrap();
    assert_eq!(home(&e, &child), Some((s2.clone(), false)));
    e.undo().unwrap();
    assert_eq!(home(&e, &child), Some((s1, true)));
}

#[test]
fn an_alt_copy_on_another_spread_is_one_step_and_leaves_the_original() {
    let (mut e, _) = fixture();
    let (s1, s2, child) = spreads_and_group_child(&e);
    let before = e.parts().clone();
    let o = e.apply("copy", &[Command::Duplicate { ids: vec![child.clone()], by: [0., 0.] }, Command::MoveToSpread { ids: vec!["@copies".into()], spread: s2.clone(), by: [5., 5.] }]).unwrap();
    assert_eq!(home(&e, &child), Some((s1, true)), "the original stays where it was");
    let copy = o.created.last().unwrap().clone();
    assert_ne!(copy, child);
    assert_eq!(home(&e, &copy).map(|h| h.0), Some(s2));
    e.undo().unwrap();
    assert_eq!(e.parts(), &before, "one undo takes the whole copy back");
}

fn synthetic(story_body: &str, frame: &str) -> Editor {
    Editor::new(BTreeMap::from([
        ("designmap.xml".into(), r#"<Document Self="d" StoryList="u20"><idPkg:Story src="Stories/Story_u20.xml"/></Document>"#.to_owned()),
        ("Stories/Story_u20.xml".into(), format!(r#"<idPkg:Story><Story Self="u20"><ParagraphStyleRange><CharacterStyleRange>{story_body}</CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>"#)),
        ("Spreads/Spread_u1.xml".into(), format!(r#"<idPkg:Spread><Spread Self="u1">{frame}</Spread></idPkg:Spread>"#)),
    ]))
}

/// A copy that the engine makes (build-b: text on a path, a story with an anchored object): a new story part with the copied
/// objects' ids, the original untouched, one undo step.
fn copied(mut e: Editor, id: &str, inside: &str) {
    let before = e.parts().clone();
    e.apply("copy", &[Command::Duplicate { ids: vec![id.into()], by: [10., 10.] }]).unwrap();
    let new: Vec<_> = e.parts().keys().filter(|k| k.starts_with("Stories/") && !before.contains_key(*k)).cloned().collect();
    assert_eq!(new.len(), 1, "{new:?}");
    let story = &e.parts()[&new[0]];
    assert!(story.contains(inside) && !story.contains("Self=\"u20\"") && !story.contains("Self=\"u30\""), "{story}");
    assert_eq!(e.parts()["Stories/Story_u20.xml"], before["Stories/Story_u20.xml"]);
    e.undo().unwrap();
    assert_eq!(e.parts(), &before);
}

#[test]
fn text_on_a_path_is_duplicated_with_its_own_story() {
    let e = synthetic("<Content>On a path</Content>", r#"<GraphicLine Self="u10" ItemTransform="1 0 0 1 0 0"><TextPath Self="u11" ParentStory="u20" PreviousTextFrame="n" NextTextFrame="n"/></GraphicLine>"#);
    copied(e, "u10", "On a path");
}

#[test]
fn a_story_with_an_anchored_object_is_duplicated_with_new_ids() {
    let e = synthetic(r#"<Content>x</Content><Rectangle Self="u30" ItemTransform="1 0 0 1 0 0"/>"#, r#"<TextFrame Self="u10" ParentStory="u20" PreviousTextFrame="n" NextTextFrame="n" ItemTransform="1 0 0 1 0 0"/>"#);
    copied(e, "u10", "<Rectangle Self=");
}
