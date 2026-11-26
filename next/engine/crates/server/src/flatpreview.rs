//! Window > Output > Flattener Preview (rules/windowmenu-c2.ko.md): the page drawn ghosted with the objects a
//! Highlight mode selects in red (`render` with a `view` of mode `flattener`), the objects the mode selects as
//! data (`flattenerPreview`), and the panel's state (`flattenerSettings`, application state: it stays when the panel
//! is hidden or the document is closed).
use crate::Session;
use folio_model::Rect;
use folio_render::Scene;
use folio_render::sep::flat::{self, Facts, Highlight, Plan, Preset};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Instant;

/// The names of the built-in presets, in the panel's order.
pub const BUILT_IN: [&str; 3] = ["[Low Resolution]", "[Medium Resolution]", "[High Resolution]"];

/// The built-in presets' values (InDesign's own: Low and Medium outline strokes, High keeps everything vector).
pub fn built_in(name: &str) -> Option<Preset> {
    Some(match name {
        "[Low Resolution]" => Preset { balance: 75.0, strokes: true, text: false, clip: false },
        "[Medium Resolution]" => Preset { balance: 75.0, strokes: true, text: false, clip: true },
        "[High Resolution]" => Preset::HIGH,
        _ => return None,
    })
}

/// A preset from a request: a built-in's name, or values (`balance`, `strokes`, `text`, `clip`) over the High Resolution ones
/// or over the built-in the `name` names.
pub fn parse_preset(v: &Value) -> Result<Preset, String> {
    let name = v.as_str().or_else(|| v["name"].as_str());
    let mut p = match name {
        Some(n) => built_in(n).or_else(|| v.is_object().then_some(Preset::HIGH)).ok_or_else(|| format!("unknown flattener preset {n:?}"))?,
        None => Preset::HIGH,
    };
    if v.is_object() {
        if let Some(b) = v["balance"].as_f64().or_else(|| v["rasterVectorBalance"].as_f64()) {
            p.balance = b.clamp(0.0, 100.0);
        }
        if let Some(b) = v["strokes"].as_bool().or_else(|| v["convertAllStrokesToOutlines"].as_bool()) {
            p.strokes = b;
        }
        if let Some(b) = v["text"].as_bool().or_else(|| v["convertAllTextToOutlines"].as_bool()) {
            p.text = b;
        }
        if let Some(b) = v["clip"].as_bool().or_else(|| v["clipComplexRegions"].as_bool()) {
            p.clip = b;
        }
    }
    Ok(p)
}

fn highlight(v: &Value) -> Result<Highlight, String> {
    let name = v.as_str().unwrap_or("none");
    Highlight::parse(name).ok_or_else(|| format!("unknown highlight {name:?}"))
}

impl Session {
    fn flat_scene(&self) -> Scene<'_> {
        Scene { doc: &self.doc, colors: &self.colors, layout: &self.layout, glyphs: &*self.fonts, glyph_cache: &self.glyph_cache, images: &self.images }
    }
}

/// The settings the preview follows: the request's `preset` (a name or values) and Ignore Spread Overrides.
fn settings_of(view: &Value) -> Result<Preset, String> {
    parse_preset(&view["preset"])
}

/// A page in the Flattener Preview view: header and RGBA pixels.
pub fn render(s: &Session, page: usize, scale: f64, region: Option<Rect>, view: &Value) -> Result<(Value, Vec<u8>), String> {
    let started = Instant::now();
    let h = highlight(&view["highlight"])?;
    let preset = settings_of(view)?;
    if view["stale"].as_bool().unwrap_or(false) {
        // a mode is chosen but the preview was not refreshed since the last edit: the live page (composited in the document's
        // CMYK blend space, as InDesign shows it) mixed half way with white
        let inks = s.doc_inks();
        let (planes, _, cached) = s.ink_planes(page, scale, region, &s.colors)?;
        let mut rgba = folio_render::sep::composite_with(&planes, inks, &s.colors, &folio_render::sep::View::Overprint, crate::sep::parse_look(&json!({})));
        for px in rgba.chunks_exact_mut(4) {
            for c in &mut px[..3] {
                *c = (*c as u16 + 255).div_ceil(2) as u8;
            }
        }
        let header = json!({"ok": true, "type": "page", "page": page, "width": planes.width, "height": planes.height,
                            "region": region.map(|r| [r.left, r.top, r.right, r.bottom]), "view": "flattener", "stale": true,
                            "stats": {"ms": started.elapsed().as_secs_f64() * 1000.0, "cached": cached}});
        return Ok((header, rgba));
    }
    let inks = s.doc_inks();
    let (planes, area, cached) = s.ink_planes(page, scale, region, &s.colors)?;
    let scene = s.flat_scene();
    let facts = flat::survey(&scene, &s.ink_images, inks, page).ok_or_else(|| format!("no page {page}"))?;
    let plan = flat::select(&facts, h, &preset);
    let mask = if plan.layers.is_empty() { None } else { flat::coverage(&scene, &s.ink_images, inks, page, scale, &area, planes.width, planes.height, &plan) };
    let rgba = flat::compose(&planes, inks, mask.as_deref());
    let header = json!({"ok": true, "type": "page", "page": page, "width": planes.width, "height": planes.height,
                        "region": region.map(|r| [r.left, r.top, r.right, r.bottom]), "view": "flattener",
                        "stats": {"ms": started.elapsed().as_secs_f64() * 1000.0, "cached": cached, "items": facts.len(), "layers": plan.layers.len()}});
    Ok((header, rgba))
}

const PART_NAMES: [&str; 5] = ["behind", "fill", "content", "stroke", "inner"];

fn parts_of(bits: u8) -> Vec<&'static str> {
    PART_NAMES.iter().enumerate().filter(|(i, _)| bits & (1 << i) != 0).map(|(_, n)| *n).collect()
}

fn plan_json(facts: &[Facts], plan: &Plan) -> Value {
    let id = |k: &flat::Key| facts.iter().find(|f| f.key == *k).map(|f| f.id.clone()).unwrap_or_default();
    let rows = |maps: &[std::collections::HashMap<flat::Key, u8>]| -> Vec<Vec<Value>> {
        maps.iter()
            .map(|m| {
                let mut rows: Vec<(&flat::Key, &u8)> = m.iter().collect();
                rows.sort();
                rows.into_iter().map(|(k, bits)| json!({"id": id(k), "parts": parts_of(*bits)})).collect::<Vec<_>>()
            })
            .collect()
    };
    // a layer paints the intersection of its `intersect` lists, hard-edged when `hard`, without what its `minus` lists cover
    let layers: Vec<Value> = plan.layers.iter().map(|l| json!({"intersect": rows(&l.all), "minus": rows(&l.minus), "hard": l.hard})).collect();
    json!(layers)
}

/// `flattenerPreview`: what a Highlight mode selects on a page, as data (item ids with the parts painted).
pub fn report(req: &Value, s: Option<&Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let page = req["page"].as_u64().unwrap_or(0) as usize;
    let h = highlight(&req["highlight"])?;
    let preset = settings_of(req)?;
    let scene = s.flat_scene();
    let facts = flat::survey(&scene, &s.ink_images, s.doc_inks(), page).ok_or_else(|| format!("no page {page}"))?;
    let plan = flat::select(&facts, h, &preset);
    let items: Vec<Value> = facts
        .iter()
        .map(|f| json!({"id": f.id, "transparent": f.transparent(), "transparentParts": parts_of(f.trans_parts()), "text": f.text, "stroke": f.stroke, "gradient": f.gradient, "image": f.image,
                        "bounds": [f.ext.left, f.ext.top, f.ext.right, f.ext.bottom]}))
        .collect();
    Ok(json!({"type": "flattenerPreview", "page": page, "highlight": req["highlight"], "layers": plan_json(&facts, &plan), "items": items}))
}

#[derive(Clone)]
struct State {
    highlight: String,
    auto_refresh: bool,
    preset: String,
    ignore_spread_overrides: bool,
    /// What Apply Settings to Print wrote: the print dialog starts from it.
    print_apply: Option<(String, bool)>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

const HIGHLIGHTS: [&str; 9] = ["none", "complexRegions", "transparent", "affected", "affectedGraphics", "outlinedStrokes", "outlinedText", "rasterFill", "rasterized"];

/// `flattenerSettings`: reads the panel's state and applies the changes in the request's `set` (`highlight`, `autoRefresh`, `preset`,
/// `ignoreSpreadOverrides`); `apply: true` is the Apply Settings to Print button. Replies with the state and which controls are on.
pub fn settings(req: &Value) -> Result<Value, String> {
    let mut guard = STATE.lock().expect("shared state must remain unpoisoned after each operation");
    let st = guard.get_or_insert_with(|| State { highlight: "none".into(), auto_refresh: false, preset: "[High Resolution]".into(), ignore_spread_overrides: false, print_apply: None });
    if let Some(set) = req.get("set").filter(|v| v.is_object()) {
        if let Some(h) = set.get("highlight") {
            let h = h.as_str().filter(|h| HIGHLIGHTS.contains(h)).ok_or("unknown highlight")?;
            st.highlight = h.to_owned();
        }
        if let Some(b) = set["autoRefresh"].as_bool() {
            st.auto_refresh = b;
        }
        if let Some(p) = set.get("preset") {
            st.preset = p.as_str().ok_or("preset must be a name")?.to_owned();
        }
        if let Some(b) = set["ignoreSpreadOverrides"].as_bool() {
            st.ignore_spread_overrides = b;
        }
    }
    let on = st.highlight != "none";
    if req["apply"].as_bool().unwrap_or(false) {
        if !on {
            return Err("Apply Settings to Print needs a Highlight mode".into());
        }
        st.print_apply = Some((st.preset.clone(), st.ignore_spread_overrides));
    }
    Ok(json!({
        "type": "flattenerSettings",
        "state": {"highlight": st.highlight, "autoRefresh": st.auto_refresh, "preset": st.preset, "ignoreSpreadOverrides": st.ignore_spread_overrides},
        "highlights": HIGHLIGHTS, "builtInPresets": BUILT_IN,
        // the controls the panel keeps on: all but Highlight need a Highlight mode; Refresh is off while Auto Refresh is on
        "enabled": {"highlight": true, "autoRefresh": on, "refresh": on && !st.auto_refresh, "preset": on, "ignoreSpreadOverrides": on, "applySettingsToPrint": on},
        "printApply": st.print_apply.as_ref().map(|(n, i)| json!({"flattenerPresetName": n, "ignoreSpreadOverrides": i})),
    }))
}
