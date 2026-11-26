//! Live corners: the yellow square on a selected rectangle or text frame, and the four yellow diamonds it turns on
//! (`liveCornerDrag`, `cycleCornerShape`). The screen sends only how far the diamond was dragged and which modifier keys were down;
//! what that does to the corners is worked out here, so a script (or an LLM) can do the same drag with the same command.
//!
//! Rules (measured in InDesign 2025 with real mouse input, `artifacts/liveinput-c1-20261023`, `next/oracle/rules/liveinput-c1.ko.md`):
//! - A diamond sits on the frame's own top or bottom edge, `radius` in from its corner (on the corner while the corner has no shape).
//!   Dragging it inward makes the radius larger, outward smaller; the new radius is the dragged corner's start radius (0 when it has no
//!   shape) plus `by`, never below 0 and never above half the shorter side of the frame (measured: tall 60 x 300 and wide 300 x 60 frames
//!   stop at 30, a 200 x 100 frame at 50).
//! - Without Shift every corner gets the new radius (corners that had other sizes lose them); with Shift (`only_corner`) only the
//!   dragged one changes.
//! - The shape: a dragged corner that has none becomes a rounded corner (and so does every other corner that has none, unless
//!   `only_corner`); corners that already have a shape keep it (measured: bevel, inset and fancy corners stay what they were, a corner
//!   with no shape next to them stays without one but takes the radius).
//! - One drag is one undo step (the screen sends it when the button goes up; InDesign draws no rounded preview while dragging).
//! - Alt-click on a diamond turns the shapes through None > Rounded > Fancy > Bevel > Inset > Inverse Rounded > None ...: the clicked
//!   corner's next shape goes to all four corners, with Shift (`only_corner`) to the clicked one only. The sizes stay as they are
//!   (measured, also the 12 pt size of a corner that has never had a shape).
use super::corners::{CORNERS, Corner, set_corners};
use super::*;

const SHAPE_ORDER: [&str; 6] = ["None", "RoundedCorner", "FancyCorner", "BevelCorner", "InsetCorner", "InverseRoundedCorner"];
const DEFAULT_RADIUS: f64 = 12.0;
const ENABLE: &str = "EnableStrokeAndCornerOptions";

/// The radius a diamond leaves after being dragged `by` points inward from where it was (`start`), for a frame whose shorter side is `shorter`.
pub fn radius_after(start: f64, by: f64, shorter: f64) -> f64 {
    (start + by).clamp(0.0, (shorter / 2.0).max(0.0))
}

/// The shape after `shape` in the Alt-click cycle (an unknown shape starts it at the first).
pub fn next_shape(shape: &str) -> &'static str {
    let at = SHAPE_ORDER.iter().position(|s| *s == shape).map_or(0, |k| k + 1);
    SHAPE_ORDER[at % SHAPE_ORDER.len()]
}

/// The corners after the drag of corner `corner` (0 top left, 1 top right, 2 bottom left, 3 bottom right) by `by` points inward.
pub fn dragged(corners: &[Corner; 4], corner: usize, by: f64, only_corner: bool, shorter: f64) -> [Corner; 4] {
    let start = &corners[corner];
    let had_shape = start.shape != "None";
    let radius = radius_after(if had_shape { start.radius } else { 0.0 }, by, shorter);
    let mut out = corners.clone();
    for (k, c) in out.iter_mut().enumerate() {
        if only_corner && k != corner {
            continue;
        }
        c.radius = radius;
        if !had_shape && c.shape == "None" {
            c.shape = "RoundedCorner".into();
        }
    }
    out
}

/// The corners after an Alt-click on corner `corner`.
pub fn cycled(corners: &[Corner; 4], corner: usize, only_corner: bool) -> [Corner; 4] {
    let next = next_shape(&corners[corner].shape);
    let mut out = corners.clone();
    for (k, c) in out.iter_mut().enumerate() {
        if !only_corner || k == corner {
            c.shape = next.into();
        }
    }
    out
}

/// The four corners an item has now, as InDesign writes them (a corner without its own attributes: the item's single `CornerOption` /
/// `CornerRadius`, then the object style's, then none and 12 pt).
fn current(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> [Corner; 4] {
    let get = |name: &str| ix.attr(t, i, name).map(str::to_owned).or_else(|| style_attr(w, ix, t, i, name, ENABLE));
    let corner = |c: &str| Corner {
        shape: get(&format!("{c}CornerOption")).or_else(|| get("CornerOption")).unwrap_or_else(|| "None".into()),
        radius: get(&format!("{c}CornerRadius")).or_else(|| get("CornerRadius")).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_RADIUS),
    };
    [corner(CORNERS[0]), corner(CORNERS[1]), corner(CORNERS[2]), corner(CORNERS[3])]
}

fn shorter_side(ix: &Index, t: &str, i: usize, id: &str) -> Result<f64, EditError> {
    let b = own_path(ix, t, i)
        .and_then(|p| geom::path_bounds(&p.subpaths.iter().map(|(_, pts, closed)| (pts.clone(), *closed)).collect::<Vec<_>>()))
        .ok_or_else(|| EditError::new("unsupported", format!("{id} has no path")))?;
    Ok((b[2] - b[0]).min(b[3] - b[1]))
}

fn check(corner: &usize, ids: &[String]) -> Result<(), EditError> {
    if *corner > 3 {
        return Err(EditError::new("invalid", "a corner is 0 (top left), 1 (top right), 2 (bottom left) or 3 (bottom right)"));
    }
    if ids.is_empty() {
        return Err(EditError::new("invalid", "nothing is selected"));
    }
    Ok(())
}

/// Dragging a diamond: `liveCornerDrag`. `by` is how many points the diamond moved inward along its edge.
pub(crate) fn drag(w: &mut Work<'_>, ids: &[String], corner: &usize, by: &f64, only_corner: &bool) -> Result<(), EditError> {
    check(corner, ids)?;
    if !by.is_finite() {
        return Err(EditError::new("invalid", "the drag is a number of points"));
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let now = current(w, &ix, &t, i);
        let shorter = shorter_side(&ix, &t, i, id)?;
        let next = dragged(&now, *corner, *by, *only_corner, shorter);
        set_corners(w, std::slice::from_ref(id), &next)?;
    }
    Ok(())
}

/// Alt-click on a diamond: `cycleCornerShape`.
pub(crate) fn cycle(w: &mut Work<'_>, ids: &[String], corner: &usize, only_corner: &bool) -> Result<(), EditError> {
    check(corner, ids)?;
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let next = cycled(&current(w, &ix, &t, i), *corner, *only_corner);
        set_corners(w, std::slice::from_ref(id), &next)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(shape: &str, radius: f64) -> Corner { Corner { shape: shape.into(), radius } }

    #[test]
    fn radius_stops_at_half_the_shorter_side() {
        assert_eq!(radius_after(0.0, 14.0, 100.0), 14.0);
        assert_eq!(radius_after(0.0, 400.0, 100.0), 50.0);
        assert_eq!(radius_after(0.0, 400.0, 60.0), 30.0);
        assert_eq!(radius_after(20.0, -12.0, 100.0), 8.0);
        assert_eq!(radius_after(20.0, -40.0, 100.0), 0.0);
    }

    #[test]
    fn a_cornerless_frame_turns_rounded_all_round() {
        let now = [c("None", 12.0), c("None", 12.0), c("None", 12.0), c("None", 12.0)];
        let after = dragged(&now, 0, 25.0, false, 100.0);
        assert!(after.iter().all(|k| k.shape == "RoundedCorner" && k.radius == 25.0));
        // the drag starts from 0 at a corner with no shape, not from the 12 pt it stores
        assert_eq!(dragged(&now, 1, 3.0, false, 100.0)[1].radius, 3.0);
    }

    #[test]
    fn shift_changes_one_corner_and_shapes_stay() {
        let now = [c("BevelCorner", 10.0), c("InsetCorner", 20.0), c("FancyCorner", 30.0), c("None", 40.0)];
        let all = dragged(&now, 0, 15.0, false, 100.0);
        assert_eq!(all.iter().map(|k| (k.shape.as_str(), k.radius)).collect::<Vec<_>>(),
            vec![("BevelCorner", 25.0), ("InsetCorner", 25.0), ("FancyCorner", 25.0), ("None", 25.0)]);
        let one = dragged(&now, 1, 14.25, true, 100.0);
        assert_eq!(one[1], c("InsetCorner", 34.25));
        assert_eq!((&one[0], &one[2], &one[3]), (&now[0], &now[2], &now[3]));
    }

    #[test]
    fn the_cycle_goes_round_through_six_shapes() {
        let mut shape = "None".to_string();
        let seen: Vec<String> = (0..7).map(|_| { shape = next_shape(&shape).into(); shape.clone() }).collect();
        assert_eq!(seen, ["RoundedCorner", "FancyCorner", "BevelCorner", "InsetCorner", "InverseRoundedCorner", "None", "RoundedCorner"]);
        let now = [c("BevelCorner", 10.0), c("RoundedCorner", 20.0), c("RoundedCorner", 30.0), c("RoundedCorner", 40.0)];
        // Alt-click on the second corner: its next shape (fancy) goes to all four, the sizes stay
        let all = cycled(&now, 1, false);
        assert!(all.iter().all(|k| k.shape == "FancyCorner"));
        assert_eq!(all.iter().map(|k| k.radius).collect::<Vec<_>>(), vec![10.0, 20.0, 30.0, 40.0]);
        // with Shift only the clicked one
        let one = cycled(&now, 0, true);
        assert_eq!(one[0].shape, "InsetCorner");
        assert_eq!(&one[1..], &now[1..]);
    }

    /// Every measured InDesign 2025 case (next/oracle/measured/live-corners-2025.json): from the corners it started with and the radius the dragged corner
    /// ended at, the rules give all four corners back exactly (the pointer-to-points step is the screen's, tested in livecorners.test.cjs).
    #[test]
    fn the_measured_drags_and_cycles_come_out_as_in_indesign() {
        let all: serde_json::Value = serde_json::from_str(include_str!("../../../../../oracle/measured/live-corners-2025.json")).unwrap();
        let corners = |v: &serde_json::Value| -> [Corner; 4] {
            let a = v.as_array().unwrap();
            std::array::from_fn(|k| Corner { shape: a[k][0].as_str().unwrap().into(), radius: a[k][1].as_f64().unwrap() })
        };
        let (mut drags, mut cycles) = (0, 0);
        for c in all["drags"].as_array().unwrap() {
            let (before, after) = (corners(&c["before"]), corners(&c["after"]));
            let k = c["corner"].as_u64().unwrap() as usize;
            let (w, h) = (c["w"].as_f64().unwrap(), c["h"].as_f64().unwrap());
            let start = if before[k].shape == "None" { 0.0 } else { before[k].radius };
            let by = after[k].radius - start;
            let got = dragged(&before, k, by, c["shift"].as_bool().unwrap(), w.min(h));
            // a zero drag leaves the corners alone in InDesign (the diamond never moved): nothing to compare
            if c["label"].as_str().unwrap().ends_with("h0") { assert_eq!(after, before, "{c}"); continue; }
            assert_eq!(got.iter().map(|x| (x.shape.clone(), (x.radius * 1e4).round())).collect::<Vec<_>>(),
                after.iter().map(|x| (x.shape.clone(), (x.radius * 1e4).round())).collect::<Vec<_>>(), "{}", c["label"]);
            drags += 1;
        }
        for c in all["cycles"].as_array().unwrap() {
            let got = cycled(&corners(&c["before"]), c["corner"].as_u64().unwrap() as usize, c["only_corner"].as_bool().unwrap());
            assert_eq!(got, corners(&c["after"]), "{}", c["label"]);
            cycles += 1;
        }
        assert!(drags >= 30 && cycles >= 20, "{drags} drags, {cycles} cycles");
    }
}
