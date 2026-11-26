use crate::{EditError, Work, xml::Index};
use folio_idml::docprefs::{self, PART};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn set(w: &mut Work<'_>, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    let changes = props.iter().map(|(key, value)| {
        let f = docprefs::field(key).map_err(|e| EditError::new("invalid", e))?;
        let value = f.value(value).map_err(|e| EditError::new("invalid", e))?;
        Ok((f, value))
    }).collect::<Result<Vec<_>, EditError>>()?;
    let before = docprefs::settings(w.text(PART)?).map_err(|e| EditError::new("xml", e))?;
    let mut recompose = false;
    for (f, value) in changes {
        if before.get(&f.key()) == props.get(&f.key()) { continue; }
        let t = w.text(PART)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let edit = if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == f.element) {
            crate::ops::attr_edit(&ix, &t, i, f.attr, &value)
        } else {
            let root = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Preferences").ok_or_else(|| EditError::new("xml", "missing Preferences"))?;
            let at = ix.nodes[root].close_start;
            (at..at, format!("\t<{} {}=\"{}\" />\n", f.element, f.attr, crate::xml::escape(&value)))
        };
        w.splice(PART, vec![edit])?;
        recompose |= f.recompose;
    }
    if recompose {
        let stories: Vec<_> = w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
        for story in stories { w.recompose(&story); }
    }
    Ok(())
}

impl crate::Editor {
    pub fn document_preferences(&self) -> Result<BTreeMap<String, Value>, EditError> {
        let source = self.part(PART).ok_or_else(|| EditError::new("notFound", PART))?;
        docprefs::settings(source).map_err(|e| EditError::new("xml", e))
    }
}
