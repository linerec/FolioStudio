use super::*;

pub(crate) fn apply(w: &mut Work<'_>, story: &str, start: &usize, end: &usize, conditions: &[String]) -> Result<(), EditError> {
    let (start, end) = (*start, *end);
    let definitions = folio_idml::conditions::read(&w.ed.parts).map_err(|e| EditError::new("xml", e))?;
    let selected = conditions.iter().map(|name| definitions.iter().find(|c| &c.id == name || &c.name == name)
        .ok_or_else(|| EditError::new("notFound", format!("condition {name}")))).collect::<Result<Vec<_>, _>>()?;
    if !selected.is_empty() && selected.iter().all(|c| !c.visible) {
        return Err(unsupported("applying only hidden conditions is not supported"));
    }
    if start == end { return Err(unsupported("condition typing attributes are not supported")); }
    let mut ids = selected.iter().map(|c| c.id.replace(' ', "%20")).collect::<Vec<_>>();
    ids.sort(); ids.dedup();
    let (part, ix, i, id) = locate(w, story)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, start, end)?;
    let mut cache = BTreeMap::new();
    for at in start..end {
        let old = s.atoms[at].c;
        let new = if let Some(&c) = cache.get(&old) { c } else {
            let mut attrs = style_attrs(&s.characters[old]);
            if ids.is_empty() { attrs.remove("AppliedConditions"); }
            else { attrs.insert("AppliedConditions".into(), attr(ids.join(" "))); }
            let c = s.characters.len();
            s.characters.push(make_style(&s.characters[old], &attrs));
            cache.insert(old, c); c
        };
        s.atoms[at].c = new;
    }
    w.ed.text_state.remove(&id);
    write(w, &part, &id, &s)
}

/// Conditional Text panel ▸ a condition row's right-click Unapply (build-c): `condition` (its id or name) leaves the text [`start`, `end`);
/// the other conditions of that text stay. Text without the condition is left as it is.
pub(crate) fn unapply(w: &mut Work<'_>, story: &str, start: &usize, end: &usize, condition: &str) -> Result<(), EditError> {
    let (start, end) = (*start, *end);
    let definitions = folio_idml::conditions::read(&w.ed.parts).map_err(|e| EditError::new("xml", e))?;
    let c = definitions.iter().find(|c| c.id == condition || c.name == condition).ok_or_else(|| EditError::new("notFound", format!("condition {condition}")))?;
    if start == end { return Err(unsupported("select text to unapply a condition from")); }
    let token = c.id.replace(' ', "%20");
    let (part, ix, i, id) = locate(w, story)?;
    let mut s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, start, end)?;
    let mut cache = BTreeMap::new();
    for at in start..end {
        let old = s.atoms[at].c;
        let new = if let Some(&c) = cache.get(&old) { c } else {
            let mut attrs = style_attrs(&s.characters[old]);
            let kept: Vec<String> = attrs.get("AppliedConditions").map(|p| p.value.split_whitespace().filter(|t| *t != token).map(str::to_owned).collect()).unwrap_or_default();
            if kept.is_empty() { attrs.remove("AppliedConditions"); } else { attrs.insert("AppliedConditions".into(), attr(kept.join(" "))); }
            let c = s.characters.len();
            s.characters.push(make_style(&s.characters[old], &attrs));
            cache.insert(old, c); c
        };
        s.atoms[at].c = new;
    }
    w.ed.text_state.remove(&id);
    write(w, &part, &id, &s)
}
