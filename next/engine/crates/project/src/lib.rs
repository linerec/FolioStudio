//! Folio projects and the `.folio` document format (principle 1, "Folio project").
//!
//! A project is a folder with `folio.project` at its root ([`manifest`]). Everything a
//! document refers to is stored as a root-relative, `/`-separated, NFC path with a SHA-256
//! and a size ([`paths`], [`hash`]), so the whole folder can be shared on a NAS or copied to
//! another drive, depth, name or OS and still open with every link found. Machine-specific
//! things (real folders of named locations, window state, recent lists, user names) never
//! go into the project; they are app settings keyed by the project id.
//!
//! ```text
//! <project>/
//!   folio.project            id, name, created, settings, named-location names
//!   <document>.folio         IDML parts + folio/ parts (see [`format`])
//!   assets/<document>/…      imported links, extracted embedded files, previews
//!   fonts/…                  font files the licences allow to copy
//!   .folio/cache/            regenerable only (hash index, import staging), hash-named
//!   .folio/locks/            one lock file per open document (NAS sharing)
//! ```
//!
//! Layers: this crate owns the rules and the file formats; the engine (`folio-engine`) runs
//! the INDD reader, builds the model and calls [`import`], [`open`] and [`export`].

pub mod atomic;
pub mod conflicts;
pub mod docfind;
pub mod export;
pub mod format;
pub mod hash;
pub mod idmlparts;
pub mod import;
pub mod limits;
pub mod lock;
pub mod manifest;
pub mod newdoc;
pub mod newfile;
pub mod open;
pub mod paths;
pub mod presets;
pub mod tree;
pub mod util;
pub mod library;
pub mod book;
pub mod book_pagination;

pub use manifest::{Project, find_root};
pub use paths::RelPath;
