use super::geometry::{outline, paths_bounds};
use super::*;
use folio_model::{Affine, ItemKind, Layer, Page, PathPoint, Point, Rect, Spread, SubPath, TextFrame};
use serde_json::json;

fn rect_path(l: f64, t: f64, r: f64, b: f64) -> Vec<SubPath> {
    let p = |x: f64, y: f64| {
        let a = Point::new(x, y);
        PathPoint { anchor: a, left: a, right: a }
    };
    vec![SubPath { points: vec![p(l, t), p(l, b), p(r, b), p(r, t)], open: false }]
}

fn rect_item(id: &str, l: f64, t: f64, r: f64, b: f64) -> PageItem {
    PageItem { id: id.into(), kind: ItemKind::Rectangle { content: None }, paths: rect_path(l, t, r, b), ..PageItem::default() }
}

fn page(id: &str, index: usize, tx: f64, master: Option<&str>) -> Page {
    Page {
        id: id.into(),
        name: format!("{}", index + 1),
        bounds: Rect { left: 0.0, top: 0.0, right: 600.0, bottom: 800.0 },
        transform: Affine::translate(tx, -400.0),
        master: master.map(str::to_owned),
        index,
        margins: [36.0, 48.0, 36.0, 24.0],
        ..Page::default()
    }
}

/// Two facing pages (x −600…0 and 0…600) on master "M" (two pages as well).
fn doc() -> Document {
    let mut d = Document { layers: vec![
        Layer { id: "top".into(), name: "Top".into(), visible: true, printable: true, order: 1, ..Layer::default() },
        Layer { id: "bottom".into(), name: "Bottom".into(), visible: false, printable: true, order: 0, locked: true, ..Layer::default() },
    ], ..Document::default() };
    d.print.facing_pages = true;
    d.print.facing_pages_stored = true;
    let mut master = Spread { id: "M".into(), name: Some("A-Master".into()), ..Spread::default() };
    master.pages = vec![page("mp1", 0, -600.0, None), page("mp2", 1, 0.0, None)];
    master.items = vec![
        // left master page, overridden on document page 0
        rect_item("m-left", -580.0, -380.0, -500.0, -300.0),
        // right master page
        rect_item("m-right", 20.0, -380.0, 100.0, -300.0),
        // across the spine: shown on both pages
        rect_item("m-wide", -100.0, 0.0, 100.0, 50.0),
    ];
    d.master_spreads.push(master);
    let mut s = Spread { id: "S".into(), ..Spread::default() };
    let mut p0 = page("p0", 0, -600.0, Some("M"));
    p0.master_overrides = vec!["m-left".into()];
    s.pages = vec![p0, page("p1", 1, 0.0, Some("M"))];
    let mut wide = rect_item("wide", -400.0, -100.0, 300.0, 100.0);
    wide.layer = Some("top".into());
    // its centre (−50, 0) is left of the spine: listed on page 0 only
    let mut low = rect_item("low", 100.0, 100.0, 200.0, 200.0);
    low.layer = Some("bottom".into());
    low.name = Some("Low box".into());
    // rotated 90° about the spread origin, then moved right: a 100 × 20 bar standing up
    let mut rotated = rect_item("rot", 0.0, 0.0, 100.0, 20.0);
    rotated.transform = Affine::new(0.0, 1.0, -1.0, 0.0, 300.0, -200.0);
    rotated.locked = true;
    let mut frame = rect_item("tf", 50.0, -350.0, 250.0, -150.0);
    frame.kind = ItemKind::TextFrame(TextFrame { story: "st".into(), columns: 2, ..TextFrame::default() });
    s.items = vec![wide, low, rotated, frame];
    d.spreads.push(s);
    d
}

fn pages_of(d: &Document, page: usize) -> Value {
    let layout = DocumentLayout { overset: vec!["st".into()], ..DocumentLayout::default() };
    let colors = Colors::unmanaged();
    let index = Index::new(d, &layout, &colors);
    serde_json::to_value(page_items(d, &colors, &index, page).unwrap()).unwrap()
}

fn ids(v: &Value) -> Vec<&str> {
    v.as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect()
}

#[test]
fn an_item_across_two_pages_is_listed_once_on_the_page_of_its_centre() {
    let d = doc();
    let p0 = pages_of(&d, 0);
    let p1 = pages_of(&d, 1);
    assert_eq!(ids(&p0["items"]), ["wide"]);
    // bottom layer first
    assert_eq!(ids(&p1["items"]), ["low", "rot", "tf"]);
    // page coordinates of page 0: its top-left is (−600, −400)
    assert_eq!(p0["items"][0]["bounds"], json!([200.0, 300.0, 900.0, 500.0]));
    assert_eq!(p1["items"][0]["bounds"], json!([100.0, 500.0, 200.0, 600.0]));
    // z is one order over the spread: masters of both pages (1 + 2), then the items
    assert_eq!(p0["items"][0]["z"], json!(6));
    assert_eq!(p1["items"][0]["z"], json!(3));
    assert_eq!(p1["items"][0]["visible"], json!(false));
    assert_eq!(p1["items"][0]["name"], json!("Low box"));
    assert_eq!(p1["items"][0]["layer"], json!("bottom"));
    assert_eq!(p1["side"], json!("right"));
    assert_eq!(p0["side"], json!("left"));
    let tf = &p1["items"][2];
    assert_eq!(tf["kind"], json!("textFrame"));
    assert_eq!(tf["text"]["overset"], json!(true));
    assert_eq!(tf["text"]["columns"], json!(2));
}

#[test]
fn anchored_children_take_a_z_position_when_querying_the_other_page() {
    use folio_model::layout::{FrameText, LaidLine, PlacedObject};
    use folio_model::{Paragraph, Story, TextRun};

    let mut d = doc();
    let anchor = rect_item("anchor", 0.0, 0.0, 8.0, 8.0);
    let run = TextRun { text: "\u{fffc}".into(), inline: vec![anchor], ..Default::default() };
    let paragraph = Paragraph { runs: vec![run], ..Default::default() };
    let story = Story { id: "st".into(), paragraphs: vec![paragraph], ..Default::default() };
    d.stories.insert(story.id.clone(), story);

    let object = PlacedObject { item: "anchor".into(), ..Default::default() };
    let line = LaidLine { objects: vec![object], ..Default::default() };
    let mut layout = DocumentLayout::default();
    layout.frames.insert("tf".into(), FrameText { lines: vec![line], ..Default::default() });
    let colors = Colors::unmanaged();
    let index = Index::new(&d, &layout, &colors);
    let page = |n| serde_json::to_value(page_items(&d, &colors, &index, n).unwrap()).unwrap();
    let right = page(1);
    let left = page(0);
    let child = &right["items"][2]["children"][0];
    assert_eq!(child["id"], "anchor");
    assert_eq!(left["items"][0]["id"], "wide");
    assert_eq!(child["z"].as_u64().unwrap() + 1, left["items"][0]["z"].as_u64().unwrap());
}

#[test]
fn master_items_leave_out_overridden_ones_and_show_where_they_reach_the_page() {
    let d = doc();
    let p0 = pages_of(&d, 0);
    let p1 = pages_of(&d, 1);
    assert_eq!(ids(&p0["masterItems"]), ["m-wide"]);
    assert_eq!(ids(&p1["masterItems"]), ["m-right", "m-wide"]);
    assert_eq!(p0["masterItems"][0]["master"], json!(true));
    assert_eq!(p1["masterItems"][0]["bounds"], json!([20.0, 20.0, 100.0, 100.0]));
    assert_eq!(p1["masterItems"][1]["bounds"], json!([-100.0, 400.0, 100.0, 450.0]));
    assert_eq!(p0["masterItems"][0]["z"], json!(0));
    assert_eq!(p1["masterItems"][0]["z"], json!(1));
}

#[test]
fn a_rotated_item_has_its_transform_and_bounds_in_page_coordinates() {
    let d = doc();
    let p1 = pages_of(&d, 1);
    let rot = &p1["items"][1];
    assert_eq!(rot["id"], json!("rot"));
    // inner → spread (0 1 −1 0 300 −200), then spread → page (0, +400)
    assert_eq!(rot["transform"], json!([0.0, 1.0, -1.0, 0.0, 300.0, 200.0]));
    assert_eq!(rot["inner"], json!([0.0, 0.0, 100.0, 20.0]));
    // (x, y) → (300 − y, 200 + x): x 280…300, y 200…300
    assert_eq!(rot["bounds"], json!([280.0, 200.0, 300.0, 300.0]));
    assert_eq!(rot["locked"], json!(true));
    // still an axis-aligned rectangle: no outline needed
    assert_eq!(rot["outline"], Value::Null);
    let mut d2 = d.clone();
    d2.spreads[0].items[2].transform = Affine::new(0.6, 0.8, -0.8, 0.6, 300.0, -200.0);
    let p = pages_of(&d2, 1);
    let rot = &p["items"][1];
    let o = rot["outline"][0].as_array().unwrap();
    assert_eq!(o.len(), 4);
    // (100, 20) → (300 + 60 − 16, −200 + 80 + 12) in the spread, y + 400 on the page
    assert_eq!(o[2], json!([344.0, 292.0]));
    assert_eq!(rot["bounds"], json!([284.0, 200.0, 360.0, 292.0]));
}

#[test]
fn margins_columns_and_guides_are_in_page_coordinates() {
    let mut d = doc();
    d.spreads[0].pages[0].columns = folio_model::PageColumns { count: 2, gutter: 12.0, positions: Some(vec![0.0, 258.0, 270.0, 528.0]), direction: "Horizontal".into() };
    d.spreads[0].guides = vec![
        folio_model::Guide { id: "g1".into(), horizontal: false, position: -300.0, page: Some(0), ..Default::default() },
        folio_model::Guide { id: "g2".into(), horizontal: true, position: 0.0, page: None, fit_to_page: false, layer: Some("bottom".into()), ..Default::default() },
        folio_model::Guide { id: "g3".into(), horizontal: true, position: 10.0, page: Some(1), ..Default::default() },
    ];
    let p0 = pages_of(&d, 0);
    assert_eq!(p0["margins"], json!([36.0, 48.0, 36.0, 24.0]));
    // a left-hand page: the live area starts at the stored right (outside) margin
    assert_eq!(p0["columns"]["positions"], json!([24.0, 282.0, 294.0, 552.0]));
    let g = p0["guides"].as_array().unwrap();
    assert_eq!(g.len(), 2);
    assert_eq!(g[0]["orientation"], json!("vertical"));
    assert_eq!(g[0]["location"], json!(300.0));
    assert_eq!(g[1]["location"], json!(400.0));
    assert_eq!(g[1]["fitToPage"], json!(false));
    // the guide's layer is locked
    assert_eq!(g[1]["locked"], json!(true));
    assert_eq!(g[0]["color"], json!([0, 210, 255]));
    let p1 = pages_of(&d, 1);
    assert_eq!(p1["guides"].as_array().unwrap().len(), 2);
}

#[test]
fn document_info_lists_layers_top_first_and_pages_with_their_side() {
    let d = doc();
    let info = document_info(&d, &Colors::unmanaged(), true);
    assert_eq!(ids(&info["layers"]), ["top", "bottom"]);
    assert_eq!(info["layers"][0]["items"], json!(1));
    assert_eq!(info["layers"][1]["locked"], json!(true));
    assert_eq!(info["layers"][0]["color"], json!([79, 153, 255]));
    assert_eq!(info["pages"][1], json!({"id": "p1", "index": 1, "master": "M", "spreadIndex": 0, "spreadOffset": [600.0, 0.0], "spreadOrigin":[0.0,-400.0], "side": "right"}));
    assert_eq!(info["masters"][0], json!({"id": "M", "name": "A-Master", "prefix": "A", "baseName": "Master", "pages": 2, "basedOn": null}));
    assert_eq!(info["facingPages"], json!(true));
}

#[test]
fn replies_are_json_headers_and_detailed_outlines_are_capped() {
    let mut d = doc();
    // a 500-point zigzag: listed with at most 64 points
    let pts = (0..500).map(|i| {
        let a = Point::new(i as f64, if i % 2 == 0 { 0.0 } else { 5.0 });
        PathPoint { anchor: a, left: a, right: a }
    });
    let mut zig = rect_item("zig", 0.0, 0.0, 1.0, 1.0);
    zig.kind = ItemKind::Polygon { content: None };
    zig.paths = vec![SubPath { points: pts.collect(), open: true }];
    zig.transform = Affine::translate(10.0, 0.0);
    d.spreads[0].items.push(zig);
    let layout = DocumentLayout::default();
    let colors = Colors::unmanaged();
    let index = std::sync::OnceLock::new();
    let text = command(&json!({"cmd": "pageItems", "page": 1}), &d, &layout, &colors, &index).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["type"], json!("pageItems"));
    assert!(v["ms"].as_f64().is_some());
    let zig = v["items"]["items"].as_array().unwrap().iter().find(|i| i["id"] == "zig").unwrap();
    let o = zig["outline"][0].as_array().unwrap();
    assert!(o.len() <= 65, "{}", o.len());
    // the ends of an open path stay
    assert_eq!(o[0], json!([10.0, 400.0]));
    assert_eq!(o[o.len() - 1], json!([509.0, 405.0]));
    assert_eq!(zig["bounds"], json!([10.0, 400.0, 509.0, 405.0]));
    let info: Value = serde_json::from_str(&command(&json!({"cmd": "documentInfo"}), &d, &layout, &colors, &index).unwrap()).unwrap();
    assert_eq!(info["type"], json!("documentInfo"));
    assert_eq!(info["info"]["rulerOrigin"], json!("SpreadOrigin"));
    assert_eq!(info["info"]["zeroPoint"], json!([0.0, 0.0]));
    assert!(command(&json!({"cmd": "pageItems", "page": 9}), &d, &layout, &colors, &index).is_err());
}

#[test]
fn curves_bound_at_their_extremes_not_their_handles() {
    // a circle of radius 10 around (0, 0) from four arcs
    let k = 10.0 * folio_model::geom::KAPPA;
    let pt = |x: f64, y: f64, lx: f64, ly: f64, rx: f64, ry: f64| PathPoint { anchor: Point::new(x, y), left: Point::new(lx, ly), right: Point::new(rx, ry) };
    let circle = vec![SubPath {
        points: vec![pt(0.0, -10.0, -k, -10.0, k, -10.0), pt(10.0, 0.0, 10.0, -k, 10.0, k), pt(0.0, 10.0, k, 10.0, -k, 10.0), pt(-10.0, 0.0, -10.0, k, -10.0, -k)],
        open: false,
    }];
    // rotated 45°: handles would reach 10·(1 + κ)/√2 ≈ 10.97; the circle stays at 10
    let c = std::f64::consts::FRAC_1_SQRT_2;
    let b = paths_bounds(&circle, &Affine::new(c, c, -c, c, 0.0, 0.0)).unwrap();
    assert!((b.right - 10.0).abs() < 0.01 && (b.top + 10.0).abs() < 0.01, "{b:?}");
    // four anchors, two points inside each of the four arcs
    let o = outline(&circle, &Affine::IDENTITY, Some(&b)).unwrap();
    assert_eq!(o[0].len(), 12);
}
