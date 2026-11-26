//! Merge page item defaults with the applied style and write preference elements in IDML order.
use super::placement::Placement;
use super::{EXPORT, EditError, INCOPY, Index, ItemKind, Values, find_tag, geom, raw, resolved};

pub(super) fn item_attributes<'a>(
    kind: ItemKind,
    defaults: &'a Values,
    styles: &Index,
    style_text: &str,
    master: bool,
    placement: &Placement,
) -> Result<(&'a str, Values), EditError> {
    let style = if kind.placeholder() {
        "ObjectStyle/$ID/[None]"
    } else {
        defaults
            .get(if kind == ItemKind::TextFrame {
                "AppliedTextObjectStyle"
            } else {
                "AppliedGraphicObjectStyle"
            })
            .map(String::as_str)
            .ok_or_else(|| EditError::new("xml", "missing default object style"))?
    };
    let (style_attrs, _) = resolved(styles, style_text, style)?;
    let mut a = Values::new();
    for (k, v) in [
        (
            "ContentType",
            if kind.placeholder() {
                "GraphicType"
            } else if kind == ItemKind::TextFrame {
                "TextType"
            } else {
                "Unassigned"
            },
        ),
        ("OverriddenPageItemProps", ""),
        (
            "HorizontalLayoutConstraints",
            "FlexibleDimension FixedDimension FlexibleDimension",
        ),
        (
            "VerticalLayoutConstraints",
            "FlexibleDimension FixedDimension FlexibleDimension",
        ),
        ("Locked", "false"),
        ("LocalDisplaySetting", "Default"),
        ("Visible", "true"),
        ("Name", "$ID/"),
        ("GradientFillStart", "0 0"),
        ("GradientFillLength", "0"),
        ("GradientFillAngle", "0"),
        ("GradientStrokeStart", "0 0"),
        ("GradientStrokeLength", "0"),
        ("GradientStrokeAngle", "0"),
        ("GradientFillHiliteLength", "0"),
        ("GradientFillHiliteAngle", "0"),
        ("GradientStrokeHiliteLength", "0"),
        ("GradientStrokeHiliteAngle", "0"),
    ] {
        a.insert(k.into(), v.into());
    }
    // InDesign writes AllowOverrides on every item drawn in a parent (a page item of a document does not carry it)
    if master {
        a.insert("AllowOverrides".into(), "true".into());
    }
    if kind == ItemKind::Line {
        a.insert("LockState".into(), "None".into());
    } else if kind != ItemKind::TextFrame {
        a.insert("StoryTitle".into(), "$ID/".into());
    }
    if !kind.placeholder() && kind != ItemKind::TextFrame {
        for (k, v) in defaults {
            if k.starts_with("Applied") || k == "Self" {
                continue;
            }
            if k.starts_with("Overprint") && v == "false" && !style_attrs.contains_key(k) {
                continue;
            }
            if style_attrs.get(k) != Some(v) || a.contains_key(k) {
                a.insert(k.clone(), v.clone());
            }
        }
    }
    // Explicit [None] resets the gradient handles too (create-*Frame); a frame the mouse drew keeps them at zero (liveinput-c2).
    if kind.placeholder() && !placement.upright {
        for paint in ["Fill", "Stroke"] {
            a.insert(
                format!("Gradient{paint}Start"),
                geom::pair((placement.local_rect[0], placement.local_rect[3])),
            );
            a.insert(
                format!("Gradient{paint}Length"),
                geom::num(placement.local_rect[2] - placement.local_rect[0]),
            );
        }
    }
    Ok((style, a))
}

pub(super) fn append_options(
    extra: &mut String,
    kind: ItemKind,
    px: &Index,
    prefs: &str,
) -> Result<(), EditError> {
    let wi = find_tag(px, prefs, "TextWrapPreference")?;
    let mut wrap = raw(px, prefs, wi);
    if let Some(&ci) = px.named(prefs, wi, "ContourOption").first() {
        let n = &px.nodes[ci];
        wrap.replace_range(n.start - px.nodes[wi].start..n.end - px.nodes[wi].start, "");
    }
    extra.push_str(&wrap);
    if kind != ItemKind::TextFrame && kind != ItemKind::Line {
        extra.push_str(INCOPY);
        if !kind.placeholder() {
            extra.push_str(&raw(px, prefs, find_tag(px, prefs, "FrameFittingOption")?));
        }
    }
    extra.push_str(EXPORT);
    Ok(())
}
