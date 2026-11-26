//! Settings of pages and spreads in the Pages panel menu, measured in InDesign 2025 (next/oracle/indesign/probe-page-settings.jsx,
//! probe-flattener-range.jsx; oracle pages-settings):
//! - Color Label: `<PageColor type="enumeration">` in the page's `Properties`, `UseMasterColor` (the parent's colour shows), `Nothing`
//!   or an interface colour (Pink, LightBlue, Violet…); a parent's own colour is written on its spread and on each of its pages;
//! - Show Parent Items: the spread's `ShowMasterItems`;
//! - Spread Flattening: the spread's `FlattenerOverride` (Default, None, Custom) and its `FlattenerPreference`, whose values are
//!   kept whatever the level (raster/vector balance 0–100 in `Properties/RasterVectorBalance`, line art and text 1–9600 and gradient
//!   and mesh 1–1200 resolution — values outside are cut to the range, a balance outside is refused).
use super::*;

/// The edit that makes `PageColor` of element `i` read `color`.
fn color_edit(ix: &Index, t: &str, i: usize, color: &str) -> Result<(std::ops::Range<usize>, String), EditError> {
    let n = &ix.nodes[i];
    if n.empty {
        return Err(EditError::new("xml", "an empty page element"));
    }
    let element = |indent: &str| format!("<PageColor type=\"enumeration\">{color}</PageColor>{indent}");
    if let Some(&props) = ix.named(t, i, "Properties").first() {
        if let Some(&c) = ix.named(t, props, "PageColor").first() {
            let pc = &ix.nodes[c];
            // a self-closing PageColor is written out
            return Ok(if pc.empty { (pc.start..pc.end, element("")) } else { (pc.open_end..pc.close_start, color.to_owned()) });
        }
        let pn = &ix.nodes[props];
        let indent = indent_before(t, pn.start).to_owned();
        // a self-closing Properties is written out
        if pn.empty {
            return Ok((pn.start..pn.end, format!("<Properties>\n{indent}\t{}\n{indent}</Properties>", element(""))));
        }
        let close = pn.close_start;
        let indent = indent_before(t, close);
        // after the indent already in front of the closing tag: the new element in one more level, then the closing tag's indent again
        return Ok((close..close, format!("\t{}\n{indent}", element(""))));
    }
    let indent = indent_before(t, n.start).to_owned();
    Ok((n.open_end..n.open_end, format!("\n{indent}\t<Properties>\n{indent}\t\t{}\n{indent}\t</Properties>", element(""))))
}

fn check_color(color: &str) -> Result<(), EditError> {
    if color == "UseMasterColor" || color == "Nothing" || folio_model::UI_COLORS.contains(&color) {
        Ok(())
    } else {
        Err(EditError::new("invalid", format!("{color} is not a colour label")))
    }
}

pub(super) fn set_page_color(w: &mut Work<'_>, pages: &[String], masters: &[String], color: &str) -> Result<(), EditError> {
    check_color(color)?;
    for id in pages {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        if ix.local(&t, i) != "Page" {
            return Err(EditError::new("invalid", format!("{id} is not a page")));
        }
        w.splice(&part, vec![color_edit(&ix, &t, i, color)?])?;
    }
    for id in masters {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        if ix.local(&t, i) != "MasterSpread" {
            return Err(EditError::new("invalid", format!("{id} is not a parent")));
        }
        let mut edits = vec![color_edit(&ix, &t, i, color)?];
        for p in ix.named(&t, i, "Page") {
            edits.push(color_edit(&ix, &t, p, color)?);
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}

fn spread_of(w: &mut Work<'_>, id: &str) -> Result<(String, String, Index, usize), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, i) != "Spread" || !part.starts_with("Spreads/") {
        return Err(EditError::new("invalid", format!("{id} is not a spread of the document")));
    }
    Ok((part, t, ix, i))
}

pub(super) fn set_show_master(w: &mut Work<'_>, spreads: &[String], show: bool) -> Result<(), EditError> {
    for id in spreads {
        let (part, t, ix, i) = spread_of(w, id)?;
        w.splice(&part, vec![attr_edit(&ix, &t, i, "ShowMasterItems", &bool_text(show))])?;
    }
    Ok(())
}

/// The custom flattener values a command gives (none: left as they are).
#[derive(Clone, Copy, Default)]
pub(super) struct Custom {
    pub balance: Option<f64>,
    pub line_art: Option<f64>,
    pub gradient: Option<f64>,
    pub clip: Option<bool>,
    pub strokes: Option<bool>,
    pub text: Option<bool>,
}

pub(super) fn set_flattener(w: &mut Work<'_>, spreads: &[String], level: Option<&str>, custom: Custom) -> Result<(), EditError> {
    if let Some(l) = level
        && !["Default", "None", "Custom"].contains(&l)
    {
        return Err(EditError::new("invalid", format!("{l} is not a flattener level")));
    }
    if custom.balance.is_some_and(|b| !(0.0..=100.0).contains(&b)) {
        return Err(EditError::new("invalid", "the raster/vector balance is 0 to 100"));
    }
    for id in spreads {
        let (part, t, ix, i) = spread_of(w, id)?;
        let mut edits = Vec::new();
        if let Some(l) = level {
            edits.push(attr_edit(&ix, &t, i, "FlattenerOverride", l));
        }
        let wanted = custom.balance.is_some() || custom.line_art.is_some() || custom.gradient.is_some() || custom.clip.is_some() || custom.strokes.is_some() || custom.text.is_some();
        if wanted {
            let Some(&f) = ix.named(&t, i, "FlattenerPreference").first() else {
                return Err(EditError::new("xml", "a spread without FlattenerPreference"));
            };
            let mut attr = |name: &str, v: Option<String>| {
                if let Some(v) = v {
                    edits.push(attr_edit(&ix, &t, f, name, &v));
                }
            };
            attr("LineArtAndTextResolution", custom.line_art.map(|v| geom::num(v.clamp(1.0, 9600.0))));
            attr("GradientAndMeshResolution", custom.gradient.map(|v| geom::num(v.clamp(1.0, 1200.0))));
            attr("ClipComplexRegions", custom.clip.map(bool_text));
            attr("ConvertAllStrokesToOutlines", custom.strokes.map(bool_text));
            attr("ConvertAllTextToOutlines", custom.text.map(bool_text));
            if let Some(b) = custom.balance {
                let props = ix.named(&t, f, "Properties").first().copied().ok_or_else(|| EditError::new("xml", "FlattenerPreference without Properties"))?;
                let r = ix.named(&t, props, "RasterVectorBalance").first().copied().ok_or_else(|| EditError::new("xml", "FlattenerPreference without RasterVectorBalance"))?;
                edits.push((ix.nodes[r].open_end..ix.nodes[r].close_start, geom::num(b)));
            }
        }
        w.splice(&part, edits)?;
    }
    Ok(())
}
