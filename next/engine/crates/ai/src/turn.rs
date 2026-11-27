use folio_edit::{Checkpoint, EditError, Editor, GroupSummary};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Turn {
    pub start: Checkpoint,
    pub end: Checkpoint,
    pub summary: GroupSummary,
}

pub fn history_label(text: &str) -> String {
    let first_line = text.trim().lines().next().unwrap_or_default();
    if first_line.is_empty() {
        return "AI".into();
    }
    let summary: String = first_line.chars().take(64).collect();
    format!("AI: {summary}")
}

pub fn begin(editor: &mut Editor, label: &str) -> Result<Checkpoint, EditError> {
    let start = editor.checkpoint();
    editor.begin_group(label)?;
    Ok(start)
}

pub fn finish(editor: &mut Editor, start: Checkpoint) -> Result<Turn, EditError> {
    let summary = editor.end_group()?;
    Ok(Turn {
        start,
        end: editor.checkpoint(),
        summary,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Restore {
    pub steps: Vec<String>,
    pub needs_confirmation: bool,
    pub applied: bool,
}

/// Callers keep the editor locked from planning through application. Confirm against the
/// observed revision so a newly interleaved user step always requires a fresh decision.
pub fn restore_checkpoint(
    editor: &mut Editor,
    point: &Checkpoint,
    expected_steps: usize,
    dry_run: bool,
    confirmed_rev: Option<u64>,
) -> Result<Restore, EditError> {
    let steps = editor.steps_since(point)?;
    let needs_confirmation =
        steps.len() > expected_steps && confirmed_rev != Some(editor.checkpoint().rev);
    let applied = !dry_run && !needs_confirmation && !steps.is_empty();
    if applied {
        for _ in &steps {
            editor
                .undo()
                .ok_or_else(|| EditError::new("invalid", "checkpoint is no longer reachable"))?;
        }
    }
    Ok(Restore {
        steps,
        needs_confirmation,
        applied,
    })
}

pub fn revert_turn(
    editor: &mut Editor,
    turn: &Turn,
    dry_run: bool,
    confirmed_rev: Option<u64>,
) -> Result<Restore, EditError> {
    if turn.end.depth == turn.start.depth {
        return Ok(Restore {
            steps: Vec::new(),
            needs_confirmation: false,
            applied: false,
        });
    }
    // The turn itself must still be on this branch; a new user edit could have replaced it.
    editor.steps_since(&turn.end)?;
    restore_checkpoint(
        editor,
        &turn.start,
        turn.end.depth - turn.start.depth,
        dry_run,
        confirmed_rev,
    )
}
