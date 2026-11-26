//! Adjust Layout ▸ Adjust Font Size and Leading: how InDesign finds the scale `k` of the text (layoutadj-c7; rules in
//! `next/oracle/rules/layoutadj-c7.ko.md`, measurements `next/oracle/goldens/layoutadj7-fontfit/mass-model.json`).
//!
//! InDesign does not compose the text again at the trial sizes. From the story composed at its old size it takes the number of lines `n0`, the
//! width `w0` of the frame, and the natural width `w_last` of the last line, and counts the lines of a trial from the "length" of the text
//! (`mass`) in the new frame width: `n1 = ceil(k * mass / w1)`. A trial passes while the text does not grow taller than the frame did,
//! `n1 * k <= sy * n0`; `k` is bisected between the smaller and the larger of the frame's scales.
//!
//! The search lives here; the facts it needs (lines of the story at its old size, the width of the column, the width of each paragraph's last line,
//! the model's small constant `e`) come from the host: the editor knows no composition, so the server composes the document and hands them over in
//! `fit_metrics` (`folio_text::fontfit`), as it does the glyph shapes of Create Outlines. Only the measured case is supported: one frame, one column,
//! one font and size; several paragraphs go through the same model paragraph by paragraph (not measured). Everything else is refused with a reason.
use super::*;
use serde::Deserialize;
use std::collections::BTreeMap;

/// The "length" of a story in units of width: every line but the last counts as the full frame width, the last as its natural width plus `e`.
pub(crate) fn mass(n0: usize, w0: f64, w_last: f64, e: f64) -> f64 {
    (n0.saturating_sub(1)) as f64 * w0 + w_last + e
}

/// The scale found for a story of `mass` and `n0` lines when the frame grows by `sx` across and `sy` down to a width of `w1`.
#[cfg(test)]
pub(crate) fn scale(mass: f64, n0: usize, sx: f64, sy: f64, w1: f64) -> f64 {
    scale_paragraphs(&[mass], n0, sx, sy, w1)
}

/// [`scale`] of a story of several paragraphs, each with its own mass: the lines of a trial are the sum of the paragraphs' (one paragraph is the measured case).
pub(crate) fn scale_paragraphs(masses: &[f64], n0: usize, sx: f64, sy: f64, w1: f64) -> f64 {
    let (mut lo, mut hi) = (sx.min(sy), sx.max(sy));
    let passes = |k: f64| {
        let n1: f64 = masses.iter().map(|m| ((k * m / w1 - 1e-9).ceil()).max(1.0)).sum();
        n1 * k <= sy * n0 as f64 + 1e-9
    };
    while hi - lo >= 0.01 {
        let mid = (lo + hi) / 2.0;
        if passes(mid) { lo = mid } else { hi = mid }
    }
    lo
}

/// A size or fixed leading of `size` pt scaled by `k`, floored to a tenth of a point.
pub(crate) fn new_size(size: f64, k: f64) -> f64 {
    (size * k * 10.0 + 1e-9).floor() / 10.0
}

/// What the host measured of one text frame (`folio_text::fontfit::FrameFit`).
#[derive(Deserialize)]
struct FrameFit {
    // Keep deserialization of this required string: removing it would accept malformed metrics.
    #[allow(dead_code)]
    story: String,
    width: f64,
    size: f64,
    leading: Option<f64>,
    e: f64,
    paragraphs: Vec<(usize, f64)>,
}

/// The facts of every text frame, or why it cannot be adjusted (`folio_text::fontfit::Fits`).
#[derive(Deserialize)]
pub(super) struct Metrics {
    frames: BTreeMap<String, Result<FrameFit, String>>,
}

impl Metrics {
    pub(super) fn parse(text: &Option<String>) -> Result<Metrics, EditError> {
        let text = text.as_deref().ok_or_else(|| EditError::new("unsupported", "adjusting the font size needs the composed text of the document (fit_metrics)"))?;
        serde_json::from_str(text).map_err(|e| EditError::new("invalid", format!("fit_metrics: {e}")))
    }
}

/// Sets the size (and the fixed leading) of the text of the frame `id` for its box going from `old` to `new` (`[left, top, right, bottom]`).
/// A frame whose size does not change keeps its text; a frame the host could not describe refuses the whole adjustment.
pub(super) fn apply(w: &mut Work<'_>, id: &str, old: [f64; 4], new: [f64; 4], metrics: &Metrics, limits: (f64, f64)) -> Result<(), EditError> {
    let (ow, oh, nw, nh) = (old[2] - old[0], old[3] - old[1], new[2] - new[0], new[3] - new[1]);
    if ow <= 0.0 || oh <= 0.0 || ((nw - ow).abs() < 1e-9 && (nh - oh).abs() < 1e-9) {
        return Ok(());
    }
    let fit = match metrics.frames.get(id) {
        Some(Ok(f)) => f,
        Some(Err(why)) => return Err(EditError::new("unsupported", format!("{id}: {why}"))),
        None => return Err(EditError::new("unsupported", format!("{id}: the frame is not in the composed document"))),
    };
    if fit.paragraphs.is_empty() {
        return Ok(());
    }
    let (sx, sy) = (nw / ow, nh / oh);
    let n0: usize = fit.paragraphs.iter().map(|p| p.0).sum();
    let masses: Vec<f64> = fit.paragraphs.iter().map(|(n, last)| mass(*n, fit.width, *last, fit.e)).collect();
    let k = scale_paragraphs(&masses, n0, sx, sy, fit.width * sx);
    // a size never goes past the limits further than it already was
    let (lo, hi) = (limits.0.min(fit.size), limits.1.max(fit.size));
    let size = new_size(fit.size, k).clamp(lo, hi);
    let mut props = BTreeMap::new();
    props.insert("pointSize".to_owned(), serde_json::json!(size));
    if let Some(l) = fit.leading {
        props.insert("leading".to_owned(), serde_json::json!(new_size(l, k).max(0.1)));
    }
    if size == fit.size && fit.leading.is_none() {
        return Ok(());
    }
    // the whole story takes it, as the T button's formatFrameText does (one k for the story)
    crate::text::format::frame_text::run(w, &[id.to_owned()], &props)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every measured case: c5 group b (five texts, four page sizes) and the DOM sweeps of layoutadj-c7 (Times New Roman, Arial, Courier New, Verdana,
    /// Georgia, Tahoma, Consolas, Malgun Gothic; 9, 12 and 18 pt; frame widths 160 and 200).
    #[test]
    fn the_search_finds_the_sizes_indesign_gave() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/layoutadj7-fontfit/mass-model.json");
        let golden: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let cases = golden["cases"].as_array().unwrap();
        assert!(cases.len() >= 200);
        let (mut exact, mut off) = (0, Vec::new());
        for c in cases {
            let f = |k: &str| c[k].as_f64().unwrap();
            let m = mass(c["n0"].as_u64().unwrap() as usize, f("w0"), f("w_last"), f("e"));
            let k = scale(m, c["n0"].as_u64().unwrap() as usize, f("sx"), f("sy"), f("w1"));
            let got = new_size(f("S"), k);
            if (got - f("size")).abs() < 1e-6 { exact += 1 } else { off.push((c["name"].as_str().unwrap().to_owned(), f("size"), got)) }
        }
        // three cases next to the end of the search (0.1 to 0.2 pt) fall on the other side of the 0.01 step of the bisection
        assert!(off.len() <= 3, "{off:?}");
        assert!(off.iter().all(|(_, want, got)| (want - got).abs() < 0.25), "{off:?}");
        assert_eq!(exact + off.len(), cases.len());
    }

    /// A short single line text only grows as much as the frame does in height (c5).
    #[test]
    fn a_one_line_text_follows_the_height() {
        let m = mass(1, 160.0, 60.0, 3.0);
        let k = scale(m, 1, 2.25, 1.294, 360.0);
        assert!((new_size(12.0, k) - 15.5).abs() < 1e-9);
    }
}
