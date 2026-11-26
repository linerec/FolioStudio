//! File ▸ Adjust Layout with "Adjust font size": the editor knows no composition, so the host measures the text frames (lines of each paragraph at the old
//! size, the width of the column, the width of each last line: `folio_text::fontfit`) and hands them over in `fit_metrics`.
//! Rules and measurements: next/oracle/rules/layoutadj-c7.ko.md.
use folio_edit::{Command, commands::RegisteredCommand};

pub fn prepare(commands: &mut [Command], s: &crate::Session) -> Result<(), String> {
    for command in commands.iter_mut() {
        if let Command::Registered(RegisteredCommand::AdjustLayout { adjust_font_size: Some(true), fit_metrics, .. }) = command {
            let fits = folio_text::fontfit::frame_fits(&s.doc, &s.layout, &s.fonts);
            *fit_metrics = Some(serde_json::to_string(&fits).map_err(|e| e.to_string())?);
        }
    }
    Ok(())
}
