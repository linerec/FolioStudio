use super::*;

/// An uncompressed 8-bit RGB PSD with one alpha channel, `res` as its image resources.
fn psd(w: usize, h: usize, gray: impl Fn(usize, usize) -> u8, alpha: impl Fn(usize, usize) -> u8, res: &[u8]) -> Vec<u8> {
    let mut b = b"8BPS\0\x01\0\0\0\0\0\0\0\x04".to_vec();
    b.extend((h as u32).to_be_bytes());
    b.extend((w as u32).to_be_bytes());
    b.extend([0, 8, 0, 3, 0, 0, 0, 0]);
    b.extend((res.len() as u32).to_be_bytes());
    b.extend(res);
    b.extend([0, 0, 0, 0, 0, 0]);
    for _ in 0..3 { for y in 0..h { for x in 0..w { b.push(gray(x, y)); } } }
    for y in 0..h { for x in 0..w { b.push(alpha(x, y)); } }
    b
}

fn block(id: u16, name: &[u8], data: &[u8]) -> Vec<u8> {
    let mut b = b"8BIM".to_vec();
    b.extend(id.to_be_bytes());
    b.push(name.len() as u8);
    b.extend(name);
    if (name.len() + 1) % 2 == 1 { b.push(0); }
    b.extend((data.len() as u32).to_be_bytes());
    b.extend(data);
    if data.len() % 2 == 1 { b.push(0); }
    b
}

fn req(kind: &str) -> Request {
    Request { kind: kind.into(), threshold: 25.0, tolerance: 2.0, ..Default::default() }
}

fn poly_area(s: &Subpath) -> f64 {
    let p: Vec<(f64, f64)> = s.points.iter().map(|p| p[1]).collect();
    (0..p.len()).map(|i| { let (a, b) = (p[i], p[(i + 1) % p.len()]); a.0 * b.1 - b.0 * a.1 }).sum::<f64>() / 2.0
}

fn disc(x: usize, y: usize) -> bool {
    let (dx, dy) = (x as f64 + 0.5 - 50.0, y as f64 + 0.5 - 40.0);
    dx * dx + dy * dy <= 900.0
}

#[test]
fn an_image_with_no_white_is_clipped_by_its_bounds() {
    let b = psd(64, 64, |_, _| 128, |_, _| 0, b"");
    let subs = extract(&b, &req("DetectEdges"), [0.0, 0.0, 64.0, 64.0]).unwrap();
    assert_eq!(subs.len(), 1);
    let anchors: Vec<_> = subs[0].points.iter().map(|p| p[1]).collect();
    assert_eq!(anchors.len(), 4, "{anchors:?}");
    for c in [(0.0, 0.0), (64.0, 0.0), (64.0, 64.0), (0.0, 64.0)] { assert!(anchors.contains(&c), "{anchors:?}"); }
    assert!(subs[0].points.iter().all(|p| p[0] == p[1] && p[2] == p[1]), "straight sides");
}

#[test]
fn detect_edges_traces_the_dark_shape_and_tolerance_smooths_it() {
    let b = psd(100, 80, |x, y| if disc(x, y) { 40 } else { 255 }, |_, _| 0, b"");
    let mut r = req("DetectEdges");
    r.tolerance = 0.0;
    let exact = extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap();
    assert_eq!(exact.len(), 1);
    let a = poly_area(&exact[0]);
    // the pixel outline: the cells whose centres are in the disc, clockwise on the page (positive in y-down)
    let cells = (0..80).flat_map(|y| (0..100).map(move |x| (x, y))).filter(|&(x, y)| disc(x, y)).count() as f64;
    assert_eq!(a, cells);
    r.tolerance = 2.0;
    let smooth = extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap();
    assert!(smooth[0].points.len() < exact[0].points.len() / 4, "{} vs {}", smooth[0].points.len(), exact[0].points.len());
    // the scale of the graphic: 2 pt per pixel
    let big = extract(&b, &r, [10.0, 0.0, 210.0, 160.0]).unwrap();
    assert!((big[0].points[0][1].0 - (10.0 + 2.0 * smooth[0].points[0][1].0)).abs() < 1e-9);
}

#[test]
fn threshold_inset_and_inverting() {
    let b = psd(100, 80, |x, y| if disc(x, y) { 240 } else { 255 }, |_, _| 0, b"");
    // darkness 15: hidden at the threshold 25, kept at 10
    assert!(extract(&b, &req("DetectEdges"), [0.0, 0.0, 100.0, 80.0]).is_err());
    let mut r = req("DetectEdges");
    r.threshold = 10.0;
    r.tolerance = 0.0;
    let base = poly_area(&extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap()[0]);
    r.inset = 3.0;
    let inset = poly_area(&extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap()[0]);
    let expect = std::f64::consts::PI * 27.0 * 27.0;
    assert!((inset - expect).abs() < expect * 0.05, "{inset} {expect}");
    r.inset = -4.0;
    let grown = poly_area(&extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap()[0]);
    assert!(grown > base && (grown - std::f64::consts::PI * 34.0 * 34.0).abs() < 200.0, "{grown}");
    r.inset = 0.0;
    r.invert = true;
    let inv = extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap();
    assert_eq!(inv.len(), 2);
    assert_eq!(inv[0].points.iter().map(|p| p[1]).collect::<Vec<_>>(), vec![(0.0, 0.0), (100.0, 0.0), (100.0, 80.0), (0.0, 80.0), (0.0, 0.0)]);
}

#[test]
fn holes_only_with_inside_edges_and_restricted_to_the_frame() {
    let b = psd(100, 80, |x, y| if disc(x, y) && !(40..60).contains(&x) { 40 } else if disc(x, y) && !(30..50).contains(&y) { 40 } else { 255 }, |_, _| 0, b"");
    let mut r = req("DetectEdges");
    r.tolerance = 0.0;
    assert_eq!(extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap().len(), 1);
    r.inside_edges = true;
    let both = extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap();
    assert_eq!(both.len(), 2);
    assert!(poly_area(&both[0]) * poly_area(&both[1]) < 0.0, "the hole runs the other way");
    r.inside_edges = false;
    r.frame = Some([0.0, 0.0, 50.0, 80.0]);
    let cut = extract(&b, &r, [0.0, 0.0, 100.0, 80.0]).unwrap();
    assert!(cut[0].points.iter().all(|p| p[1].0 <= 50.0));
}

#[test]
fn alpha_channels_and_photoshop_paths_by_name() {
    let mut path = vec![0u8; 26];
    path[1] = 6;
    let mut len = vec![0u8; 26];
    len[3] = 3;
    path.extend(len);
    let fx = |v: f64| ((v * f64::from(1 << 24)).round() as i32).to_be_bytes();
    for (pre, a, post) in [((0.1, 0.8), (0.1, 0.9), (0.2, 0.95)), ((0.4, 0.1), (0.5, 0.05), (0.6, 0.1)), ((0.8, 0.95), (0.9, 0.9), (0.9, 0.8))] {
        path.extend([0, 2]);
        for (x, y) in [pre, a, post] { path.extend(fx(y)); path.extend(fx(x)); }
    }
    let res = [block(1006, b"", b"\x05Shape"), block(2000, b"Tri", &path)].concat();
    let b = psd(200, 150, |_, _| 255, |x, y| if (20..180).contains(&x) && (10..140).contains(&y) { 255 } else { 0 }, &res);
    assert_eq!(sources(&b), Sources { alpha_channels: vec!["Shape".into()], paths: vec!["Tri".into()] });
    let mut r = req("PhotoshopPath");
    r.path_name = "Tri".into();
    let tri = extract(&b, &r, [0.0, 0.0, 200.0, 150.0]).unwrap();
    // measured in InDesign 2025: the point before an anchor is its left direction
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
    assert!(near(tri[0].points[0][0], (20.0, 120.0)) && near(tri[0].points[0][1], (20.0, 135.0)) && near(tri[0].points[0][2], (40.0, 142.5)), "{:?}", tri[0].points[0]);
    r.kind = "AlphaChannel".into();
    r.path_name = "Shape".into();
    r.tolerance = 0.0;
    let a = extract(&b, &r, [0.0, 0.0, 200.0, 150.0]).unwrap();
    assert_eq!(poly_area(&a[0]), 160.0 * 130.0);
}

/// The cases of `artifacts/build-b-20261023/tools/clip-probe.jsx` on its image, as JSON for `clip_dump.py --folio-json`
/// (`FOLIO_CLIP_PROBE=<psd> FOLIO_CLIP_OUT=<json> cargo test -p folio-render --lib clip_probe -- --ignored`).
#[test]
#[ignore]
fn clip_probe() {
    let (Ok(psd), Ok(out)) = (std::env::var("FOLIO_CLIP_PROBE"), std::env::var("FOLIO_CLIP_OUT")) else { return };
    let b = std::fs::read(psd).unwrap();
    let e = |f: &dyn Fn(&mut Request)| { let mut r = req("DetectEdges"); f(&mut r); r };
    let cases: Vec<(&str, Request)> = vec![
        ("edges", e(&|_| {})), ("edges-inside", e(&|r| r.inside_edges = true)), ("edges-t10", e(&|r| r.threshold = 10.0)),
        ("edges-tol0", e(&|r| r.tolerance = 0.0)), ("edges-tol8", e(&|r| r.tolerance = 8.0)), ("edges-inset3", e(&|r| r.inset = 3.0)),
        ("edges-outset4", e(&|r| r.inset = -4.0)), ("edges-invert", e(&|r| r.invert = true)),
        ("alpha", e(&|r| { r.kind = "AlphaChannel".into(); r.path_name = "Shape".into(); })),
        ("photoshop", e(&|r| { r.kind = "PhotoshopPath".into(); r.path_name = "Tri".into(); })),
        ("edges-restrict", e(&|r| r.frame = Some([0.0, 0.0, 120.0, 150.0]))), ("edges-t100", e(&|r| r.threshold = 100.0)),
    ];
    let mut all = std::collections::BTreeMap::new();
    for (name, r) in cases {
        let subs = extract(&b, &r, [0.0, 0.0, 200.0, 150.0]).unwrap();
        all.insert(name, subs.iter().map(|s| s.points.iter().map(|p| p.iter().map(|q| vec![q.0, q.1]).collect::<Vec<_>>()).collect::<Vec<_>>()).collect::<Vec<_>>());
    }
    std::fs::write(out, serde_json::to_string(&all).unwrap()).unwrap();
}
