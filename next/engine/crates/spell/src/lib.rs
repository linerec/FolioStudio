//! Spell checking for Edit > Spelling: Hunspell dictionaries (pure Rust reader), the user
//! dictionary rules and the autocorrect rules. No file or OS access: the dictionaries are
//! compiled in and read the first time they are needed.
//!
//! What matches InDesign (measured, see `artifacts/spell-c1-20261023/STATUS.ko.md`):
//! the user dictionary rules (`user`), the option names and defaults (`Options`), and the
//! autocorrect table rules (`autocorrect`). What cannot: which words a dictionary calls wrong.
//! InDesign uses Hunspell for English and has no Korean dictionary at all, so checking Korean is
//! Folio's own addition.
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod autocorrect;
#[cfg(test)]
mod robustness_tests;
mod check;
mod dict;
pub mod user;
mod words;

pub use check::{check_text, is_known_word, suggest, Issue, IssueKind, Options, Run};
pub use dict::{Bundled, Dictionaries, Lang};
pub use user::{UserDictionaries, UserDictionary};
