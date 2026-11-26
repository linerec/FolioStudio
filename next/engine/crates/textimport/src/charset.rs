//! The "Character Set" choice of the text import (`TextImportCharacterSet`). The names are InDesign's.
//! `ANSI` is the Windows Latin code page (1252) on every platform: InDesign takes the computer's ANSI code page,
//! the product must not (a project read on another computer must give the same text). Undefined bytes of that
//! page (0x81, 0x8D, 0x8F, 0x90, 0x9D) become a space (placetext-c1, `ansi-all`).
use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CharacterSet {
    #[default]
    #[serde(rename = "ANSI")]
    Ansi,
    #[serde(rename = "UTF8")]
    Utf8,
    /// UTF-16, little endian without a mark.
    #[serde(rename = "UTF16")]
    Utf16,
    /// Korean (EUC-KR, Windows 949).
    #[serde(rename = "KSC5601")]
    Ksc5601,
    #[serde(rename = "SHIFTJIS90MS")]
    ShiftJis90Ms,
    #[serde(rename = "SHIFTJIS90PV")]
    ShiftJis90Pv,
    #[serde(rename = "RECOMMENDSHIFTJIS83PV")]
    RecommendShiftJis83Pv,
    #[serde(rename = "GB2312")]
    Gb2312,
    #[serde(rename = "GB18030")]
    Gb18030,
    #[serde(rename = "CHINESE_BIG_5")]
    ChineseBig5,
    #[serde(rename = "MACINTOSH_ROMAN")]
    MacintoshRoman,
    #[serde(rename = "WINDOWS_CYRILLIC")]
    WindowsCyrillic,
    #[serde(rename = "WINDOWS_EE")]
    WindowsEe,
    #[serde(rename = "WINDOWS_CE")]
    WindowsCe,
    #[serde(rename = "WINDOWS_GREEK")]
    WindowsGreek,
    #[serde(rename = "WINDOWS_TURKISH")]
    WindowsTurkish,
    #[serde(rename = "WINDOWS_BALTIC")]
    WindowsBaltic,
    #[serde(rename = "WINDOWS_ARABIC")]
    WindowsArabic,
    #[serde(rename = "WINDOWS_HEBREW")]
    WindowsHebrew,
    #[serde(rename = "CYRILLIC_KOI8R")]
    CyrillicKoi8r,
    #[serde(rename = "CYRILLIC_KOI8U")]
    CyrillicKoi8u,
    #[serde(rename = "CYRILLIC_ISO")]
    CyrillicIso,
    #[serde(rename = "GREEK_ISO")]
    GreekIso,
    #[serde(rename = "CENTRALEUROPEAN_ISO")]
    CentralEuropeanIso,
    #[serde(rename = "TURKISH_ISO")]
    TurkishIso,
    /// Folio's own choice: the set the bytes look like (`detect`). The import options window starts with it.
    #[serde(rename = "AUTO")]
    Auto,
}

impl CharacterSet {
    /// The set that reads `bytes`: the set itself, or for `Auto` the one the bytes look like. A code page of another script
    /// keeps the frame's font for the text (InDesign also names that script's font, KSC5601: `Adobe Myungjo Std`; Folio does
    /// not, the text takes the document's own fonts and font fallback).
    pub fn resolve(self, bytes: &[u8]) -> Self {
        if self == Self::Auto { detect(bytes) } else { self }
    }

    /// The decoder for bytes with no mark; `None` for the sets read by hand (ANSI, UTF-8, UTF-16, and AUTO before it is resolved).
    pub(crate) fn encoding(self) -> Option<&'static Encoding> {
        use encoding_rs as e;
        Some(match self {
            Self::Ansi | Self::Utf8 | Self::Utf16 | Self::Auto => return None,
            Self::Ksc5601 => e::EUC_KR,
            Self::ShiftJis90Ms | Self::ShiftJis90Pv | Self::RecommendShiftJis83Pv => e::SHIFT_JIS,
            Self::Gb2312 => e::GBK,
            Self::Gb18030 => e::GB18030,
            Self::ChineseBig5 => e::BIG5,
            Self::MacintoshRoman => e::MACINTOSH,
            Self::WindowsCyrillic => e::WINDOWS_1251,
            Self::WindowsEe | Self::WindowsCe => e::WINDOWS_1250,
            Self::WindowsGreek => e::WINDOWS_1253,
            Self::WindowsTurkish => e::WINDOWS_1254,
            Self::WindowsBaltic => e::WINDOWS_1257,
            Self::WindowsArabic => e::WINDOWS_1256,
            Self::WindowsHebrew => e::WINDOWS_1255,
            Self::CyrillicKoi8r => e::KOI8_R,
            Self::CyrillicKoi8u => e::KOI8_U,
            Self::CyrillicIso => e::ISO_8859_5,
            Self::GreekIso => e::ISO_8859_7,
            Self::CentralEuropeanIso => e::ISO_8859_2,
            Self::TurkishIso => e::WINDOWS_1254,
        })
    }
}

/// The set `bytes` look like (the "AUTO" choice; Folio's own, InDesign has no such choice): a byte order mark decides; then UTF-16 LE
/// without a mark (a zero byte after most characters), UTF-8, the Korean, Japanese and Chinese code pages (each read without errors, scored
/// by how many of the most common characters of its language the text has), and for single-byte text the Cyrillic, Greek, Hebrew and
/// Arabic code pages when most letters are not ASCII; everything else is ANSI (Windows 1252).
pub fn detect(bytes: &[u8]) -> CharacterSet {
    use CharacterSet as C;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return C::Utf8;
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return C::Utf16;
    }
    if bytes.len() >= 4 {
        let pairs = bytes.len() / 2;
        let odd_zero = bytes.chunks_exact(2).filter(|p| p[1] == 0 && p[0] != 0).count();
        if odd_zero * 10 >= pairs * 3 && bytes.chunks_exact(2).filter(|p| p[0] == 0).count() * 20 < pairs {
            return C::Utf16;
        }
    }
    if bytes.is_ascii() {
        return C::Ansi;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return C::Utf8;
    }
    let decode = |e: &'static Encoding| e.decode_without_bom_handling_and_without_replacement(bytes).map(|s| s.into_owned());
    let share = |text: &str, hit: &dyn Fn(char) -> bool| {
        let wide = text.chars().filter(|c| !c.is_ascii()).count().max(1);
        text.chars().filter(|c| hit(*c)).count() as f64 / wide as f64
    };
    let mut best = (0.0, C::Ansi);
    let mut consider = |score: f64, set: C| {
        if score > best.0 {
            best = (score, set);
        }
    };
    if let Some(t) = decode(encoding_rs::EUC_KR) {
        consider(share(&t, &|c| COMMON_KO.contains(c)) + 0.1 * share(&t, &|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)), C::Ksc5601);
    }
    if let Some(t) = decode(encoding_rs::SHIFT_JIS) {
        consider(share(&t, &|c| ('\u{3040}'..='\u{30FF}').contains(&c)), C::ShiftJis90Ms);
    }
    let han = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
    if let Some(t) = decode(encoding_rs::GB18030) {
        consider(share(&t, &|c| COMMON_ZH.contains(c)) + 0.05 * share(&t, &han), C::Gb18030);
    }
    if let Some(t) = decode(encoding_rs::BIG5) {
        consider(share(&t, &|c| COMMON_ZH.contains(c)) + 0.05 * share(&t, &han), C::ChineseBig5);
    }
    if best.0 >= 0.1 {
        return best.1;
    }
    // single-byte text: another script only when most of its letters are not ASCII (`café` stays Latin)
    let letters = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    let high = bytes.iter().filter(|b| **b >= 0xC0).count();
    if high * 10 < (letters + high) * 4 {
        return C::Ansi;
    }
    let mut best = (0.0, C::Ansi);
    for (e, set, common) in [
        (encoding_rs::WINDOWS_1251, C::WindowsCyrillic, COMMON_RU),
        (encoding_rs::KOI8_R, C::CyrillicKoi8r, COMMON_RU),
        (encoding_rs::WINDOWS_1253, C::WindowsGreek, COMMON_EL),
        (encoding_rs::WINDOWS_1255, C::WindowsHebrew, COMMON_HE),
        (encoding_rs::WINDOWS_1256, C::WindowsArabic, COMMON_AR),
    ] {
        let (t, _) = e.decode_without_bom_handling(bytes);
        let score = share(&t, &|c| common.contains(c));
        if score > best.0 {
            best = (score, set);
        }
    }
    if best.0 >= 0.25 { best.1 } else { C::Ansi }
}

/// The most common characters of each language (a frequency list, used only to tell code pages apart).
const COMMON_KO: &str = "이다는의에가고하을를한지서기로사도리대자어아인정시수일나해있국그었것들게요니라만주우상보부장적성여원전동제경화되문과회내와생소면개없우관";
const COMMON_ZH: &str = "的一是不了在人有我他这這个個们們中来來上大为為和国國地到以说說时時要就出会會也你对對生能而子那得于於着著下自之年过過发發后後作里裡用道行所然家种種事成方多经經么麼去法学學如都同现現当當没沒动動面起看定天分还還进進好小部其些主样樣理心她本前开開但因只从從想实實";
const COMMON_RU: &str = "оеаинтсрвлкмдпуяыьгзбчйхжшюцщэф";
const COMMON_EL: &str = "αοιετνσυρπκμλάέίόήωηςδγ";
const COMMON_HE: &str = "יוהלארמתבשנקכעדפסגזחטצ";
const COMMON_AR: &str = "اليمونرتبعهدسقفكحجشصخطضذثغزظء";

/// Windows 1252 for bytes 0x80-0x9F (index = byte - 0x80); the undefined ones are a space.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', ' ', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', ' ', '\u{017D}', ' ',
    ' ', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', ' ', '\u{017E}', '\u{0178}',
];

pub(crate) fn decode_ansi(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}
