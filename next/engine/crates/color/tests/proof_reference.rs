//! View ▸ Proof Colors and Proof Setup against InDesign 2026 (the CMYK device rows were measured again in 2025 with the same colours, see
//! proof_devices_2025.rs; the Working CMYK RGB row below is the 2025 one).
//!
//! Document `artifacts/screencolor-c1-20261023/fixtures/x1-patches.idml` (US Letter, document CMYK Coated FOGRA39, RGB sRGB):
//! row one are the CMYK patches C, M, Y, K, 4-colour black, K 50 %, 60/40/40/100, row two the RGB patches (255,0,0), (0,255,0),
//! (0,0,255), (128,128,128), (200,100,50), (237,28,36), (0,174,239). The expected colours are the mean of the centre 16x16
//! pixels of each patch in the document window, with View ▸ Proof Colors on and the setup in the test's name
//! (`tools/x1_custom.py`, results `out/x1_custom_probe1.json` .. `probe3.json`). Document CMYK is the built-in menu item (the RGB row of
//! the Custom… run with Simulate Black Ink on is a stale transform of InDesign, see proof.rs).

use folio_color::{Colors, ProfileIndex, ProofSpec, icc::Intent, read_profile};

const CMYK: [[f64; 4]; 7] = [
    [100.0, 0.0, 0.0, 0.0], [0.0, 100.0, 0.0, 0.0], [0.0, 0.0, 100.0, 0.0], [0.0, 0.0, 0.0, 100.0],
    [100.0, 100.0, 100.0, 100.0], [0.0, 0.0, 0.0, 50.0], [60.0, 40.0, 40.0, 100.0],
];
const RGB: [[u8; 3]; 7] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [128, 128, 128], [200, 100, 50], [237, 28, 36], [0, 174, 239]];
const FOGRA39: &str = "Coated FOGRA39 (ISO 12647-2:2004)";
const SWOP: &str = "U.S. Web Coated (SWOP) v2";

fn profile(name: &str) -> Option<Vec<u8>> {
    let index = ProfileIndex::scan_installed();
    match index.find(name) {
        Some(p) => Some(read_profile(&p).expect("installed profile reads")),
        None => {
            eprintln!("SKIPPED: ICC profile \"{name}\" is not installed");
            None
        }
    }
}

/// `tolerance` = (CMYK patches, RGB patches): the ink patches agree within 3 levels, the RGB patches (converted through
/// the device's CMYK, out-of-gamut colours at the edge of its table) within 5.
fn check(name: &str, colors: &Colors, cmyk: [[u8; 3]; 7], rgb: [[u8; 3]; 7], tolerance: (i32, i32)) {
    let mut worst = 0;
    let mut worst_rgb = 0;
    for (i, (p, e)) in CMYK.iter().zip(cmyk).enumerate() {
        let got = colors.cmyk_to_srgb(p[0], p[1], p[2], p[3]);
        let d = (0..3).map(|c| (got[c] as i32 - e[c] as i32).abs()).max().unwrap();
        eprintln!("{name} CMYK {i}: got {got:?} InDesign {e:?} diff {d}");
        worst = worst.max(d);
    }
    for (i, (p, e)) in RGB.iter().zip(rgb).enumerate() {
        let got = colors.rgb_to_srgb(*p);
        let d = (0..3).map(|c| (got[c] as i32 - e[c] as i32).abs()).max().unwrap();
        eprintln!("{name} RGB {i}: got {got:?} InDesign {e:?} diff {d}");
        worst_rgb = worst_rgb.max(d);
    }
    assert!(worst <= tolerance.0, "{name}: worst CMYK patch difference {worst} > {}", tolerance.0);
    assert!(worst_rgb <= tolerance.1, "{name}: worst RGB patch difference {worst_rgb} > {}", tolerance.1);
}

fn spec(simulate_black: bool, simulate_paper: bool) -> ProofSpec {
    ProofSpec { preserve_numbers: true, simulate_black, simulate_paper, intent: Intent::Relative }
}


#[test]
fn custom_fogra39_proof() {
    let Some(device) = profile(FOGRA39) else { return };
    let colors = Colors::proofed(&device, None, None, spec(false, false)).unwrap();
    check("custom_fogra39_proof", &colors, [[0, 159, 227], [229, 0, 126], [255, 237, 0], [29, 29, 27], [0, 0, 0], [157, 157, 156], [2, 10, 10]], [[229, 36, 33], [102, 179, 46], [42, 75, 155], [128, 128, 128], [201, 100, 50], [229, 36, 40], [40, 171, 227]], (3, 5));
}

#[test]
fn document_cmyk_proof_is_custom_with_black_ink() {
    let Some(device) = profile(FOGRA39) else { return };
    let colors = Colors::proofed(&device, None, None, spec(true, false)).unwrap();
    check("document_cmyk_proof_is_custom_with_black_ink", &colors, [[0, 160, 228], [230, 12, 128], [255, 238, 0], [43, 43, 42], [28, 27, 24], [159, 159, 158], [30, 33, 33]], [[229, 48, 46], [106, 180, 56], [53, 80, 156], [130, 131, 131], [201, 104, 59], [230, 48, 51], [51, 173, 227]], (3, 5));
}

#[test]
fn simulate_paper_color_is_absolute_colorimetric() {
    let Some(device) = profile(FOGRA39) else { return };
    let colors = Colors::proofed(&device, None, None, spec(true, true)).unwrap();
    check("simulate_paper_color_is_absolute_colorimetric", &colors, [[0, 151, 218], [216, 11, 122], [247, 224, 0], [40, 40, 40], [26, 25, 22], [149, 149, 151], [27, 30, 31]], [[216, 45, 44], [99, 170, 54], [46, 76, 149], [122, 123, 125], [190, 98, 57], [216, 45, 49], [39, 163, 218]], (3, 5));
}

#[test]
fn custom_swop_proof() {
    let Some(device) = profile(SWOP) else { return };
    let colors = Colors::proofed(&device, None, None, spec(false, false)).unwrap();
    check("custom_swop_proof", &colors, [[0, 174, 239], [236, 0, 140], [255, 242, 0], [35, 31, 32], [0, 0, 0], [147, 149, 152], [0, 0, 0]], [[237, 32, 36], [105, 189, 69], [57, 83, 164], [128, 128, 128], [201, 100, 48], [236, 32, 39], [42, 172, 226]], (3, 5));
}

#[test]
fn working_cmyk_proof_is_custom_with_black_ink() {
    // The RGB patches are those InDesign 2025 shows when Working CMYK is chosen in a new window (gray 128: 133). The InDesign 2026 run
    // that gave 128 here (21 levels from this) was InDesign's older RGB conversion: it reuses a conversion built earlier for the same device
    // and settings (2025 shows 128 after Custom… SWOP with Simulate Black Ink off, FOGRA39, then Working CMYK; see proofcolors.ko.md).
    let Some(device) = profile(SWOP) else { return };
    let colors = Colors::proofed(&device, None, None, spec(true, false)).unwrap();
    check("working_cmyk_proof_is_custom_with_black_ink", &colors, [[0, 176, 240], [237, 39, 144], [255, 242, 21], [55, 53, 53], [32, 31, 31], [151, 153, 155], [36, 38, 39]], [[238, 53, 56], [112, 191, 80], [70, 92, 167], [133, 133, 133], [202, 107, 64], [237, 53, 57], [60, 174, 227]], (3, 5));
}

#[test]
fn swop_proof_converting_the_numbers() {
    let Some(fogra) = profile(FOGRA39) else { return };
    let Some(device) = profile(SWOP) else { return };
    let colors = Colors::proofed(&device, Some(&fogra), None, ProofSpec { preserve_numbers: false, simulate_black: false, simulate_paper: false, intent: Intent::Relative }).unwrap();
    check("swop_proof_converting_the_numbers", &colors, [[0, 158, 221], [228, 8, 127], [255, 236, 0], [28, 28, 27], [9, 8, 4], [156, 156, 156], [7, 13, 13]], [[237, 32, 36], [105, 189, 69], [57, 83, 164], [128, 128, 128], [201, 100, 48], [236, 32, 39], [42, 172, 226]], (4, 5));
}

