//! Drawing in the view of a turned spread (`createInView`, rules/drawtools.toml drawtools.turned.*): what is made is checked
//! against InDesign by the oracle scenarios pages-turned/turn-* and pages-xform/xform-*; here, what is refused.
use folio_edit::{commands::RegisteredCommand, Editor, xml::Index};
use std::collections::BTreeMap;
use std::path::Path;

fn fixture(name: &str, spread_transform: Option<(&str, &str)>, page_transform: Option<(&str, &str)>) -> (Editor, Vec<String>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(name);
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        if entry.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut entry, &mut s).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), s);
    }
    for (tag, (id, matrix)) in [("<Spread Self=\"", spread_transform), ("<Page Self=\"", page_transform)].into_iter().filter_map(|(t, v)| v.map(|v| (t, v))) {
        for text in parts.values_mut() {
            let Some(at) = text.find(&format!("{tag}{id}\"")) else { continue };
            let from = text[at..].find(" ItemTransform=\"").unwrap() + at + " ItemTransform=\"".len();
            let to = text[from..].find('"').unwrap() + from;
            text.replace_range(from..to, matrix);
        }
    }
    let mut pages = Vec::new();
    for t in parts.values().filter(|t| t.contains("<idPkg:Spread ")) {
        let ix = Index::scan(t).unwrap();
        pages.extend((0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Page").filter_map(|i| ix.attr(t, i, "Self").map(str::to_owned)));
    }
    (Editor::new(parts), pages)
}

fn draw(page: &str, kind: &str, rect: [f64; 4]) -> RegisteredCommand {
    RegisteredCommand::CreateInView { page: page.into(), kind: kind.into(), rect, layer: String::new(), sides: 6, inset: 0. }
}

fn refused(ed: &mut Editor, command: RegisteredCommand) -> String {
    let before = ed.parts().clone();
    let error = ed.apply("draw", &[command.into()]).expect_err("drawing should be refused");
    assert_eq!(ed.parts(), &before, "a refused drawing left something behind");
    error.to_string()
}

#[test]
fn a_turned_spread_takes_shapes() {
    let (mut ed, pages) = fixture("pages-turned.idml", None, None);
    for kind in ["rectangle", "ellipse", "line", "textFrame", "polygon"] {
        ed.apply("draw", &[draw(&pages[0], kind, [40., 60., 190., 150.]).into()]).unwrap_or_else(|e| panic!("{kind}: {e}"));
    }
}

#[test]
fn placeholder_frames_on_a_turned_or_scaled_spread_are_refused() {
    let (mut ed, pages) = fixture("pages-turned.idml", None, None);
    for kind in ["rectangleFrame", "ellipseFrame", "polygonFrame"] {
        assert!(refused(&mut ed, draw(&pages[0], kind, [40., 60., 190., 150.])).contains("placeholder frame"), "{kind}");
    }
    let (mut ed, pages) = fixture("pages-xform.idml", None, None);
    assert!(refused(&mut ed, draw(&pages[4], "rectangleFrame", [40., 60., 190., 150.])).contains("placeholder frame"));
}

#[test]
fn other_turns_and_turned_pages_are_refused() {
    // 30 degrees: InDesign's script draws a skewed quad there (nothing to compare)
    let (mut ed, pages) = fixture("pages.idml", Some(("u147", "0.8660254037844387 0.5 -0.5 0.8660254037844387 0 0")), None);
    assert!(refused(&mut ed, draw(&pages[0], "rectangle", [40., 60., 190., 150.])).contains("has not been measured"));
    // a stretched spread, a mirrored one
    let (mut ed, pages) = fixture("pages.idml", Some(("u147", "2 0 0 1 0 0")), None);
    assert!(refused(&mut ed, draw(&pages[0], "rectangle", [40., 60., 190., 150.])).contains("has not been measured"));
    let (mut ed, pages) = fixture("pages.idml", Some(("u147", "-1 0 0 1 0 0")), None);
    assert!(refused(&mut ed, draw(&pages[0], "rectangle", [40., 60., 190., 150.])).contains("has not been measured"));
    // a page with its own quarter turn is drawn on since pen-c3, from its bounding-box corner and upright on the spread (tests/page_turned_draw.rs);
    // on a spread that is turned as well it is still refused
    let (mut ed, pages) = fixture("pages.idml", None, Some(("u14c", "0 1 -1 0 0 -396")));
    ed.apply("draw", &[draw(&pages[0], "rectangle", [40., 60., 190., 150.]).into()]).expect("a turned page is drawn on");
    let (mut ed, pages) = fixture("pages.idml", Some(("u147", "0 1 -1 0 0 0")), Some(("u14c", "0 1 -1 0 0 -396")));
    assert!(refused(&mut ed, draw(&pages[0], "rectangle", [40., 60., 190., 150.])).contains("has not been measured"));
}

#[test]
fn the_view_and_the_page_agree_on_an_upright_spread() {
    let (mut a, pages) = fixture("pages.idml", None, None);
    let (mut b, _) = fixture("pages.idml", None, None);
    a.apply("draw", &[draw(&pages[1], "ellipse", [40., 60., 190., 150.]).into()]).unwrap();
    b.apply("draw", &[folio_edit::Command::CreateItem { page: pages[1].clone(), kind: folio_edit::ItemKind::Ellipse, rect: [40., 60., 190., 150.], layer: None, sides: 6 }]).unwrap();
    assert_eq!(a.parts(), b.parts());
}
