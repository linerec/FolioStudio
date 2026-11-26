//! Optical margin alignment against InDesign: the line starts and ends InDesign measured in
//! `artifacts/optmargin-20261023` (hidden documents set with OpticalMarginAlignment on, DOM line
//! x/end) against the Folio layout of the same IDML. Skips when the measurements are missing.
//!
//! Run: `cargo test --release -p folio-text --test optical_margin_layout -- --nocapture`

#[path = "../../../test_support.rs"]
mod test_support;

use folio_text::{FontStore, layout_document};
use serde_json::Value;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/optmargin-20261023")
}

/// InDesign's DOM line (text, x, end) of a measured case.
fn measured(case: &str) -> Vec<(String, f64, f64)> {
    for file in ["glyphs", "size", "align", "nested", "bounds", "nestedoff", "clamp"] {
        let Ok(bytes) = std::fs::read(root().join(format!("{file}.json"))) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else { continue };
        for c in v["cases"].as_array().into_iter().flatten() {
            if c["name"] == case {
                return c["lines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|l| (l["text"].as_str().unwrap_or("").to_string(), l["x"].as_f64().unwrap_or(f64::NAN), l["end"].as_f64().unwrap_or(f64::NAN)))
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Our lines of the document in reading order: (start x, end x) of the text clusters.
fn ours(store: &FontStore, case: &str) -> Option<(Vec<(f64, f64)>, bool)> {
    let doc = folio_idml::read_package(&root().join(format!("{case}.idml")), None).ok()?;
    let optical = doc.stories.values().any(|s| s.optical_margin.enabled);
    let layout = layout_document(&doc, store);
    let mut lines = Vec::new();
    for f in layout.frames.values() {
        for l in &f.lines {
            if let (Some(a), Some(b)) = (l.text_clusters.first(), l.text_clusters.last()) {
                lines.push((a.x0, b.x1));
            }
        }
    }
    Some((lines, optical))
}

/// Per measured line: (our start - InDesign's x, our end - InDesign's end).
fn errors(store: &FontStore, case: &str) -> Option<Vec<(f64, f64)>> {
    let (lines, _) = ours(store, case)?;
    let want = measured(case);
    (!want.is_empty()).then(|| lines.iter().zip(&want).map(|(a, w)| (a.0 - w.1, a.1 - w.2)).collect())
}

/// Not compared here (known gap, optmargin.ko.md): `size72` (optical size 48 after the 4x limit) puts the dot and the dashes
/// 0.6 pt and T 0.5 pt off.
///
/// The frame's text starts 5 pt left of the DOM coordinate and the DOM `end` counts the paragraph mark, in the optical-off
/// cases already; kerning pairs InDesign sets differently show there too. So an optical-on case is compared with the
/// matching optical-off case: the difference of the errors is what optical margin alignment itself changes.
#[test]
fn optical_margin_matches_indesign_against_off_baseline() {
    if !root().join("align.json").exists() {
        eprintln!("skipped: measurements missing");
        return;
    }
    let store = FontStore::discover(&[]);
    let mut failures = Vec::new();
    for (on, off) in [("arial-on", "arial-off"), ("times-on", "times-off"), ("right", "right-off"), ("center", "center-off"), ("ignore", "arial-off"), ("size24", "arial-off"), ("size6", "arial-off")] {
        let family = if on == "times-on" { "Times New Roman" } else { "Arial" };
        if !store.resolve(family, "Regular", &[]).exact {
            test_support::skip(&format!("{on}/{off}: {family} Regular is required for the measured line positions"));
            continue;
        }
        let (Some(a), Some(b)) = (errors(&store, on), errors(&store, off)) else {
            eprintln!("{on}: not measured here, skipped");
            continue;
        };
        let (mut ws, mut we) = (0.0f64, 0.0f64);
        for (i, (x, y)) in a.iter().zip(&b).enumerate().take(21) {
            let (ds, de) = (x.0 - y.0, x.1 - y.1);
            ws = ws.max(ds.abs());
            we = we.max(de.abs());
            if ds.abs() > 0.25 || de.abs() > 0.5 {
                failures.push(format!("{on} line {i}: start {ds:+.3}, end {de:+.3} vs {off}"));
            }
        }
        eprintln!("{on} vs {off}: worst start change error {ws:.3} pt, worst end change error {we:.3} pt");
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn print_layout_of_measured_cases() {
    if !root().join("align.json").exists() {
        return;
    }
    let store = FontStore::discover(&[]);
    for case in ["arial-off", "arial-on", "right", "center", "justify", "indent", "ignore", "size24", "size6", "point24", "size72"] {
        let Some((lines, optical)) = ours(&store, case) else {
            eprintln!("{case}: cannot read");
            continue;
        };
        let want = measured(case);
        eprintln!("== {case}: {} measured lines, {} ours, optical story {optical}", want.len(), lines.len());
        for (i, (a, b)) in lines.iter().enumerate().take(21) {
            if let Some(w) = want.get(i) {
                eprintln!("  {i:2} {:<12} ours {a:8.3} .. {b:8.3} | InDesign {:8.3} .. {:8.3} | d {:+.3} {:+.3}", w.0.trim().chars().take(12).collect::<String>(), w.1, w.2, a - w.1, b - w.2);
            }
        }
    }
}
