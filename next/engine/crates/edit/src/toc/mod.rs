//! Table of contents editing, in the same transaction as its style and frame.
use crate::xml::fragment as xml;
mod styles;
mod content;
mod story;
mod links;
mod import;
pub(crate) use import::import;
pub(crate) use styles::{save, delete};
pub(crate) use story::{create, update};

impl crate::Editor {
    pub fn toc_styles(&self) -> Result<Vec<folio_model::toc::TocStyle>, crate::EditError> {
        styles::all(self.parts())
    }
}
