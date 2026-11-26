//! Page-panel edits. Raw XML fragments retain unknown properties and page-owned items.
//! Measurements: next/oracle/goldens/pages (InDesign 2025).
use super::*;
use std::collections::BTreeSet;
mod fontfit;
mod geometry;
mod layoutadjust;
mod movepage;
mod pagesize;
pub(crate) use geometry::set_page_geometry;
pub(crate) use movepage::move_page;
pub(crate) use pagesize::set_page_size;
pub use pagesize::{PAGE_SIZE_PRESETS, page_size_of_preset, page_size_preset_of};
pub(crate) use layoutadjust::adjust_layout;
pub use layoutadjust::{LayoutAdjustSettings, auto_margins};

mod page_xml;
mod spread;
mod book;
mod page_layout;
mod sections;
mod page_commands;

use page_xml::{scan, get, set, nums, bounds, mat, shift, element, rewrite};
use spread::{Page, Spread};
use book::Book;
pub(super) use page_layout::{copy_layout, set_pasteboard};
pub(crate) use page_layout::rotate_view;
pub(crate) use sections::{set_book_start, delete_section};
pub(super) use page_commands::{run, run_resizing};
