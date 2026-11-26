//! Measured style application and direct formatting; see oracle text-format cases.
use super::*;
use crate::xml::escape;
use serde_json::Value;
use std::collections::BTreeMap;
pub(crate) mod styles;
pub(crate) mod break_link;
pub(crate) mod clear_overrides;
pub(crate) mod find;
pub(crate) mod glyph;
pub(crate) mod spell;
mod opentype;
mod paint;
mod columns;
mod ranges;
pub(crate) mod conditions;
pub(crate) mod listconvert;
pub(crate) mod frame_text;
pub(crate) mod swap_paint;
pub(crate) mod scale_text;

mod properties;
mod selection;
mod style_application;
mod controls;
mod direct_format;
mod list_format;
mod clipboard;
#[cfg(test)]
mod tests;

use properties::{Property, Attrs, attr, property, read_attrs, style_attrs, make_style, val, equal, normalize, Catalog, META};
pub(super) use properties::{note_styles, rebase_character, with_attr, without_attr, object_style, typing_style};
use selection::{paragraph_range, inherit, validate};
pub(crate) use selection::info;
pub(crate) use style_application::{apply_style, apply_style_variant, apply_style_to_story};
use controls::{direct, controls, validate_paragraph_options, control_key, api_value};
use direct_format::format_story_range;
pub(crate) use direct_format::format;
pub(crate) use list_format::{toggle_list, toggle_numbering_restart};
pub(crate) use clipboard::{copy, paste};
pub use clipboard::TextFragment;
