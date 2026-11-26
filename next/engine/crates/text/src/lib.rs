//! Fonts, shaping and frame text layout for Folio Studio.
//!
//! - [`FontStore`]: font discovery (system, user, Adobe Fonts and document folders),
//!   matching of InDesign family + style names, memory-mapped faces, glyph outlines
//!   ([`folio_model::layout::GlyphSource`]). Internally synchronised (`Send + Sync`).
//! - [`shape`]: rustybuzz shaping of paragraphs into composer clusters, with InDesign's
//!   tracking, horizontal scale, case and position rules.
//! - [`layout`]: frame chains, columns, baselines and line breaking with `folio_compose`.

pub mod autosize;
pub mod fontfit;
pub mod fonts;
pub mod glyphs;
pub mod glyphfill;
pub mod opentype;
pub mod optical_margin;
pub mod outlines;
mod textpath;
pub mod textpath_handles;
pub mod layout;
pub mod shape;
pub mod vertical;

pub use autosize::{Fit, fit_frames};
pub use fonts::{FaceInfo, FaceMetrics, Faces, FontSource, FontStore, Resolved, Writing};
pub use layout::{
    DocumentFrames, PageFrameText, StoryLayout, format_number, layout_document, layout_document_with_threads,
    layout_master_page_numbers, layout_story, text_variable_values, variable_results,
};
pub use layout::filevars::iso_from_unix;
