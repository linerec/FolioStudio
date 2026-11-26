//! The Effects dialog (Object ▸ Effects; next/oracle/scenarios/basic-effects.json). Each effect
//! is a child of a level's transparency element (blending.rs) and carries only the settings that
//! differ from InDesign's defaults (or from the object style's): turned on, a drop shadow is
//! `<DropShadowSetting Mode="Drop" />`; turned off, the switch goes and the other values stay
//! (`<DropShadowSetting Size="12" />`), and an effect left with nothing goes. A drop shadow keeps
//! only its offsets. The angle describes the light direction, so the offset points away from it.
//!
//! One table holds every effect's settings, their kinds and InDesign's defaults (as its
//! Preferences list them, in that order); it writes the settings and reads them back for the
//! dialog.
use std::collections::BTreeMap;

use serde_json::Value;

use super::blending::{Level, MODES, child_attrs, child_edit, level_element};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    DropShadow,
    InnerShadow,
    OuterGlow,
    InnerGlow,
    BevelAndEmboss,
    Satin,
    BasicFeather,
    DirectionalFeather,
    GradientFeather,
}

#[derive(Clone, Copy)]
enum Kind {
    Num(f64, f64),
    Bool,
    Pick(&'static [&'static str]),
    Color,
    Point,
}

/// (IDML attribute, kind, InDesign's default)
type Setting = (&'static str, Kind, &'static str);

struct Spec {
    effect: Effect,
    element: &'static str,
    /// The switch: (attribute, on, off).
    switch: (&'static str, &'static str, &'static str),
    settings: &'static [Setting],
}

const PCT: Kind = Kind::Num(0.0, 100.0);
const LEN: Kind = Kind::Num(0.0, 1000.0);
const OFFSET: Kind = Kind::Num(-1000.0, 1000.0);
const ANGLE: Kind = Kind::Num(-360.0, 360.0);
const BLEND: Kind = Kind::Pick(&MODES);
const TECHNIQUE: Kind = Kind::Pick(&["Softer", "Precise"]);

const SPECS: [Spec; 9] = [
    Spec { effect: Effect::DropShadow, element: "DropShadowSetting", switch: ("Mode", "Drop", "None"), settings: &[
        ("BlendMode", BLEND, "Multiply"), ("Opacity", PCT, "75"), ("XOffset", OFFSET, "7"), ("YOffset", OFFSET, "7"), ("Size", LEN, "5"),
        ("EffectColor", Kind::Color, "Color/Black"), ("Noise", PCT, "0"), ("Spread", PCT, "0"), ("UseGlobalLight", Kind::Bool, "false"),
        ("KnockedOut", Kind::Bool, "true"), ("HonorOtherEffects", Kind::Bool, "false")] },
    Spec { effect: Effect::BasicFeather, element: "FeatherSetting", switch: ("Mode", "Standard", "None"), settings: &[
        ("Width", LEN, "9"), ("CornerType", Kind::Pick(&["Diffusion", "Rounded", "Sharp"]), "Diffusion"), ("Noise", PCT, "0"), ("ChokeAmount", PCT, "0")] },
    Spec { effect: Effect::InnerShadow, element: "InnerShadowSetting", switch: ("Applied", "true", "false"), settings: &[
        ("EffectColor", Kind::Color, "Color/Black"), ("BlendMode", BLEND, "Multiply"), ("Opacity", PCT, "75"), ("Angle", ANGLE, "120"),
        ("Distance", LEN, "7"), ("UseGlobalLight", Kind::Bool, "false"), ("ChokeAmount", PCT, "0"), ("Size", LEN, "7"), ("Noise", PCT, "0")] },
    Spec { effect: Effect::OuterGlow, element: "OuterGlowSetting", switch: ("Applied", "true", "false"), settings: &[
        ("BlendMode", BLEND, "Screen"), ("Opacity", PCT, "75"), ("Noise", PCT, "0"), ("EffectColor", Kind::Color, "Color/Paper"),
        ("Technique", TECHNIQUE, "Softer"), ("Spread", PCT, "0"), ("Size", LEN, "7")] },
    Spec { effect: Effect::InnerGlow, element: "InnerGlowSetting", switch: ("Applied", "true", "false"), settings: &[
        ("BlendMode", BLEND, "Screen"), ("Opacity", PCT, "75"), ("Noise", PCT, "0"), ("EffectColor", Kind::Color, "Color/Paper"),
        ("Technique", TECHNIQUE, "Softer"), ("Spread", PCT, "0"), ("Size", LEN, "7"), ("Source", Kind::Pick(&["EdgeSourced", "CenterSourced"]), "EdgeSourced")] },
    Spec { effect: Effect::BevelAndEmboss, element: "BevelAndEmbossSetting", switch: ("Applied", "true", "false"), settings: &[
        ("Style", Kind::Pick(&["InnerBevel", "OuterBevel", "Emboss", "PillowEmboss"]), "InnerBevel"),
        ("Technique", Kind::Pick(&["SmoothContour", "ChiselHard", "ChiselSoft"]), "SmoothContour"), ("Depth", Kind::Num(1.0, 1000.0), "100"),
        ("Direction", Kind::Pick(&["Up", "Down"]), "Up"), ("Size", LEN, "7"), ("Soften", LEN, "0"), ("Angle", ANGLE, "120"),
        ("Altitude", Kind::Num(0.0, 90.0), "30"), ("UseGlobalLight", Kind::Bool, "false"), ("HighlightColor", Kind::Color, "Color/Paper"),
        ("HighlightBlendMode", BLEND, "Screen"), ("HighlightOpacity", PCT, "75"), ("ShadowColor", Kind::Color, "Color/Black"),
        ("ShadowBlendMode", BLEND, "Multiply"), ("ShadowOpacity", PCT, "75")] },
    Spec { effect: Effect::Satin, element: "SatinSetting", switch: ("Applied", "true", "false"), settings: &[
        ("EffectColor", Kind::Color, "Color/Black"), ("BlendMode", BLEND, "Multiply"), ("Opacity", PCT, "50"), ("Angle", ANGLE, "120"),
        ("Distance", LEN, "7"), ("Size", LEN, "7"), ("InvertEffect", Kind::Bool, "false")] },
    Spec { effect: Effect::DirectionalFeather, element: "DirectionalFeatherSetting", switch: ("Applied", "true", "false"), settings: &[
        ("LeftWidth", LEN, "0"), ("RightWidth", LEN, "0"), ("TopWidth", LEN, "0"), ("BottomWidth", LEN, "0"), ("ChokeAmount", PCT, "0"),
        ("Angle", ANGLE, "0"), ("FollowShapeMode", Kind::Pick(&["LeadingEdge", "AllEdges", "FirstEdge"]), "LeadingEdge"), ("Noise", PCT, "0")] },
    Spec { effect: Effect::GradientFeather, element: "GradientFeatherSetting", switch: ("Applied", "true", "false"), settings: &[
        ("Type", Kind::Pick(&["Linear", "Radial"]), "Linear"), ("Angle", ANGLE, "0"), ("Length", Kind::Num(0.0, 100_000.0), "0"),
        ("GradientStart", Kind::Point, "0 0"), ("HiliteAngle", ANGLE, "0"), ("HiliteLength", Kind::Num(0.0, 100_000.0), "0")] },
];

fn spec(effect: Effect) -> &'static Spec {
    SPECS.iter().find(|s| s.effect == effect).expect("every effect has a spec")
}

/// A value as IDML writes it, checked against its kind.
fn written(name: &str, kind: Kind, v: &Value) -> Result<String, EditError> {
    let bad = || Err(EditError::new("invalid", format!("{name}: {v} is not a value of it")));
    match kind {
        Kind::Num(lo, hi) => match v.as_f64() {
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(geom::num(x)),
            Some(_) => Err(EditError::new("invalid", format!("{name}: from {lo} to {hi}"))),
            None => bad(),
        },
        Kind::Bool => v.as_bool().map(|b| b.to_string()).map_or_else(bad, Ok),
        Kind::Pick(list) => match v.as_str() {
            Some(s) if list.contains(&s) => Ok(s.to_owned()),
            _ => bad(),
        },
        // an effect takes a colour, a tint or a mixed ink, not None or a gradient
        Kind::Color => match v.as_str() {
            Some(s) if ["Color/", "Tint/", "MixedInk/"].iter().any(|p| s.starts_with(p)) => Ok(s.to_owned()),
            _ => bad(),
        },
        Kind::Point => match v.as_array().map(|a| a.iter().map(Value::as_f64).collect::<Option<Vec<_>>>()) {
            Some(Some(p)) if p.len() == 2 && p.iter().all(|x| x.is_finite()) => Ok(format!("{} {}", geom::num(p[0]), geom::num(p[1]))),
            _ => bad(),
        },
    }
}

/// The written value as JSON for the dialog.
fn shown(kind: Kind, s: &str) -> Value {
    match kind {
        Kind::Num(..) => s.trim().parse::<f64>().map_or(Value::Null, Value::from),
        Kind::Bool => Value::Bool(s == "true"),
        Kind::Point => geom::parse_pair(s).map_or(Value::Null, |(x, y)| serde_json::json!([x, y])),
        Kind::Pick(_) | Kind::Color => Value::from(s),
    }
}

fn same(a: &str, b: &str) -> bool {
    match (geom::parse_pair(a), geom::parse_pair(b)) {
        (Some(p), Some(q)) if a.contains(' ') || b.contains(' ') => (p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9,
        _ => a == b || same_number(a, b),
    }
}

/// A drop shadow's distance and angle (degrees, the light's, in (−180°, 180°]) from its offsets,
/// as InDesign's scripting works them out: the angle of (x, −y) in single precision, turned half
/// a circle; the distance in double precision.
fn distance_angle(x: f64, y: f64) -> (f64, f64) {
    let angle = (-y).atan2(x).to_degrees() as f32 + 180.0;
    let angle = if angle > 180.0 { angle - 360.0 } else { angle };
    (x.hypot(y), f64::from(angle))
}

/// The offsets of a distance and angle: the angle turned half a circle and the distance in single
/// precision (fx-shadow-angle: −7.000000133081195 for 45° at the default distance).
fn offsets(d: f64, angle: f64) -> (f64, f64) {
    let angle = (f64::from(angle as f32 + 180.0) % 360.0).to_radians();
    let (sin, cos) = angle.sin_cos();
    let distance = f64::from(d as f32);
    (cos * distance, -sin * distance)
}

struct Target<'a> {
    owner: usize,
    level: &'static str,
    /// The object style category of the level (none: a placed graphic, no style).
    enable: &'static str,
    item: usize,
    spec: &'a Spec,
}

impl Target<'_> {
    /// What the object's style, else InDesign, gives a setting.
    fn base(&self, w: &Work<'_>, ix: &Index, t: &str, name: &str, default: &str) -> String {
        (!self.enable.is_empty()).then(|| objectstyle::from_style(w, ix, t, self.item, &[self.level, self.spec.element], name, self.enable)).flatten()
            .unwrap_or_else(|| default.to_owned())
    }

    /// Every setting as it is now (the switch first).
    fn now(&self, w: &Work<'_>, ix: &Index, t: &str) -> Vec<(&'static str, Kind, String)> {
        let own = child_attrs(ix, t, self.owner, self.level, self.spec.element);
        let (sw, _, off) = self.spec.switch;
        std::iter::once((sw, Kind::Bool, off)).chain(self.spec.settings.iter().copied()).map(|(name, kind, default)| {
            let v = own.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or_else(|| self.base(w, ix, t, name, default));
            (name, kind, v)
        }).collect()
    }
}

fn target<'a>(ix: &Index, t: &str, i: usize, id: &str, level: Level, spec: &'a Spec) -> Result<Target<'a>, EditError> {
    if ix.local(t, i) == "Group" && level != Level::Object {
        return Err(EditError::new("invalid", format!("{id} is a group: its effects are the object's")));
    }
    let (owner, name, enable) = level_element(ix, t, i, id, level)?;
    Ok(Target { owner, level: name, enable, item: i, spec })
}

pub(super) fn set_effect(w: &mut Work<'_>, ids: &[String], level: Level, effect: Effect, on: Option<bool>, values: &BTreeMap<String, Value>) -> Result<(), EditError> {
    let spec = spec(effect);
    // check every value before anything changes
    let mut given: Vec<(&str, String)> = Vec::new();
    let polar = effect == Effect::DropShadow && (values.contains_key("Distance") || values.contains_key("Angle"));
    for (k, v) in values {
        if polar && matches!(k.as_str(), "Distance" | "Angle") {
            let kind = if k == "Distance" { LEN } else { ANGLE };
            written(k, kind, v)?;
            continue;
        }
        let Some(&(name, kind, _)) = spec.settings.iter().find(|s| s.0 == k) else {
            return Err(EditError::new("invalid", format!("{} has no setting {k}", spec.element)));
        };
        given.push((name, written(name, kind, v)?));
    }
    if polar && given.iter().any(|(k, _)| matches!(*k, "XOffset" | "YOffset")) {
        return Err(EditError::new("invalid", "a drop shadow takes its offsets or its distance and angle, not both"));
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        let tg = target(&ix, &t, i, id, level, spec)?;
        let now = tg.now(w, &ix, &t);
        let mut want: Vec<(&str, String)> = given.clone();
        if let Some(on) = on {
            let (sw, yes, no) = spec.switch;
            want.push((sw, (if on { yes } else { no }).to_owned()));
        }
        if polar {
            let num = |k: &str| now.iter().find(|s| s.0 == k).and_then(|s| s.2.parse::<f64>().ok()).unwrap_or(0.0);
            let (d0, a0) = distance_angle(num("XOffset"), num("YOffset"));
            let d = values.get("Distance").and_then(Value::as_f64).unwrap_or(d0);
            let a = values.get("Angle").and_then(Value::as_f64).unwrap_or(a0);
            let (x, y) = offsets(d, a);
            want.push(("XOffset", geom::num(x)));
            want.push(("YOffset", geom::num(y)));
        }
        // the settings in their order: the object's own where it has them, the changed ones
        // only where they differ from the style or InDesign
        let own = child_attrs(&ix, &t, tg.owner, tg.level, spec.element);
        let (sw, _, off) = spec.switch;
        let mut attrs = Vec::new();
        for (name, _, default) in std::iter::once((sw, Kind::Bool, off)).chain(spec.settings.iter().copied()) {
            let value = match want.iter().rev().find(|(k, _)| *k == name) {
                Some((_, v)) => (!same(v, &tg.base(w, &ix, &t, name, default))).then(|| v.clone()),
                None => own.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()),
            };
            if let Some(v) = value {
                attrs.push((name.to_owned(), v));
            }
        }
        // what the table does not know stays, after it
        attrs.extend(own.into_iter().filter(|(k, _)| k != sw && !spec.settings.iter().any(|s| s.0 == k)));
        if let Some(e) = child_edit(&ix, &t, tg.owner, tg.level, spec.element, &attrs) {
            w.splice(&part, vec![e])?;
        }
    }
    Ok(())
}

/// The Effects dialog's view of an object: for each of its levels, the opacity and blend mode
/// and every effect with all its settings (InDesign's defaults where the object has none; a drop
/// shadow also gets its distance and angle), and the document's global light.
pub(super) fn effects_of(w: &mut Work<'_>, id: Option<&str>) -> Result<Value, EditError> {
    let Some(id) = id else { return Ok(serde_json::json!({"levels": {}, "globalLight": global_light(w)})) };
    let (part, ix, i) = w.find(id)?;
    let w = &*w;
    let t = w.text(&part)?;
    let kind = ix.local(t, i);
    let mut levels = vec![Level::Object];
    if kind != "Group" {
        levels.extend([Level::Stroke, Level::Fill]);
    }
    if kind == "TextFrame" {
        levels.push(Level::Text);
    }
    if ix.children(i).iter().any(|&c| GRAPHICS.contains(&ix.local(t, c))) {
        levels.push(Level::Graphic);
    }
    let mut out = serde_json::Map::new();
    for level in levels {
        let mut fx = serde_json::Map::new();
        for spec in &SPECS {
            let tg = target(&ix, t, i, id, level, spec)?;
            let now = tg.now(w, &ix, t);
            let mut values: serde_json::Map<String, Value> = now[1..].iter().map(|(k, kind, v)| ((*k).to_owned(), shown(*kind, v))).collect();
            if spec.effect == Effect::DropShadow {
                let num = |k: &str| values.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                let (d, a) = distance_angle(num("XOffset"), num("YOffset"));
                values.insert("Distance".into(), Value::from(d));
                values.insert("Angle".into(), Value::from(a));
            }
            let on = now[0].2 == spec.switch.1;
            let name = serde_json::to_value(spec.effect).unwrap_or_default();
            fx.insert(name.as_str().unwrap_or_default().to_owned(), serde_json::json!({"on": on, "values": values}));
        }
        let (owner, name, _) = level_element(&ix, t, i, id, level)?;
        let blend = child_attrs(&ix, t, owner, name, "BlendingSetting");
        let get = |k: &str| blend.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let opacity = get("Opacity").and_then(|v| v.parse::<f64>().ok()).unwrap_or(100.0);
        let level_name = serde_json::to_value(level).unwrap_or_default();
        out.insert(level_name.as_str().unwrap_or_default().to_owned(), serde_json::json!({
            "opacity": opacity, "mode": get("BlendMode").unwrap_or_else(|| "Normal".into()), "effects": fx,
        }));
    }
    Ok(serde_json::json!({"levels": out, "globalLight": global_light(w)}))
}

/// The document's global light (`TransparencyPreference`): `{angle, altitude}`.
fn global_light(w: &Work<'_>) -> Value {
    let light = w.text("Resources/Preferences.xml").ok().and_then(|p| {
        let px = Index::scan(p).ok()?;
        let at = (0..px.nodes.len()).find(|&n| px.local(p, n) == "TransparencyPreference")?;
        let f = |k: &str, d: f64| px.attr(p, at, k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
        Some((f("GlobalLightAngle", 120.0), f("GlobalLightAltitude", 30.0)))
    }).unwrap_or((120.0, 30.0));
    serde_json::json!({"angle": light.0, "altitude": light.1})
}

/// Object ▸ Effects ▸ Global Light (oracle: fx-global-light-set): the document's light, which
/// effects that use the global light take their angle (and a bevel its altitude) from.
pub(super) fn set_global_light(w: &mut Work<'_>, angle: Option<f64>, altitude: Option<f64>) -> Result<(), EditError> {
    if angle.is_some_and(|a| !(a.is_finite() && (-180.0..=180.0).contains(&a))) || altitude.is_some_and(|a| !(a.is_finite() && (0.0..=90.0).contains(&a))) {
        return Err(EditError::new("invalid", "global light: angle −180° to 180°, altitude 0° to 90°"));
    }
    let part = "Resources/Preferences.xml";
    let t = w.text(part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let at = (0..ix.nodes.len()).find(|&n| ix.local(&t, n) == "TransparencyPreference")
        .ok_or_else(|| EditError::new("unsupported", "the document has no TransparencyPreference"))?;
    let mut edits = Vec::new();
    for (k, v) in [("GlobalLightAngle", angle), ("GlobalLightAltitude", altitude)] {
        if let Some(v) = v {
            edits.push(attr_edit(&ix, &t, at, k, &geom::num(v)));
        }
    }
    if edits.is_empty() {
        return Ok(());
    }
    w.splice(part, edits)
}

/// The lengths Adjust Effects when Scaling multiplies (oracle basic-transform-prefs tp-fx-*): the
/// shadow's offsets and size, a glow's, a feather's, a bevel's. Not measured: the directional
/// feather's widths and the gradient feather's length.
const LENGTHS: [(&str, &str); 12] = [
    ("DropShadowSetting", "XOffset"), ("DropShadowSetting", "YOffset"), ("DropShadowSetting", "Size"),
    ("FeatherSetting", "Width"),
    ("InnerShadowSetting", "Distance"), ("InnerShadowSetting", "Size"),
    ("OuterGlowSetting", "Size"), ("InnerGlowSetting", "Size"),
    ("BevelAndEmbossSetting", "Size"), ("BevelAndEmbossSetting", "Soften"),
    ("SatinSetting", "Distance"), ("SatinSetting", "Size"),
];

/// The one factor every effect length takes under a scale of (`sx`, `sy`): the horizontal one,
/// for the shadow's X offset, Y offset and size alike (3 × 2 gave 3, 2 × 3 gave 2:
/// tp-fx-on-nonuniform, tp-fx-on-tall).
fn effect_factor(sx: f64, _sy: f64) -> f64 {
    sx
}

/// The changes that scale the lengths of item `i`'s applied effects (every level) by (`sx`, `sy`);
/// a setting left at InDesign's default is written out once it differs from it.
pub(super) fn scale_lengths(w: &Work<'_>, ix: &Index, t: &str, i: usize, id: &str, sx: f64, sy: f64) -> Result<Vec<(std::ops::Range<usize>, String)>, EditError> {
    let factor = effect_factor(sx, sy);
    let mut edits = Vec::new();
    if factor == 1.0 {
        return Ok(edits);
    }
    for level in [Level::Object, Level::Fill, Level::Stroke, Level::Text, Level::Graphic] {
        for spec in &SPECS {
            let Ok(tg) = target(ix, t, i, id, level, spec) else { continue };
            let own = child_attrs(ix, t, tg.owner, tg.level, spec.element);
            let now = tg.now(w, ix, t);
            if own.is_empty() || now.first().is_none_or(|n| n.2 != spec.switch.1) {
                continue;
            }
            let scaled: Vec<(&str, String)> = LENGTHS.iter().filter(|l| l.0 == spec.element).filter_map(|&(_, name)| {
                let v = now.iter().find(|n| n.0 == name).and_then(|n| n.2.parse::<f64>().ok())?;
                Some((name, geom::num(v * factor)))
            }).collect();
            if scaled.is_empty() {
                continue;
            }
            let attrs: Vec<(String, String)> = now.iter().filter_map(|(name, _, v)| match scaled.iter().find(|s| s.0 == *name) {
                Some((_, n)) => Some(((*name).to_owned(), n.clone())),
                None => own.iter().any(|(k, _)| k == name).then(|| ((*name).to_owned(), v.clone())),
            }).collect();
            edits.extend(child_edit(ix, t, tg.owner, tg.level, spec.element, &attrs));
        }
    }
    Ok(edits)
}
