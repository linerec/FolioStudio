//! Type ▸ Text Variables ▸ Define: new, edit and rename a text variable of the document (oracle variables var-def-*, var-edit-*).
//! Measured with the DOM's `textVariables.add` and `variableOptions`:
//! - a variable is a `<TextVariable Self="dTextVariablen{Name}" Name VariableType>` of the designmap with one preference element whose
//!   attributes (its kind decides which) are written in InDesign's order; the designmap lists the variables sorted by name, so a new
//!   or renamed one goes where its name sorts;
//! - editing keeps the element in place (renaming moves it) and rewrites the preference; renaming changes the `Self`, the `Name` and
//!   the instances' `Name` and `AssociatedTextVariable`;
//! - the result an instance stores follows the definition: the parent page's placeholder of a running header is
//!   `TextBefore<Name>TextAfter` (var-edit-rename: ">> <Heading Text>"), a custom text's is its contents; the others come from the
//!   layout (`folio_text::variable_results`, the server's pass).
//!   The tokens InDesign writes were measured for: `SearchStrategy` FirstOnPage / LastOnPage, `ChangeCase` None / Uppercase (the others
//!   follow the same pattern: Lowercase, Titlecase, Sentencecase), `Scope` SectionScope (DocumentScope follows), `Format` Current.
use super::*;
use std::collections::BTreeMap;

const DM: &str = "designmap.xml";

/// What a kind of variable keeps: its preference element, the attributes in InDesign's order with their defaults.
fn kind_of(kind: &str) -> Option<(&'static str, &'static [(&'static str, &'static str)])> {
    Some(match kind {
        "CustomTextType" => ("CustomTextVariablePreference", &[]),
        "MatchParagraphStyleType" => ("MatchParagraphStylePreference", &[("TextBefore", ""), ("TextAfter", ""), ("AppliedParagraphStyle", "ParagraphStyle/$ID/NormalParagraphStyle"), ("SearchStrategy", "FirstOnPage"), ("ChangeCase", "None"), ("DeleteEndPunctuation", "false")]),
        "MatchCharacterStyleType" => ("MatchCharacterStylePreference", &[("TextBefore", ""), ("TextAfter", ""), ("AppliedCharacterStyle", "CharacterStyle/$ID/[No character style]"), ("SearchStrategy", "FirstOnPage"), ("ChangeCase", "None"), ("DeleteEndPunctuation", "false")]),
        "ChapterNumberType" => ("ChapterNumberVariablePreference", &[("TextBefore", ""), ("Format", "Current"), ("TextAfter", "")]),
        "LastPageNumberType" => ("PageNumberVariablePreference", &[("TextBefore", ""), ("Format", "Current"), ("TextAfter", ""), ("Scope", "SectionScope")]),
        "CreationDateType" | "ModificationDateType" | "OutputDateType" => ("DateVariablePreference", &[("TextBefore", ""), ("Format", "MM/dd/yy"), ("TextAfter", "")]),
        "FileNameType" => ("FileNameVariablePreference", &[("TextBefore", ""), ("IncludePath", "false"), ("IncludeExtension", "false"), ("TextAfter", "")]),
        // Image Name (measured in a new document: the variable named "Image Name")
        "LiveCaptionType" => ("CaptionMetadataVariablePreference", &[("TextBefore", ""), ("MetadataProviderName", "$ID/#LinkInfoNameStr"), ("TextAfter", "")]),
        _ => return None,
    })
}

/// The tokens a setting accepts, where InDesign has a fixed list.
fn allowed(key: &str, value: &str) -> bool {
    match key {
        "SearchStrategy" => matches!(value, "FirstOnPage" | "LastOnPage"),
        "ChangeCase" => matches!(value, "None" | "Uppercase" | "Lowercase" | "Titlecase" | "Sentencecase"),
        "Scope" => matches!(value, "SectionScope" | "DocumentScope"),
        "DeleteEndPunctuation" | "IncludePath" | "IncludeExtension" => matches!(value, "true" | "false"),
        "Format" if !value.is_empty() => true,
        _ => true,
    }
}

fn self_id(name: &str) -> String {
    // InDesign writes a colon of the name as a dash in the id ("Live Metadata Caption: Name", captions probe-c)
    format!("dTextVariablen{}", name.replace(':', "-"))
}

/// The sort key of the designmap's list: by name, ignoring case, the unnamed cross-reference ones first ("<?AID …").
fn sort_key(name: &str) -> (String, String) {
    (name.to_lowercase(), name.to_owned())
}

fn scan(w: &Work<'_>) -> Result<(String, Index), EditError> {
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    Ok((t, ix))
}

fn variables(ix: &Index, t: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "TextVariable" && ix.attr(t, i, "Self").is_some()).collect()
}

/// The element of a variable: its preference with `options` over `base` (the preference's own values, else the defaults).
fn element(self_id: &str, name: &str, kind: &str, options: &BTreeMap<String, String>, base: &BTreeMap<String, String>, indent: &str) -> Result<String, EditError> {
    let (pref, attrs) = kind_of(kind).ok_or_else(|| EditError::new("invalid", format!("variable type {kind} is not supported")))?;
    for (k, v) in options {
        if k != "Contents" && !attrs.iter().any(|(a, _)| a == k) {
            return Err(EditError::new("invalid", format!("{k} is not a setting of a {kind} variable")));
        }
        if !allowed(k, v) {
            return Err(EditError::new("invalid", format!("{k}: {v} is not accepted")));
        }
    }
    let mut s = format!("{indent}<TextVariable Self=\"{}\" Name=\"{}\" VariableType=\"{kind}\">", escape(self_id), escape(name));
    if kind == "CustomTextType" {
        let contents = options.get("Contents").or_else(|| base.get("Contents")).cloned().unwrap_or_default();
        s.push_str(&format!("\n{indent}\t<{pref}>\n{indent}\t\t<Properties>\n{indent}\t\t\t<Contents type=\"string\">{}</Contents>\n{indent}\t\t</Properties>\n{indent}\t</{pref}>", escape(&contents)));
    } else {
        s.push_str(&format!("\n{indent}\t<{pref}"));
        for (k, d) in attrs {
            let v = options.get(*k).or_else(|| base.get(*k)).map(String::as_str).unwrap_or(d);
            s.push_str(&format!(" {k}=\"{}\"", escape(v)));
        }
        s.push_str(" />");
    }
    s.push_str(&format!("\n{indent}</TextVariable>"));
    Ok(s)
}

/// Type ▸ Text Variables ▸ Define ▸ New.
pub(super) fn define(w: &mut Work<'_>, name: &str, kind: &str, options: &BTreeMap<String, String>) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let all = variables(&ix, &t);
    if name.trim().is_empty() {
        return Err(EditError::new("invalid", "a text variable needs a name"));
    }
    let id = self_id(name);
    if all.iter().any(|&i| ix.attr(&t, i, "Self") == Some(escape(&id).as_str()) || ix.value(&t, i, "Name").as_deref() == Some(name)) {
        return Err(EditError::new("invalid", format!("a text variable named {name} exists")));
    }
    let first = *all.first().ok_or_else(|| EditError::new("invalid", "designmap has no text variable to place the new one beside"))?;
    let indent = indent_before(&t, ix.nodes[first].start).to_owned();
    let new = element(&id, name, kind, options, &BTreeMap::new(), &indent)?;
    // before the first variable that sorts after it, else after the last
    let key = sort_key(name);
    let at = match all.iter().find(|&&i| sort_key(&ix.value(&t, i, "Name").unwrap_or_default()) > key) {
        Some(&i) => (ix.nodes[i].start..ix.nodes[i].start, format!("{}\n{indent}", new.trim_start())),
        None => {
            let end = ix.nodes[*all.last().unwrap()].end;
            (end..end, format!("\n{new}"))
        }
    };
    w.splice(DM, vec![at])
}

/// What a parent's running header shows for the variable: its name in angle brackets, the text before and after around it
/// (oracle var-edit-rename: ">> <Heading Text>").
fn placeholder(settings: &BTreeMap<String, String>, name: &str) -> String {
    format!("{}<{name}>{}", settings.get("TextBefore").map_or("", String::as_str), settings.get("TextAfter").map_or("", String::as_str))
}

/// The settings a variable has now (the preference's attributes, a custom text's contents).
fn current(ix: &Index, t: &str, v: usize) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for &c in ix.children(v) {
        for a in ix.attrs(t, c) {
            out.insert(t[a.name.0..a.name.1].to_owned(), crate::xml::unescape(&t[a.value.0..a.value.1]));
        }
        if let Some(n) = ix.path(t, c, &["Properties", "Contents"]) {
            out.insert("Contents".into(), crate::xml::unescape(t[ix.nodes[n].open_end..ix.nodes[n].close_start].trim()));
        }
    }
    out
}

/// Type ▸ Text Variables ▸ Define ▸ Edit: new settings (and a new name) of the variable `id`. The instances take the name; a custom
/// text's instances take its contents (the other results follow from the layout).
pub(super) fn edit(w: &mut Work<'_>, id: &str, rename: Option<&str>, options: &BTreeMap<String, String>) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let all = variables(&ix, &t);
    let v = *all.iter().find(|&&i| ix.attr(&t, i, "Self") == Some(escape(id).as_str())).ok_or_else(|| EditError::new("notFound", format!("text variable {id}")))?;
    let kind = ix.attr(&t, v, "VariableType").unwrap_or_default().to_owned();
    let old_name = ix.value(&t, v, "Name").unwrap_or_default();
    let name = rename.unwrap_or(&old_name).to_owned();
    if rename.is_some_and(|n| n != old_name && all.iter().any(|&i| i != v && ix.value(&t, i, "Name").as_deref() == Some(n))) {
        return Err(EditError::new("invalid", format!("a text variable named {name} exists")));
    }
    let new_id = if rename.is_some() { self_id(&name) } else { id.to_owned() };
    let indent = indent_before(&t, ix.nodes[v].start).to_owned();
    let base = current(&ix, &t, v);
    let new = element(&new_id, &name, &kind, options, &base, &indent)?;
    let span = ix.nodes[v].start..ix.nodes[v].end;
    let mut edits = Vec::new();
    // renamed: it moves to where its name sorts, i.e. before the first other variable that sorts after it
    let others: Vec<usize> = all.iter().copied().filter(|&i| i != v).collect();
    let key = sort_key(&name);
    let target = others.iter().find(|&&i| sort_key(&ix.value(&t, i, "Name").unwrap_or_default()) > key).copied();
    let successor = all.iter().position(|&x| x == v).and_then(|p| all.get(p + 1).copied());
    if rename.is_none() || target == successor {
        edits.push((span.clone(), new.trim_start().to_owned()));
    } else {
        let line = t[..span.start].rfind('\n').unwrap_or(span.start);
        edits.push((line..span.end, String::new()));
        match target {
            Some(i) => edits.push((ix.nodes[i].start..ix.nodes[i].start, format!("{}\n{indent}", new.trim_start()))),
            None => {
                let end = ix.nodes[*others.last().ok_or_else(|| EditError::new("invalid", "no text variable"))?].end;
                edits.push((end..end, format!("\n{new}")));
            }
        }
    }
    w.splice(DM, edits)?;
    // the instances: name, the variable, a custom text's result, the placeholder a parent's running header shows
    let contents = if kind == "CustomTextType" { options.get("Contents").cloned() } else { None };
    let matched = matches!(kind.as_str(), "MatchParagraphStyleType" | "MatchCharacterStyleType");
    let (old_placeholder, new_placeholder) = (placeholder(&base, &old_name), placeholder(&{ let mut s = base.clone(); s.extend(options.clone()); s }, &name));
    let parts: Vec<String> = w.ed.parts.keys().filter(|n| n.starts_with("Stories/")).cloned().collect();
    for part in parts {
        let text = w.text(&part)?.to_owned();
        if !text.contains("<TextVariableInstance ") {
            continue;
        }
        let sx = Index::scan(&text).map_err(|e| EditError::new("xml", e))?;
        let mut changes = Vec::new();
        for i in (0..sx.nodes.len()).filter(|&i| sx.local(&text, i) == "TextVariableInstance") {
            if sx.attr(&text, i, "AssociatedTextVariable") != Some(escape(id).as_str()) {
                continue;
            }
            if rename.is_some() {
                changes.push(attr_edit(&sx, &text, i, "Name", &name));
                changes.push(attr_edit(&sx, &text, i, "AssociatedTextVariable", &new_id));
            }
            if let Some(c) = &contents {
                changes.push(attr_edit(&sx, &text, i, "ResultText", c));
            } else if matched && sx.value(&text, i, "ResultText").as_deref() == Some(old_placeholder.as_str()) && old_placeholder != new_placeholder {
                changes.push(attr_edit(&sx, &text, i, "ResultText", &new_placeholder));
            }
        }
        if !changes.is_empty() {
            w.splice(&part, changes)?;
        }
    }
    Ok(())
}

/// Define ▸ Delete: the variable goes, and so do its instances: each becomes the text it shows, except the placeholder a parent's
/// running header shows (`<Name>`, no text). Deleting a variable that is in use with the scripting `remove()` crashed InDesign
/// (oracle: var-del-*, 2026-10-22), and after a partial convert it dropped the instance left over without text, so what the Delete
/// button does with the instances of a used variable is not measured: keeping their text is the choice that loses nothing. An unused
/// variable just goes (var-del-unused).
pub(super) fn delete(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let (t, ix) = scan(w)?;
    let v = *variables(&ix, &t).iter().find(|&&i| ix.attr(&t, i, "Self") == Some(escape(id).as_str())).ok_or_else(|| EditError::new("notFound", format!("text variable {id}")))?;
    let name = ix.value(&t, v, "Name").unwrap_or_default();
    let matched = matches!(ix.attr(&t, v, "VariableType"), Some("MatchParagraphStyleType" | "MatchCharacterStyleType"));
    let shown = placeholder(&current(&ix, &t, v), &name);
    let mark = format!("AssociatedTextVariable=\"{}\"", escape(id));
    let stories: Vec<String> = w.ed.parts.iter().filter(|(n, text)| n.starts_with("Stories/") && text.contains(&mark)).map(|(n, _)| n.trim_start_matches("Stories/Story_").trim_end_matches(".xml").to_owned()).collect();
    for story in stories {
        crate::text::convert_instances(w, &story, &|_, inst| (inst.variable == id).then(|| if matched && inst.result == shown { String::new() } else { inst.result.clone() }))?;
    }
    let (t, ix) = scan(w)?;
    let v = *variables(&ix, &t).iter().find(|&&i| ix.attr(&t, i, "Self") == Some(escape(id).as_str())).ok_or_else(|| EditError::new("notFound", format!("text variable {id}")))?;
    let span = ix.nodes[v].start..ix.nodes[v].end;
    let line = t[..span.start].rfind('\n').unwrap_or(span.start);
    w.splice(DM, vec![(line..span.end, String::new())])
}
