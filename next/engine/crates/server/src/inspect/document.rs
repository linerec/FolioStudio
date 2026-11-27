//! Build the documentInfo reply for document settings, pages and panel lists.

use super::geometry::{page_rects, side};
use super::r3;
use super::swatches::swatch_json;
use folio_color::Colors;
use folio_model::Document;
use serde_json::{Value, json};
use std::collections::HashMap;

/// The style's name as the panels show it: InDesign's own styles by their interface names,
/// others without `$ID/` and without the group path InDesign writes before them.
fn style_name(raw: &str, group: &[String]) -> String {
    let builtin = match raw {
        "$ID/[No paragraph style]" => Some("[No Paragraph Style]"),
        "$ID/NormalParagraphStyle" => Some("[Basic Paragraph]"),
        "$ID/[No character style]" | "$ID/[None]" => Some("[None]"),
        "$ID/[Normal Graphics Frame]" => Some("[Basic Graphics Frame]"),
        "$ID/[Normal Text Frame]" => Some("[Basic Text Frame]"),
        "$ID/[Normal Grid]" => Some("[Basic Grid]"),
        _ => None,
    };
    if let Some(b) = builtin {
        return b.to_owned();
    }
    let name = raw.strip_prefix("$ID/").unwrap_or(raw);
    if !group.is_empty() {
        let prefix = group.iter().map(|g| g.strip_prefix("$ID/").unwrap_or(g)).collect::<Vec<_>>().join(":") + ":";
        if let Some(rest) = name.strip_prefix(&prefix)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    name.to_owned()
}

/// Styles with InDesign's own root styles first (in the panels' order), the rest in
/// document order.
fn styles_json(entries: &[folio_model::StyleEntry], first: &[&str]) -> Value {
    let mut list: Vec<&folio_model::StyleEntry> = entries.iter().collect();
    list.sort_by_key(|e| if e.group.is_empty() { first.iter().position(|f| *f == e.name).unwrap_or(usize::MAX) } else { usize::MAX });
    Value::Array(
        list.into_iter()
            .map(|e| {
                let group: Vec<&str> = e.group.iter().map(|g| g.strip_prefix("$ID/").unwrap_or(g)).collect();
                json!({"id": e.id, "name": style_name(&e.name, &e.group), "group": group})
            })
            .collect(),
    )
}

/// Cell and table styles: with the style each is based on and its settings (the style dialogs edit them).
fn table_styles_json(entries: &[folio_model::StyleEntry], first: &[&str]) -> Value {
    let mut list: Vec<&folio_model::StyleEntry> = entries.iter().collect();
    list.sort_by_key(|e| if e.group.is_empty() { first.iter().position(|f| *f == e.name).unwrap_or(usize::MAX) } else { usize::MAX });
    Value::Array(list.into_iter().map(|e| {
        let group: Vec<&str> = e.group.iter().map(|g| g.strip_prefix("$ID/").unwrap_or(g)).collect();
        json!({"id": e.id, "name": style_name(&e.name, &e.group), "group": group, "basedOn": e.based_on, "settings": e.settings})
    }).collect())
}

/// The cell or table style groups, each with its `Self` and the group names from the top (`CellStyleGroup/$ID/Grp%3aSub`: Grp, Sub).
fn style_groups_json(ids: &[String]) -> Value {
    Value::Array(ids.iter().map(|id| {
        let rest = id.split_once('/').map_or(id.as_str(), |(_, r)| r);
        let path: Vec<&str> = rest.strip_prefix("$ID/").unwrap_or(rest).split("%3a").collect();
        json!({"id": id, "path": path})
    }).collect())
}

/// The object styles as the panel lists them, with `used`: applied to an item of a page or of a parent page (Select All Unused).
fn object_styles_json(doc: &Document, entries: &[folio_model::StyleEntry]) -> Value {
    fn collect<'a>(items: &'a [folio_model::PageItem], used: &mut std::collections::HashSet<&'a str>) {
        for item in items {
            if let Some(s) = item.object_style.as_deref() {
                used.insert(s);
            }
            if let folio_model::ItemKind::Group { children } = &item.kind {
                collect(children, used);
            }
        }
    }
    let mut used = std::collections::HashSet::new();
    for spread in doc.spreads.iter().chain(&doc.master_spreads) {
        collect(&spread.items, &mut used);
    }
    let Value::Array(mut list) = styles_json(entries, &["$ID/[None]", "$ID/[Normal Graphics Frame]", "$ID/[Normal Text Frame]", "$ID/[Normal Grid]"]) else { return Value::Null };
    for row in &mut list {
        let is_used = row["id"].as_str().is_some_and(|id| used.contains(id));
        row["used"] = json!(is_used);
    }
    Value::Array(list)
}

pub(super) fn document_info(doc: &Document, colors: &Colors, facing: bool) -> Value {
    let mut layer_items: HashMap<&str, usize> = HashMap::new();
    for item in doc.spreads.iter().flat_map(|s| &s.items) {
        if let Some(l) = &item.layer {
            *layer_items.entry(l.as_str()).or_default() += 1;
        }
    }
    let mut layers: Vec<&folio_model::Layer> = doc.layers.iter().collect();
    layers.sort_by_key(|l| std::cmp::Reverse(l.order));
    let layers: Vec<Value> = layers
        .into_iter()
        .map(|l| {
            json!({"id": l.id, "name": l.name, "visible": l.visible, "printable": l.printable, "locked": l.locked,
                   "color": l.color.rgb(), "colorName": match &l.color { folio_model::UiColor::Named(n) => Some(n.clone()), _ => None },
                   "showGuides": l.show_guides, "lockGuides": l.lock_guides,
                   "items": layer_items.get(l.id.as_str()).copied().unwrap_or(0)})
        })
        .collect();

    let order: Vec<&str> = if doc.swatch_order.is_empty() {
        // a document built without the reader's order: the specials first, then by id
        const FIRST: [&str; 4] = ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black"];
        let rest = doc.swatches.keys().chain(doc.gradients.keys()).map(String::as_str).filter(|id| !FIRST.contains(id));
        FIRST.iter().copied().filter(|f| doc.swatches.contains_key(*f)).chain(rest).collect()
    } else {
        doc.swatch_order.iter().map(String::as_str).collect()
    };
    let swatches: Vec<Value> = order.into_iter().filter_map(|id| swatch_json(doc, colors, id)).collect();

    let masters: Vec<Value> = doc
        .master_spreads
        .iter()
        .map(|m| {
            let name = m.name.clone().unwrap_or_else(|| m.id.clone());
            let split = name.split_once('-');
            let prefix = m.name_prefix.clone().or_else(|| split.map(|(p, _)| p.to_owned())).unwrap_or_default();
            let base = m.base_name.clone().or_else(|| split.map(|(_, b)| b.to_owned())).unwrap_or_else(|| name.clone());
            // Based On: the parent its pages take
            let based_on = m.pages.first().and_then(|p| p.master.clone());
            json!({"id": m.id, "name": name, "prefix": prefix, "baseName": base, "pages": m.pages.len(), "basedOn": based_on})
        })
        .collect();

    let mut pages = Vec::new();
    for (si, s) in doc.spreads.iter().enumerate() {
        let rects = page_rects(s);
        let ox = rects.iter().map(|r| r.left).fold(f64::INFINITY, f64::min);
        let oy = rects.iter().map(|r| r.top).fold(f64::INFINITY, f64::min);
        for (p, r) in s.pages.iter().zip(&rects) {
            pages.push(json!({"id": p.id, "index": p.index, "master": p.master, "spreadIndex": si,
                              "spreadOffset": [r3(r.left - ox), r3(r.top - oy)],
                              "spreadOrigin": [r.left, r.top], "side": side(facing, s, p)}));
        }
    }
    pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(0));
    // the pages of the master spreads (opened like a document in the editor: Pages panel, double click a master)
    let mut master_pages = Vec::new();
    for (mi, m) in doc.master_spreads.iter().enumerate() {
        let rects = page_rects(m);
        let ox = rects.iter().map(|r| r.left).fold(f64::INFINITY, f64::min);
        let oy = rects.iter().map(|r| r.top).fold(f64::INFINITY, f64::min);
        for (p, r) in m.pages.iter().zip(&rects) {
            master_pages.push(json!({"id": p.id, "index": p.index, "name": p.name, "master": p.master, "masterSpread": m.id, "spreadIndex": doc.spreads.len() + mi,
                                     "size": [r3(r.width()), r3(r.height())], "spreadOffset": [r3(r.left - ox), r3(r.top - oy)],
                                     "spreadOrigin": [r.left, r.top], "side": side(facing, m, p)}));
        }
    }

    let st = &doc.styles;
    json!({
        "units": {"horizontal": doc.view.horizontal_units, "vertical": doc.view.vertical_units},
        "rulerOrigin": doc.view.ruler_origin,
        "zeroPoint": doc.view.zero_point,
        // View ▸ Grids & Guides: the toggles and what the grids look like
        "view": {
            "strokeUnits": doc.view.stroke_units, "cursorKeyIncrement": doc.view.cursor_key_increment, "leadingKeyIncrement": doc.view.leading_key_increment,
            "baselineShiftKeyIncrement": doc.view.baseline_shift_key_increment, "kerningKeyIncrement": doc.view.kerning_key_increment, "snapZone": doc.view.snap_zone,
            "rulerGuides": {"threshold": doc.view.ruler_guides_threshold, "color": doc.view.ruler_guides_color.rgb(), "colorName": doc.view.ruler_guides_color.name()},
            "pasteboard": {"horizontal": doc.view.pasteboard_margins[0], "vertical": doc.view.pasteboard_margins[1], "matchPreviewBackground": doc.view.match_preview_background,
                           "previewColor": doc.view.preview_background_color.rgb(), "previewColorName": doc.view.preview_background_color.name(),
                           "bleedColor": doc.view.bleed_guide_color.rgb(), "bleedColorName": doc.view.bleed_guide_color.name(),
                           "slugColor": doc.view.slug_guide_color.rgb(), "slugColorName": doc.view.slug_guide_color.name()},
            "guidesLocked": doc.view.guides_locked, "guidesSnap": doc.view.guides_snap, "guidesInBack": doc.view.guides_in_back,
            "columnGuidesLocked": doc.view.column_guides_locked, "gridsInBack": doc.view.grid.grids_in_back,
            "baselineGridShown": doc.view.grid.baseline_shown, "documentGridShown": doc.view.grid.document_shown, "documentGridSnap": doc.view.grid.document_snap,
            "baseline": {"start": doc.view.grid.baseline_start, "division": doc.view.grid.baseline_division, "threshold": doc.view.grid.baseline_threshold,
                         "relativeToMargin": doc.view.grid.baseline_relative_to.starts_with("TopOfMargin"), "color": doc.view.grid.baseline_color.rgb(), "colorName": doc.view.grid.baseline_color.name()},
            "grid": {"horizontal": doc.view.grid.horizontal_division, "vertical": doc.view.grid.vertical_division,
                     "horizontalSub": doc.view.grid.horizontal_subdivision, "verticalSub": doc.view.grid.vertical_subdivision, "color": doc.view.grid.grid_color.rgb(), "colorName": doc.view.grid.grid_color.name()},
        },
        "editorial": doc.stories.iter().map(|(id,s)| json!({"id":id,"notes":s.notes,"changes":s.changes,"trackChanges":s.track_changes})).collect::<Vec<_>>(),
        "facingPages": facing,
        "masterPages": master_pages,
        "layers": layers,
        "swatches": swatches,
        "masters": masters,
        "pages": pages,
        "paragraphStyles": styles_json(&st.paragraph, &["$ID/[No paragraph style]", "$ID/NormalParagraphStyle"]),
        "characterStyles": styles_json(&st.character, &["$ID/[No character style]"]),
        "objectStyles": object_styles_json(doc, &st.object),
        "cellStyles": table_styles_json(&st.cell, &["$ID/[None]"]),
        "tableStyles": table_styles_json(&st.table, &["$ID/[No table style]", "$ID/[Basic Table]"]),
        "paragraphStyleGroups": style_groups_json(&st.paragraph_groups),
        "characterStyleGroups": style_groups_json(&st.character_groups),
        "cellStyleGroups": style_groups_json(&st.cell_groups),
        "objectStyleGroups": style_groups_json(&st.object_groups),
        "tableStyleGroups": style_groups_json(&st.table_groups),
        // the text variables Type > Text Variables > Insert Variable offers (the built-in cross-reference ones are not listed)
        // the Ink Manager's list: the four process inks, then the spot inks (name without `$ID/`; alias = the ink it prints on)
        "inks": doc.print.inks.iter().map(|k| json!({"name": k.name, "process": k.process, "convertToProcess": k.convert_to_process, "alias": k.alias, "inkType": k.ink_type, "print": k.print})).collect::<Vec<_>>(),
        "textVariables": doc.text_variables.values().filter(|v| !v.name.starts_with("<?AID")).map(|v| json!({"id": v.id, "name": v.name, "kind": v.kind, "settings": v.settings})).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_as_the_panels_show_them() {
        assert_eq!(style_name("$ID/NormalParagraphStyle", &[]), "[Basic Paragraph]");
        assert_eq!(style_name("$ID/[No paragraph style]", &[]), "[No Paragraph Style]");
        assert_eq!(style_name("$ID/[Normal Text Frame]", &[]), "[Basic Text Frame]");
        assert_eq!(style_name("Heads:Sub:Title", &["$ID/Heads".into(), "Sub".into()]), "Title");
        assert_eq!(style_name("Body", &[]), "Body");
        let entries = vec![
            folio_model::StyleEntry { id: "ParagraphStyle/Body".into(), name: "Body".into(), group: vec![], ..Default::default() },
            folio_model::StyleEntry { id: "ParagraphStyle/$ID/NormalParagraphStyle".into(), name: "$ID/NormalParagraphStyle".into(), group: vec![], ..Default::default() },
            folio_model::StyleEntry { id: "ParagraphStyle/$ID/[No paragraph style]".into(), name: "$ID/[No paragraph style]".into(), group: vec![], ..Default::default() },
        ];
        let v = styles_json(&entries, &["$ID/[No paragraph style]", "$ID/NormalParagraphStyle"]);
        let names: Vec<&str> = v.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["[No Paragraph Style]", "[Basic Paragraph]", "Body"]);
    }
}
