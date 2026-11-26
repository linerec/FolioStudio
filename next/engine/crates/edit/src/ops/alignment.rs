//! InDesign alignment/distribution. Only translations change; see basic-align.json.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlignMode {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
    HSpace,
    VSpace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlignBounds {
    #[default]
    Selection,
    Margin,
    Page,
    Spread,
    Key,
    /// The spread's pages with the document bleed around them (measured in InDesign 2025: left the
    /// inside-or-left bleed and right the outside-or-right one, on a spread as on a single page).
    Bleed,
}

type Box4 = [f64; 4];
fn union(a: Box4, b: Box4) -> Box4 {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn point_box(p: (f64, f64)) -> Box4 {
    [p.0, p.1, p.0, p.1]
}
fn unit(p: (f64, f64)) -> (f64, f64) {
    let n = p.0.hypot(p.1);
    if n < 1e-14 {
        (0.0, 0.0)
    } else {
        (p.0 / n, p.1 / n)
    }
}
fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

/// Stroke outline, then transform: rotating/shearing a stroked rectangle does not mean
/// adding half the weight to its already transformed box (align-wide-stroke,
/// align-selection-left/right). Groups union the children's outlines (align-rotated-group).
pub(super) fn visible(w: &Work<'_>, ix: &Index, t: &str, i: usize, m: Mat) -> Option<Box4> {
    visible_hair(w, ix, t, i, m, false)
}

/// `visible`, and with `hair` a hairline stroke (weight 0, colour applied) reaches half a point outside the path, as InDesign's
/// Adjust Layout counts it (layoutadj-c2).
pub(super) fn visible_hair(w: &Work<'_>, ix: &Index, t: &str, i: usize, m: Mat, hair: bool) -> Option<Box4> {
    if ix.local(t, i) == "Group" {
        return ix
            .children(i)
            .iter()
            .copied()
            .filter(|&c| is_item(ix, t, c))
            .filter_map(|c| visible_hair(w, ix, t, c, m.then_after(&item_transform(ix, t, c)), hair))
            .reduce(union);
    }
    let b = own_bounds(ix, t, i, &m)?;
    let mut stroke = Stroke::of(w, ix, t, i);
    if hair && stroke.outset <= 0.0 {
        let weight: f64 = stroke_attr(w, ix, t, i, "StrokeWeight").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if weight <= 0.0 && stroke_attr(w, ix, t, i, "StrokeColor").is_some_and(|c| c != "Swatch/None") { stroke.outset = 0.5; }
    }
    if stroke.outset <= 0.0 {
        return Some(b);
    }
    let path = own_path(ix, t, i)?;
    stroked(b, path.subpaths.iter().map(|(_, p, c)| (p.as_slice(), *c)), &m, &stroke)
}

/// What of an item's stroke reaches outside its path: how far (alignment applied), the joins,
/// caps and miter limit.
#[derive(Clone, Debug)]
pub(super) struct Stroke {
    pub outset: f64,
    pub join: String,
    pub cap: String,
    pub limit: f64,
}

impl Stroke {
    pub(super) fn of(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> Stroke {
        Stroke {
            outset: stroke_outset(w, ix, t, i),
            join: stroke_attr(w, ix, t, i, "EndJoin").unwrap_or_default(),
            cap: stroke_attr(w, ix, t, i, "EndCap").unwrap_or_default(),
            limit: stroke_attr(w, ix, t, i, "MiterLimit").and_then(|s| s.parse::<f64>().ok()).unwrap_or(4.0),
        }
    }
}

/// The bounds through `m` of paths drawn with `stroke` (InDesign's visible bounds), from the
/// paths' own bounds `b`: stroke sides, curve offsets, caps and mitred joins.
pub(super) fn stroked<'p>(mut b: Box4, subpaths: impl Iterator<Item = (&'p [PathPoint], bool)>, m: &Mat, stroke: &Stroke) -> Option<Box4> {
    let (o, join, cap, limit) = (stroke.outset, stroke.join.as_str(), stroke.cap.as_str(), stroke.limit);
    if o <= 0.0 {
        return Some(b);
    }
    for (pts, closed) in subpaths {
        if pts.len() < 2 {
            continue;
        }
        let n = pts.len();
        let segments = if closed { n } else { n - 1 };
        let mut tangents = Vec::new();
        for j in 0..segments {
            let (p, q) = (&pts[j], &pts[(j + 1) % n]);
            let c = [p.anchor, p.right, q.left, q.anchor];
            let start = [sub(c[1], c[0]), sub(c[2], c[0]), sub(c[3], c[0])]
                .into_iter()
                .find(|v| v.0.hypot(v.1) > 1e-12)
                .map(unit)?;
            let end = [sub(c[3], c[2]), sub(c[3], c[1]), sub(c[3], c[0])]
                .into_iter()
                .find(|v| v.0.hypot(v.1) > 1e-12)
                .map(unit)?;
            tangents.push((start, end));
            // Exact straight sides (including butt line caps): align-oval-line.
            let straight = p.anchor == p.right && q.left == q.anchor;
            for sign in [-1.0, 1.0] {
                if straight {
                    for a in [p.anchor, q.anchor] {
                        b = union(
                            b,
                            point_box(
                                m.apply((a.0 - start.1 * o * sign, a.1 + start.0 * o * sign)),
                            ),
                        );
                    }
                } else {
                    // Extremes of the offset curve. Bracket locally, then refine to well
                    // below the oracle's 1e-6 point tolerance; control-point bounds are wrong.
                    let offset = |v: f64| {
                        let u = 1.0 - v;
                        let at = (
                            u * u * u * c[0].0
                                + 3.0 * u * u * v * c[1].0
                                + 3.0 * u * v * v * c[2].0
                                + v * v * v * c[3].0,
                            u * u * u * c[0].1
                                + 3.0 * u * u * v * c[1].1
                                + 3.0 * u * v * v * c[2].1
                                + v * v * v * c[3].1,
                        );
                        let d = unit((
                            u * u * (c[1].0 - c[0].0)
                                + 2.0 * u * v * (c[2].0 - c[1].0)
                                + v * v * (c[3].0 - c[2].0),
                            u * u * (c[1].1 - c[0].1)
                                + 2.0 * u * v * (c[2].1 - c[1].1)
                                + v * v * (c[3].1 - c[2].1),
                        ));
                        let d = if d == (0.0, 0.0) {
                            if v < 0.5 { start } else { end }
                        } else {
                            d
                        };
                        m.apply((at.0 - d.1 * o * sign, at.1 + d.0 * o * sign))
                    };
                    let samples: Vec<_> = (0..=32).map(|k| offset(k as f64 / 32.0)).collect();
                    for p in &samples {
                        b = union(b, point_box(*p));
                    }
                    for axis in 0..2 {
                        let val = |p: (f64, f64)| if axis == 0 { p.0 } else { p.1 };
                        for k in 1..32 {
                            let (a, v, z) =
                                (val(samples[k - 1]), val(samples[k]), val(samples[k + 1]));
                            if (v > a && v > z) || (v < a && v < z) {
                                let sign = if v > a { 1.0 } else { -1.0 };
                                let (mut lo, mut hi) =
                                    ((k - 1) as f64 / 32.0, (k + 1) as f64 / 32.0);
                                for _ in 0..64 {
                                    let x = lo + (hi - lo) / 3.0;
                                    let y = hi - (hi - lo) / 3.0;
                                    if val(offset(x)) * sign < val(offset(y)) * sign {
                                        lo = x;
                                    } else {
                                        hi = y;
                                    }
                                }
                                b = union(b, point_box(offset((lo + hi) * 0.5)));
                            }
                        }
                    }
                }
            }
        }
        let ellipse = |a: (f64, f64)| {
            let c = m.apply(a);
            let dx = o * m.0[0].hypot(m.0[2]);
            let dy = o * m.0[1].hypot(m.0[3]);
            [c.0 - dx, c.1 - dy, c.0 + dx, c.1 + dy]
        };
        for j in 0..n {
            if !closed && (j == 0 || j == n - 1) {
                if cap == "RoundEndCap" {
                    b = union(b, ellipse(pts[j].anchor));
                }
                if cap == "ProjectingEndCap" {
                    let d = if j == 0 {
                        let d = tangents[0].0;
                        (-d.0, -d.1)
                    } else {
                        tangents[segments - 1].1
                    };
                    for sign in [-1.0, 1.0] {
                        b = union(
                            b,
                            point_box(m.apply((
                                pts[j].anchor.0 + o * (d.0 - sign * d.1),
                                pts[j].anchor.1 + o * (d.1 + sign * d.0),
                            ))),
                        );
                    }
                }
                continue;
            }
            let (a, z) = (tangents[(j + n - 1) % n].1, tangents[j].0);
            if join == "RoundEndJoin" {
                b = union(b, ellipse(pts[j].anchor));
            } else if join != "BevelEndJoin" {
                let denom = 1.0 + a.0 * z.0 + a.1 * z.1;
                if denom > 1e-12 {
                    let v = (-(a.1 + z.1) * o / denom, (a.0 + z.0) * o / denom);
                    if v.0.hypot(v.1) <= o * limit {
                        let sign = if a.0 * z.1 - a.1 * z.0 > 0.0 {
                            -1.0
                        } else {
                            1.0
                        };
                        b = union(
                            b,
                            point_box(m.apply((
                                pts[j].anchor.0 + v.0 * sign,
                                pts[j].anchor.1 + v.1 * sign,
                            ))),
                        );
                    }
                }
            }
        }
    }
    Some(b)
}

/// `b` with the document bleed (`DocumentPreference`) around it.
fn bleed_box(w: &Work<'_>, b: Box4) -> Box4 {
    let get = |name: &str| w.text("Resources/Preferences.xml").ok().and_then(|prefs| {
        let ix = Index::scan(prefs).ok()?;
        let i = (0..ix.nodes.len()).find(|&i| ix.local(prefs, i) == "DocumentPreference")?;
        ix.attr(prefs, i, name)?.parse::<f64>().ok()
    }).unwrap_or(0.0);
    [b[0] - get("DocumentBleedInsideOrLeftOffset"), b[1] - get("DocumentBleedTopOffset"), b[2] + get("DocumentBleedOutsideOrRightOffset"), b[3] + get("DocumentBleedBottomOffset")]
}

fn page_box(ix: &Index, t: &str, i: usize, margin: bool, facing: bool) -> Option<Box4> {
    let v: Vec<f64> = ix
        .attr(t, i, "GeometricBounds")?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if v.len() != 4 {
        return None;
    }
    let mut b = [v[1], v[0], v[3], v[2]];
    if margin && let Some(&p) = ix.named(t, i, "MarginPreference").first() {
        let get = |k| {
            ix.attr(t, p, k)
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        // Left/Right mean inside/outside on facing pages. BindingLocation separates
        // the left/right pages, not the order of the selected items (basic-margins).
        let left_page = facing
            && ix.nodes[i].parent.is_some_and(|s| {
                let binding = ix
                    .attr(t, s, "BindingLocation")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                ix.named(t, s, "Page")
                    .iter()
                    .position(|&p| p == i)
                    .is_some_and(|n| n < binding)
            });
        let (left, right) = if left_page {
            ("Right", "Left")
        } else {
            ("Left", "Right")
        };
        b = [
            b[0] + get(left),
            b[1] + get("Top"),
            b[2] - get(right),
            b[3] - get("Bottom"),
        ];
    }
    let m = item_transform(ix, t, i);
    [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])]
        .into_iter()
        .map(|p| point_box(m.apply(p)))
        .reduce(union)
}

pub(super) fn run(
    w: &mut Work<'_>,
    ids: &[String],
    how: AlignMode,
    bounds: AlignBounds,
    key: Option<&str>,
    gap: Option<f64>,
    distribute: bool,
) -> Result<(), EditError> {
    let invalid = |msg| EditError::new("invalid", msg);
    let space = matches!(how, AlignMode::HSpace | AlignMode::VSpace);
    if !distribute && space {
        return Err(invalid("spacing is a distribution"));
    }
    if gap.is_some_and(|g| !g.is_finite()) {
        return Err(invalid("spacing must be finite"));
    }
    // InDesign 2025's Document.distribute(PAGE_BOUNDS, BOTTOM_EDGES) uses selection
    // bounds. Repeated captures and three selections confirm it, including a right page:
    // distribute-page-bottom, -shapes, -facing. Margin/spread bottom work as requested.
    let bounds =
        if distribute && bounds == AlignBounds::Page && how == AlignMode::Bottom && gap.is_none() {
            AlignBounds::Selection
        } else {
            bounds
        };
    let axis = if matches!(
        how,
        AlignMode::Left | AlignMode::HCenter | AlignMode::Right | AlignMode::HSpace
    ) {
        0
    } else {
        1
    };
    let fraction = match how {
        AlignMode::Left | AlignMode::Top => 0.0,
        AlignMode::Right | AlignMode::Bottom => 1.0,
        _ => 0.5,
    };
    let pos = |b: Box4| b[axis] + (b[axis + 2] - b[axis]) * fraction;
    let size = |b: Box4| b[axis + 2] - b[axis];
    let mut items: Vec<(String, Box4)> = Vec::new();
    let mut locked = std::collections::BTreeSet::new();
    let mut part = None;
    for id in ids {
        if items.iter().any(|(s, _)| s == id) {
            continue;
        }
        let (p, ix, i) = w.find(id)?;
        let t = w.text(&p)?;
        check_item(&ix, t, i, id)?;
        if ix.ancestors(i).any(|a| {
            ix.attr(t, a, "Self")
                .is_some_and(|s| ids.iter().any(|x| x == s))
        }) {
            continue;
        }
        if part.as_ref().is_some_and(|a| a != &p) {
            return Err(invalid("align objects on one spread at a time"));
        }
        part = Some(p.clone());
        if std::iter::once(i)
            .chain(ix.ancestors(i))
            .any(|a| ix.attr(t, a, "Locked") == Some("true"))
        {
            locked.insert(id.clone());
        }
        let b = visible(
            w,
            &ix,
            t,
            i,
            parent_transform(&ix, t, i).then_after(&item_transform(&ix, t, i)),
        )
        .ok_or_else(|| {
            EditError::new("unsupported", format!("{id}: cannot find alignment bounds"))
        })?;
        if !b.iter().all(|v| v.is_finite()) {
            return Err(invalid("bounds must be finite"));
        }
        items.push((id.clone(), b));
    }
    let selection = items
        .iter()
        .map(|(_, b)| *b)
        .reduce(union)
        .ok_or_else(|| invalid("nothing to align"))?;
    let key_box = if bounds == AlignBounds::Key {
        Some(
            items
                .iter()
                .find(|(id, _)| Some(id.as_str()) == key)
                .ok_or_else(|| invalid("select a key object from the selection"))?
                .1,
        )
    } else {
        None
    };
    let target = match bounds {
        // A locked selected object is the fixed reference for alignment, even if it is
        // not the selection's outermost edge (align-locked, align-locked-left).
        AlignBounds::Selection if !distribute && !locked.is_empty() => items
            .iter()
            .filter(|(id, _)| locked.contains(id))
            .map(|(_, b)| *b)
            .reduce(union)
            .unwrap(),
        AlignBounds::Selection => selection,
        AlignBounds::Key => key_box.unwrap(),
        _ => {
            let p = part.as_ref().unwrap();
            let ix = w.index(p)?;
            let t = w.text(p)?;
            let facing = w
                .text("Resources/Preferences.xml")
                .ok()
                .and_then(|prefs| {
                    let ix = Index::scan(prefs).ok()?;
                    (0..ix.nodes.len())
                        .find(|&i| ix.local(prefs, i) == "DocumentPreference")
                        .map(|i| ix.attr(prefs, i, "FacingPages") == Some("true"))
                })
                .unwrap_or(false);
            let pages: Vec<_> = (0..ix.nodes.len())
                .filter(|&i| ix.local(t, i) == "Page")
                .filter_map(|i| page_box(&ix, t, i, false, facing).map(|b| (i, b)))
                .collect();
            if bounds == AlignBounds::Spread || bounds == AlignBounds::Bleed {
                let b = pages
                    .iter()
                    .map(|(_, b)| *b)
                    .reduce(union)
                    .ok_or_else(|| invalid("spread has no pages"))?;
                if bounds == AlignBounds::Bleed { bleed_box(w, b) } else { b }
            } else {
                // The page under the selection's centre, measured in align-facing-page.
                let cx = (selection[0] + selection[2]) * 0.5;
                let cy = (selection[1] + selection[3]) * 0.5;
                let &(i, _) = pages
                    .iter()
                    .min_by(|(_, a), (_, b)| {
                        let distance =
                            |b: &Box4| (cx - cx.clamp(b[0], b[2])).hypot(cy - cy.clamp(b[1], b[3]));
                        distance(a).total_cmp(&distance(b))
                    })
                    .ok_or_else(|| invalid("spread has no pages"))?;
                page_box(&ix, t, i, bounds == AlignBounds::Margin, facing).unwrap()
            }
        }
    };
    let mut moves = Vec::new();
    if !distribute {
        for (id, b) in items {
            moves.push((id, pos(target) - pos(b)));
        }
    } else {
        if items.len() < 2 {
            return Err(invalid("select at least two objects to distribute"));
        }
        // Edge distributions sort by that edge, spacing sorts by centre:
        // distribute-selection-left vs distribute-selection-hSpace.
        items.sort_by(|a, b| pos(a.1).total_cmp(&pos(b.1)));
        let n = items.len();
        if bounds == AlignBounds::Key || gap.is_some() {
            let distance = gap.ok_or_else(|| invalid("key distribution needs a distance"))?;
            let k = if bounds == AlignBounds::Key {
                items
                    .iter()
                    .position(|(id, _)| Some(id.as_str()) == key)
                    .unwrap()
            } else {
                0
            };
            let mut dest: Vec<f64> = items.iter().map(|(_, b)| pos(*b)).collect();
            for j in k + 1..n {
                dest[j] = dest[j - 1]
                    + distance
                    + if space {
                        (size(items[j - 1].1) + size(items[j].1)) * 0.5
                    } else {
                        0.0
                    };
            }
            for j in (0..k).rev() {
                dest[j] = dest[j + 1]
                    - distance
                    - if space {
                        (size(items[j + 1].1) + size(items[j].1)) * 0.5
                    } else {
                        0.0
                    };
            }
            for (j, (id, b)) in items.into_iter().enumerate() {
                moves.push((id, dest[j] - pos(b)));
            }
        } else if space {
            let (start, end) = if bounds == AlignBounds::Selection {
                (items[0].1[axis], items[n - 1].1[axis + 2])
            } else {
                (target[axis], target[axis + 2])
            };
            let distance =
                (end - start - items.iter().map(|(_, b)| size(*b)).sum::<f64>()) / (n - 1) as f64;
            let mut cursor = start;
            for (id, b) in items {
                moves.push((id, cursor - b[axis]));
                cursor += size(b) + distance;
            }
        } else {
            let (start, end) = if bounds == AlignBounds::Selection {
                (pos(items[0].1), pos(items[n - 1].1))
            } else {
                (
                    target[axis] + size(items[0].1) * fraction,
                    target[axis + 2] - size(items[n - 1].1) * (1.0 - fraction),
                )
            };
            for (j, (id, b)) in items.into_iter().enumerate() {
                moves.push((
                    id,
                    start + (end - start) * j as f64 / (n - 1) as f64 - pos(b),
                ));
            }
        }
    }
    for (id, delta) in moves {
        if !locked.contains(&id) && delta.abs() > 1e-10 {
            move_items(
                w,
                &[id],
                if axis == 0 {
                    (delta, 0.0)
                } else {
                    (0.0, delta)
                },
            )?;
        }
    }
    Ok(())
}
