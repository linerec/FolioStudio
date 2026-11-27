use crate::{Session, edit, requests::Context};
use folio_ai::turn::{self, Turn};
use folio_edit::Checkpoint;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, Weak};

pub struct OpenGroup {
    shared: edit::Shared,
    document: String,
    start: Checkpoint,
}
pub struct Completed {
    shared: Weak<Mutex<edit::EditState>>,
    document: String,
    turn: Turn,
}
pub fn include(
    groups: &mut Vec<OpenGroup>,
    s: Option<&Session>,
    label: &str,
) -> Result<(), String> {
    let Some((s, shared)) = s.and_then(|s| Some((s, s.edit.as_ref()?))) else {
        return Ok(());
    };
    if groups.iter().any(|g| Arc::ptr_eq(&g.shared, shared)) {
        return Ok(());
    }
    let start = turn::begin(
        &mut shared.lock().map_err(|_| "editor unavailable")?.editor,
        label,
    )
    .map_err(|e| e.to_string())?;
    groups.push(OpenGroup {
        shared: shared.clone(),
        document: super::document_id(s),
        start,
    });
    Ok(())
}
pub fn finish(groups: Vec<OpenGroup>) -> Result<Vec<Completed>, String> {
    groups
        .into_iter()
        .map(|g| {
            let turn = turn::finish(
                &mut g.shared.lock().map_err(|_| "editor unavailable")?.editor,
                g.start,
            )
            .map_err(|e| e.to_string())?;
            Ok(Completed {
                shared: Arc::downgrade(&g.shared),
                document: g.document,
                turn,
            })
        })
        .collect()
}
pub fn cards(completed: &[Completed]) -> Vec<Value> {
    completed.iter().map(|g|json!({"document":g.document,"start":g.turn.start,"end":g.turn.end,"summary":g.turn.summary})).collect()
}
pub fn restore(ctx: &Context<'_>, groups: &[Completed], req: &Value) -> Result<Value, String> {
    let mut session = ctx.session.lock().map_err(|_| "document unavailable")?;
    let s = session.as_ref().ok_or("no document open")?;
    let shared = s.edit.as_ref().ok_or("document is read-only")?;
    let current = groups
        .iter()
        .find(|g| Weak::ptr_eq(&g.shared, &Arc::downgrade(shared)))
        .ok_or("checkpoint document is no longer open")?;
    let turn = &current.turn;
    let mut state = shared.lock().map_err(|_| "editor unavailable")?;
    let rev = state.editor.checkpoint().rev;
    let confirm = req["confirmedRev"].as_u64();
    let plan = if req["cmd"] == "aiRevertTurn" {
        turn::revert_turn(&mut state.editor, turn, true, confirm)
    } else {
        let (point, expected) = match req["edge"].as_str().unwrap_or("start") {
            "start" => (
                &turn.start,
                if state.editor.steps_since(&turn.end).is_ok() {
                    turn.end.depth - turn.start.depth
                } else {
                    0
                },
            ),
            "end" => (&turn.end, 0),
            _ => return Err("unknown checkpoint edge".into()),
        };
        turn::restore_checkpoint(&mut state.editor, point, expected, true, confirm)
    }
    .map_err(|e| e.to_string())?;
    drop(state);
    let mut applied = 0;
    if req["dryRun"] != true && !plan.needs_confirmation {
        for _ in &plan.steps {
            let current = session.as_ref().ok_or("no document open")?;
            let (next, header) = edit::run(&json!({"cmd":"undo"}), current)
                .map_err(|e| e["error"].as_str().unwrap_or("undo failed").to_owned())?;
            if let Some(next) = next {
                crate::send(
                    ctx.out,
                    json!({"event":"external-edit","path":next.source_path,"detail":{"source":"ai","edit":header}}),
                    &[],
                );
                *session = Some(Arc::new(next));
            }
            applied += 1;
        }
    }
    Ok(
        json!({"document":current.document,"rev":rev,"steps":plan.steps,"needsConfirmation":plan.needs_confirmation,"applied":applied>0}),
    )
}
