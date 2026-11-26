//! Conditional Text panel (build-c, 2026-11-21): condition sets (Create New Set, Redefine, Delete, choosing a set), Load Conditions
//! (and Sets) from another document, and which conditions the text uses (Select All Unused). A set is InDesign's own element in
//! designmap.xml, after the conditions:
//! `<ConditionSet Self="ConditionSet/<name>" Name="<name>"><Properties><SetConditions><VisibilityPair Condition="Condition/<c>" Visibility="true"/>…`
//! and the set in use is `ConditionalTextPreference/@ActiveConditionSet` (`n`: none). Choosing a set shows and hides its conditions as the set
//! says (each through the condition's own visibility change, so hidden text moves as it does for the panel's eye).
use super::*;
use crate::xml::{Index, escape, unescape};
use std::collections::BTreeMap;

const DM: &str = "designmap.xml";

fn invalid(m: impl Into<String>) -> EditError { EditError::new("invalid", m) }

fn scan(t: &str) -> Result<Index, EditError> { Index::scan(t).map_err(|e| EditError::new("xml", e)) }

/// The top-level elements of designmap.xml named `tag`.
fn top(ix: &Index, t: &str, tag: &str) -> Vec<usize> {
    (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == tag && ix.nodes[i].parent.is_some_and(|p| ix.nodes[p].parent.is_none())).collect()
}

/// A set as the screen reads it: name, id, and the visibility it gives each condition (condition id → visible).
pub fn read_sets(designmap: &str) -> Result<(Vec<(String, String, Vec<(String, bool)>)>, String), EditError> {
    let ix = scan(designmap)?;
    let mut out = Vec::new();
    for i in top(&ix, designmap, "ConditionSet") {
        let pairs = (i..ix.nodes.len()).take_while(|&k| ix.nodes[k].start < ix.nodes[i].end)
            .filter(|&k| ix.local(designmap, k) == "VisibilityPair")
            .filter_map(|k| Some((unescape(ix.attr(designmap, k, "Condition")?), ix.attr(designmap, k, "Visibility") != Some("false")))).collect();
        out.push((unescape(ix.attr(designmap, i, "Self").unwrap_or_default()), unescape(ix.attr(designmap, i, "Name").unwrap_or_default()), pairs));
    }
    let active = top(&ix, designmap, "ConditionalTextPreference").first().and_then(|&p| ix.attr(designmap, p, "ActiveConditionSet")).map(unescape).unwrap_or_else(|| "n".into());
    Ok((out, active))
}

fn set_active(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    let t = w.text(DM)?.to_owned();
    let ix = scan(&t)?;
    let Some(&p) = top(&ix, &t, "ConditionalTextPreference").first() else { return Ok(()) };
    let edit = attr_edit(&ix, &t, p, "ActiveConditionSet", id);
    w.splice(DM, vec![edit])
}

fn set_xml(id: &str, name: &str, pairs: &[(String, bool)]) -> String {
    let mut s = format!("<ConditionSet Self=\"{}\" Name=\"{}\">\n\t\t<Properties>\n\t\t\t<SetConditions>\n", escape(id), escape(name));
    for (c, v) in pairs { s.push_str(&format!("\t\t\t\t<VisibilityPair Condition=\"{}\" Visibility=\"{v}\" />\n", escape(c))); }
    s.push_str("\t\t\t</SetConditions>\n\t\t</Properties>\n\t</ConditionSet>");
    s
}

/// The visibility of every condition now (the order of the document's conditions).
fn current(w: &Work<'_>) -> Result<Vec<(String, bool)>, EditError> {
    Ok(folio_idml::conditions::read(&w.ed.parts).map_err(|e| EditError::new("xml", e))?.into_iter().map(|c| (c.id, c.visible)).collect())
}

fn find_set(w: &Work<'_>, name: &str) -> Result<(String, String, Vec<(String, bool)>), EditError> {
    read_sets(w.text(DM)?)?.0.into_iter().find(|s| s.0 == name || s.1 == name).ok_or_else(|| EditError::new("notFound", format!("condition set {name}")))
}

/// Create New Set: a set named `name` with the visibility every condition has now; it becomes the set in use. Its id is in `created`.
pub(crate) fn create(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let name = name.trim();
    if name.is_empty() { return Err(invalid("a condition set needs a name")); }
    if name.contains(['%', '/']) || name.chars().any(char::is_control) { return Err(invalid("a condition set name cannot have %, / or control characters")); }
    let (sets, _) = read_sets(w.text(DM)?)?;
    if sets.iter().any(|s| s.1 == name) { return Err(invalid("A condition set with that name exists already.")); }
    let id = format!("ConditionSet/{name}");
    let xml = set_xml(&id, name, &current(w)?);
    let t = w.text(DM)?.to_owned();
    let ix = scan(&t)?;
    let at = top(&ix, &t, "ConditionSet").last().or(top(&ix, &t, "Condition").last()).map(|&i| ix.nodes[i].end)
        .ok_or_else(|| invalid("the document has no conditions to make a set of"))?;
    w.splice(DM, vec![(at..at, format!("\n\t{xml}"))])?;
    set_active(w, &id)?;
    w.created.push(id);
    Ok(())
}

/// Redefine: the set takes the visibility every condition has now.
pub(crate) fn redefine(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let (id, set_name, _) = find_set(w, name)?;
    let xml = set_xml(&id, &set_name, &current(w)?);
    let t = w.text(DM)?.to_owned();
    let ix = scan(&t)?;
    let i = ix.by_self(&t, &escape(&id)).ok_or_else(|| EditError::new("notFound", format!("condition set {name}")))?;
    w.splice(DM, vec![(ix.nodes[i].start..ix.nodes[i].end, xml)])?;
    set_active(w, &id)
}

/// Delete: the set goes; when it was in use, no set is.
pub(crate) fn delete(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let (id, _, _) = find_set(w, name)?;
    let (_, active) = read_sets(w.text(DM)?)?;
    let t = w.text(DM)?.to_owned();
    let ix = scan(&t)?;
    let i = ix.by_self(&t, &escape(&id)).ok_or_else(|| EditError::new("notFound", format!("condition set {name}")))?;
    let mut start = ix.nodes[i].start;
    while start > 0 && matches!(t.as_bytes()[start - 1], b'\t' | b' ') { start -= 1; }
    if start > 0 && t.as_bytes()[start - 1] == b'\n' { start -= 1; }
    w.splice(DM, vec![(start..ix.nodes[i].end, String::new())])?;
    if active == id { set_active(w, "n")?; }
    Ok(())
}

/// Choosing a set: its conditions are shown and hidden as it says (the others stay), and it becomes the set in use.
pub(crate) fn apply(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let (id, _, pairs) = find_set(w, name)?;
    let now: BTreeMap<String, bool> = current(w)?.into_iter().collect();
    for (condition, visible) in pairs {
        if now.get(&condition).is_some_and(|v| *v != visible) { conditions::set_visible(w, &condition, &visible)?; }
    }
    set_active(w, &id)
}

/// Load Conditions / Load Conditions and Sets: the conditions (and with `sets` the condition sets) of another document, given as its
/// package parts (`designmap.xml`); a condition or set with the same name takes the loaded one's settings, the others are added. The
/// same rules as Book ▸ Synchronize for conditional text (ops/book_resources.rs, measured with InDesign 2025).
pub(crate) fn load(w: &mut Work<'_>, parts: &BTreeMap<String, String>, sets: &bool) -> Result<(), EditError> {
    let dm = parts.get(DM).ok_or_else(|| invalid("the other document's designmap.xml is needed"))?;
    let mut source = dm.clone();
    if !*sets {
        let ix = scan(&source)?;
        let mut cuts: Vec<std::ops::Range<usize>> = top(&ix, &source, "ConditionSet").into_iter().map(|i| ix.nodes[i].start..ix.nodes[i].end).collect();
        cuts.reverse();
        for r in cuts { source.replace_range(r, ""); }
    }
    let mut given = BTreeMap::new();
    given.insert(DM.to_owned(), source);
    super::book_resources::sync(w, &["conditionalText".to_owned()], &given)
}

impl crate::Editor {
    /// Which conditions the text uses: condition id → the number of character ranges that name it (hidden text included). A condition
    /// not in the map is used nowhere (Select All Unused).
    pub fn condition_usage(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for (part, t) in &self.parts {
            if !part.starts_with("Stories/") { continue; }
            let Ok(ix) = Index::scan(t) else { continue };
            for i in 0..ix.nodes.len() {
                let Some(list) = ix.attr(t, i, "AppliedConditions") else { continue };
                for token in list.split_whitespace() {
                    *out.entry(unescape(token).replace("%20", " ")).or_insert(0) += 1;
                }
            }
        }
        out
    }

    /// The condition sets (`[{id, name, conditions: {condition id: visible}}]`) and the set in use (`n`: none).
    pub fn condition_sets(&self) -> Result<(serde_json::Value, String), EditError> {
        let (sets, active) = read_sets(self.part(DM).ok_or_else(|| invalid("missing designmap"))?)?;
        let list = sets.into_iter().map(|(id, name, pairs)| serde_json::json!({"id": id, "name": name,
            "conditions": pairs.into_iter().collect::<BTreeMap<_, _>>()})).collect();
        Ok((serde_json::Value::Array(list), active))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Command, Editor, commands::RegisteredCommand};
    use std::collections::BTreeMap;

    const DM: &str = "<?xml version=\"1.0\"?>\n<Document Self=\"d\">\n\t<Condition Self=\"Condition/Web\" Name=\"Web\" IndicatorMethod=\"UseUnderline\" UnderlineIndicatorAppearance=\"Wavy\" Visible=\"true\">\n\t\t<Properties>\n\t\t\t<IndicatorColor type=\"enumeration\">LightBlue</IndicatorColor>\n\t\t</Properties>\n\t</Condition>\n\t<Condition Self=\"Condition/Print\" Name=\"Print\" IndicatorMethod=\"UseUnderline\" UnderlineIndicatorAppearance=\"Wavy\" Visible=\"true\">\n\t\t<Properties>\n\t\t\t<IndicatorColor type=\"enumeration\">Red</IndicatorColor>\n\t\t</Properties>\n\t</Condition>\n\t<ConditionalTextPreference ShowConditionIndicators=\"ShowIndicators\" ActiveConditionSet=\"n\" />\n\t<idPkg:Story src=\"Stories/Story_st.xml\" />\n</Document>";

    fn doc() -> Editor {
        let mut parts = BTreeMap::new();
        parts.insert("designmap.xml".into(), DM.into());
        parts.insert("Stories/Story_st.xml".into(), "<idPkg:Story><Story Self=\"st\"><ParagraphStyleRange><CharacterStyleRange AppliedCharacterStyle=\"n\" AppliedConditions=\"Condition/Web\"><Content>Web text</Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"n\"><Content> plain</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>".into());
        Editor::new(parts)
    }

    fn run(ed: &mut Editor, c: RegisteredCommand) -> Result<Vec<String>, crate::EditError> {
        ed.apply("x", &[Command::Registered(c)]).map(|o| o.created)
    }

    #[test]
    fn sets_capture_and_apply_visibility() {
        let mut ed = doc();
        let original = ed.parts().clone();
        assert_eq!(run(&mut ed, RegisteredCommand::CreateConditionSet { name: "All".into() }).unwrap(), ["ConditionSet/All"]);
        run(&mut ed, RegisteredCommand::SetConditionVisible { name: "Condition/Web".into(), visible: false }).unwrap();
        run(&mut ed, RegisteredCommand::CreateConditionSet { name: "Print only".into() }).unwrap();
        let (sets, active) = ed.condition_sets().unwrap();
        assert_eq!(active, "ConditionSet/Print only");
        assert_eq!(sets[0]["conditions"]["Condition/Web"], true);
        assert_eq!(sets[1]["conditions"]["Condition/Web"], false);
        run(&mut ed, RegisteredCommand::ApplyConditionSet { name: "All".into() }).unwrap();
        assert!(ed.parts()["designmap.xml"].contains("Self=\"Condition/Web\" Name=\"Web\" IndicatorMethod=\"UseUnderline\" UnderlineIndicatorAppearance=\"Wavy\" Visible=\"true\""));
        assert_eq!(ed.condition_sets().unwrap().1, "ConditionSet/All");
        run(&mut ed, RegisteredCommand::SetConditionVisible { name: "Condition/Print".into(), visible: false }).unwrap();
        run(&mut ed, RegisteredCommand::RedefineConditionSet { name: "All".into() }).unwrap();
        assert_eq!(ed.condition_sets().unwrap().0[0]["conditions"]["Condition/Print"], false);
        run(&mut ed, RegisteredCommand::DeleteConditionSet { name: "All".into() }).unwrap();
        let (sets, active) = ed.condition_sets().unwrap();
        assert_eq!((sets.as_array().unwrap().len(), active.as_str()), (1, "n"));
        assert!(run(&mut ed, RegisteredCommand::CreateConditionSet { name: "Print only".into() }).is_err(), "names are unique");
        for _ in 0..7 { ed.undo(); }
        assert_eq!(ed.parts(), &original, "every step undoes");
    }

    #[test]
    fn usage_load_and_unapply() {
        let mut ed = doc();
        assert_eq!(ed.condition_usage().get("Condition/Web"), Some(&1));
        assert_eq!(ed.condition_usage().get("Condition/Print"), None, "Print is unused");
        // load from another document: a new condition and its set; with sets off only the condition comes
        let other = DM.replace("Condition/Print\" Name=\"Print\"", "Condition/Draft\" Name=\"Draft\"")
            .replace("\t<ConditionalTextPreference", "\t<ConditionSet Self=\"ConditionSet/S\" Name=\"S\">\n\t\t<Properties>\n\t\t\t<SetConditions>\n\t\t\t\t<VisibilityPair Condition=\"Condition/Draft\" Visibility=\"false\" />\n\t\t\t</SetConditions>\n\t\t</Properties>\n\t</ConditionSet>\n\t<ConditionalTextPreference");
        let parts: BTreeMap<String, String> = [("designmap.xml".to_owned(), other)].into();
        run(&mut ed, RegisteredCommand::LoadConditions { parts: parts.clone(), sets: false }).unwrap();
        assert!(ed.parts()["designmap.xml"].contains("Name=\"Draft\"") && !ed.parts()["designmap.xml"].contains("<ConditionSet"));
        ed.undo();
        run(&mut ed, RegisteredCommand::LoadConditions { parts, sets: true }).unwrap();
        assert!(ed.parts()["designmap.xml"].contains("Name=\"Draft\"") && ed.parts()["designmap.xml"].contains("<ConditionSet Self=\"ConditionSet/S\""));
        // Unapply one condition from part of the text: the rest keeps it
        run(&mut ed, RegisteredCommand::UnapplyCondition { story: "st".into(), start: 0, end: 3, condition: "Web".into() }).unwrap();
        let story = &ed.parts()["Stories/Story_st.xml"];
        assert!(story.contains("<Content>Web</Content>") && story.contains("AppliedConditions=\"Condition/Web\"><Content> text</Content>"), "{story}");
    }
}
