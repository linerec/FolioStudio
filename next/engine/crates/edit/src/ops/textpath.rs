//! Operations on text attached to an existing page-item path.
use super::*;
use std::collections::BTreeMap;

fn paths(ix: &Index, t: &str, i: usize) -> Vec<usize> {
    ix.children(i).iter().copied().filter(|&c| ix.local(t, c) == "TextPath").collect()
}

impl crate::Editor {
    pub fn text_path_options(&self, id: &str) -> Result<serde_json::Value, EditError> {
        let (t, ix, i) = self.parts().values().find_map(|t| {
            let ix = Index::scan(t).ok()?;
            let i = (0..ix.nodes.len()).find(|&i| ix.attr(t,i,"Self") == Some(id))?;
            Some((t, ix, i))
        }).ok_or_else(|| EditError::new("notFound", id))?;
        let children = paths(&ix, t, i);
        let p = *children.first().ok_or_else(|| EditError::new("invalid", "item has no text path"))?;
        let props: BTreeMap<_,_> = OPTION_NAMES.iter().filter_map(|&k| ix.attr(t,p,k).map(|v|(k,v))).collect();
        Ok(serde_json::json!({"props":props,"supported":folio_idml::textpath::supported(&t[ix.nodes[i].start..ix.nodes[i].end]).is_some()}))
    }
    /// The brackets a Selection tool drag of one bracket would give (see `textpath_tool::drag_values`); nothing changes.
    pub fn text_path_drag_values(&mut self, id: &str, which: &str, point: &[f64; 2], from: &[f64; 2]) -> Result<crate::BracketDrag, EditError> {
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        super::textpath_tool::drag_values(&mut w, id, which, point, from)
    }
    pub fn text_path_items(&self) -> Result<Vec<String>, EditError> {
        self.text_path_items_for_story(None)
    }

    pub fn text_path_items_for_story(&self, story: Option<&str>) -> Result<Vec<String>, EditError> {
        let mut ids = Vec::new();
        for (name, t) in self.parts() {
            if !name.starts_with("Spreads/") && !name.starts_with("MasterSpreads/") { continue; }
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            for i in 0..ix.nodes.len() {
                if is_item(&ix, t, i) && paths(&ix, t, i).iter().any(|&p|story.is_none_or(|s|ix.attr(t,p,"ParentStory")==Some(s)))
                    && let Some(id) = ix.attr(t, i, "Self") { ids.push(id.to_owned()); }
            }
        }
        Ok(ids)
    }
}

const OPTION_NAMES: &[&str] = &["PathAlignment", "TextAlignment", "PathEffect", "FlipPathEffect", "PathSpacing", "StartBracket", "EndBracket"];

pub(crate) fn set_options(w: &mut Work<'_>, ids: &[String], props: &BTreeMap<String, String>) -> Result<(), EditError> {
    if ids.is_empty() || props.is_empty() || props.keys().any(|k| !OPTION_NAMES.contains(&k.as_str())) {
        return Err(EditError::new("invalid", "select a text path and supported options"));
    }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        unlocked(w, &ix, &t, i)?;
        let before = folio_idml::textpath::supported(&t[ix.nodes[i].start..ix.nodes[i].end])
            .ok_or_else(|| EditError::new("unsupported", "select an unthreaded open text path"))?;
        let p = paths(&ix, &t, i)[0];
        let child = ix.attr(&t,p,"Self").ok_or_else(|| EditError::new("invalid", "text path has no id"))?.to_owned();
        let mut attrs: Vec<_> = props.iter().map(|(k,v)|(k.as_str(),v.clone())).collect();
        // A new path alignment moves the brackets onto the new offset path, as InDesign does (brackets given in the same command win).
        let alignment = match props.get("PathAlignment").map(String::as_str) {
            Some("TopPathAlignment") => Some(folio_model::textpath::PathAlignment::Top),
            Some("CenterPathAlignment") => Some(folio_model::textpath::PathAlignment::Center),
            Some("BottomPathAlignment") => Some(folio_model::textpath::PathAlignment::Bottom),
            _ => None,
        };
        if let (Some(alignment), false) = (alignment, props.contains_key("StartBracket") && props.contains_key("EndBracket"))
            && let Some((start, end)) = folio_idml::textpath::brackets_for_alignment(&t[ix.nodes[i].start..ix.nodes[i].end], alignment) {
            if !props.contains_key("StartBracket") { attrs.push(("StartBracket", start.to_string())); }
            if !props.contains_key("EndBracket") { attrs.push(("EndBracket", end.to_string())); }
        }
        set_attrs(w, &child, &attrs)?;
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?;
        if folio_idml::textpath::supported(&t[ix.nodes[i].start..ix.nodes[i].end]).is_none() {
            return Err(EditError::new("unsupported", "unsupported text path options or bracket range"));
        }
        w.recompose(&before.frame.story);
    }
    Ok(())
}

pub(super) fn unlocked(w: &Work<'_>, ix: &Index, t: &str, i: usize) -> Result<(), EditError> {
    if std::iter::once(i).chain(ix.ancestors(i)).any(|n| ix.attr(t,n,"Locked") == Some("true")) {
        return Err(EditError::new("locked", "text path is locked"));
    }
    let dm = w.text("designmap.xml")?;
    let dx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
    if (0..dx.nodes.len()).any(|k| dx.local(dm,k)=="Layer" && dx.attr(dm,k,"Self")==ix.attr(t,i,"ItemLayer") && dx.attr(dm,k,"Locked")==Some("true")) {
        return Err(EditError::new("locked", "text path layer is locked"));
    }
    Ok(())
}

pub(crate) fn remove(w: &mut Work<'_>, ids: &[String]) -> Result<(), EditError> {
    if ids.is_empty() { return Err(EditError::new("invalid", "select a text path")); }
    for id in ids {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        unlocked(w, &ix, &t, i)?;
        let children = paths(&ix, &t, i);
        if children.is_empty() { return Err(EditError::new("invalid", "item has no text path")); }
        let child_ids = children.iter().map(|&p| ix.attr(&t,p,"Self").map(str::to_owned)
            .ok_or_else(|| EditError::new("invalid", "text path has no id"))).collect::<Result<Vec<_>,_>>()?;
        for child in child_ids {
            delete(w, &child)?;
        }
    }
    Ok(())
}
