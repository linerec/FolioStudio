//! Live effects as InDesign writes them for PDF/X-4: the object's silhouette is rasterised
//! (300 ppi) into an 8-bit alpha and used as a luminosity soft mask — behind the object for
//! drop shadows and outer glows, over it for inner shadows, glows, satin and bevels, or on
//! the object's own group for feathers. The rasteriser is the screen's
//! (`folio_render::effects`), so both show the same effect.

pub use folio_render::effects::*;
