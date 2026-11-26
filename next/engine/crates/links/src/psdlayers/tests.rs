use super::*;

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/Links/objlayer-c4.psd")).unwrap()
}

fn with(options: &Options, set: &[(u32, bool)]) -> Options {
    let mut o = options.clone();
    for l in &mut o.layers {
        if let Some((_, v)) = set.iter().find(|(id, _)| *id == l.id) {
            l.visible = *v;
        }
    }
    o
}

/// Merged image of a flattened PSD: (channels, planes).
fn merged(b: &[u8]) -> (usize, Vec<u8>) {
    let ch = be16(b, 12).unwrap() as usize;
    let (h, w) = (be32(b, 14).unwrap() as usize, be32(b, 18).unwrap() as usize);
    let mut at = 26;
    for _ in 0..3 {
        at += 4 + be32(b, at).unwrap() as usize;
    }
    assert_eq!(be16(b, at), Some(0));
    (ch, b[at + 2..at + 2 + ch * w * h].to_vec())
}

fn px(img: &(usize, Vec<u8>), x: usize, y: usize) -> Vec<u8> {
    let plane = 160 * 96;
    (0..img.0).map(|c| img.1[c * plane + y * 160 + x]).collect()
}

#[test]
fn lists_layers_top_first_with_record_index_ids() {
    let o = read(&fixture()).unwrap().unwrap();
    let list: Vec<_> = o.layers.iter().map(|l| (l.id, l.name.as_str(), l.original, l.visible)).collect();
    assert_eq!(list, [(2, "Red", false, false), (1, "Green", true, true), (0, "Base", true, true)]);
    assert_eq!(o.update_link, "KeepOverrides");
}

#[test]
fn untouched_options_keep_the_saved_picture() {
    let b = fixture();
    let o = read(&b).unwrap().unwrap();
    assert_eq!(flatten(&b, &o).unwrap(), None);
    assert!(!overrides(&o));
}

#[test]
fn showing_the_hidden_layer_draws_it_over_the_others() {
    let b = fixture();
    let o = with(&read(&b).unwrap().unwrap(), &[(2, true)]);
    assert!(overrides(&o));
    let img = merged(&flatten(&b, &o).unwrap().unwrap());
    assert_eq!(img.0, 3);
    assert_eq!(px(&img, 5, 5), [128, 128, 128]); // Base only
    assert_eq!(px(&img, 60, 40), [122, 148, 124]); // Green at 50 % over Base
    assert_eq!(px(&img, 100, 20), [188, 112, 104]); // Red over Green
    assert_eq!(px(&img, 140, 60), [188, 112, 104]); // Red over Base
}

#[test]
fn a_transparent_composition_keeps_its_alpha() {
    let b = fixture();
    let o = with(&read(&b).unwrap().unwrap(), &[(0, false)]);
    let out = flatten(&b, &o).unwrap().unwrap();
    assert_eq!(be16(&out, 12), Some(4));
    let img = merged(&out);
    assert_eq!(px(&img, 5, 5), [0, 0, 0, 0]);
    assert_eq!(px(&img, 60, 40), [116, 168, 120, 128]);
    // the merged transparency is announced by a negative layer count
    let mut at = 26;
    for _ in 0..2 {
        at += 4 + be32(&out, at).unwrap() as usize;
    }
    assert_eq!(be16(&out, at + 8), Some(0xFFFF));
    let nothing = with(&o, &[(1, false)]);
    let img = merged(&flatten(&b, &nothing).unwrap().unwrap());
    assert_eq!(img.0, 4);
    assert!(img.1.iter().all(|v| *v == 0), "nothing shown: every pixel clear");
}

#[test]
fn a_changed_source_is_refused_instead_of_showing_the_wrong_layer() {
    let b = fixture();
    let mut o = read(&b).unwrap().unwrap();
    o.layers[0].name = "Other".into();
    assert!(flatten(&b, &o).is_err());
    let mut o = read(&b).unwrap().unwrap();
    o.layers.push(Layer { id: 9, name: "Gone".into(), original: true, visible: false, children: Vec::new(), adjustment: false });
    assert!(flatten(&b, &o).is_err());
}

#[test]
fn a_flat_psd_has_no_layer_list() {
    let flat = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/Links/place-rgb.psd")).unwrap();
    assert_eq!(read(&flat).unwrap(), None);
}

// psdlayers-c1: a 240 x 168 PSD of 24 px cells (10 per row); `artifacts/psdlayers-c1-20261023/tools/build-psd.py`
// writes it. Backdrop quadrants (12 px) TL TR BL BR: (60,120,180) (200,170,90) (120,120,120) (230,200,210);
// a full-cell layer is (170,90,70) in the top half and (100,160,110) in the bottom half.

fn cells_psd() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/Links/psdlayers-c1.psd")).unwrap()
}

/// The pixel at (`x`, `y`) of cell `cell` after showing the layers named in `show`.
fn cell_px(show: &[&str], cell: usize, x: usize, y: usize) -> Vec<u8> {
    let b = cells_psd();
    let mut o = read(&b).unwrap().unwrap();
    for l in &mut o.layers {
        if show.contains(&l.name.as_str()) {
            l.visible = true;
        }
    }
    let flat = flatten(&b, &o).unwrap().expect("shown layers change the picture");
    let (ch, w, h) = (be16(&flat, 12).unwrap() as usize, be32(&flat, 18).unwrap() as usize, be32(&flat, 14).unwrap() as usize);
    let mut at = 26;
    for _ in 0..3 {
        at += 4 + be32(&flat, at).unwrap() as usize;
    }
    let (px, py) = ((cell % 10) * 24 + x, (cell / 10) * 24 + y);
    (0..ch).map(|c| flat[at + 2 + c * w * h + py * w + px]).collect()
}

fn refused(show: &[&str]) -> String {
    let b = cells_psd();
    let mut o = read(&b).unwrap().unwrap();
    for l in &mut o.layers {
        if show.contains(&l.name.as_str()) {
            l.visible = true;
        }
    }
    flatten(&b, &o).unwrap_err()
}

#[test]
fn a_layer_group_is_one_entry_with_its_layers_nested() {
    let o = read(&cells_psd()).unwrap().unwrap();
    let outer = o.layers.iter().find(|l| l.name == "Outer").expect("the group is listed");
    assert_eq!(outer.children.iter().map(|c| (c.name.as_str(), c.id)).collect::<Vec<_>>(), [("Inner", 90), ("n-bg", 87)]);
    assert_eq!(outer.children[0].children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["n-mul"]);
    assert!(o.layers.iter().all(|l| l.name != "n-bg" && l.name != "</Layer group>" && l.name != "g1-mul"), "group members and dividers are not listed");
    assert_eq!(o.layers.last().map(|l| l.name.as_str()), Some("Base"));
}

#[test]
fn blend_modes_use_the_photoshop_formulas() {
    // top-left quadrant: backdrop (60,120,180), layer (170,90,70)
    assert_eq!(cell_px(&["bm-mul"], 0, 6, 6), [40, 42, 49]);
    assert_eq!(cell_px(&["bm-screen"], 1, 6, 6), [190, 168, 201]);
    assert_eq!(cell_px(&["bm-darken"], 7, 6, 6), [60, 90, 70]);
    assert_eq!(cell_px(&["bm-lighten"], 8, 6, 6), [170, 120, 180]);
}

#[test]
fn opacity_and_fill_thin_the_layer_the_same_way_in_a_normal_mode() {
    let by_opacity = cell_px(&["op50"], 25, 6, 6);
    assert_eq!(by_opacity, [115, 105, 125]);
    assert_eq!(cell_px(&["fill50"], 26, 6, 6), by_opacity);
    assert_eq!(cell_px(&["op50-fill50"], 27, 6, 6), [88, 112, 152]);
}

#[test]
fn fill_acts_before_the_clamp_in_the_dodge_and_burn_modes() {
    // bottom-right quadrant: backdrop (230,200,210) + 50 % of (30,20,50) when fill is 50 % (measured: InDesign adds it, then clamps)
    assert_eq!(cell_px(&["sp-lineardodge-fill50"], 36, 18, 18), [245, 210, 235]);
    let (cb, cs) = ([0.9, 0.9, 0.9], [0.4, 0.4, 0.4]);
    assert_eq!(blend::Mode::LinearDodge.blend_fill(cb, cs, 0.5)[0], 1.0, "0.9 + 0.5 × 0.4 clamps to 1; clamping first would give 0.95");
}

#[test]
fn a_pixel_mask_hides_the_layer_where_it_is_black() {
    // ramp mask, 0 on the left to 255 on the right, over the top-left backdrop (60,120,180) and the layer (170,90,70)
    let left = cell_px(&["mask-ramp"], 44, 0, 6);
    let right = cell_px(&["mask-ramp"], 44, 23, 6);
    assert_eq!(left, [60, 120, 180]);
    assert!(right[0] >= 165 && right[1] <= 95 && right[2] <= 75, "almost the whole layer on the right: {right:?}");
    // outside the mask rectangle the default colour decides; the disabled mask is ignored
    assert_eq!(cell_px(&["mask-small-d0"], 45, 1, 1), [60, 120, 180]);
    assert_eq!(cell_px(&["mask-small-d255"], 46, 1, 1), [170, 90, 70]);
    assert_eq!(cell_px(&["mask-disabled"], 47, 6, 6), [170, 90, 70]);
}

#[test]
fn a_clipped_layer_only_shows_where_its_base_does() {
    // cell 53: base is a 16 px square (inset 4), the clipped layer fills the cell
    assert_eq!(cell_px(&["clipbase-norm", "clipped-norm"], 53, 6, 6), [170, 90, 70]);
    assert_eq!(cell_px(&["clipbase-norm", "clipped-norm"], 53, 1, 1), [60, 120, 180], "outside the base nothing is drawn");
    // a hidden base takes the layers clipped to it along
    assert_eq!(cell_px(&["clipped-h"], 55, 6, 6), [60, 120, 180]);
}

#[test]
fn a_pass_through_group_blends_with_what_is_below_and_an_isolated_one_does_not() {
    // multiply child in a group over the top-left backdrop (60,120,180) with the layer (170,90,70)
    assert_eq!(cell_px(&["G1"], 58, 6, 6), [40, 42, 49]);
    assert_eq!(cell_px(&["G2"], 59, 6, 6), [170, 90, 70]);
}

#[test]
fn unmeasured_features_are_refused_not_guessed() {
    assert!(refused(&["bm-hardmix"]).contains("hMix"));
}

/// Every layer shown: the composition of the PSD of a cells golden (`oracle/goldens/objlayer-c5/<dir>/cells.json`, single pixels of InDesign 2025's
/// PDF/X-4 raster away from the shape edges) against Folio's, within the JPEG noise of InDesign's output.
fn composes_like_indesign(dir: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle");
    let golden: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("goldens/objlayer-c5").join(dir).join("cells.json")).unwrap()).unwrap();
    let bytes = std::fs::read(root.join("fixtures/Links").join(golden["psd"].as_str().unwrap())).unwrap();
    let mut options = read(&bytes).unwrap().unwrap();
    for l in &mut options.layers {
        l.visible = true;
    }
    let out = flatten(&bytes, &options).unwrap().expect("a different picture than the saved one");
    let (w, h) = (be32(&out, 18).unwrap() as usize, be32(&out, 14).unwrap() as usize);
    let mut at = 26;
    for _ in 0..3 {
        at += 4 + be32(&out, at).unwrap() as usize;
    }
    let plane = &out[at + 2..];
    let tolerance = golden["tolerance"].as_i64().unwrap();
    for cell in golden["cells"].as_array().unwrap() {
        let i = cell["cell"].as_u64().unwrap() as usize;
        let (ox, oy) = ((i % 10) * 24, (i / 10) * 24);
        for s in cell["samples"].as_array().unwrap() {
            let (x, y) = (ox + s[0].as_u64().unwrap() as usize, oy + s[1].as_u64().unwrap() as usize);
            for c in 0..3 {
                let (got, want) = (plane[c * w * h + y * w + x] as i64, s[2 + c].as_i64().unwrap());
                assert!((got - want).abs() <= tolerance, "cell {i} {} at {},{} channel {c}: Folio {got}, InDesign {want}", cell["name"], s[0], s[1]);
            }
        }
    }
}

/// Vector masks, adjustment layers and fills (objlayer-c5, 11 cells).
#[test]
fn vector_masks_adjustments_and_fills_match_indesign() {
    composes_like_indesign("cells");
}

/// An invert adjustment clipped to a layer only changes that layer; one inside a pass-through group changes what is below the group.
#[test]
fn clipped_and_grouped_adjustments_match_indesign() {
    composes_like_indesign("clipgroup");
}

/// A 16-bit file is refused: InDesign drew an own 16-bit test file with some layers stretched sideways (objlayer-c5), so nothing is guessed.
#[test]
fn sixteen_bit_files_are_refused() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../oracle/fixtures/Links/objlayer-c5-rgb16.psd")).unwrap();
    let mut options = read(&bytes).unwrap().unwrap();
    options.layers[0].visible = !options.layers[0].visible;
    let err = flatten(&bytes, &options).unwrap_err();
    assert!(err.contains("8-bit"), "{err}");
}
