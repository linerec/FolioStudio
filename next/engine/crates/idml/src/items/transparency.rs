//! Reads item blend modes and object, fill, stroke and content effects.

use super::{GRAPHIC_ELEMENTS, ItemReader, point};
use crate::color::Colors;
use crate::styles::{Cat, ObjectProps};
use crate::warn::Warnings;
use folio_model::{BlendMode, PageItem};

impl<'a> ItemReader<'a, '_> {
    pub(super) fn transparency(&self, o: &ObjectProps<'a, '_>, item: &mut PageItem, w: &mut Warnings) {
        let e = o.item;
        item.print.effects = applied_effects(o);
        item.print.fx = object_effects(o, "TransparencySetting", self.colors, self.global_light, w);
        item.print.level_fx.stroke = object_effects(o, "StrokeTransparencySetting", self.colors, self.global_light, w);
        item.print.level_fx.fill = object_effects(o, "FillTransparencySetting", self.colors, self.global_light, w);
        let graphic_props = e.elements().find(|c| GRAPHIC_ELEMENTS.contains(&c.name)).map(|g| ObjectProps {
            item: g, sheet: self.sheet, chain: self.sheet.object_chain(g.attr("AppliedObjectStyle"), w),
        });
        item.print.level_fx.content = match graphic_props.as_ref().filter(|g| g.item.child("TransparencySetting").is_some() || !g.chain.is_empty()) {
            Some(g) => object_effects(g, "TransparencySetting", self.colors, self.global_light, w),
            None => object_effects(o, "ContentTransparencySetting", self.colors, self.global_light, w),
        };
        let part = |setting: &str| -> Option<(f64, BlendMode)> {
            let opacity = o.f64(&[setting, "BlendingSetting"], "Opacity", Cat::Transparency).map(|v| (v / 100.0).clamp(0.0, 1.0)).unwrap_or(1.0);
            let mode = o.str(&[setting, "BlendingSetting"], "BlendMode", Cat::Transparency).and_then(blend_mode).unwrap_or(BlendMode::Normal);
            (opacity < 1.0 || mode != BlendMode::Normal).then_some((opacity, mode))
        };
        item.print.fill_blend = part("FillTransparencySetting");
        item.print.stroke_blend = part("StrokeTransparencySetting");
        // The Effects panel's Graphic level is the placed graphic's own TransparencySetting
        // (basic-opacity: opacity-content-60, blend-graphic-multiply); it wins over the frame's
        // ContentTransparencySetting.
        let opacity = graphic_props.as_ref().and_then(|g| g.f64(&["TransparencySetting", "BlendingSetting"], "Opacity", Cat::Transparency)).map(|v| (v / 100.0).clamp(0.0, 1.0)).unwrap_or(1.0);
        let mode_name = graphic_props.as_ref().and_then(|g| g.str(&["TransparencySetting", "BlendingSetting"], "BlendMode", Cat::Transparency)).filter(|m| *m != "Normal");
        let mode = mode_name.and_then(blend_mode).unwrap_or(BlendMode::Normal);
        if let Some(m) = mode_name {
            item.print.effects.push(format!("blend mode {m} (content)"));
        }
        if opacity < 1.0 && !item.print.effects.iter().any(|f| f == "content opacity") {
            item.print.effects.push("content opacity".into());
        }
        let graphic_blend = (opacity < 1.0 || mode != BlendMode::Normal).then_some((opacity, mode));
        item.print.content_blend = graphic_blend.or_else(|| part("ContentTransparencySetting"));
        let blending = ["TransparencySetting", "BlendingSetting"];
        item.print.isolate_blending = o.bool(&blending, "IsolateBlending", Cat::Transparency).unwrap_or(false);
        item.print.knockout_group = o.bool(&blending, "KnockoutGroup", Cat::Transparency).unwrap_or(false);
        item.opacity = o
            .f64(&["TransparencySetting", "BlendingSetting"], "Opacity", Cat::Transparency)
            .map(|v| (v / 100.0).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        if let Some(m) = o.str(&["TransparencySetting", "BlendingSetting"], "BlendMode", Cat::Transparency) {
            match blend_mode(m) {
                Some(b) => item.blend_mode = b,
                None => w.add(format!("blend mode {m} read as Normal")),
            }
        }
    }
}

fn blend_mode(m: &str) -> Option<BlendMode> {
    Some(match m {
        "Normal" => BlendMode::Normal,
        "Multiply" => BlendMode::Multiply,
        "Screen" => BlendMode::Screen,
        "Overlay" => BlendMode::Overlay,
        "SoftLight" => BlendMode::SoftLight,
        "HardLight" => BlendMode::HardLight,
        "ColorDodge" => BlendMode::ColorDodge,
        "ColorBurn" => BlendMode::ColorBurn,
        "Darken" => BlendMode::Darken,
        "Lighten" => BlendMode::Lighten,
        "Difference" => BlendMode::Difference,
        "Exclusion" => BlendMode::Exclusion,
        "Hue" => BlendMode::Hue,
        "Saturation" => BlendMode::Saturation,
        "Color" => BlendMode::Color,
        "Luminosity" => BlendMode::Luminosity,
        _ => return None,
    })
}

/// Effects InDesign applies to an item (its own settings, then its object style), and fill /
/// stroke / content transparency the model does not carry. An effect element is written
/// whenever any of its settings differs from the default, so only `Mode` (drop shadow,
/// feather) or `Applied="true"` (the others) turns it on.
/// The effects of one level (`t`: `TransparencySetting`, `StrokeTransparencySetting`, …) with
/// InDesign's defaults for the attributes IDML leaves out.
fn object_effects(o: &ObjectProps<'_, '_>, t: &str, colors: &Colors, light: (f64, f64), w: &mut Warnings) -> folio_model::ObjectEffects {
    let global = |fx: &str| o.bool(&[t, fx], "UseGlobalLight", Cat::Transparency).unwrap_or(false);
    let f = |fx: &str, k: &str, d: f64| o.f64(&[t, fx], k, Cat::Transparency).unwrap_or(d);
    let s = |fx: &str, k: &str| o.str(&[t, fx], k, Cat::Transparency);
    let on = |fx: &str, key: &str| match s(fx, key) {
        Some(v) if key == "Mode" => v != "None",
        Some(v) => v == "true",
        None => false,
    };
    let shadow = |fx: &str, blend: BlendMode, opacity: f64, x: f64, y: f64, size: f64, color: &str, w: &mut Warnings| folio_model::ShadowEffect {
        blend: s(fx, "BlendMode").and_then(blend_mode).unwrap_or(blend),
        opacity: (f(fx, "Opacity", opacity) / 100.0).clamp(0.0, 1.0),
        x_offset: f(fx, "XOffset", x),
        y_offset: f(fx, "YOffset", y),
        size: f(fx, "Size", size).max(0.0),
        spread: (f(fx, "Spread", 0.0) / 100.0).clamp(0.0, 1.0),
        noise: f(fx, "Noise", 0.0),
        color: colors.paint(Some(s(fx, "EffectColor").unwrap_or(color)), None, w),
        knocked_out: o.bool(&[t, fx], "KnockedOut", Cat::Transparency).unwrap_or(true),
        precise: s(fx, "Technique") == Some("Precise"),
    };
    let mut e = folio_model::ObjectEffects::default();
    if on("DropShadowSetting", "Mode") {
        let mut d = shadow("DropShadowSetting", BlendMode::Multiply, 75.0, 7.0, 7.0, 5.0, "Color/Black", w);
        // with the global light the shadow keeps its distance and falls away from that light
        // (basic-effects: fx-global-light-shadow)
        if global("DropShadowSetting") {
            let (dist, a) = (d.x_offset.hypot(d.y_offset), light.0.to_radians());
            (d.x_offset, d.y_offset) = (-dist * a.cos(), dist * a.sin());
        }
        e.drop_shadow = Some(d);
    }
    if on("OuterGlowSetting", "Applied") {
        // a glow is Paper unless it says otherwise (basic-effects: fx-outer-glow-paper)
        e.outer_glow = Some(shadow("OuterGlowSetting", BlendMode::Screen, 75.0, 0.0, 0.0, 7.0, "Color/Paper", w));
        if let Some(g) = &mut e.outer_glow {
            g.x_offset = 0.0;
            g.y_offset = 0.0;
        }
    }
    if on("FeatherSetting", "Mode") {
        e.feather = Some(folio_model::FeatherEffect {
            width: f("FeatherSetting", "Width", 9.0).max(0.0),
            choke: (f("FeatherSetting", "ChokeAmount", 0.0) / 100.0).clamp(0.0, 1.0),
            corners: match s("FeatherSetting", "CornerType") {
                Some("Sharp") => folio_model::FeatherCorners::Sharp,
                Some("Rounded") => folio_model::FeatherCorners::Rounded,
                _ => folio_model::FeatherCorners::Diffusion,
            },
            noise: f("FeatherSetting", "Noise", 0.0).clamp(0.0, 100.0),
        });
    }
    if on("DirectionalFeatherSetting", "Applied") {
        let d = "DirectionalFeatherSetting";
        e.directional_feather = Some(folio_model::DirectionalFeather {
            left: f(d, "LeftWidth", 0.0).max(0.0),
            right: f(d, "RightWidth", 0.0).max(0.0),
            top: f(d, "TopWidth", 0.0).max(0.0),
            bottom: f(d, "BottomWidth", 0.0).max(0.0),
            choke: (f(d, "ChokeAmount", 0.0) / 100.0).clamp(0.0, 1.0),
            angle: f(d, "Angle", 0.0),
            follow: match s(d, "FollowShapeMode") {
                Some("AllEdges") => folio_model::FollowShape::AllEdges,
                Some("FirstEdge") => folio_model::FollowShape::FirstEdge,
                _ => folio_model::FollowShape::LeadingEdge,
            },
            noise: f(d, "Noise", 0.0).clamp(0.0, 100.0),
        });
    }
    // Light angles are counter-clockwise from 3 o'clock; a shadow falls away from the light.
    let away = |angle: f64, distance: f64| {
        let a = angle.to_radians();
        (-distance * a.cos(), distance * a.sin())
    };
    if on("InnerShadowSetting", "Applied") {
        let fx = "InnerShadowSetting";
        let angle = if global(fx) { light.0 } else { f(fx, "Angle", 120.0) };
        let (x, y) = away(angle, f(fx, "Distance", 7.0));
        e.inner_shadow = Some(folio_model::ShadowEffect {
            blend: s(fx, "BlendMode").and_then(blend_mode).unwrap_or(BlendMode::Multiply),
            opacity: (f(fx, "Opacity", 75.0) / 100.0).clamp(0.0, 1.0),
            x_offset: x,
            y_offset: y,
            size: f(fx, "Size", 7.0).max(0.0),
            spread: (f(fx, "ChokeAmount", 0.0) / 100.0).clamp(0.0, 1.0),
            noise: f(fx, "Noise", 0.0),
            color: colors.paint(Some(s(fx, "EffectColor").unwrap_or("Color/Black")), None, w),
            knocked_out: false,
            precise: false,
        });
    }
    if on("InnerGlowSetting", "Applied") {
        let fx = "InnerGlowSetting";
        e.inner_glow = Some(folio_model::InnerGlow {
            blend: s(fx, "BlendMode").and_then(blend_mode).unwrap_or(BlendMode::Screen),
            opacity: (f(fx, "Opacity", 75.0) / 100.0).clamp(0.0, 1.0),
            size: f(fx, "Size", 7.0).max(0.0),
            choke: (f(fx, "Spread", 0.0) / 100.0).clamp(0.0, 1.0),
            noise: f(fx, "Noise", 0.0),
            color: colors.paint(Some(s(fx, "EffectColor").unwrap_or("Color/Paper")), None, w),
            center: s(fx, "Source") == Some("CenterSourced"),
            precise: s(fx, "Technique") == Some("Precise"),
        });
    }
    if on("BevelAndEmbossSetting", "Applied") {
        let fx = "BevelAndEmbossSetting";
        e.bevel = Some(folio_model::BevelEmboss {
            style: match s(fx, "Style") {
                Some("OuterBevel") => folio_model::BevelStyle::Outer,
                Some("Emboss") => folio_model::BevelStyle::Emboss,
                Some("PillowEmboss") => folio_model::BevelStyle::Pillow,
                _ => folio_model::BevelStyle::Inner,
            },
            technique: match s(fx, "Technique") {
                Some("ChiselHard") => folio_model::BevelTechnique::ChiselHard,
                Some("ChiselSoft") => folio_model::BevelTechnique::ChiselSoft,
                _ => folio_model::BevelTechnique::Smooth,
            },
            size: f(fx, "Size", 7.0).max(0.0),
            soften: f(fx, "Soften", 0.0).max(0.0),
            depth: (f(fx, "Depth", 100.0) / 100.0).clamp(0.0, 10.0),
            up: s(fx, "Direction") != Some("Down"),
            angle: if global(fx) { light.0 } else { f(fx, "Angle", 120.0) },
            altitude: if global(fx) { light.1 } else { f(fx, "Altitude", 30.0) },
            highlight: colors.paint(Some(s(fx, "HighlightColor").unwrap_or("Color/Paper")), None, w),
            highlight_blend: s(fx, "HighlightBlendMode").and_then(blend_mode).unwrap_or(BlendMode::Screen),
            highlight_opacity: (f(fx, "HighlightOpacity", 75.0) / 100.0).clamp(0.0, 1.0),
            shadow: colors.paint(Some(s(fx, "ShadowColor").unwrap_or("Color/Black")), None, w),
            shadow_blend: s(fx, "ShadowBlendMode").and_then(blend_mode).unwrap_or(BlendMode::Multiply),
            shadow_opacity: (f(fx, "ShadowOpacity", 75.0) / 100.0).clamp(0.0, 1.0),
        });
    }
    if on("SatinSetting", "Applied") {
        let fx = "SatinSetting";
        e.satin = Some(folio_model::Satin {
            blend: s(fx, "BlendMode").and_then(blend_mode).unwrap_or(BlendMode::Multiply),
            opacity: (f(fx, "Opacity", 50.0) / 100.0).clamp(0.0, 1.0),
            angle: f(fx, "Angle", 120.0),
            distance: f(fx, "Distance", 7.0),
            size: f(fx, "Size", 7.0).max(0.0),
            invert: s(fx, "InvertEffect") == Some("true"),
            color: colors.paint(Some(s(fx, "EffectColor").unwrap_or("Color/Black")), None, w),
        });
    }
    if on("GradientFeatherSetting", "Applied") {
        let g = "GradientFeatherSetting";
        e.gradient_feather = Some(folio_model::GradientFeather {
            kind: if s(g, "Type") == Some("Radial") { folio_model::GradientKind::Radial } else { folio_model::GradientKind::Linear },
            angle: f(g, "Angle", 0.0),
            length: o.f64(&[t, g], "Length", Cat::Transparency),
            start: s(g, "GradientStart").and_then(|v| point(Some(v))),
            stops: Vec::new(),
        });
    }
    e
}

fn applied_effects(o: &ObjectProps<'_, '_>) -> Vec<String> {
    const EFFECTS: [(&str, &str); 9] = [
        ("DropShadowSetting", "Mode"),
        ("FeatherSetting", "Mode"),
        ("InnerShadowSetting", "Applied"),
        ("OuterGlowSetting", "Applied"),
        ("InnerGlowSetting", "Applied"),
        ("BevelAndEmbossSetting", "Applied"),
        ("SatinSetting", "Applied"),
        ("DirectionalFeatherSetting", "Applied"),
        ("GradientFeatherSetting", "Applied"),
    ];
    let mut out = Vec::new();
    for (setting, what) in [
        ("TransparencySetting", "object"),
        ("FillTransparencySetting", "fill"),
        ("StrokeTransparencySetting", "stroke"),
        ("ContentTransparencySetting", "content"),
    ] {
        for (fx, key) in EFFECTS {
            let on = match o.str(&[setting, fx], key, Cat::Transparency) {
                Some(v) if key == "Mode" => v != "None",
                Some(v) => v == "true",
                None => false,
            };
            if on {
                out.push(format!("{fx} ({what})"));
            }
        }
        if what != "object" {
            if let Some(m) = o.str(&[setting, "BlendingSetting"], "BlendMode", Cat::Transparency)
                && m != "Normal"
            {
                out.push(format!("blend mode {m} ({what})"));
            }
            if o.f64(&[setting, "BlendingSetting"], "Opacity", Cat::Transparency).is_some_and(|v| v < 100.0) {
                out.push(format!("{what} opacity"));
            }
        }
    }
    out
}
