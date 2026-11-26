//! Separations and overprint preview commands: the plates of the document (`inks`), pages
//! rendered as ink and shown as a view (`render` with `view`), and the ink at a point
//! (`inkAt`). The ink planes of recent renders are kept, so switching plates or views and
//! reading the ink under the cursor reuse them instead of drawing the page again.

use folio_model::Rect;
use folio_render::sep::{DocInks, InkKind, InkPlanes, Look, View};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// A render's view, from the request's `view` object.
pub fn parse_view(v: &Value, inks: &DocInks) -> Result<View, String> {
    match v["mode"].as_str().unwrap_or("") {
        "overprint" => Ok(View::Overprint),
        "separations" => {
            // `inks`: the plates shown (names); absent = all
            let visible = match v["inks"].as_array() {
                Some(names) => {
                    let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
                    inks.inks.iter().map(|k| names.contains(&k.name.as_str())).collect()
                }
                None => vec![true; inks.len()],
            };
            Ok(View::Separations { visible })
        }
        "inkLimit" => Ok(View::InkLimit { limit: v["limit"].as_f64().unwrap_or(300.0).clamp(0.0, 10_000.0) }),
        other => Err(format!("unknown view mode {other:?}")),
    }
}

/// The panel menu's switches from the request's `view` object (`singlePlatesBlack`, on when absent; `desaturateBlack`, off when absent).
pub fn parse_look(v: &Value) -> Look {
    Look { single_plates_black: v["singlePlatesBlack"].as_bool().unwrap_or(true), desaturate_black: v["desaturateBlack"].as_bool().unwrap_or(false) }
}

pub fn view_name(v: &View) -> &'static str {
    match v {
        View::Overprint => "overprint",
        View::Separations { .. } => "separations",
        View::InkLimit { .. } => "inkLimit",
    }
}

/// The plates as the window lists them.
pub fn inks_view(inks: &DocInks) -> Value {
    let rows: Vec<Value> = inks
        .inks
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let pages: Vec<usize> = inks.pages.iter().enumerate().filter(|(_, p)| p.contains(&i)).map(|(page, _)| page).collect();
            json!({
                "name": k.name,
                "kind": match k.kind { InkKind::Process => "process", InkKind::Spot => "spot" },
                "inkType": k.ink_type,
                "rgb": k.rgb,
                "cmyk": k.cmyk.map(|v| (v as f64 * 1000.0).round() / 10.0),
                "aliases": k.aliases,
                "pages": pages,
            })
        })
        .collect();
    json!({"inks": rows, "pages": inks.pages, "converted": inks.converted})
}

/// One cached render of ink planes.
struct Entry {
    page: usize,
    scale: f64,
    /// The page area the planes show (points from the page's top left).
    area: Rect,
    planes: Arc<InkPlanes>,
    /// The conversions the planes were drawn with (`Colors::key`): a proof draws RGB content into the proof device's inks.
    colors: u64,
}

/// Ink planes of recent renders, newest last, within a byte budget.
#[derive(Default)]
pub struct PlaneCache {
    entries: Mutex<Vec<Entry>>,
}

const BUDGET: usize = 384 << 20;
const MAX_ENTRIES: usize = 48;

impl PlaneCache {
    pub fn get(&self, page: usize, scale: f64, area: &Rect, colors: u64) -> Option<Arc<InkPlanes>> {
        let mut e = self.entries.lock().expect("shared state must remain unpoisoned after each operation");
        let i = e.iter().position(|x| x.page == page && x.colors == colors && (x.scale - scale).abs() < 1e-9 && same(&x.area, area))?;
        let hit = e.remove(i);
        let planes = hit.planes.clone();
        e.push(hit);
        Some(planes)
    }

    pub fn put(&self, page: usize, scale: f64, area: Rect, planes: Arc<InkPlanes>, colors: u64) {
        let mut e = self.entries.lock().expect("shared state must remain unpoisoned after each operation");
        e.retain(|x| !(x.page == page && x.colors == colors && (x.scale - scale).abs() < 1e-9 && same(&x.area, &area)));
        e.push(Entry { page, scale, area, planes, colors });
        while e.len() > MAX_ENTRIES || (e.len() > 1 && e.iter().map(|x| x.planes.bytes()).sum::<usize>() > BUDGET) {
            e.remove(0);
        }
    }

    /// The sharpest cached planes of `page` that show the point `(x, y)`, with the pixel.
    pub fn at_point(&self, page: usize, x: f64, y: f64) -> Option<(Arc<InkPlanes>, u32, u32, f64)> {
        let e = self.entries.lock().expect("shared state must remain unpoisoned after each operation");
        e.iter()
            .filter(|c| c.page == page && x >= c.area.left && x < c.area.right && y >= c.area.top && y < c.area.bottom)
            .max_by(|a, b| a.scale.total_cmp(&b.scale))
            .and_then(|c| {
                let px = ((x - c.area.left) * c.scale).floor() as i64;
                let py = ((y - c.area.top) * c.scale).floor() as i64;
                (px >= 0 && py >= 0 && px < c.planes.width as i64 && py < c.planes.height as i64).then(|| (c.planes.clone(), px as u32, py as u32, c.scale))
            })
    }
}

fn same(a: &Rect, b: &Rect) -> bool {
    (a.left - b.left).abs() < 1e-6 && (a.top - b.top).abs() < 1e-6 && (a.right - b.right).abs() < 1e-6 && (a.bottom - b.bottom).abs() < 1e-6
}

/// The `inkAt` reply: every plate's ink at the point and the total.
pub fn ink_at_view(inks: &DocInks, values: &[f64], page: usize, x: f64, y: f64, scale: f64, cached: bool) -> Value {
    let rows: Vec<Value> = inks.inks.iter().zip(values).map(|(k, v)| json!({"name": k.name, "percent": (v * 10.0).round() / 10.0})).collect();
    let total: f64 = values.iter().sum();
    json!({"type": "inkAt", "page": page, "x": x, "y": y, "inks": rows, "total": (total * 10.0).round() / 10.0, "scale": scale, "cached": cached})
}
