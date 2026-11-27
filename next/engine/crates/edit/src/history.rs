//! History panel (build-c, 2026-11-21): Delete (one step), Clear History, and New Document from a step. These act on the undo record itself,
//! so they are editor operations beside undo and redo (the server's `historyAction`), not edits inside a step.
//!
//! A step can be deleted on its own when no later step changed a part it changed: the later steps do not depend on it, so its change is
//! taken back exactly (its byte splices, in reverse) and the step leaves the record. A step a later one depends on is refused with the
//! reason (`depends`); the newest step can always go. The redo steps are forgotten, as after any new change.
use crate::{EditError, Editor, Outcome, Splice};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub commands: Vec<String>,
    pub changed_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Checkpoint {
    pub rev: u64,
    pub depth: usize,
    document: u64,
    epoch: u64,
    tip: Option<u64>,
}

pub(super) struct Group {
    pub start: usize,
    pub start_stamp: u64,
    label: String,
}

impl crate::Txn {
    pub(super) fn append(&mut self, later: Self) {
        self.splices.extend(later.splices);
        self.recompose.extend(later.recompose);
        self.commands.extend(later.commands);
        self.text_after = later.text_after;
        self.stamp = later.stamp;
    }
}

fn elements(text: Option<&String>) -> BTreeMap<&str, &str> {
    let Some(text) = text else { return BTreeMap::new() };
    let Ok(ix) = crate::xml::Index::scan(text) else { return BTreeMap::new() };
    ix.nodes.iter().enumerate().filter_map(|(i, n)| Some((ix.attr(text, i, "Self")?, &text[n.start..n.end]))).collect()
}

impl Editor {
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint { rev: self.rev, depth: self.undo.len(), document: self.doc, epoch: self.history_epoch, tip: self.undo.last().map(|t| t.stamp) }
    }

    /// Steps to undo, newest first. A checkpoint on a discarded history branch cannot be restored.
    pub fn steps_since(&self, point: &Checkpoint) -> Result<Vec<String>, EditError> {
        let prefix = self.undo.get(..point.depth);
        if self.doc != point.document || self.history_epoch != point.epoch ||
            !prefix.is_some_and(|p| p.last().map(|t| t.stamp) == point.tip) {
            return Err(EditError::new("staleCheckpoint", "history has changed since this checkpoint"));
        }
        Ok(self.undo[point.depth..].iter().rev().map(|t| t.label.clone()).collect())
    }

    pub fn begin_group(&mut self, label: &str) -> Result<(), EditError> {
        if self.group.is_some() || self.preview.is_some() { return Err(EditError::new("busy", "an edit group or preview is active")); }
        self.group = Some(Group {start:self.undo.len(),start_stamp:self.part_tick,label:label.to_owned()});
        Ok(())
    }

    pub fn end_group(&mut self) -> Result<GroupSummary, EditError> {
        let Group {start,label,start_stamp} = self.group.take().ok_or_else(|| EditError::new("invalid", "no open group"))?;
        self.redo.retain(|txn|txn.stamp<=start_stamp);
        let mut txns = self.undo.split_off(start).into_iter();
        let Some(mut txn) = txns.next() else { return Ok(GroupSummary { commands: vec![], changed_ids: vec![] }); };
        for later in txns { txn.append(later); }
        txn.label = label;
        let parts = parts_of(&txn.splices);
        let mut before = parts.iter().filter_map(|&name| Some((name.to_owned(), self.parts.get(name)?.clone()))).collect();
        for splice in txn.splices.iter().rev() { undo_into(&mut before, splice); }
        let mut changed = BTreeSet::new();
        for part in parts {
            let old = elements(before.get(part));
            let new = elements(self.parts.get(part));
            changed.extend(old.keys().chain(new.keys()).filter(|id| old.get(*id) != new.get(*id)).map(|id| id.to_string()));
        }
        let commands = std::mem::take(&mut txn.commands);
        self.undo.push(txn);
        Ok(GroupSummary { commands, changed_ids: changed.into_iter().collect() })
    }
}

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
        if self.group.is_some() { return Err(EditError::new("busy", "edit group is active")); }
        self.history_step_deletable(index)?;
        self.history_epoch += 1;
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
        if self.group.is_some() { return vec![]; }
        self.history_epoch += 1;
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
    fn group_keeps_only_successful_edits_and_restores_exact_parts() {
        let mut ed = doc();
        let start = ed.parts().clone();
        let point = ed.checkpoint();
        ed.begin_group("title").unwrap();
        insert(&mut ed, 0, "A");
        assert!(ed.apply("invalid", &[Command::InsertText { story:"missing".into(), at:0, text:"x".into() }]).is_err());
        insert(&mut ed, 1, "B");
        ed.undo().unwrap();
        ed.redo().unwrap();
        let summary = ed.end_group().unwrap();
        assert_eq!(summary.commands, ["insertText", "insertText"]);
        assert_eq!(summary.changed_ids, ["st"]);
        assert_eq!(ed.steps_since(&point).unwrap(), ["title"]);
        let end = ed.parts().clone();
        ed.undo().unwrap();
        assert_eq!(ed.parts(), &start);
        ed.redo().unwrap();
        assert_eq!(ed.parts(), &end);
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
