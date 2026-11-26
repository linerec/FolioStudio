//! Display conversion through the ICC profiles installed on this machine.
//!
//! Expected values are LittleCMS 2.18 results (relative colorimetric, black point
//! compensation, LittleCMS's built-in sRGB). Tests that need a profile print an explicit
//! "SKIPPED" line and return when it is not installed.

use folio_color::{BlendSpace, Colors, ProfileIndex, ProfileStatus, document_colors, read_profile};
use std::sync::OnceLock;

const SWOP: &str = "U.S. Web Coated (SWOP) v2";
const FOGRA39: &str = "Coated FOGRA39 (ISO 12647-2:2004)";
const ADOBE_RGB: &str = "Adobe RGB (1998)";

fn index() -> &'static ProfileIndex {
    static INDEX: OnceLock<ProfileIndex> = OnceLock::new();
    INDEX.get_or_init(ProfileIndex::scan_installed)
}

/// The installed profile's bytes, or `None` after printing why the test is skipped.
fn installed(name: &str, test: &str) -> Option<Vec<u8>> {
    match index().find(name) {
        Some(path) => Some(read_profile(&path).expect("installed profile reads")),
        None => {
            eprintln!("SKIPPED {test}: ICC profile \"{name}\" is not installed (searched {:?})", index().roots());
            None
        }
    }
}

fn close(a: [u8; 3], b: [u8; 3], tolerance: i32) -> bool {
    a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= tolerance)
}

#[test]
fn swop_cmyk_matches_littlecms() {
    let Some(swop) = installed(SWOP, "swop_cmyk_matches_littlecms") else { return };
    let c = Colors::from_profiles(Some(&swop), None).unwrap();
    // Paper, black ink, cyan, magenta + yellow.
    assert_eq!(c.cmyk_to_srgb(0.0, 0.0, 0.0, 0.0), [255, 255, 255]);
    assert_eq!(c.cmyk_to_srgb(0.0, 0.0, 0.0, 100.0), [35, 31, 32]);
    assert_eq!(c.cmyk_to_srgb(100.0, 0.0, 0.0, 0.0), [0, 174, 239]);
    assert_eq!(c.cmyk8_to_srgb(0, 255, 255, 0), [237, 28, 36]);
    // The cover background C71 M28.4 (MuPDF shows 58 151 211).
    assert!(close(c.cmyk_to_srgb(71.0, 28.4, 0.0, 0.0), [58, 151, 211], 1));
    // Monotone in black ink.
    let mut prev = 256;
    for k in 0..=100 {
        let v = c.cmyk_to_srgb(0.0, 0.0, 0.0, k as f64)[1] as i32;
        assert!(v <= prev, "k={k}");
        prev = v;
    }
    // CMYK content looks the same in both blend spaces; gray becomes black ink.
    assert_eq!(c.gray_through_cmyk(0), [35, 31, 32]);
}

#[test]
fn swop_blend_space_round_trip_matches_littlecms() {
    let Some(swop) = installed(SWOP, "swop_blend_space_round_trip_matches_littlecms") else { return };
    let c = Colors::from_profiles(Some(&swop), None).unwrap();
    assert_eq!(c.rgb_through_cmyk([255, 255, 255]), [255, 255, 255]);
    assert!(close(c.rgb_through_cmyk([0, 255, 0]), [106, 189, 69], 1), "{:?}", c.rgb_through_cmyk([0, 255, 0]));
    // A saturated yellow, where the exact pipeline and LittleCMS's 8-bit precalculation
    // differ most: LittleCMS gives 251 239 11.
    assert!(close(c.rgb_through_cmyk([255, 239, 0]), [251, 239, 11], 1), "{:?}", c.rgb_through_cmyk([255, 239, 0]));
    // Pixels take the same path within a level or so.
    let mut px = vec![0, 255, 0, 255, 255, 239, 0, 255, 120, 60, 200, 255];
    c.rgba_to_display(&mut px, BlendSpace::Cmyk);
    for (p, rgb) in px.chunks_exact(4).zip([[0, 255, 0], [255, 239, 0], [120, 60, 200]]) {
        assert!(close([p[0], p[1], p[2]], c.rgb_through_cmyk(rgb), 2), "{p:?}");
    }
    // An RGB page leaves sRGB pixels alone.
    let mut px = vec![1, 2, 3, 255];
    c.rgba_to_display(&mut px, BlendSpace::Rgb);
    assert_eq!(px, [1, 2, 3, 255]);
}

#[test]
fn another_cmyk_profile_converts_differently() {
    let (Some(swop), Some(fogra)) = (
        installed(SWOP, "another_cmyk_profile_converts_differently"),
        installed(FOGRA39, "another_cmyk_profile_converts_differently"),
    ) else {
        return;
    };
    let s = Colors::from_profiles(Some(&swop), None).unwrap();
    let f = Colors::from_profiles(Some(&fogra), None).unwrap();
    assert_ne!(s.key(), f.key());
    // LittleCMS: paper white, 100 % black 29 29 27, 100 % cyan 0 159 227.
    assert_eq!(f.cmyk_to_srgb(0.0, 0.0, 0.0, 0.0), [255, 255, 255]);
    assert!(close(f.cmyk_to_srgb(0.0, 0.0, 0.0, 100.0), [29, 29, 27], 1), "{:?}", f.cmyk_to_srgb(0.0, 0.0, 0.0, 100.0));
    assert!(close(f.cmyk_to_srgb(100.0, 0.0, 0.0, 0.0), [0, 159, 227], 1), "{:?}", f.cmyk_to_srgb(100.0, 0.0, 0.0, 0.0));
    assert_ne!(f.cmyk_to_srgb(100.0, 0.0, 0.0, 0.0), s.cmyk_to_srgb(100.0, 0.0, 0.0, 0.0));
}

#[test]
fn adobe_rgb_document_profile() {
    let (Some(swop), Some(adobe)) = (installed(SWOP, "adobe_rgb_document_profile"), installed(ADOBE_RGB, "adobe_rgb_document_profile")) else {
        return;
    };
    let c = Colors::from_profiles(Some(&swop), Some(&adobe)).unwrap();
    assert!(c.rgb_managed());
    // Adobe RGB green is outside sRGB: LittleCMS clips it to 0 255 0 (relative colorimetric).
    assert_eq!(c.rgb_to_srgb([0, 255, 0]), [0, 255, 0]);
    assert_eq!(c.rgb_to_srgb([255, 255, 255]), [255, 255, 255]);
    assert_eq!(c.rgb_to_srgb([0, 0, 0]), [0, 0, 0]);
    // A mid gray stays gray: LittleCMS gives 129.
    let g = c.rgb_to_srgb([128, 128, 128]);
    assert!(g[0] == g[1] && g[1] == g[2] && (g[0] as i32 - 129).abs() <= 1, "{g:?}");
    // Lab content in the CMYK blend space goes through sRGB, not the document RGB.
    let lab_green = folio_color::lab_to_srgb([60.0, -60.0, 50.0]);
    assert_ne!(c.srgb_through_cmyk(lab_green), c.rgb_through_cmyk(lab_green));
}

#[test]
fn document_colors_uses_the_cache_and_reports_missing_profiles() {
    let Some(_) = installed(SWOP, "document_colors_uses_the_cache_and_reports_missing_profiles") else { return };
    let dir = std::env::temp_dir().join(format!("folio-color-doc-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let first = document_colors(Some(SWOP), Some(folio_color::SRGB_NAME), index(), None, Some(&dir));
    assert!(first.warnings.is_empty(), "{:?}", first.warnings);
    assert_eq!((first.cmyk.status.clone(), first.rgb.status.clone()), (ProfileStatus::Used, ProfileStatus::Display));
    assert!(!first.report.cached && first.report.build_ms.is_some());
    let second = document_colors(Some(SWOP), Some(folio_color::SRGB_NAME), index(), None, Some(&dir));
    assert!(second.report.cached, "{:?}", second.report);
    assert_eq!(second.colors.key(), first.colors.key());
    for rgb in [[0, 0, 0], [12, 200, 99], [255, 239, 0]] {
        assert_eq!(second.colors.rgb_through_cmyk(rgb), first.colors.rgb_through_cmyk(rgb));
    }
    assert_eq!(second.colors.cmyk_to_srgb(0.0, 0.0, 0.0, 100.0), [35, 31, 32]);

    // A profile nobody has: unmanaged CMYK and a warning naming it.
    let missing = document_colors(Some("No Such CMYK Profile v9"), Some("No Such RGB"), index(), None, Some(&dir));
    assert_eq!(missing.cmyk.status, ProfileStatus::Missing);
    assert!(!missing.colors.cmyk_managed());
    assert_eq!(missing.colors.cmyk_to_srgb(0.0, 0.0, 0.0, 100.0), [0, 0, 0]);
    assert!(missing.warnings.iter().any(|w| w.contains("\"No Such CMYK Profile v9\" is not installed")), "{:?}", missing.warnings);
    assert!(missing.warnings.iter().any(|w| w.contains("\"No Such RGB\"")), "{:?}", missing.warnings);
    let none = document_colors(None, None, index(), None, None);
    assert_eq!(none.warnings.len(), 2, "{:?}", none.warnings);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_profile_in_the_document_folder_is_found() {
    let Some(path) = index().find(SWOP) else {
        eprintln!("SKIPPED a_profile_in_the_document_folder_is_found: \"{SWOP}\" is not installed");
        return;
    };
    let dir = std::env::temp_dir().join(format!("folio-color-docdir-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("renamed.icc");
    std::fs::copy(&path, &copy).unwrap();
    // An empty index stands for a machine without the profile installed.
    let empty = ProfileIndex::scan(&[]);
    assert_eq!(empty.find_for_document(SWOP, Some(&dir)), Some(copy.clone()));
    assert_eq!(empty.find_for_document(SWOP, None), None);
    let _ = std::fs::remove_dir_all(&dir);
}
