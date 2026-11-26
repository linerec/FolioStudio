//! Transparency settings of page items and object styles. See `docs/format/transparency.md`.

use super::applied::StyleValues;
use super::kind::Kind;
use super::values::Node;
use super::xml::Xml;
use super::{Writer, num};
use crate::model::{Attrs, Value};

/// Blend mode codes: the index in the schema's `BlendMode` enumeration
/// (`docs/format/transparency.md`, blend mode codes).
const BLEND_MODES: &[(u32, &str)] = &[
    (0, "Normal"),
    (1, "Multiply"),
    (2, "Screen"),
    (3, "Overlay"),
    (4, "SoftLight"),
    (5, "HardLight"),
    (6, "ColorDodge"),
    (7, "ColorBurn"),
    (8, "Darken"),
    (9, "Lighten"),
    (10, "Difference"),
    (11, "Exclusion"),
    (12, "Hue"),
    (13, "Saturation"),
    (14, "Color"),
    (15, "Luminosity"),
];

/// Setting elements, in the order the schema lists them. The index is the
/// setting's number in [`block_id`] and [`Slot::Extra`].
const SETTINGS: [&str; 4] = [
    "TransparencySetting",
    "StrokeTransparencySetting",
    "FillTransparencySetting",
    "ContentTransparencySetting",
];

/// Where an attribute of a setting is stored (`docs/format/transparency.md`,
/// blocks; the complete layout is in `FOLIO_FORK.txt`, revision folio.4).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// A position in the setting's block of 69 attributes ([`block_id`]).
    Block(u32),
    /// Outside the blocks: the ID of the object setting and the step to
    /// the next setting (stroke, fill, content).
    Extra(u32, u32),
}

/// The first 18 positions of the object block, whose IDs are not
/// consecutive: blending (0–3), drop shadow (4–13), feather (14–17).
const OBJECT_HEAD: [u32; 18] = [
    0x10816, 0x10817, 0x10818, 0x10819, 0x1081A, 0x1081B, 0x1081C, 0x1081E, 0x1081F, 0x10820,
    0x1084A, 0x1084B, 0x10837, 0x10890, 0x10821, 0x10822, 0x10825, 0x1084C,
];

/// The attribute-list ID of position `pos` of the block of setting
/// `setting` (0 object, 1 stroke, 2 fill, 3 content). Each block holds
/// the same 69 attributes in the same order; the stroke, fill and content
/// blocks have consecutive IDs with a gap after position 15 (and, for
/// fill, after position 37).
fn block_id(setting: usize, pos: u32) -> u32 {
    match setting {
        0 if pos < 18 => OBJECT_HEAD[pos as usize],
        0 => 0x1084D + (pos - 18),
        1 if pos < 16 => 0x10891 + pos,
        1 => 0x108A3 + (pos - 16),
        2 if pos < 16 => 0x108D8 + pos,
        2 if pos < 38 => 0x108EA + (pos - 16),
        2 => 0x1EB00 + (pos - 38),
        _ if pos < 16 => 0x1EB28 + pos,
        _ => 0x1EB3A + (pos - 16),
    }
}

/// The attribute-list ID of `slot` in setting `setting`.
fn slot_id(setting: usize, slot: Slot) -> u32 {
    match slot {
        Slot::Block(pos) => block_id(setting, pos),
        Slot::Extra(object, step) => object + step * setting as u32,
    }
}

const SHADOW_MODES: &[(u32, &str)] = &[(0, "None"), (1, "Drop")];
const FEATHER_MODES: &[(u32, &str)] = &[(0, "None"), (3, "Standard")];
const FEATHER_CORNERS: &[(u32, &str)] = &[(0, "Sharp"), (1, "Rounded"), (2, "Diffusion")];
const GLOW_TECHNIQUES: &[(u32, &str)] = &[(0, "Softer"), (1, "Precise")];
const GLOW_SOURCES: &[(u32, &str)] = &[(0, "CenterSourced"), (1, "EdgeSourced")];
const BEVEL_STYLES: &[(u32, &str)] = &[
    (0, "OuterBevel"),
    (1, "InnerBevel"),
    (2, "Emboss"),
    (3, "PillowEmboss"),
];
const BEVEL_TECHNIQUES: &[(u32, &str)] =
    &[(0, "SmoothContour"), (1, "ChiselHard"), (2, "ChiselSoft")];
const BEVEL_DIRECTIONS: &[(u32, &str)] = &[(0, "Up"), (1, "Down")];
const FOLLOW_SHAPE: &[(u32, &str)] = &[(0, "None"), (1, "LeadingEdge"), (2, "AllEdges")];
const GRADIENT_TYPES: &[(u32, &str)] = &[(1, "Linear"), (2, "Radial")];

const PERCENT: Kind = Kind::Range(0.0, 100.0);
const ANGLE: Kind = Kind::Range(-180.0, 180.0);
const LENGTH: Kind = Kind::Range(0.0, 1000.0);
const OFFSET: Kind = Kind::Range(-1000.0, 1000.0);
const BLEND: Kind = Kind::Enum(BLEND_MODES);

/// An effect attribute: IDML name, where it is stored and its kind.
type Attr = (&'static str, Slot, Kind);

use Slot::{Block as B, Extra as X};

/// Effect elements of a setting in schema order, each with its attributes
/// in the order InDesign writes them. Ranges are the schema's, except
/// sizes, widths, distances and depths, which keep 0–1000.
const EFFECTS: &[(&str, &[Attr])] = &[
    (
        "BlendingSetting",
        &[
            ("BlendMode", B(1), BLEND),
            ("Opacity", B(0), PERCENT),
            ("KnockoutGroup", B(2), Kind::Bool),
            ("IsolateBlending", B(3), Kind::Bool),
        ],
    ),
    (
        "DropShadowSetting",
        &[
            ("Mode", B(4), Kind::Enum(SHADOW_MODES)),
            ("BlendMode", B(8), BLEND),
            ("Opacity", B(7), PERCENT),
            ("XOffset", B(5), OFFSET),
            ("YOffset", B(6), OFFSET),
            ("Size", B(9), LENGTH),
            ("EffectColor", DROP_SHADOW_COLOR, Kind::Swatch),
            ("Noise", B(11), PERCENT),
            ("Spread", B(10), PERCENT),
            ("UseGlobalLight", B(13), Kind::Bool),
            ("KnockedOut", KNOCKED_OUT, Kind::Bool),
            ("HonorOtherEffects", X(0x1EBCB, 1), Kind::Bool),
        ],
    ),
    (
        "FeatherSetting",
        &[
            ("Mode", B(14), Kind::Enum(FEATHER_MODES)),
            ("Width", B(15), LENGTH),
            ("CornerType", B(16), Kind::Enum(FEATHER_CORNERS)),
            ("Noise", B(17), PERCENT),
            ("ChokeAmount", X(0x1EBB3, 1), PERCENT),
        ],
    ),
    (
        "InnerShadowSetting",
        &[
            ("Applied", B(18), Kind::Bool),
            ("EffectColor", B(19), Kind::Swatch),
            ("BlendMode", B(20), BLEND),
            ("Opacity", B(21), PERCENT),
            ("Angle", B(22), ANGLE),
            ("Distance", B(23), LENGTH),
            ("UseGlobalLight", B(24), Kind::Bool),
            ("ChokeAmount", B(25), PERCENT),
            ("Size", B(26), LENGTH),
            ("Noise", B(27), PERCENT),
        ],
    ),
    (
        "OuterGlowSetting",
        &[
            ("Applied", B(28), Kind::Bool),
            ("BlendMode", B(29), BLEND),
            ("Opacity", B(30), PERCENT),
            ("Noise", B(31), PERCENT),
            ("EffectColor", B(32), Kind::Swatch),
            ("Technique", B(33), Kind::Enum(GLOW_TECHNIQUES)),
            ("Spread", B(34), PERCENT),
            ("Size", B(35), LENGTH),
        ],
    ),
    (
        "InnerGlowSetting",
        &[
            ("Applied", B(36), Kind::Bool),
            ("BlendMode", B(37), BLEND),
            ("Opacity", B(38), PERCENT),
            ("Noise", B(39), PERCENT),
            ("EffectColor", B(40), Kind::Swatch),
            ("Technique", B(41), Kind::Enum(GLOW_TECHNIQUES)),
            ("Spread", B(42), PERCENT),
            ("Size", B(43), LENGTH),
            ("Source", B(44), Kind::Enum(GLOW_SOURCES)),
        ],
    ),
    (
        "BevelAndEmbossSetting",
        &[
            ("Applied", B(45), Kind::Bool),
            ("Style", B(46), Kind::Enum(BEVEL_STYLES)),
            ("Technique", B(47), Kind::Enum(BEVEL_TECHNIQUES)),
            ("Depth", B(48), LENGTH),
            ("Direction", B(49), Kind::Enum(BEVEL_DIRECTIONS)),
            ("Size", B(50), LENGTH),
            ("Soften", B(51), LENGTH),
            ("Angle", B(52), ANGLE),
            ("Altitude", B(53), PERCENT),
            ("UseGlobalLight", B(54), Kind::Bool),
            ("HighlightColor", B(55), Kind::Swatch),
            ("HighlightBlendMode", B(56), BLEND),
            ("HighlightOpacity", B(57), PERCENT),
            ("ShadowColor", B(58), Kind::Swatch),
            ("ShadowBlendMode", B(59), BLEND),
            ("ShadowOpacity", B(60), PERCENT),
        ],
    ),
    (
        "SatinSetting",
        &[
            ("Applied", B(61), Kind::Bool),
            ("EffectColor", B(62), Kind::Swatch),
            ("BlendMode", B(63), BLEND),
            ("Opacity", B(64), PERCENT),
            ("Angle", B(65), ANGLE),
            ("Distance", B(66), LENGTH),
            ("Size", B(67), LENGTH),
            ("InvertEffect", B(68), Kind::Bool),
        ],
    ),
    (
        "DirectionalFeatherSetting",
        &[
            ("Applied", X(0x1EB74, 5), Kind::Bool),
            ("LeftWidth", X(0x1EBB7, 4), LENGTH),
            ("RightWidth", X(0x1EBB8, 4), LENGTH),
            ("TopWidth", X(0x1EBB9, 4), LENGTH),
            ("BottomWidth", X(0x1EBBA, 4), LENGTH),
            ("ChokeAmount", X(0x1EBC7, 1), PERCENT),
            ("Angle", X(0x1EB76, 5), ANGLE),
            ("FollowShapeMode", X(0x1EB77, 5), Kind::Enum(FOLLOW_SHAPE)),
            ("Noise", X(0x1EB78, 5), PERCENT),
        ],
    ),
    (
        "GradientFeatherSetting",
        &[
            ("Applied", X(0x1EB8A, 9), Kind::Bool),
            ("Type", X(0x1EB8B, 9), Kind::Enum(GRADIENT_TYPES)),
            ("Angle", X(0x1EB8D, 9), ANGLE),
            ("Length", X(0x1EB8E, 9), Kind::Number),
            ("GradientStart", X(0x1EB8F, 9), Kind::Point),
            ("HiliteAngle", X(0x1EB91, 9), Kind::Number),
            ("HiliteLength", X(0x1EB92, 9), Kind::Number),
        ],
    ),
];

/// Opacity gradient stops of a gradient feather: the ID of the object
/// setting's list and the step to the next setting.
const STOPS: Slot = X(0x1EB8C, 9);

/// The drop shadow's `EffectColor`. Where the object style stores no
/// swatch (0), IDML writes the item's colour, except for the one item
/// whose colour is Black; the converter leaves Black out there, as the
/// samples do not show why (`docs/format/transparency.md`, drop shadow
/// colour). The same holds for every effect colour: items whose effect
/// is not applied store Black where IDML has none (inner shadow colour),
/// while other colours of effects that are not applied are written
/// (FOLIO_FORK.txt, folio.4).
const DROP_SHADOW_COLOR: Slot = B(12);

/// The drop shadow's `KnockedOut`. An item or object style chain that
/// does not store it has `true` (`docs/format/transparency.md`, drop
/// shadow knockout); the root style stores 1 in all four settings.
const KNOCKED_OUT: Slot = X(0x1EB6F, 1);

/// The value of an attribute that an item or style chain does not store,
/// where the samples show one.
fn unstored(id: u32) -> Option<&'static Value> {
    const TRUE: Value = Value::Enum(1);
    (0..SETTINGS.len())
        .any(|s| slot_id(s, KNOCKED_OUT) == id)
        .then_some(&TRUE)
}

/// The index of the drop shadow in [`EFFECTS`].
const DROP_SHADOW: usize = 1;

/// The setting and effect (indexes in [`SETTINGS`] and [`EFFECTS`]) of
/// each attribute ID, opacity stops included.
fn effect_index() -> &'static std::collections::HashMap<u32, (usize, usize)> {
    static INDEX: std::sync::OnceLock<std::collections::HashMap<u32, (usize, usize)>> =
        std::sync::OnceLock::new();
    INDEX.get_or_init(|| {
        let mut m = std::collections::HashMap::new();
        for k in 0..SETTINGS.len() {
            for (e, (effect, list)) in EFFECTS.iter().enumerate() {
                for a in list.iter() {
                    m.insert(slot_id(k, a.1), (k, e));
                }
                if *effect == "GradientFeatherSetting" {
                    m.insert(slot_id(k, STOPS), (k, e));
                }
            }
        }
        m
    })
}

/// An effect element: name, attributes and opacity stops.
type Effect = (&'static str, Vec<(&'static str, String)>, Vec<Stop>);

/// One opacity gradient stop, as IDML writes it.
#[derive(Debug, PartialEq)]
struct Stop {
    opacity: f64,
    location: f64,
    /// Relative position of the midpoint between the previous stop and
    /// this one, in percent; `None` for the first stop.
    midpoint: Option<f64>,
}

/// Opacity stops as IDML writes them, from the stops of the attribute
/// list (location, absolute midpoint position, opacity). `None` for no
/// stops, or two stops at the same location (the midpoint is undefined).
fn stops(raw: &[[f64; 3]]) -> Option<Vec<Stop>> {
    if raw.is_empty() || (1..raw.len()).any(|i| raw[i][0] == raw[i - 1][0]) {
        return None;
    }
    Some(
        raw.iter()
            .enumerate()
            .map(|(i, &[location, _, opacity])| {
                let midpoint = (i > 0).then(|| {
                    let [prev, mid, _] = raw[i - 1];
                    (mid - prev) / (location - prev) * 100.0
                });
                Stop {
                    opacity,
                    location: super::round(location * 100.0),
                    midpoint,
                }
            })
            .collect(),
    )
}

/// What the values of a list are compared with.
#[derive(Clone, Copy)]
enum Base<'a> {
    /// A page item's values: those of its object style, if it has one.
    Item(Option<&'a StyleValues>),
    /// An object style's values: those of the root style `[None]`.
    Root(&'a Attrs),
}

/// A value left out because it is outside the schema's range: effect,
/// attribute, value.
type LeftOut = (&'static str, &'static str, f64);

/// The setting elements of `attrs`, with the values that differ from
/// `base` in any digit (all values of an item without a style), and an
/// effect element only when one of its values or its opacity stops
/// differ. `owner` is the `Self` of the item or style, which names the
/// opacity stops.
fn settings(w: &Writer, attrs: &Attrs, base: Base, owner: &str) -> (Vec<Node>, Vec<LeftOut>) {
    let mut left_out = Vec::new();
    let mut out = Vec::new();
    // The effects the list stores an attribute of; only these can have a
    // value to write, apart from the knockout an item with a style does
    // not store (`unstored`).
    let mut touched = [[false; EFFECTS.len()]; SETTINGS.len()];
    for (id, _) in &attrs.values {
        if let Some(&(k, e)) = effect_index().get(id) {
            touched[k][e] = true;
        }
    }
    if let Base::Item(Some(_)) = base {
        for t in &mut touched {
            t[DROP_SHADOW] = true;
        }
    }
    for (k, &setting) in SETTINGS.iter().enumerate() {
        let mut effects: Vec<Effect> = Vec::new();
        for (e, &(effect, list)) in EFFECTS.iter().enumerate() {
            if !touched[k][e] {
                continue;
            }
            let mut values = Vec::new();
            for &(name, slot, kind) in list {
                let id = slot_id(k, slot);
                let (v, base_value, has_base) = match base {
                    Base::Item(style) => {
                        let Some(v) = attrs.get(id).or_else(|| style.and_then(|_| unstored(id)))
                        else {
                            continue;
                        };
                        (
                            Some(v),
                            style.and_then(|s| s.transparency(id).or_else(|| unstored(id))),
                            style.is_some(),
                        )
                    }
                    // Only IDs that both lists hold (object styles,
                    // chunk 0x1B92C).
                    Base::Root(root) => match (attrs.get(id), root.get(id)) {
                        (Some(v), Some(r)) => (Some(v), Some(r), true),
                        _ => continue,
                    },
                };
                let Some(v) = v else {
                    continue;
                };
                if let Kind::Range(lo, hi) = kind
                    && let Some(f) = v.as_f64()
                    && !(lo..=hi).contains(&f)
                {
                    left_out.push((effect, name, f));
                    continue;
                }
                match w.value_text(kind, v) {
                    Some(t) => {
                        let base_text = base_value.and_then(|b| w.value_text(kind, b));
                        let same = base_text.as_ref().is_some_and(|b| *b == t);
                        let unshown = matches!(kind, Kind::Swatch)
                            && has_base
                            && base_text.is_none()
                            && t == "Color/Black";
                        if !same && !unshown {
                            values.push((name, t));
                        }
                    }
                    None if kind.is_code() => attrs.unknown_code(id, v),
                    None => {}
                }
            }
            let stop_list = if effect == "GradientFeatherSetting" {
                let id = slot_id(k, STOPS);
                let base_stops = match base {
                    Base::Item(style) => style.and_then(|s| s.transparency(id)),
                    Base::Root(root) => root.get(id),
                };
                match attrs.get(id) {
                    Some(Value::Stops(raw)) if !matches!(base_stops, Some(Value::Stops(st)) if st == raw) => {
                        stops(raw).unwrap_or_default()
                    }
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            if !values.is_empty() || !stop_list.is_empty() {
                effects.push((effect, values, stop_list));
            }
        }
        if effects.is_empty() {
            continue;
        }
        let children = effects
            .into_iter()
            .map(|(effect, values, stop_list)| Node {
                tag: effect.into(),
                attrs: values.into_iter().map(|(n, v)| (n.into(), v)).collect(),
                text: None,
                children: stop_list
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let mut attrs = vec![
                            (
                                "Self".to_string(),
                                format!("{owner}{setting}1{effect}1OpacityGradientStop{i}"),
                            ),
                            ("Opacity".into(), num(s.opacity)),
                            ("Location".into(), num(s.location)),
                        ];
                        if let Some(m) = s.midpoint {
                            attrs.push(("Midpoint".into(), num(m)));
                        }
                        Node {
                            tag: "OpacityGradientStop".into(),
                            attrs,
                            ..Node::default()
                        }
                    })
                    .collect(),
            })
            .collect();
        out.push(Node {
            tag: setting.into(),
            children,
            ..Node::default()
        });
    }
    (out, left_out)
}

/// Write the transparency settings of a page item: the values that
/// differ from the object style `style` (all values without a style).
/// `item` is the item's `Self`. Returns the values left out because they
/// are outside the schema's range.
pub(super) fn write(
    w: &Writer,
    x: &mut Xml,
    attrs: &Attrs,
    style: Option<&StyleValues>,
    item: &str,
) -> Vec<LeftOut> {
    let (nodes, left_out) = settings(w, attrs, Base::Item(style), item);
    for n in &nodes {
        n.write(x);
    }
    left_out
}

/// The transparency settings of object style `os`: the values that
/// differ from those of the root style `[None]`, whatever the style is
/// based on (`docs/format/transparency.md`, object styles). None for an
/// InDesign 4.0 document, whose lists have another layout.
pub(super) fn style_settings(
    w: &Writer,
    os: &crate::model::ObjectStyle,
    root: &crate::model::ObjectStyle,
) -> (Vec<Node>, Vec<LeftOut>) {
    if w.doc.version.major <= 4 {
        return (Vec::new(), Vec::new());
    }
    settings(
        w,
        &os.transparency,
        Base::Root(&root.transparency),
        &super::uref(Some(os.uid)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_opacity_stops() {
        // Two stops: location 0.15, midpoint at 0.5, opacity 100; location
        // 0.85, midpoint 1, opacity 0.
        let s = stops(&[[0.15, 0.5, 100.0], [0.85, 1.0, 0.0]]).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(
            (s[0].opacity, s[0].location, s[0].midpoint),
            (100.0, 15.0, None)
        );
        assert_eq!(s[1].location, 85.0);
        assert!((s[1].midpoint.unwrap() - 50.0).abs() < 1e-9);
        assert_eq!(stops(&[]), None);
    }

    #[test]
    fn stops_at_one_location_are_not_read() {
        assert_eq!(stops(&[[0.5, 0.5, 100.0], [0.5, 1.0, 0.0]]), None);
    }

    #[test]
    fn style_values_are_compared_with_the_root() {
        let mut doc = crate::model::Document::default();
        doc.swatches.insert(0xB, "Color/Black".into());
        doc.swatches.insert(0x20, "Color/u20".into());
        let w = Writer::for_test(&doc);
        let list = |values: Vec<(u32, Value)>| {
            let mut a = Attrs::default();
            a.values = values;
            a
        };
        let root = list(vec![
            (0x10816, Value::Double(100.0)),
            (0x10837, Value::Ref(0)),
            (0x108D8, Value::Double(100.0)),
            (0x1EB8C, Value::Stops(Vec::new())),
        ]);
        let style = |color: u32| {
            list(vec![
                (0x10816, Value::Double(30.0)),
                (0x10837, Value::Ref(color)),
                (0x108D8, Value::Double(100.0)),
                // Not in the root's list.
                (0x10817, Value::Int(1)),
                (
                    0x1EB8C,
                    Value::Stops(vec![[0.0, 0.5, 100.0], [1.0, 1.0, 0.0]]),
                ),
            ])
        };
        let (nodes, _) = settings(&w, &style(0xB), Base::Root(&root), "u2a");
        assert_eq!(nodes.len(), 1);
        let t = &nodes[0];
        assert_eq!(t.tag, "TransparencySetting");
        assert_eq!(
            t.child("BlendingSetting").unwrap().attrs,
            [("Opacity".to_string(), "30".to_string())]
        );
        // Black is left out where the root stores no swatch.
        assert!(t.child("DropShadowSetting").is_none());
        let g = t.child("GradientFeatherSetting").unwrap();
        assert_eq!(
            g.children[1].attr("Self"),
            Some("u2aTransparencySetting1GradientFeatherSetting1OpacityGradientStop1")
        );
        let (nodes, _) = settings(&w, &style(0x20), Base::Root(&root), "u2a");
        assert_eq!(
            nodes[0]
                .child("DropShadowSetting")
                .unwrap()
                .attr("EffectColor"),
            Some("Color/u20")
        );
    }

    #[test]
    fn block_positions_name_the_census_ids() {
        // The IDs the census items stored for each setting
        // (FOLIO_FORK.txt, revision folio.4): blending opacity, drop
        // shadow colour and global light, feather noise, inner shadow
        // applied, bevel shadow colour, satin invert.
        let ids = |pos| (0..4).map(|s| block_id(s, pos)).collect::<Vec<_>>();
        assert_eq!(ids(0), [0x10816, 0x10891, 0x108D8, 0x1EB28]);
        assert_eq!(ids(12), [0x10837, 0x1089D, 0x108E4, 0x1EB34]);
        assert_eq!(ids(13), [0x10890, 0x1089E, 0x108E5, 0x1EB35]);
        assert_eq!(ids(17), [0x1084C, 0x108A4, 0x108EB, 0x1EB3B]);
        assert_eq!(ids(18), [0x1084D, 0x108A5, 0x108EC, 0x1EB3C]);
        assert_eq!(ids(58), [0x10875, 0x108CD, 0x1EB14, 0x1EB64]);
        assert_eq!(ids(68), [0x1087F, 0x108D7, 0x1EB1E, 0x1EB6E]);
        let extra = |slot| (0..4).map(|s| slot_id(s, slot)).collect::<Vec<_>>();
        assert_eq!(extra(X(0x1EB74, 5)), [0x1EB74, 0x1EB79, 0x1EB7E, 0x1EB83]);
        assert_eq!(extra(X(0x1EBB7, 4)), [0x1EBB7, 0x1EBBB, 0x1EBBF, 0x1EBC3]);
        assert_eq!(extra(STOPS), [0x1EB8C, 0x1EB95, 0x1EB9E, 0x1EBA7]);
        assert_eq!(EFFECTS[DROP_SHADOW].0, "DropShadowSetting");
        assert_eq!(effect_index().len(), 4 * 89);
        // Every attribute has its own ID within a setting.
        for s in 0..4 {
            let mut all: Vec<u32> = EFFECTS
                .iter()
                .flat_map(|(_, list)| list.iter().map(move |a| slot_id(s, a.1)))
                .collect();
            let n = all.len();
            all.sort_unstable();
            all.dedup();
            assert_eq!(all.len(), n);
        }
    }

    #[test]
    fn writes_every_setting_and_effect() {
        let mut doc = crate::model::Document::default();
        doc.swatches.insert(0xE4, "Color/CensusA".into());
        let w = Writer::for_test(&doc);
        let mut a = Attrs::default();
        // Content bevel (applied, emboss, chisel hard, highlight colour),
        // stroke directional feather and fill feather corner.
        a.values = vec![
            (0x1EB57, Value::Enum(1)),
            (0x1EB58, Value::Int(2)),
            (0x1EB59, Value::Int(1)),
            (0x1EB61, Value::Ref(0xE4)),
            (0x1EB79, Value::Enum(1)),
            (0x1EBBB, Value::Double(3.5)),
            (0x1EB7C, Value::Int(2)),
            (0x108E6, Value::Int(3)),
            (0x108EA, Value::Int(1)),
        ];
        let (nodes, _) = settings(&w, &a, Base::Item(None), "u2a");
        let tags: Vec<_> = nodes.iter().map(|n| n.tag.as_str()).collect();
        assert_eq!(
            tags,
            [
                "StrokeTransparencySetting",
                "FillTransparencySetting",
                "ContentTransparencySetting"
            ]
        );
        let d = nodes[0].child("DirectionalFeatherSetting").unwrap();
        assert_eq!(d.attr("LeftWidth"), Some("3.5"));
        assert_eq!(d.attr("FollowShapeMode"), Some("AllEdges"));
        let f = nodes[1].child("FeatherSetting").unwrap();
        assert_eq!(
            (f.attr("Mode"), f.attr("CornerType")),
            (Some("Standard"), Some("Rounded"))
        );
        let b = nodes[2].child("BevelAndEmbossSetting").unwrap();
        assert_eq!(b.attr("Style"), Some("Emboss"));
        assert_eq!(b.attr("Technique"), Some("ChiselHard"));
        assert_eq!(b.attr("HighlightColor"), Some("Color/CensusA"));
        // The colour of an effect that is not applied is written too.
        a.values.remove(0);
        let (nodes, _) = settings(&w, &a, Base::Item(None), "u2a");
        let b = nodes[2].child("BevelAndEmbossSetting").unwrap();
        assert_eq!(b.attr("HighlightColor"), Some("Color/CensusA"));
    }
}
