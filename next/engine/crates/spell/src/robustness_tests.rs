use crate::{Dictionaries, Lang, Options, Run, UserDictionaries, autocorrect, check_text};

struct MissingDictionaries;
impl Dictionaries for MissingDictionaries {
    fn get(&self, _: Lang) -> Option<&spellbook::Dictionary> {
        None
    }
}

#[test]
fn invalid_alias_numbers_are_preserved() {
    let aff = "SET UTF-8\nAF 1\nAF AB\n";
    let dic = "6\nvalid/1\nzero/0\nmissing/2\nnegative/-1\ntext/nope\nhuge/999999999999999999999999999999999999\n";
    let (actual_aff, actual_dic) = crate::dict::expand_flag_aliases(aff, dic);
    assert_eq!(actual_aff, "SET UTF-8\n");
    assert_eq!(actual_dic, dic.replace("valid/1", "valid/AB"));
    assert_eq!(crate::dict::expand_flag_aliases("", ""), (String::new(), String::new()));
}

#[test]
fn missing_dictionaries_and_outside_runs_do_not_produce_issues() {
    let runs = [Run { start: usize::MAX, end: 0, language: "missing".into() }];
    for text in ["", "hello hello", "\0한글\u{301} 😀 don't-"] {
        let issues = check_text(&MissingDictionaries, text, &runs, "English: USA", &Options::default(), &UserDictionaries::new(), &[], true);
        assert!(issues.is_empty());
        assert!(crate::suggest(&MissingDictionaries, "missing", text, usize::MAX).is_empty());
    }
}

#[test]
fn typing_beyond_end_keeps_clamped_cursor_behavior() {
    let story: Vec<char> = "teh".chars().collect();
    let pairs = [("teh".into(), "the".into())];
    let rules = autocorrect::Rules { auto_correct: true, capitalization_errors: false, quotes: None };
    let at_end = autocorrect::type_text(&story, story.len(), " ", |_| Some(&pairs), |_, _| false, rules, true);
    let outside = autocorrect::type_text(&story, usize::MAX, " ", |_| Some(&pairs), |_, _| false, rules, true);
    assert_eq!(outside, at_end);
    assert_eq!(outside.unwrap().replacement, "the ");
    assert!(autocorrect::type_text(&[], usize::MAX, "", |_| None, |_, _| false, rules, true).is_none());
}

#[test]
fn token_boundaries_stay_within_unusual_unicode_input() {
    for text in ["", "'", "--", "\u{301}", "a'", "'a", "a--b", "한글\0😀é\u{301}"] {
        let chars: Vec<char> = text.chars().collect();
        for token in crate::words::tokenize(&chars) {
            assert!(token.start < token.end && token.end <= chars.len());
            assert_eq!(token.text, chars[token.start..token.end].iter().collect::<String>());
        }
    }
}
