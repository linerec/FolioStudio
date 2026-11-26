//! History panel (build-c, 2026-11-21): Delete (one step), Clear History, and New Document from a step. These act on the undo record itself,
//! so they are editor operations beside undo and redo (the server's `historyAction`), not edits inside a step.
//!
//! A step can be deleted on its own when no later step changed a part it changed: the later steps do not depend on it, so its change is
//! taken back exactly (its byte splices, in reverse) and the step leaves the record. A step a later one depends on is refused with the
//! reason (`depends`); the newest step can always go. The redo steps are forgotten, as after any new change.
use crate::{EditError, Editor, Outcome, Splice};
use std::collections::{BTreeMap, BTreeSet};

fn parts_of(splices: &[Splice]) -> BTreeSet<&str> {
    splices.iter().map(|s| match s { Splice::Text { part, .. } | Splice::Part { part, .. } => part.as_str() }).collect()
}

fn undo_into(parts: &mut BTreeMap<String, String>, s: &Splice) {
    match s {
        Splice::Text { part, at, removed, inserted } => {
            if let Some(t) = parts.get_mut(part) { t.replace_range(*at..at + inserted.len(), removed); }
        }
        Splice::Part { part, before, .. } => match before {
            Some(b) => { parts.insert(part.clone(), b.clone()); }
            None => { parts.remove(part); }
        },
    }
}

impl Editor {
    /// Whether undo step `index` (0 = the oldest of `history().0`) can be deleted on its own; Err is the reason it cannot.
    pub fn history_step_deletable(&self, index: usize) -> Result<(), EditError> {
        let Some(txn) = self.undo.get(index) else { return Err(EditError::new("notFound", "no such history step")) };
        let mine = parts_of(&txn.splices);
        if let Some(later) = self.undo[index + 1..].iter().find(|t| !parts_of(&t.splices).is_disjoint(&mine)) {
            return Err(EditError::new("depends", format!("the later step \"{}\" changed what this step changed", later.label)));
        }
        Ok(())
    }

    /// History ▸ Delete: undo step `index` is taken out of the record and its change taken back (see the module comment).
    pub fn delete_history_step(&mut self, index: usize) -> Result<Outcome, EditError> {
        self.history_step_deletable(index)?;
        let settled = self.preview_settle();
        let txn = self.undo.remove(index);
        let last = index == self.undo.len();
        for s in txn.splices.iter().rev() { self.unsplice(s); }
        self.where_is.clear();
        if last { self.text_state = txn.text_before.clone(); }
        let mut out = self.finish(&txn);
        out.parts = out.parts.into_iter().chain(settled).collect::<BTreeSet<_>>().into_iter().collect();
        self.redo.clear();
        Ok(out)
    }

    /// History ▸ Clear History: every undo and redo step is forgotten; the document stays as it is (it still has its unsaved changes).
    pub fn clear_history(&mut self) -> Vec<String> {
        let settled = self.preview_settle();
        self.undo.clear();
        self.redo.clear();
        settled
    }

    /// History ▸ New Document: the parts as they were after the first `done` undo steps (0 = as opened), the document itself unchanged.
    pub fn parts_at_step(&self, done: usize) -> Result<BTreeMap<String, String>, EditError> {
        if done > self.undo.len() { return Err(EditError::new("notFound", "no such history step")); }
        let mut parts = self.parts.clone();
        for txn in self.undo[done..].iter().rev() {
            for s in txn.splices.iter().rev() { undo_into(&mut parts, s); }
        }
        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Command, Editor, commands::RegisteredCommand};
    use std::collections::BTreeMap;

    fn doc() -> Editor {
        let mut parts = BTreeMap::new();
        parts.insert("designmap.xml".into(), "<?xml version=\"1.0\"?>\n<Document Self=\"d\">\n\t<Condition Self=\"Condition/A\" Name=\"A\" IndicatorMethod=\"UseUnderline\" UnderlineIndicatorAppearance=\"Wavy\" Visible=\"true\">\n\t\t<Properties>\n\t\t\t<IndicatorColor type=\"enumeration\">LightBlue</IndicatorColor>\n\t\t</Properties>\n\t</Condition>\n\t<ConditionalTextPreference ShowConditionIndicators=\"ShowIndicators\" ActiveConditionSet=\"n\" />\n\t<idPkg:Story src=\"Stories/Story_st.xml\" />\n</Document>".into());
        parts.insert("Stories/Story_st.xml".into(), "<idPkg:Story><Story Self=\"st\"><ParagraphStyleRange><CharacterStyleRange AppliedCharacterStyle=\"n\"><Content>Hello</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>".into());
        Editor::new(parts)
    }

    fn insert(ed: &mut Editor, at: usize, text: &str) {
        ed.apply("type", &[Command::InsertText { story: "st".into(), at, text: text.into() }]).unwrap();
    }

    #[test]
    fn a_step_nothing_later_depends_on_can_go() {
        let mut ed = doc();
        let opened = ed.parts().clone();
        // step 0 changes the story, step 1 the designmap (a new condition), step 2 the story again
        insert(&mut ed, 5, " world");
        let after_text = ed.parts().clone();
        ed.apply("condition", &[Command::Registered(RegisteredCommand::CreateCondition { name: "B".into() })]).unwrap();
        insert(&mut ed, 0, "X");
        assert!(ed.history_step_deletable(0).is_err(), "step 2 changed the same story");
        assert!(ed.history_step_deletable(1).is_ok(), "nothing later changed the designmap");
        let at_0 = ed.parts_at_step(0).unwrap();
        assert_eq!(at_0, opened, "New Document at step 0 is the document as opened");
        assert_eq!(ed.parts_at_step(1).unwrap(), after_text);
        ed.delete_history_step(1).unwrap();
        assert!(!ed.parts()["designmap.xml"].contains("Name=\"B\""), "the condition is gone");
        assert!(ed.parts()["Stories/Story_st.xml"].contains("XHello world"), "the later step stays");
        assert_eq!(ed.history().0, ["type", "type"]);
        // the newest step can always go: it is an undo that is not kept for redo
        ed.delete_history_step(1).unwrap();
        assert!(ed.redo().is_none());
        assert_eq!(ed.parts(), &after_text);
        ed.clear_history();
        assert!(ed.undo().is_none() && ed.history().0.is_empty());
        assert_eq!(ed.parts(), &after_text, "clearing the history leaves the document");
    }
}
