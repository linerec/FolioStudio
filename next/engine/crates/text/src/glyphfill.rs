//! Installed fonts for characters the font of the text lacks (they would be set as .notdef
//! boxes): text placed or pasted in another writing system (Hangul, kana, Han, Cyrillic …)
//! takes a face of this computer that has it. Nothing is bundled: only installed faces are
//! used. The order: the faces the operating system ships for that writing system (Windows
//! Malgun Gothic · Yu Gothic · Microsoft YaHei, macOS Apple SD Gothic Neo · Hiragino ·
//! PingFang, Linux Noto CJK), then the first installed face that has the character
//! ([`FontStore::fallbacks`]).

use crate::fonts::FontStore;
use folio_model::layout::FontId;
use std::collections::HashMap;

/// The writing system a character is filled for. Han takes the writing of the text around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillScript {
    Korean,
    Japanese,
    Chinese,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Thai,
    Devanagari,
    Symbol,
    Other,
}

impl FillScript {
    /// The script of `c`; Han (shared by Korean, Japanese and Chinese) is `han` (the writing
    /// of the text it stands in: see [`han_script`]).
    pub fn of(c: char, han: FillScript) -> FillScript {
        match c {
            '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{A960}'..='\u{A97F}' | '\u{D7B0}'..='\u{D7FF}' => FillScript::Korean,
            '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => FillScript::Japanese,
            '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FFFF}' | '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FF65}' => han,
            '\u{0400}'..='\u{052F}' | '\u{2DE0}'..='\u{2DFF}' | '\u{A640}'..='\u{A69F}' => FillScript::Cyrillic,
            '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}' => FillScript::Greek,
            '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{08A0}'..='\u{08FF}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}' => FillScript::Arabic,
            '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}' => FillScript::Hebrew,
            '\u{0E00}'..='\u{0E7F}' => FillScript::Thai,
            '\u{0900}'..='\u{097F}' => FillScript::Devanagari,
            '\u{2000}'..='\u{2BFF}' | '\u{1F000}'..='\u{1FAFF}' => FillScript::Symbol,
            _ => FillScript::Other,
        }
    }

    /// The faces the operating system ships for this writing system, best first (also SVG text's fallback, folio-render svg/rich.rs).
    pub fn os_families(self) -> &'static [&'static str] {
        #[cfg(target_os = "windows")]
        {
            match self {
                FillScript::Korean => &["Malgun Gothic", "Gulim", "Dotum", "Batang"],
                FillScript::Japanese => &["Yu Gothic", "Meiryo", "MS Gothic", "Yu Mincho", "MS Mincho"],
                FillScript::Chinese => &["Microsoft YaHei", "SimSun", "Microsoft JhengHei", "DengXian"],
                FillScript::Cyrillic | FillScript::Greek | FillScript::Other => &["Segoe UI", "Arial", "Times New Roman"],
                FillScript::Arabic => &["Segoe UI", "Arial", "Times New Roman", "Tahoma"],
                FillScript::Hebrew => &["Segoe UI", "Arial", "Times New Roman", "David"],
                FillScript::Thai => &["Leelawadee UI", "Tahoma", "Leelawadee"],
                FillScript::Devanagari => &["Nirmala UI", "Mangal"],
                FillScript::Symbol => &["Segoe UI Symbol", "Segoe UI", "Arial"],
            }
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            match self {
                FillScript::Korean => &["Apple SD Gothic Neo", "AppleGothic", "AppleMyungjo"],
                FillScript::Japanese => &["Hiragino Sans", "Hiragino Kaku Gothic ProN", "Hiragino Mincho ProN"],
                FillScript::Chinese => &["PingFang SC", "Heiti SC", "STHeiti", "PingFang TC"],
                FillScript::Cyrillic | FillScript::Greek | FillScript::Other => &["Helvetica Neue", "Helvetica", "Arial"],
                FillScript::Arabic => &["Geeza Pro", "Arial"],
                FillScript::Hebrew => &["Arial Hebrew", "Arial"],
                FillScript::Thai => &["Thonburi", "Ayuthaya"],
                FillScript::Devanagari => &["Kohinoor Devanagari", "Devanagari Sangam MN"],
                FillScript::Symbol => &["Apple Symbols", "Helvetica Neue"],
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
        {
            match self {
                FillScript::Korean => &["Noto Sans CJK KR", "Noto Sans KR", "NanumGothic", "Nanum Gothic"],
                FillScript::Japanese => &["Noto Sans CJK JP", "Noto Sans JP", "IPAGothic"],
                FillScript::Chinese => &["Noto Sans CJK SC", "Noto Sans SC", "WenQuanYi Zen Hei"],
                FillScript::Cyrillic | FillScript::Greek | FillScript::Other => &["Noto Sans", "DejaVu Sans", "Liberation Sans"],
                FillScript::Arabic => &["Noto Sans Arabic", "Noto Naskh Arabic", "DejaVu Sans"],
                FillScript::Hebrew => &["Noto Sans Hebrew", "DejaVu Sans"],
                FillScript::Thai => &["Noto Sans Thai", "Noto Serif Thai"],
                FillScript::Devanagari => &["Noto Sans Devanagari", "Lohit Devanagari"],
                FillScript::Symbol => &["Noto Sans Symbols", "Noto Sans Symbols2", "DejaVu Sans"],
            }
        }
    }
}

/// The writing Han characters of a text are filled with: Japanese when kana are among them,
/// Korean when Hangul is, else Chinese.
pub fn han_script(chars: impl IntoIterator<Item = char>) -> FillScript {
    let mut korean = false;
    for c in chars {
        match FillScript::of(c, FillScript::Chinese) {
            FillScript::Japanese => return FillScript::Japanese,
            FillScript::Korean => korean = true,
            _ => {}
        }
    }
    if korean { FillScript::Korean } else { FillScript::Chinese }
}

/// Finds installed faces for characters. Results are kept for the life of the value.
pub struct GlyphFill<'a> {
    fonts: &'a FontStore,
    families: HashMap<&'static str, Option<FontId>>,
    found: HashMap<(char, FillScript), Option<FontId>>,
}

impl<'a> GlyphFill<'a> {
    pub fn new(fonts: &'a FontStore) -> Self {
        GlyphFill { fonts, families: HashMap::new(), found: HashMap::new() }
    }

    fn has(&self, font: FontId, c: char) -> bool {
        self.fonts.with_face(font, |f| f.glyph_index(c).is_some()).unwrap_or(false)
    }

    /// An installed face that has `c` (a regular face of the family the system ships for
    /// its writing, else the first installed face with it), or none.
    pub fn face_for(&mut self, c: char, han: FillScript) -> Option<FontId> {
        let script = FillScript::of(c, han);
        if let Some(f) = self.found.get(&(c, script)) {
            return *f;
        }
        let mut out = None;
        for &family in script.os_families() {
            let font = *self.families.entry(family).or_insert_with(|| {
                let r = self.fonts.resolve(family, "Regular", &[]);
                r.exact.then_some(r.font)
            });
            if let Some(font) = font.filter(|f| self.has(*f, c)) {
                out = Some(font);
                break;
            }
        }
        if out.is_none() {
            out = self.fonts.fallbacks([c]).get(&c).copied().filter(|f| !self.fonts.is_synthetic(*f) && self.has(*f, c));
        }
        self.found.insert((c, script), out);
        out
    }

    /// The family and style names of `font` as the document writes them (`AppliedFont`,
    /// `FontStyle`), keeping `style` (the text's own, e.g. Bold) when the family has it.
    pub fn names(&self, font: FontId, style: &str) -> Option<(String, String)> {
        let info = self.fonts.face_info(font)?;
        let family = info.family.clone();
        if !style.is_empty() && style != info.style && self.fonts.resolve(&family, style, &[]).exact {
            return Some((family, style.to_owned()));
        }
        Some((family, info.style.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn han_follows_the_text_around_it() {
        assert_eq!(han_script("漢字".chars()), FillScript::Chinese);
        assert_eq!(han_script("漢字かな".chars()), FillScript::Japanese);
        assert_eq!(han_script("漢字한글".chars()), FillScript::Korean);
        assert_eq!(FillScript::of('가', FillScript::Chinese), FillScript::Korean);
        assert_eq!(FillScript::of('Ж', FillScript::Chinese), FillScript::Cyrillic);
        assert_eq!(FillScript::of('字', FillScript::Korean), FillScript::Korean);
    }

    #[test]
    fn installed_faces_fill_characters_they_have() {
        let fonts = FontStore::discover(&[]);
        let mut fill = GlyphFill::new(&fonts);
        // whatever this computer has: a face that is returned has the character
        for c in ['가', 'あ', '字', 'Ж', 'A'] {
            if let Some(f) = fill.face_for(c, FillScript::Chinese) {
                assert!(fonts.with_face(f, |t| t.glyph_index(c).is_some()).unwrap_or(false), "{c}");
                assert!(fill.names(f, "Regular").is_some());
            }
        }
    }
}
