use crate::{EditError, Editor, Work};
use std::collections::BTreeMap;

pub(super) fn place(w: &mut Work<'_>, parts: &BTreeMap<String, String>, ids: &[String], page: &str) -> Result<(), EditError> {
    let mut source = Editor::new(parts.clone());
    source.copy(ids)?;
    let previous = w.ed.clipboard.take();
    w.ed.clipboard = source.clipboard.take();
    // a library item keeps an image's "None" profile (probe bookx-c4-page-picture), where Paste makes it the document default
    if let Some(clip) = w.ed.clipboard.as_mut() { clip.keep_profile = true; }
    let result = crate::ops::clipboard::paste(w, Some(page), None, None, None, false);
    w.ed.clipboard = previous;
    result
}

/// Library item placed in text (`Asset.placeAsset(Text)`): the item's one object becomes an inline anchored object over the characters
/// `start..end` of `story` (an insertion point when they are equal).
pub(super) fn place_in_text(w: &mut Work<'_>, parts: &BTreeMap<String, String>, ids: &[String], story: &str, start: &usize, end: &usize) -> Result<(), EditError> {
    let mut source = Editor::new(parts.clone());
    source.copy(ids)?;
    let previous = w.ed.clipboard.take();
    w.ed.clipboard = source.clipboard.take();
    let result = crate::ops::inline_paste::run(w, story, *start, *end);
    w.ed.clipboard = previous;
    result
}

impl Editor {
    pub fn library_snapshot(&self, ids: &[String]) -> Result<folio_model::library::Snapshot, EditError> {
        let mut source = Editor::new(self.parts().clone());
        source.copy(ids)?;
        Ok(folio_model::library::Snapshot { parts: self.parts().clone(), ids: ids.to_vec(), links: Default::default() })
    }
}
