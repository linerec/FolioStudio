//! The bundled dictionaries, read on first use (never when a document opens).
use spellbook::Dictionary;
use std::sync::OnceLock;

const EN_AFF: &str = include_str!("../dictionaries/en_US/en_US.aff");
const EN_DIC: &str = include_str!("../dictionaries/en_US/en_US.dic");
const KO_AFF: &str = include_str!("../dictionaries/ko_KR/ko.aff");
const KO_DIC: &str = include_str!("../dictionaries/ko_KR/ko.dic");

/// A language Folio has a dictionary for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    EnUs,
    KoKr,
}

impl Lang {
    /// The name InDesign shows for the language (also the key of its user dictionary and
    /// autocorrect table). InDesign has no Korean entry: "Korean" is Folio's own.
    pub fn name(self) -> &'static str {
        match self {
            Lang::EnUs => "English: USA",
            Lang::KoKr => "Korean",
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::EnUs => "en_US",
            Lang::KoKr => "ko_KR",
        }
    }
    /// `"English: USA"`, `"$ID/English: USA"` (as IDML writes it), `"en_US"`, `"Korean"`, `"ko_KR"`.
    pub fn from_name(name: &str) -> Option<Lang> {
        match name.trim().trim_start_matches("$ID/") {
            "English: USA" | "en_US" | "en-US" => Some(Lang::EnUs),
            "Korean" | "ko_KR" | "ko-KR" | "ko" => Some(Lang::KoKr),
            _ => None,
        }
    }
}

/// Where dictionaries come from (the bundled ones, or small ones in tests).
pub trait Dictionaries {
    fn get(&self, lang: Lang) -> Option<&Dictionary>;
}

pub struct Bundled;

impl Dictionaries for Bundled {
    fn get(&self, lang: Lang) -> Option<&Dictionary> {
        static EN: OnceLock<Option<Dictionary>> = OnceLock::new();
        static KO: OnceLock<Option<Dictionary>> = OnceLock::new();
        match lang {
            Lang::EnUs => EN.get_or_init(|| Dictionary::new(EN_AFF, EN_DIC).ok()).as_ref(),
            Lang::KoKr => KO
                .get_or_init(|| {
                    let (aff, dic) = expand_flag_aliases(KO_AFF, KO_DIC);
                    Dictionary::new(&aff, &dic).ok()
                })
                .as_ref(),
        }
    }
}

/// Hunspell's `AF` flag aliases: a dictionary line `word/25` means "the flags on the 25th `AF`
/// row". The Korean dictionary is built that way, and the reader does not follow the aliases
/// (measured: with them, `학교를` is wrong; with the flags written out, it is right). So write
/// the flags out and drop the `AF` rows.
pub(crate) fn expand_flag_aliases(aff: &str, dic: &str) -> (String, String) {
    let mut rows: Vec<&str> = Vec::new();
    let mut counted = false;
    let mut out_aff = String::with_capacity(aff.len());
    for line in aff.lines() {
        if let Some(rest) = line.strip_prefix("AF ") {
            if counted {
                rows.push(rest.trim());
            }
            counted = true;
            continue;
        }
        out_aff.push_str(line);
        out_aff.push('\n');
    }
    if rows.is_empty() {
        return (aff.to_owned(), dic.to_owned());
    }
    let mut out_dic = String::with_capacity(dic.len() + dic.len() / 4);
    for (i, line) in dic.lines().enumerate() {
        if i > 0
            && let Some((word, rest)) = line.split_once('/')
        {
            let end = rest.find([' ', '\t']).unwrap_or(rest.len());
            let (flags, tail) = rest.split_at(end);
            if let Some(row) = flags.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|n| rows.get(n)) {
                out_dic.push_str(word);
                out_dic.push('/');
                out_dic.push_str(row);
                out_dic.push_str(tail);
                out_dic.push('\n');
                continue;
            }
        }
        out_dic.push_str(line);
        out_dic.push('\n');
    }
    (out_aff, out_dic)
}
