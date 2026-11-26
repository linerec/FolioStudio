//! Type ▸ Create Outlines: the editor knows no composition, so the host measures the glyph shapes of the frames
//! (positions from the composition, outlines from the fonts; `folio_text::outlines::frame_shapes`) and hands them
//! over in `shapes`. Rules and measurements: next/oracle/rules/outlines.toml.
use folio_edit::{Command, commands::RegisteredCommand};

pub fn prepare(commands: &mut [Command], s: &crate::Session) -> Result<(), String> {
    for command in commands.iter_mut() {
        if let Command::Registered(RegisteredCommand::CreateOutlines { ids, shapes } | RegisteredCommand::CreateOutlinesKeepText { ids, shapes }) = command {
            let all = folio_text::outlines::frames_to_convert(&s.doc, ids).iter().map(|id| folio_text::outlines::frame_shapes(&s.doc, &s.layout, &s.fonts, id)).collect::<Result<Vec<_>, _>>()?;
            *shapes = serde_json::to_string(&all).map_err(|e| e.to_string())?;
        }
        if let Command::Registered(RegisteredCommand::CreateOutlinesRange { story, start, end, shapes }) = command {
            let all = folio_text::outlines::range_shapes(&s.doc, &s.layout, &s.fonts, story, *start, *end)?;
            *shapes = serde_json::to_string(&all).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
