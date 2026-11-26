//! Page-panel state comes from the authoritative IDML, including preserved section data.
use folio_edit::xml::Index;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn summary(doc: &folio_model::Document) -> Value {
    json!(doc.spreads.iter().flat_map(|s|s.pages.iter().map(move|p|{
        let r=p.transform.apply_rect(&p.bounds);
        json!({"id":p.id,"index":p.index,"name":p.name,"width":r.width(),"height":r.height(),"spread":s.id})
    })).collect::<Vec<_>>())
}
/// What one spread or parent spread part says to the page settings: the entries it adds to `spreads`, `hidden`, `rotation`,
/// `hideParentItems`, `flattener`, `pageColors` and `parentColors`, as an object of those maps (empty maps left out).
fn part_settings(name: &str, t: &str) -> Value {
    let mut out = json!({"spreads":{},"hidden":{},"rotation":{},"hideParentItems":{},"flattener":{},"pageColors":{},"parentColors":{}});
    if name.starts_with("Spreads/")
        && let Ok(ix) = Index::scan(t)
    {
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "Spread"
                && let Some(id) = ix.attr(t, i, "Self")
            {
                out["spreads"][id] = json!(ix.attr(t, i, "AllowPageShuffle") != Some("false"));
                // Layout ▸ Pages ▸ Hide Spread
                if ix.attr(t, i, "SpreadHidden") == Some("true") {
                    out["hidden"][id] = json!(true);
                }
                // View ▸ Rotate Spread: the quarter turn clockwise in the spread's ItemTransform (0 is left out)
                let m: Vec<f64> = ix.attr(t, i, "ItemTransform").unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
                if m.len() == 6 {
                    let turn = if m[0] > 0.5 { 0 } else if m[1] > 0.5 { 90 } else if m[0] < -0.5 { 180 } else if m[1] < -0.5 { 270 } else { 0 };
                    if turn != 0 {
                        out["rotation"][id] = json!(turn);
                    }
                }
                // Pages panel ▸ Parent Pages ▸ Show Parent Items off
                if ix.attr(t, i, "ShowMasterItems") == Some("false") {
                    out["hideParentItems"][id] = json!(true);
                }
                // Page Attributes ▸ Spread Flattening (the custom values are kept at every level)
                let f = ix.named(t, i, "FlattenerPreference").first().copied();
                let num = |name: &str| f.and_then(|f| ix.attr(t, f, name)).and_then(|v| v.parse::<f64>().ok());
                let balance = f.and_then(|f| ix.path(t, f, &["Properties", "RasterVectorBalance"])).and_then(|j| t[ix.nodes[j].open_end..ix.nodes[j].close_start].trim().parse::<f64>().ok());
                out["flattener"][id] = json!({"level": ix.attr(t, i, "FlattenerOverride").unwrap_or("Default"), "balance": balance.unwrap_or(50.0),
                    "lineArt": num("LineArtAndTextResolution").unwrap_or(300.0), "gradient": num("GradientAndMeshResolution").unwrap_or(150.0),
                    "clip": f.and_then(|f| ix.attr(t, f, "ClipComplexRegions")) == Some("true"),
                    "strokes": f.and_then(|f| ix.attr(t, f, "ConvertAllStrokesToOutlines")) == Some("true"),
                    "text": f.and_then(|f| ix.attr(t, f, "ConvertAllTextToOutlines")) == Some("true")});
            }
            // Page Attributes ▸ Color Label (a page's own value; UseMasterColor shows its parent's)
            if ix.local(t, i) == "Page"
                && let Some(id) = ix.attr(t, i, "Self")
                && let Some(c) = ix.path(t, i, &["Properties", "PageColor"])
            {
                let v = t[ix.nodes[c].open_end..ix.nodes[c].close_start].trim();
                if v != "UseMasterColor" {
                    out["pageColors"][id] = json!(v);
                }
            }
        }
    }
    // a parent's own colour label
    if name.starts_with("MasterSpreads/")
        && let Ok(ix) = Index::scan(t)
    {
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "MasterSpread"
                && let Some(id) = ix.attr(t, i, "Self")
                && let Some(c) = ix.path(t, i, &["Properties", "PageColor"])
            {
                let v = t[ix.nodes[c].open_end..ix.nodes[c].close_start].trim();
                if v != "UseMasterColor" {
                    out["parentColors"][id] = json!(v);
                }
            }
        }
    }
    out
}

/// The page settings kept per part: a part is read again only when its revision (`Editor::part_revs`) is not the one the entry was made at.
#[derive(Default)]
pub struct SettingsCache {
    parts: std::collections::HashMap<String, (u64, std::sync::Arc<Value>)>,
}

pub fn settings(parts: &BTreeMap<String, String>) -> Value {
    settings_with(parts, &BTreeMap::new(), &mut SettingsCache::default())
}

/// `settings`, reading again only the spread parts whose revision differs from the cache's.
pub fn settings_with(parts: &BTreeMap<String, String>, revs: &BTreeMap<String, u64>, cache: &mut SettingsCache) -> Value {
    let mut out = json!({"allowShuffle":true,"spreads":{},"hidden":{},"rotation":{},"hideParentItems":{},"flattener":{},"pageColors":{},"parentColors":{},"sections":[]});
    if let Some(t) = parts.get("Resources/Preferences.xml")
        && let Ok(ix) = Index::scan(t)
        && let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "DocumentPreference") {
        out["allowShuffle"] = json!(ix.attr(t, i, "AllowPageShuffle") != Some("false"));
        for (k, a) in [("width", "PageWidth"), ("height", "PageHeight")] {
            out[k] = json!(ix.attr(t, i, a).and_then(|v| v.parse::<f64>().ok()));
        }
    }
    for (name, t) in parts {
        if !(name.starts_with("Spreads/") || name.starts_with("MasterSpreads/")) {
            continue;
        }
        let rev = revs.get(name).copied().unwrap_or(0);
        let contribution = match cache.parts.get(name) {
            Some((r, c)) if *r == rev => c.clone(),
            _ => {
                let c = std::sync::Arc::new(part_settings(name, t));
                cache.parts.insert(name.clone(), (rev, c.clone()));
                c
            }
        };
        // the part's entries go in as the part's own reading would have put them, in the order of the parts
        for (key, entries) in contribution.as_object().into_iter().flatten() {
            for (id, v) in entries.as_object().into_iter().flatten() {
                out[key.as_str()][id.as_str()] = v.clone();
            }
        }
    }
    // the screen colours of the labels in use
    let used: Vec<String> = ["pageColors", "parentColors"].iter().flat_map(|k| out[*k].as_object().into_iter().flat_map(|m| m.values().filter_map(|v| v.as_str().map(str::to_owned))).collect::<Vec<_>>()).collect();
    out["labelColors"] = json!({});
    for name in used {
        if let Some(rgb) = folio_model::ui_color_rgb(&name) {
            out["labelColors"][name] = json!(rgb);
        }
    }
    if let Some(t) = parts.get("designmap.xml")
        && let Ok(ix) = Index::scan(t)
    {
        out["sections"]=json!((0..ix.nodes.len()).filter(|&i|ix.local(t,i)=="Section").map(|i|{
            let style=ix.path(t,i,&["Properties","PageNumberStyle"]).map(|j|t[ix.nodes[j].open_end..ix.nodes[j].close_start].trim()).unwrap_or("Arabic");
            json!({"page":ix.attr(t,i,"PageStart"),"start":ix.attr(t,i,"PageNumberStart").and_then(|s|s.parse::<u32>().ok()).unwrap_or(1),
                "continue":ix.attr(t,i,"ContinueNumbering")!=Some("false"),"style":style,
                "prefix":ix.value(t,i,"SectionPrefix").unwrap_or_default(),"includePrefix":ix.attr(t,i,"IncludeSectionPrefix")==Some("true"),
                "marker":ix.value(t,i,"Marker").unwrap_or_default()})
        }).collect::<Vec<_>>());
    }
    out
}

/// The settings function as it was before the per-part cache: the cached one must give the same.
#[cfg(test)]
pub mod reference {
    use super::*;
    pub fn settings(parts: &BTreeMap<String, String>) -> Value {
        let mut out = json!({"allowShuffle":true,"spreads":{},"hidden":{},"rotation":{},"hideParentItems":{},"flattener":{},"pageColors":{},"parentColors":{},"sections":[]});
        if let Some(t) = parts.get("Resources/Preferences.xml")
            && let Ok(ix) = Index::scan(t)
            && let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "DocumentPreference") {
            out["allowShuffle"] = json!(ix.attr(t, i, "AllowPageShuffle") != Some("false"));
            for (k, a) in [("width", "PageWidth"), ("height", "PageHeight")] {
                out[k] = json!(ix.attr(t, i, a).and_then(|v| v.parse::<f64>().ok()));
            }
        }
        for (name, t) in parts {
            if name.starts_with("Spreads/")
                && let Ok(ix) = Index::scan(t)
            {
                for i in 0..ix.nodes.len() {
                    if ix.local(t, i) == "Spread"
                        && let Some(id) = ix.attr(t, i, "Self")
                    {
                        out["spreads"][id] = json!(ix.attr(t, i, "AllowPageShuffle") != Some("false"));
                        // Layout ▸ Pages ▸ Hide Spread
                        if ix.attr(t, i, "SpreadHidden") == Some("true") {
                            out["hidden"][id] = json!(true);
                        }
                        // View ▸ Rotate Spread: the quarter turn clockwise in the spread's ItemTransform (0 is left out)
                        let m: Vec<f64> = ix.attr(t, i, "ItemTransform").unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
                        if m.len() == 6 {
                            let turn = if m[0] > 0.5 { 0 } else if m[1] > 0.5 { 90 } else if m[0] < -0.5 { 180 } else if m[1] < -0.5 { 270 } else { 0 };
                            if turn != 0 {
                                out["rotation"][id] = json!(turn);
                            }
                        }
                        // Pages panel ▸ Parent Pages ▸ Show Parent Items off
                        if ix.attr(t, i, "ShowMasterItems") == Some("false") {
                            out["hideParentItems"][id] = json!(true);
                        }
                        // Page Attributes ▸ Spread Flattening (the custom values are kept at every level)
                        let f = ix.named(t, i, "FlattenerPreference").first().copied();
                        let num = |name: &str| f.and_then(|f| ix.attr(t, f, name)).and_then(|v| v.parse::<f64>().ok());
                        let balance = f.and_then(|f| ix.path(t, f, &["Properties", "RasterVectorBalance"])).and_then(|j| t[ix.nodes[j].open_end..ix.nodes[j].close_start].trim().parse::<f64>().ok());
                        out["flattener"][id] = json!({"level": ix.attr(t, i, "FlattenerOverride").unwrap_or("Default"), "balance": balance.unwrap_or(50.0),
                            "lineArt": num("LineArtAndTextResolution").unwrap_or(300.0), "gradient": num("GradientAndMeshResolution").unwrap_or(150.0),
                            "clip": f.and_then(|f| ix.attr(t, f, "ClipComplexRegions")) == Some("true"),
                            "strokes": f.and_then(|f| ix.attr(t, f, "ConvertAllStrokesToOutlines")) == Some("true"),
                            "text": f.and_then(|f| ix.attr(t, f, "ConvertAllTextToOutlines")) == Some("true")});
                    }
                    // Page Attributes ▸ Color Label (a page's own value; UseMasterColor shows its parent's)
                    if ix.local(t, i) == "Page"
                        && let Some(id) = ix.attr(t, i, "Self")
                        && let Some(c) = ix.path(t, i, &["Properties", "PageColor"])
                    {
                        let v = t[ix.nodes[c].open_end..ix.nodes[c].close_start].trim();
                        if v != "UseMasterColor" {
                            out["pageColors"][id] = json!(v);
                        }
                    }
                }
            }
            // a parent's own colour label
            if name.starts_with("MasterSpreads/")
                && let Ok(ix) = Index::scan(t)
            {
                for i in 0..ix.nodes.len() {
                    if ix.local(t, i) == "MasterSpread"
                        && let Some(id) = ix.attr(t, i, "Self")
                        && let Some(c) = ix.path(t, i, &["Properties", "PageColor"])
                    {
                        let v = t[ix.nodes[c].open_end..ix.nodes[c].close_start].trim();
                        if v != "UseMasterColor" {
                            out["parentColors"][id] = json!(v);
                        }
                    }
                }
            }
        }
        // the screen colours of the labels in use
        let used: Vec<String> = ["pageColors", "parentColors"].iter().flat_map(|k| out[*k].as_object().into_iter().flat_map(|m| m.values().filter_map(|v| v.as_str().map(str::to_owned))).collect::<Vec<_>>()).collect();
        out["labelColors"] = json!({});
        for name in used {
            if let Some(rgb) = folio_model::ui_color_rgb(&name) {
                out["labelColors"][name] = json!(rgb);
            }
        }
        if let Some(t) = parts.get("designmap.xml")
            && let Ok(ix) = Index::scan(t)
        {
            out["sections"]=json!((0..ix.nodes.len()).filter(|&i|ix.local(t,i)=="Section").map(|i|{
                let style=ix.path(t,i,&["Properties","PageNumberStyle"]).map(|j|t[ix.nodes[j].open_end..ix.nodes[j].close_start].trim()).unwrap_or("Arabic");
                json!({"page":ix.attr(t,i,"PageStart"),"start":ix.attr(t,i,"PageNumberStart").and_then(|s|s.parse::<u32>().ok()).unwrap_or(1),
                    "continue":ix.attr(t,i,"ContinueNumbering")!=Some("false"),"style":style,
                    "prefix":ix.value(t,i,"SectionPrefix").unwrap_or_default(),"includePrefix":ix.attr(t,i,"IncludeSectionPrefix")==Some("true"),
                    "marker":ix.value(t,i,"Marker").unwrap_or_default()})
            }).collect::<Vec<_>>());
        }
        out
    }
}
