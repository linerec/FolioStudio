//! Preview session: a change the document shows provisionally (a list item the pointer rests on,
//! a value being dragged) and takes back exactly, without a trace in the undo history.
//!
//! The wire commands (registry `commands/mod.rs`, each sent alone in an `edit` request):
//!   `previewStart {label}`      open a session (one that is open is cancelled first). `label` names the
//!                               undo step a commit makes.
//!   `previewApply {commands}`   show these commands provisionally. The previous provisional change of the
//!                               session is taken back first, so what is shown is always base + the last
//!                               apply. A session is opened if none is.
//!   `previewCommit {}`          make the shown change permanent: the commands of the last apply are applied
//!                               for real, as one undo step labelled by the session.
//!   `previewCancel {}`          take the shown change back. The parts are exactly the bytes they were.
//!
//! A provisional change is kept as the byte splices of [`Editor::apply`] but never enters the undo or
//! redo stacks: `rev` moves (so the shown document is never mistaken for an older one) but `version`, the
//! document's identity for "unsaved changes", does not; the revisions of parts touched and stories to
//! compose afresh are put back on cancel. Any other edit, undo or redo takes an open session back first
//! (decision: an unconfirmed preview is never kept by a side effect), and the server does the same before
//! a save or an export, so what is written is never a preview.
use crate::{Command, EditError, Editor, Outcome, Splice, Work, commands::RegisteredCommand, ops, text};
use std::collections::BTreeSet;

/// The open session.
pub(crate) struct Preview {
    label: String,
    /// What the editor was when the session began.
    touched: BTreeSet<String>,
    recompose: BTreeSet<String>,
    text_state: text::InsertionState,
    /// The provisional change applied now (None: none yet, or it changed nothing).
    shown: Option<Shown>,
}

struct Shown {
    splices: Vec<Splice>,
    commands: Vec<Command>,
}

fn parts_of(splices: &[Splice]) -> BTreeSet<String> {
    splices.iter().map(|s| match s { Splice::Text { part, .. } | Splice::Part { part, .. } => part.clone() }).collect()
}

fn alone() -> EditError {
    EditError::new("invalid", "미리보기 명령은 다른 명령과 함께 보낼 수 없습니다 (요청마다 하나)")
}

// The registry's run functions: the preview commands act on the whole editor, not inside one edit, so
// `Editor::apply` takes them before any command runs. Reaching these means one was mixed with others.
pub(crate) fn start(_: &mut Work<'_>, _: &str) -> Result<(), EditError> { Err(alone()) }
pub(crate) fn apply(_: &mut Work<'_>, _: &[Command]) -> Result<(), EditError> { Err(alone()) }
pub(crate) fn commit(_: &mut Work<'_>) -> Result<(), EditError> { Err(alone()) }
pub(crate) fn cancel(_: &mut Work<'_>) -> Result<(), EditError> { Err(alone()) }

/// The result of a preview command alone in its request; None when `commands` is anything else.
pub(crate) fn dispatch(ed: &mut Editor, label: &str, commands: &[Command]) -> Option<Result<Outcome, EditError>> {
    let [Command::Registered(c)] = commands else {
        // a preview command among others is refused, not run as an edit
        return commands.iter().any(|c| matches!(c, Command::Registered(RegisteredCommand::PreviewStart { .. } | RegisteredCommand::PreviewApply { .. } | RegisteredCommand::PreviewCommit {} | RegisteredCommand::PreviewCancel {})))
            .then(|| Err(alone()));
    };
    Some(match c {
        RegisteredCommand::PreviewStart { label } => Ok(ed.preview_start(label)),
        RegisteredCommand::PreviewApply { commands } => ed.preview_apply(label, commands),
        RegisteredCommand::PreviewCommit {} => ed.preview_commit(),
        RegisteredCommand::PreviewCancel {} => Ok(ed.preview_cancel()),
        _ => return None,
    })
}

impl Editor {
    /// The undo step name of the open session, if one is open.
    pub fn preview_label(&self) -> Option<&str> {
        self.preview.as_ref().map(|p| p.label.as_str())
    }

    /// Takes the shown change back (not the session): the parts it touched.
    fn preview_revert(&mut self) -> BTreeSet<String> {
        let Some(shown) = self.preview.as_mut().and_then(|p| p.shown.take()) else { return BTreeSet::new() };
        for s in shown.splices.iter().rev() {
            self.unsplice(s);
        }
        self.where_is.clear();
        self.frame_starts.clear();
        self.frame_spans.clear();
        self.anchor_placements.clear();
        self.anchor_targets = Default::default();
        parts_of(&shown.splices)
    }

    /// Opens a session named `label` (an open one is cancelled first). The outcome lists the parts that went back.
    pub fn preview_start(&mut self, label: &str) -> Outcome {
        let out = self.preview_cancel();
        self.preview = Some(Preview { label: label.into(), touched: self.touched.clone(), recompose: self.recompose.clone(), text_state: self.text_state.clone(), shown: None });
        Outcome { label: label.into(), ..out }
    }

    /// Shows `commands` provisionally in place of what the session showed. On an error nothing changes: the
    /// session still shows what it did.
    pub fn preview_apply(&mut self, label: &str, commands: &[Command]) -> Result<Outcome, EditError> {
        if self.preview.is_none() {
            self.preview_start(label);
        }
        let (base_touched, base_recompose, base_text) = {
            let p = self.preview.as_ref().expect("session open");
            (p.touched.clone(), p.recompose.clone(), p.text_state.clone())
        };
        let before = self.preview.as_mut().and_then(|p| p.shown.take());
        // the insertion style a trial at a caret set (typing attributes: no bytes change) belongs to the trial too
        let shown_text = self.text_state.clone();
        let mut parts = BTreeSet::new();
        if let Some(s) = &before {
            for sp in s.splices.iter().rev() {
                self.unsplice(sp);
            }
            parts.extend(parts_of(&s.splices));
        }
        self.text_state = base_text.clone();
        self.where_is.clear();
        let mut w = Work { ed: self, splices: Vec::new(), recompose: Vec::new(), created: Vec::new() };
        for c in commands {
            if let Err(e) = ops::run(&mut w, c) {
                let splices = std::mem::take(&mut w.splices);
                for s in splices.iter().rev() {
                    self.unsplice(s);
                }
                // back to what was shown
                if let Some(s) = &before {
                    for sp in &s.splices {
                        self.resplice(sp);
                    }
                }
                self.where_is.clear();
                self.text_state = shown_text;
                self.preview.as_mut().expect("session open").shown = before;
                return Err(e);
            }
        }
        let (splices, recompose, created) = (w.splices, w.recompose, w.created);
        parts.extend(parts_of(&splices));
        self.frame_starts.clear();
        self.frame_spans.clear();
        self.anchor_placements.clear();
        self.anchor_targets = Default::default();
        // the editor's bookkeeping is base + this change alone
        self.touched = base_touched;
        self.recompose = base_recompose;
        self.touched.extend(parts_of(&splices));
        self.recompose.extend(recompose.iter().cloned());
        if !parts.is_empty() {
            self.rev += 1;
        }
        let label = self.preview.as_ref().map(|p| p.label.clone()).unwrap_or_default();
        // a trial that changed only the insertion style (character formatting at a caret) is shown too: a commit applies it
        let text_changed = format!("{:?}", self.text_state) != format!("{base_text:?}");
        self.preview.as_mut().expect("session open").shown = (!splices.is_empty() || text_changed).then(|| Shown { splices, commands: commands.to_vec() });
        Ok(Outcome { rev: self.rev, label, parts: parts.into_iter().collect(), recompose, created })
    }

    /// Takes the session's shown change back and closes it. The parts are as they were when it began.
    pub fn preview_cancel(&mut self) -> Outcome {
        let parts = self.preview_revert();
        let Some(p) = self.preview.take() else { return Outcome { rev: self.rev, ..Outcome::default() } };
        self.touched = p.touched;
        self.recompose = p.recompose;
        self.text_state = p.text_state;
        if !parts.is_empty() {
            self.rev += 1;
        }
        Outcome { rev: self.rev, label: p.label, parts: parts.into_iter().collect(), ..Outcome::default() }
    }

    /// Closes the session for a commit: what to apply for real (its name and the commands of the last apply; None
    /// when nothing is shown) and the parts that went back. The caller applies the commands through its
    /// ordinary edit path, so the committed step is what the same edit without a preview would have been.
    pub fn preview_close(&mut self) -> (Option<(String, Vec<Command>)>, Vec<String>) {
        let shown = self.preview.as_ref().and_then(|p| p.shown.as_ref()).map(|s| s.commands.clone());
        let label = self.preview.as_ref().map(|p| p.label.clone()).unwrap_or_default();
        let out = self.preview_cancel();
        (shown.map(|c| (label, c)), out.parts)
    }

    /// Makes the shown change permanent as one undo step named by the session.
    pub fn preview_commit(&mut self) -> Result<Outcome, EditError> {
        let (shown, parts) = self.preview_close();
        let Some((label, commands)) = shown else { return Ok(Outcome { rev: self.rev, ..Outcome::default() }) };
        let mut out = self.apply(&label, &commands)?;
        let all: BTreeSet<String> = out.parts.iter().cloned().chain(parts).collect();
        out.parts = all.into_iter().collect();
        Ok(out)
    }

    /// An edit, undo or redo is about to run: an open session is taken back first (the parts that went back).
    pub(crate) fn preview_settle(&mut self) -> Vec<String> {
        if self.preview.is_none() {
            return Vec::new();
        }
        self.preview_cancel().parts
    }
}

#[cfg(test)]
mod tests {
    // the session's behaviour is tested against real documents in tests/preview.rs
}
