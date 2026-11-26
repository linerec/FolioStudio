use super::*;
use color_commands::{EditGradient, NewGradient, Stop};
const GRAPHIC: &str = "Resources/Graphic.xml";

fn validate(w: &Work<'_>, kind: Option<&str>, stops: Option<&[Stop]>) -> Result<(), EditError> {
    if kind.is_some_and(|k| !["Linear", "Radial"].contains(&k)) {
        return Err(EditError::new("invalid", "gradient type"));
    }
    if let Some(stops) = stops {
        if !(2..=256).contains(&stops.len()) { return Err(EditError::new("invalid", "gradient needs 2–256 stops")); }
        let (t, ix) = swatches::scan(w, GRAPHIC)?;
        let mut previous = -1.0;
        for s in stops {
            if !s.location.is_finite() || !(0.0..=100.0).contains(&s.location) || s.location < previous
                || !s.midpoint.is_finite() || !(13.0..=87.0).contains(&s.midpoint) {
                return Err(EditError::new("invalid", "gradient stop range or order"));
            }
            previous = s.location;
            let i = ix.by_self(&t, &s.color).ok_or_else(|| EditError::new("notFound", "gradient stop colour"))?;
            if !["Color", "Tint"].contains(&ix.local(&t, i)) { return Err(EditError::new("invalid", "gradient stop must be a colour")); }
        }
    }
    Ok(())
}

fn stop_xml(id: &str, s: &Stop, first: bool) -> String {
    let midpoint = if first { String::new() } else { format!(" Midpoint=\"{}\"", geom::num(s.midpoint)) };
    format!("<GradientStop Self=\"{}\" StopColor=\"{}\" Location=\"{}\"{midpoint} />", escape(id), escape(&s.color), geom::num(s.location))
}

fn name_available(w: &Work<'_>, name: &str) -> Result<(), EditError> {
    let (t, ix) = swatches::scan(w, GRAPHIC)?;
    if name.is_empty() || (0..ix.nodes.len()).any(|i| ix.value(&t, i, "Name").as_deref() == Some(name)) {
        return Err(EditError::new("invalid", "swatch name is empty or already used"));
    }
    Ok(())
}

pub(super) fn create(w: &mut Work<'_>, p: &NewGradient) -> Result<(), EditError> {
    validate(w, Some(&p.kind), Some(&p.stops))?;
    name_available(w, &p.name)?;
    let id = swatches::self_of("Gradient", &p.name);
    let slot = swatches::add_slot(w, &id)?;
    let stem = w.new_id();
    let stops = p.stops.iter().enumerate().map(|(i, s)| format!("\n\t\t{}", stop_xml(&format!("{stem}GradientStop{i:x}"), s, i == 0))).collect::<String>();
    let xml = format!("<Gradient Self=\"{}\" Type=\"{}\" Name=\"{}\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"true\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"{}\">{stops}\n\t</Gradient>", escape(&id), p.kind, escape(&p.name), escape(&slot));
    swatches::insert(w, "Gradient", &id, &xml, "StrokeStyle")
}

pub(super) fn edit(w: &mut Work<'_>, p: &EditGradient) -> Result<(), EditError> {
    validate(w, p.kind.as_deref(), p.stops.as_deref())?;
    let (t, ix) = swatches::scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, &p.id).ok_or_else(|| EditError::new("notFound", "gradient"))?;
    if ix.local(&t, i) != "Gradient" || ix.attr(&t, i, "ColorEditable") == Some("false") {
        return Err(EditError::new("locked", "gradient cannot be edited"));
    }
    let mut edits = Vec::new();
    if let Some(kind) = &p.kind { edits.push(attr_edit(&ix, &t, i, "Type", kind)); }
    if let Some(stops) = &p.stops {
        let old = ix.named(&t, i, "GradientStop");
        let stem = old.first().and_then(|&k| ix.attr(&t, k, "Self")).and_then(|id| id.split_once("GradientStop")).map(|(s, _)| s.to_owned()).unwrap_or_else(|| w.new_id());
        for (n, &k) in old.iter().enumerate() {
            if let Some(s) = stops.get(n) {
                edits.push(attr_edit(&ix, &t, k, "StopColor", &s.color));
                edits.push(attr_edit(&ix, &t, k, "Location", &geom::num(s.location)));
                if n > 0 { edits.push(attr_edit(&ix, &t, k, "Midpoint", &geom::num(s.midpoint))); }
            } else { edits.push((element_span(&ix, &t, k), String::new())); }
        }
        if stops.len() > old.len() {
            let at = old.last().map(|&k| ix.nodes[k].end).unwrap_or(ix.nodes[i].open_end);
            let added = stops.iter().enumerate().skip(old.len()).map(|(n, s)| format!("\n\t\t{}", stop_xml(&format!("{stem}GradientStop{n:x}"), s, n == 0))).collect();
            edits.push((at..at, added));
        }
    }
    w.splice(GRAPHIC, edits)?;
    if let Some(name) = &p.rename && ix.value(&t, i, "Name").as_deref() != Some(name) {
        rename(w, &p.id, name)?;
    }
    Ok(())
}

fn rename(w: &mut Work<'_>, id: &str, name: &str) -> Result<(), EditError> {
    name_available(w, name)?;
    let new = swatches::self_of("Gradient", name);
    set_attrs(w, id, &[("Name", name.to_owned())])?;
    let (t, ix) = swatches::scan(w, GRAPHIC)?;
    let i = ix.by_self(&t, id).unwrap();
    let xml = t[ix.nodes[i].start..ix.nodes[i].end].replacen(&format!("Self=\"{}\"", escape(id)), &format!("Self=\"{}\"", escape(&new)), 1);
    w.splice(GRAPHIC, vec![(element_span(&ix, &t, i), String::new())])?;
    swatches::insert(w, "Gradient", &new, &xml, "StrokeStyle")?;
    swatches::rereference(w, id, &new)?;
    let (t, ix) = swatches::scan(w, "designmap.xml")?;
    let edits = swatches::slots(&ix, &t)?.iter().filter(|(_, _, r)| r == id).map(|(k, _, _)| attr_edit(&ix, &t, *k, "SwatchItemRef", &new)).collect();
    w.splice("designmap.xml", edits)
}

pub(super) fn geometry(w: &mut Work<'_>, p: &color_commands::SetGradientGeometry) -> Result<(), EditError> {
    if p.angle.is_some_and(|v| !v.is_finite()) || p.length.is_some_and(|v| !v.is_finite() || v < 0.0)
        || p.start.is_some_and(|v| v.iter().any(|x| !x.is_finite())) {
        return Err(EditError::new("invalid", "gradient geometry"));
    }
    let (part, ix, i) = w.find(&p.id)?;
    let t = w.text(&part)?.to_owned();
    check_item(&ix, &t, i, &p.id)?;
    let prefix = if p.stroke.unwrap_or(false) { "GradientStroke" } else { "GradientFill" };
    let mut edits = Vec::new();
    if let Some(v) = p.angle { edits.push(attr_edit(&ix, &t, i, &format!("{prefix}Angle"), &geom::num(v))); }
    if let Some(v) = p.length { edits.push(attr_edit(&ix, &t, i, &format!("{prefix}Length"), &geom::num(v))); }
    if let Some(v) = p.start { edits.push(attr_edit(&ix, &t, i, &format!("{prefix}Start"), &format!("{} {}", geom::num(v[0]), geom::num(v[1])))); }
    w.splice(&part, edits)
}

/// Gradient Swatch tool on a fill or stroke that is no gradient (InDesign 2025, dragged with a real mouse,
/// artifacts/gradient-view-20261025): even a red or an empty fill takes InDesign's default gradient, an unnamed
/// hidden linear one, white (an unnamed CMYK 0 0 0 0) at 0 to Black at 100 (midpoint 50), and the dragged
/// line becomes its start, angle and length.
pub(super) fn apply_default(w: &mut Work<'_>, p: &color_commands::ApplyDefaultGradient) -> Result<(), EditError> {
    let me = ensure_default(w)?;
    let stroke = p.stroke == Some(true);
    set_paint(w, &p.id, (!stroke).then_some(me), None, stroke.then_some(me), None, None)?;
    geometry(w, &color_commands::SetGradientGeometry { id: p.id.clone(), stroke: Some(stroke), angle: Some(p.angle), length: Some(p.length), start: Some(p.start) })
}

/// The default gradient swatch (an unnamed hidden linear one, white to Black) in the document, made when it is not there: the
/// Apply Gradient button of the toolbox gives it to text (a text range's fill or stroke is then that gradient, placed by the
/// text's own `gradientFill…` attributes: automatic unless set) when the document has shown no gradient. Idempotent.
pub(super) fn ensure_default_command(w: &mut Work<'_>, _: &color_commands::EnsureDefaultGradient) -> Result<(), EditError> {
    ensure_default(w).map(|_| ())
}

/// The id of the default gradient (`ensureDefaultGradient`).
const DEFAULT_GRADIENT: &str = "Gradient/u7e1b3c01";

fn ensure_default(w: &mut Work<'_>) -> Result<&'static str, EditError> {
    let white = "Color/u2cd4fe4d";
    let me = DEFAULT_GRADIENT;
    let (t, ix) = swatches::scan(w, GRAPHIC)?;
    if ix.by_self(&t, "Color/Black").is_none() { return Err(EditError::new("notFound", "Color/Black")); }
    if ix.by_self(&t, white).is_none() {
        let element = format!("<Color Self=\"{white}\" Model=\"Process\" Space=\"CMYK\" ColorValue=\"0 0 0 0\" ColorOverride=\"Normal\" ConvertToHsb=\"false\" AlternateSpace=\"NoAlternateColor\" AlternateColorValue=\"\" Name=\"$ID/\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"false\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"n\" />");
        swatches::insert(w, "Color", white, &element, "Ink")?;
    }
    let (t, ix) = swatches::scan(w, GRAPHIC)?;
    if ix.by_self(&t, me).is_none() {
        let element = format!("<Gradient Self=\"{me}\" Type=\"Linear\" Name=\"$ID/\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"false\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"n\">
		<GradientStop Self=\"u7e1b3c01GradientStop0\" StopColor=\"{white}\" Location=\"0\" />
		<GradientStop Self=\"u7e1b3c01GradientStop1\" StopColor=\"Color/Black\" Location=\"100\" Midpoint=\"50\" />
	</Gradient>");
        swatches::insert(w, "Gradient", me, &element, "StrokeStyle")?;
    }
    Ok(me)
}
