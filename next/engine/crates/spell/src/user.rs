//! The user dictionary of one language: words the person added, and words the dictionary knows
//! but the person marked as wrong.
//!
//! Rules measured in InDesign 2025 (DOM `app.userDictionaries[i].addWord / removeWord`, probes in
//! `artifacts/spell-c1-20261023/measure/`):
//! - `addWord` takes a list of strings. Each is split at white space, lower-cased, and put in
//!   `addedWords`, which stays sorted (by code point) and has no duplicates. Empty strings are
//!   ignored. Hyphens, apostrophes and digits stay (`well-known`, `don't`, `abc123`).
//! - `removeWord` takes a list of strings and removes each from `addedWords` only when it equals
//!   a stored word exactly: no lower-casing, no splitting (`removeWord(["ALLCAPS"])` did not
//!   remove `allcaps`; `"zz probe"` did not remove `zz` or `probe`).
//! - Check Spelling's `Add` keeps the word as typed when its "Case Sensitive" box is on (default)
//!   and lower-cases it when off (InDesign 2026, read with `addedWords`: `Qqx`, `zzy`).
//! - One user dictionary per language name (60 of them in InDesign); a language has no
//!   dictionary of ours unless it is in the list the engine knows.
//! - `removedWords` (dictionary words the person excluded) could not be filled through the DOM
//!   (`removeWord` on a word of the main dictionary left it empty), so how InDesign fills it is
//!   not measured. Folio's rule: `exclude_words` lower-cases and sorts like `addedWords`.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserDictionary {
    pub added_words: BTreeSet<String>,
    pub removed_words: BTreeSet<String>,
}

fn lowered(words: &[String]) -> impl Iterator<Item = String> + '_ {
    words.iter().flat_map(|w| w.split_whitespace()).map(|w| w.to_lowercase())
}

impl UserDictionary {
    /// InDesign `addWord`.
    pub fn add_words(&mut self, words: &[String]) {
        for w in lowered(words) {
            // A word added again stops being an excluded one.
            self.removed_words.remove(&w);
            self.added_words.insert(w);
        }
    }
    /// Check Spelling's Add with "Case Sensitive" on (InDesign 2026): the word is kept as typed
    /// (`Qqx`), split at white space like `addWord`. With it off the dialog lower-cases like `add_words`.
    pub fn add_words_exact(&mut self, words: &[String]) {
        for w in words.iter().flat_map(|w| w.split_whitespace()) {
            self.removed_words.remove(&w.to_lowercase());
            self.added_words.insert(w.to_owned());
        }
    }
    /// InDesign `removeWord`: only exact matches of stored words go.
    pub fn remove_words(&mut self, words: &[String]) {
        for w in words {
            self.added_words.remove(w);
        }
    }
    /// Folio: mark dictionary words as wrong (the "Removed Words" list).
    pub fn exclude_words(&mut self, words: &[String]) {
        for w in lowered(words) {
            self.added_words.remove(&w);
            self.removed_words.insert(w);
        }
    }
    /// Folio: take words off the "Removed Words" list.
    pub fn include_words(&mut self, words: &[String]) {
        for w in words {
            self.removed_words.remove(w);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.added_words.is_empty() && self.removed_words.is_empty()
    }
}

/// Language name (InDesign's, without `$ID/`) to its user dictionary.
pub type UserDictionaries = BTreeMap<String, UserDictionary>;

pub fn language_key(name: &str) -> &str {
    name.trim().trim_start_matches("$ID/")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn words(d: &UserDictionary) -> Vec<&str> {
        d.added_words.iter().map(String::as_str).collect()
    }

    /// The sequence measured in InDesign (Telugu dictionary, probe-spell3a.jsx).
    #[test]
    fn add_and_remove_follow_the_measured_rules() {
        let mut d = UserDictionary::default();
        d.add_words(&s(&["zzfolioprobe"]));
        d.add_words(&s(&["ZZFolioProbe"]));
        d.add_words(&s(&["zzfolioprobe"]));
        assert_eq!(words(&d), ["zzfolioprobe"]);
        d.remove_words(&s(&["zzfolioprobe"]));
        assert!(d.is_empty());
        d.add_words(&s(&["zzabsentprobe"]));
        d.add_words(&s(&[""]));
        d.add_words(&s(&["zz probe"]));
        assert_eq!(words(&d), ["probe", "zz", "zzabsentprobe"]);
        d.remove_words(&s(&["zz probe"]));
        assert_eq!(words(&d), ["probe", "zz", "zzabsentprobe"]);
        d.remove_words(&s(&["zzabsentprobe"]));
        d.add_words(&s(&["zzpa", "zzpb"]));
        assert_eq!(words(&d), ["probe", "zz", "zzpa", "zzpb"]);
    }

    #[test]
    fn words_are_lowercased_sorted_and_removal_is_case_sensitive() {
        let mut d = UserDictionary::default();
        d.add_words(&s(&["Capitalized", "ALLCAPS", "zeta", "Alpha", "beta"]));
        assert_eq!(words(&d), ["allcaps", "alpha", "beta", "capitalized", "zeta"]);
        d.remove_words(&s(&["capitalized", "ALLCAPS", "Alpha"]));
        assert_eq!(words(&d), ["allcaps", "alpha", "beta", "zeta"]);
        d.add_words(&s(&["well-known", "don't", "abc123", "12345"]));
        assert_eq!(words(&d), ["12345", "abc123", "allcaps", "alpha", "beta", "don't", "well-known", "zeta"]);
    }

    #[test]
    fn excluded_words_are_kept_apart_from_added_ones() {
        let mut d = UserDictionary::default();
        d.add_words(&s(&["foo"]));
        d.exclude_words(&s(&["Foo", "Bar"]));
        assert!(d.added_words.is_empty());
        assert_eq!(d.removed_words.iter().collect::<Vec<_>>(), ["bar", "foo"]);
        d.add_words(&s(&["bar"]));
        assert_eq!(d.removed_words.iter().collect::<Vec<_>>(), ["foo"]);
    }
}
