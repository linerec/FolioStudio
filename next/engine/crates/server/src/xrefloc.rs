//! Cross-reference commands receive page and paragraph-number locations from this session's composed document
//! (as the TOC does): the engine does not compose while it edits.
use folio_edit::{Command, commands::RegisteredCommand};

pub fn prepare(commands: &mut [Command], session: &crate::Session) -> Result<(), String> {
    let uses = |c: &Command| matches!(c, Command::Registered(RegisteredCommand::InsertCrossReference { .. } | RegisteredCommand::SetCrossReference { .. } | RegisteredCommand::UpdateCrossReference { .. } | RegisteredCommand::SaveCrossReferenceFormat { .. }));
    if let Some(at) = commands.iter().position(|c| matches!(c, Command::Registered(RegisteredCommand::GenerateIndex { .. }))) {
        // only the choice of the generated paragraph styles may come before it (one undo step with the generation)
        let styles_only = commands[..at].iter().all(|c| matches!(c, Command::Registered(RegisteredCommand::SetIndexGenerationStyles { .. })));
        if at + 1 != commands.len() || !styles_only { return Err("색인 생성은 다른 편집과 분리해서 실행하십시오".into()); }
        // (overset text has no page: its markers print no page number, measured in index6-overset)
        if let Command::Registered(RegisteredCommand::GenerateIndex { locations, .. }) = &mut commands[at] {
            *locations = folio_text::layout::toc::locations(&session.doc, &session.layout);
        }
        return Ok(());
    }
    if !commands.iter().any(uses) { return Ok(()) }
    let found = folio_text::layout::toc::sources(&session.doc, &session.layout);
    for c in commands {
        if let Command::Registered(RegisteredCommand::InsertCrossReference { locations, .. } | RegisteredCommand::SetCrossReference { locations, .. } | RegisteredCommand::UpdateCrossReference { locations, .. } | RegisteredCommand::SaveCrossReferenceFormat { locations, .. }) = c {
            *locations = found.clone();
        }
    }
    Ok(())
}
