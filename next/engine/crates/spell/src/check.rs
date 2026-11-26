//! Finding the words to flag in a story's text.
//!
//! Option names and defaults are InDesign's `app.spellPreferences` (measured: the four checks are
//! on by default). Which words a dictionary calls wrong is the dictionary's, not InDesign's:
//! InDesign checks English with Hunspell and does not check Korean at all (its language list has
//! no Korean). Folio checks Hangul words with the Korean dictionary and Latin words with the
//! dictionary of the run's language; words in any other script, words with digits, and words that
//! mix scripts are not judged.
use crate::dict::{Dictionaries, Lang};
use crate::user::{language_key, UserDictionaries};
use crate::words::{capitalize, ends_sentence, is_break, is_hangul, tokenize, Token};
use serde::{Deserialize, Serialize};
use spellbook::Dictionary;

/// `app.spellPreferences` check switches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    pub check_misspelled_words: bool,
    pub check_repeated_words: bool,
    pub check_capitalized_words: bool,
    pub check_capitalized_sentences: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { check_misspelled_words: true, check_repeated_words: true, check_capitalized_words: true, check_capitalized_sentences: true }
    }
}

/// The language of a stretch of the text (scalar offsets, `end` excluded).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub language: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    Misspelled,
    Repeated,
    UncapitalizedWord,
    UncapitalizedSentence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub kind: IssueKind,
    /// The text to replace: the word, or for a repeated word both words and what lies between them
    /// (InDesign selects `the the` and offers `the`).
    pub start: usize,
    pub end: usize,
    pub word: String,
    pub language: String,
    pub suggestions: Vec<String>,
}

enum Script {
    Hangul,
    Latin,
    Skip,
}

fn script(word: &str) -> Script {
    let (mut hangul, mut latin, mut other) = (false, false, false);
    for c in word.chars() {
        if is_hangul(c) {
            hangul = true;
        } else if c.is_numeric() {
            return Script::Skip;
        } else if c.is_alphabetic() {
            // Latin letters (with accents) are those below U+0250 plus the Latin Extended blocks.
            if (c as u32) < 0x250 || matches!(c, '\u{1e00}'..='\u{1eff}') {
                latin = true;
            } else {
                other = true;
            }
        }
    }
    match (hangul, latin, other) {
        (true, false, false) => Script::Hangul,
        (false, true, false) => Script::Latin,
        _ => Script::Skip,
    }
}

fn language_at(runs: &[Run], at: usize) -> Option<&str> {
    runs.iter().find(|r| r.start <= at && at < r.end).map(|r| r.language.as_str())
}

fn normalized(word: &str) -> String {
    word.replace('\u{2019}', "'").replace('\u{2010}', "-")
}

fn accepts(dict: &Dictionary, word: &str) -> bool {
    if dict.check(word) {
        return true;
    }
    word.contains('-') && word.split('-').filter(|p| !p.is_empty()).all(|p| dict.check(p))
}

/// Whether the dictionary of `language` calls `word` a word (a word of a script no dictionary covers: no). Autocorrect capitalizes the first
/// word of a sentence only when the dictionary knows it (measured in InDesign 2025: `hello` becomes `Hello`, `zqfoo` stays).
pub fn is_known_word(dicts: &dyn Dictionaries, language: &str, word: &str) -> bool {
    let word = normalized(word);
    let lang = match script(&word) {
        Script::Hangul => Some(Lang::KoKr),
        Script::Latin => Lang::from_name(language),
        Script::Skip => None,
    };
    lang.and_then(|l| dicts.get(l)).is_some_and(|dict| accepts(dict, &word))
}

/// Suggestions for one word (the dictionary's own list, at most `limit`).
pub fn suggest(dicts: &dyn Dictionaries, language: &str, word: &str, limit: usize) -> Vec<String> {
    let lang = if word.chars().any(is_hangul) { Some(Lang::KoKr) } else { Lang::from_name(language) };
    let Some(dict) = lang.and_then(|l| dicts.get(l)) else { return Vec::new() };
    let mut out = Vec::new();
    dict.suggest(&normalized(word), &mut out);
    out.truncate(limit);
    out
}

/// Check one story's text. `text` is the story's characters (paragraph breaks as `\r`); `runs` give
/// the language of each stretch (words outside every run use `default_language`). `ignored` are
/// words the person chose to skip for the whole check (lower-case). Suggestions are only filled in
/// when `with_suggestions` is set: asking the dictionary for them takes milliseconds per word.
// Keep the public check API shared by engine callers; each argument is an independent input.
#[allow(clippy::too_many_arguments)]
pub fn check_text(
    dicts: &dyn Dictionaries,
    text: &str,
    runs: &[Run],
    default_language: &str,
    options: &Options,
    user: &UserDictionaries,
    ignored: &[String],
    with_suggestions: bool,
) -> Vec<Issue> {
    let chars: Vec<char> = text.chars().collect();
    let tokens = tokenize(&chars);
    let mut issues = Vec::new();
    let mut sentence_start = true;
    let mut previous: Option<&Token> = None;
    for token in &tokens {
        // What lies between the previous word and this one decides sentence starts and repeats.
        let gap_start = previous.map_or(0, |p| p.end);
        let gap = &chars[gap_start..token.start];
        if gap.iter().any(|&c| is_break(c) || ends_sentence(c)) {
            sentence_start = true;
        }
        let same_paragraph = !gap.iter().any(|&c| is_break(c));
        let spaces_only = !gap.is_empty() && gap.iter().all(|c| c.is_whitespace() && !is_break(*c));
        let at_sentence_start = sentence_start;
        sentence_start = false;

        let word = normalized(&token.text);
        let lower = word.to_lowercase();
        let language = language_at(runs, token.start).unwrap_or(default_language);
        let lang = match script(&word) {
            Script::Hangul => Some(Lang::KoKr),
            Script::Latin => Lang::from_name(language),
            Script::Skip => None,
        };
        let repeated = options.check_repeated_words
            && same_paragraph
            && spaces_only
            // InDesign 2026 does not call `The the` or `Cat cat` a repeat: the two words must be spelled alike
            && previous.is_some_and(|p| p.text == token.text);
        let prior = previous;
        previous = Some(token);
        let Some(lang) = lang else { continue };
        if ignored.contains(&lower) {
            continue;
        }
        let user_dict = user.get(language_key(lang.name()));
        let mut push = |kind: IssueKind, start: usize, suggestions: Vec<String>| {
            issues.push(Issue { kind, start, end: token.end, word: token.text.clone(), language: lang.name().to_owned(), suggestions });
        };
        // A word added with "Case Sensitive" is kept as typed and only matches itself; the others are kept lower-cased.
        let added = user_dict.is_some_and(|u| u.added_words.contains(&lower) || u.added_words.contains(&word));
        let excluded = user_dict.is_some_and(|u| u.removed_words.contains(&lower));
        let Some(dict) = dicts.get(lang) else { continue };
        let known = !excluded && (added || accepts(dict, &word));
        if !known {
            if options.check_misspelled_words || options.check_capitalized_words {
                let lowercase_start = word.chars().next().is_some_and(char::is_lowercase);
                let capitalized = capitalize(&word);
                if !excluded && lowercase_start && accepts(dict, &capitalized) {
                    // Only the capitalized spelling is in the dictionary (a proper noun).
                    if options.check_capitalized_words {
                        push(IssueKind::UncapitalizedWord, token.start, vec![capitalized]);
                    }
                } else if options.check_misspelled_words {
                    let list = if with_suggestions { suggest(dicts, lang.name(), &word, 8) } else { Vec::new() };
                    push(IssueKind::Misspelled, token.start, list);
                }
            }
            continue;
        }
        if repeated {
            let first = prior.expect("a repeat has a word before it");
            let both: String = chars[first.start..token.end].iter().collect();
            issues.push(Issue { kind: IssueKind::Repeated, start: first.start, end: token.end, word: both, language: lang.name().to_owned(), suggestions: vec![first.text.clone()] });
        } else if options.check_capitalized_sentences
            && at_sentence_start
            && matches!(lang, Lang::EnUs)
            && word.chars().next().is_some_and(char::is_lowercase)
        {
            push(IssueKind::UncapitalizedSentence, token.start, vec![capitalize(&token.text)]);
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    struct Tiny;
    impl Dictionaries for Tiny {
        fn get(&self, lang: Lang) -> Option<&Dictionary> {
            static EN: OnceLock<Dictionary> = OnceLock::new();
            static KO: OnceLock<Dictionary> = OnceLock::new();
            Some(match lang {
                Lang::EnUs => EN.get_or_init(|| {
                    Dictionary::new("SET UTF-8\n", "9\nthe\nquick\nbrown\nfox\njumps\nParis\nhello\nworld\nis\n").unwrap()
                }),
                Lang::KoKr => KO.get_or_init(|| Dictionary::new("SET UTF-8\n", "2\n한글\n사전\n").unwrap()),
            })
        }
    }

    fn run(text: &str, options: &Options) -> Vec<(IssueKind, String, usize, usize)> {
        check_text(&Tiny, text, &[], "English: USA", options, &UserDictionaries::new(), &[], false)
            .into_iter()
            .map(|i| (i.kind, i.word, i.start, i.end))
            .collect()
    }

    #[test]
    fn flags_misspelled_words_with_scalar_offsets() {
        let found = run("The quikc fox. 한글 사전 한굴", &Options::default());
        assert_eq!(found, [
            (IssueKind::Misspelled, "quikc".into(), 4, 9),
            (IssueKind::Misspelled, "한굴".into(), 21, 23),
        ]);
    }

    /// InDesign 2026 (Check Spelling): `the the` is one Repeated Word item that selects both words and
    /// offers `the`; `The the` and `Cat cat` are not repeats.
    #[test]
    fn repeated_words_cover_both_words_and_are_case_sensitive() {
        let found = run("Hello the the fox", &Options::default());
        assert_eq!(found, [(IssueKind::Repeated, "the the".into(), 6, 13)]);
        // a lower-case sentence start is reported first, as InDesign does for `a a`
        assert_eq!(run("the the fox", &Options::default())[0].0, IssueKind::UncapitalizedSentence);
        let found = check_text(&Tiny, "Is fox  fox", &[], "English: USA", &Options::default(), &UserDictionaries::new(), &[], false);
        assert_eq!((found[0].start, found[0].end, found[0].word.as_str(), found[0].suggestions.clone()), (3, 11, "fox  fox", vec!["fox".to_string()]));
        // not when spelled differently in case, nor across a paragraph break or a punctuation mark
        assert!(run("Hello The the fox", &Options::default()).is_empty());
        assert!(run("the
the fox", &Options::default()).iter().all(|f| f.0 != IssueKind::Repeated));
        assert!(run("the, the fox", &Options::default()).iter().all(|f| f.0 != IssueKind::Repeated));
        let off = Options { check_repeated_words: false, ..Options::default() };
        assert!(run("Hello the the fox", &off).is_empty());
    }

    #[test]
    fn capital_checks_follow_their_options() {
        // "paris" is only in the dictionary as "Paris"; "fox" starts a sentence in lower case
        let found = run("Hello paris. fox jumps", &Options::default());
        assert_eq!(found, [
            (IssueKind::UncapitalizedWord, "paris".into(), 6, 11),
            (IssueKind::UncapitalizedSentence, "fox".into(), 13, 16),
        ]);
        let off = Options { check_capitalized_words: false, check_capitalized_sentences: false, ..Options::default() };
        assert!(run("Hello paris. fox jumps", &off).is_empty());
        // the first word of the text and of a paragraph start a sentence too
        assert_eq!(run("hello world\rfox", &Options::default())[0].1, "hello");
    }

    #[test]
    fn a_case_sensitive_user_word_matches_only_as_typed() {
        let mut user = UserDictionaries::new();
        user.entry("English: USA".into()).or_default().add_words_exact(&["Quikc".into()]);
        let words = |text: &str| -> Vec<String> {
            check_text(&Tiny, text, &[], "English: USA", &Options::default(), &user, &[], false).into_iter().map(|i| i.word).collect()
        };
        assert!(words("The Quikc fox").is_empty());
        assert_eq!(words("The quikc fox"), ["quikc"]);
    }

    #[test]
    fn misspelled_check_can_be_turned_off_and_user_words_are_accepted() {
        let off = Options { check_misspelled_words: false, ..Options::default() };
        assert!(run("The quikc fox", &off).is_empty());
        let mut user = UserDictionaries::new();
        user.entry("English: USA".into()).or_default().add_words(&["Quikc".into()]);
        let found = check_text(&Tiny, "The quikc fox", &[], "English: USA", &Options::default(), &user, &[], false);
        assert!(found.is_empty());
        // excluded dictionary words are wrong, and the person can skip a word for this check
        user.get_mut("English: USA").unwrap().exclude_words(&["fox".into()]);
        let found = check_text(&Tiny, "The quikc fox", &[], "English: USA", &Options::default(), &user, &[], false);
        assert_eq!(found.iter().map(|i| i.word.as_str()).collect::<Vec<_>>(), ["fox"]);
        let found = check_text(&Tiny, "The quikc fox", &[], "English: USA", &Options::default(), &user, &["fox".into()], false);
        assert!(found.is_empty());
    }

    #[test]
    fn digits_other_scripts_and_unsupported_languages_are_not_judged() {
        assert!(run("abc123 日本語 Привет 한글x", &Options::default()).is_empty());
        let found = check_text(&Tiny, "quikc", &[Run { start: 0, end: 5, language: "$ID/French".into() }], "English: USA", &Options::default(), &UserDictionaries::new(), &[], false);
        assert!(found.is_empty());
    }

    /// The real dictionaries: Korean needs its flag aliases written out, or `학교를` is flagged.
    #[test]
    fn bundled_dictionaries_know_real_words() {
        let user = UserDictionaries::new();
        let words = |text: &str| -> Vec<String> {
            check_text(&crate::Bundled, text, &[], "English: USA", &Options::default(), &user, &[], false).into_iter().map(|i| i.word).collect()
        };
        assert_eq!(words("Hello world, this is a helo."), ["helo"]);
        assert_eq!(words("학교를 다니는 사람이 맞춤뻡을 안녕하세요"), ["맞춤뻡을"]);
        let list = suggest(&crate::Bundled, "English: USA", "wrold", 8);
        assert!(list.iter().any(|w| w == "world"), "{list:?}");
        let list = suggest(&crate::Bundled, "Korean", "맞춤뻡", 8);
        assert!(list.iter().any(|w| w == "맞춤법"), "{list:?}");
    }

    #[test]
    fn aliases_in_a_dictionary_are_written_out() {
        let aff = "SET UTF-8\nFLAG num\nAF 2\nAF 1\nAF 1,2\nSFX 1 Y 1\nSFX 1 0 s .\nSFX 2 Y 1\nSFX 2 0 ed .\n";
        let (aff2, dic2) = crate::dict::expand_flag_aliases(aff, "2\ncat/1\nwalk/2\n");
        assert!(!aff2.contains("AF "));
        assert_eq!(dic2, "2\ncat/1\nwalk/1,2\n");
    }
}
