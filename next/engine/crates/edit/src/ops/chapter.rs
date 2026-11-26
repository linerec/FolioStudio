use crate::{EditError, Editor, Work, xml::Index};
use folio_idml::chapter::PART;

pub(crate) fn set(w: &mut Work<'_>, number: &u32, source: &str) -> Result<(), EditError> {
    if *number == 0 || !matches!(source, "UserDefined" | "ContinueFromPreviousDocument") {
        return Err(EditError::new("invalid", "chapter number or source"));
    }
    for (attr, value) in [("ChapterNumber", number.to_string()), ("ChapterNumberSource", source.to_owned())] {
        let t = w.text(PART)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let edit = if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "ChapterNumberPreference") {
            if ix.attr(&t, i, attr) == Some(value.as_str()) { continue; }
            crate::ops::attr_edit(&ix, &t, i, attr, &value)
        } else {
            let root = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Preferences").ok_or_else(|| EditError::new("xml", "missing Preferences"))?;
            let at = ix.nodes[root].close_start;
            (at..at, format!("<ChapterNumberPreference {attr}=\"{value}\" />"))
        };
        w.splice(PART, vec![edit])?;
    }
    let stories: Vec<_> = w.ed.parts.keys().filter_map(|p| p.strip_prefix("Stories/Story_")?.strip_suffix(".xml").map(str::to_owned)).collect();
    for story in stories { w.recompose(&story); }
    Ok(())
}

impl Editor {
    pub fn chapter_number(&self) -> Result<(String, String), String> {
        folio_idml::chapter::read(self.part(PART).ok_or("missing Preferences")?)
    }

    pub fn update_book_chapter(&mut self, previous: Option<u32>) -> Result<u32, String> {
        let (number, source) = self.chapter_number()?;
        let number = match source.as_str() {
            "UserDefined" => number.parse::<u32>().map_err(|e| e.to_string())?,
            "ContinueFromPreviousDocument" => previous.unwrap_or(0).checked_add(1).ok_or("chapter overflow")?,
            _ => return Err(format!("unsupported chapter source: {source}")),
        };
        self.apply("chapter", &[crate::commands::RegisteredCommand::SetChapterNumber { number, source }.into()]).map_err(|e| e.to_string())?;
        Ok(number)
    }
}
