//! Match requested families and styles and choose substitutes by writing system.

use super::*;

/// Writing systems with their own substitute for a missing font. InDesign substitutes a
/// missing font with one face chosen by the writing script the document recorded for it
/// (`WritingScript` in the font table) and sets every character that face lacks as its
/// .notdef box. Measured with InDesign 2025 on this machine (artifacts/next-20261018/fonts/):
/// Roman (0) or unrecorded → Myriad Pro, Korean (3) → Gulim, Arabic (4) → Adobe Arabic, with
/// boxes for Hangul, Arabic, Thai, … in Myriad Pro and for Arabic in Gulim. Folio Studio
/// takes the same face for the text it covers but never prints boxes for a missing font:
/// characters the substitute lacks come from the substitute of their own writing system,
/// else from any installed face that has them ([`FontStore::fallbacks`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Writing {
    Latin,
    Korean,
    KoreanSerif,
    Japanese,
    JapaneseSerif,
    Chinese,
    Arabic,
    Hebrew,
    Thai,
    Devanagari,
}

impl Writing {
    /// Substitute families in order of preference: InDesign's choice first (Adobe faces
    /// are found when installed for all applications, in Adobe Fonts or a document's font
    /// folder; InDesign's own copies in its program folder are not used), then faces that
    /// come with Windows and macOS.
    fn families(self) -> &'static [&'static str] {
        match self {
            Writing::Latin => LATIN,
            Writing::Korean => KOREAN_SANS,
            Writing::KoreanSerif => KOREAN_SERIF,
            Writing::Japanese => JAPANESE_SANS,
            Writing::JapaneseSerif => JAPANESE_SERIF,
            Writing::Chinese => CHINESE,
            Writing::Arabic => ARABIC,
            Writing::Hebrew => HEBREW,
            Writing::Thai => THAI,
            Writing::Devanagari => DEVANAGARI,
        }
    }

    /// The writing system a missing font is for, from its name (and `OpenTypeCID` for
    /// Korean), when the document records no `WritingScript` for it.
    pub fn of_font(family: &str, font_type: Option<&str>) -> Writing {
        let n = norm(family);
        let has = |keys: &[&str]| keys.iter().any(|k| n.contains(k));
        let serif = looks_serif(family);
        if has(&["arabic", "naskh", "kufi", "nastaliq", "thuluth", "farsi", "persian", "urdu"]) {
            Writing::Arabic
        } else if has(&["hebrew"]) {
            Writing::Hebrew
        } else if has(&["thai"]) {
            Writing::Thai
        } else if has(&["devanagari", "hindi"]) {
            Writing::Devanagari
        } else if has(JAPANESE_KEYS) || n.ends_with("jp") {
            if serif { Writing::JapaneseSerif } else { Writing::Japanese }
        } else if has(CHINESE_KEYS) {
            Writing::Chinese
        } else if looks_korean(family, font_type) {
            if serif { Writing::KoreanSerif } else { Writing::Korean }
        } else {
            Writing::Latin
        }
    }

    /// The writing system of a font's recorded `WritingScript` (a Mac OS script code), the
    /// basis InDesign substitutes a missing font by; `None` for codes without a substitute
    /// list here (the name decides then). Serif or sans for Korean and Japanese comes from
    /// the name.
    pub fn of_script(script: u32, family: &str) -> Option<Writing> {
        let serif = looks_serif(family);
        Some(match script {
            // Roman, Greek, Cyrillic, Central European Roman, Vietnamese.
            0 | 6 | 7 | 29 | 30 => Writing::Latin,
            1 if serif => Writing::JapaneseSerif,
            1 => Writing::Japanese,
            2 | 25 => Writing::Chinese,
            3 if serif => Writing::KoreanSerif,
            3 => Writing::Korean,
            4 | 31 => Writing::Arabic,
            5 => Writing::Hebrew,
            9 => Writing::Devanagari,
            21 => Writing::Thai,
            _ => return None,
        })
    }

    /// Writing systems whose substitutes are tried for a character a face lacks.
    fn for_char(c: char) -> &'static [Writing] {
        match c {
            '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{A960}'..='\u{A97F}' | '\u{D7B0}'..='\u{D7FF}' => {
                &[Writing::Korean]
            }
            // Hanja first: Korean faces carry KS X 1001's 4,888.
            '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FFFF}' => {
                &[Writing::Korean, Writing::Japanese, Writing::Chinese]
            }
            '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => &[Writing::Japanese],
            '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{08A0}'..='\u{08FF}' | '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}' => {
                &[Writing::Arabic]
            }
            '\u{0590}'..='\u{05FF}' | '\u{FB1D}'..='\u{FB4F}' => &[Writing::Hebrew],
            '\u{0E00}'..='\u{0E7F}' => &[Writing::Thai],
            '\u{0900}'..='\u{097F}' | '\u{A8E0}'..='\u{A8FF}' => &[Writing::Devanagari],
            _ => &[Writing::Latin],
        }
    }
}

/// Substitutes for missing fonts, as InDesign chooses them: Myriad Pro for Latin fonts
/// (#347, PF Din Text Cond Pro, 541 characters, median advance error 0.000 pt); for Korean
/// ones Gulim, the face InDesign 2026 embedded for every missing Korean font of the
/// 2026-10 issue on Windows (Gmarket Sans, NEXON Lv1 Gothic OTF, HGGGothicssi_Pro), with
/// Adobe Gothic Std activated — #347's Hangul advances (full-width in both) matched either.
/// Next in each list: what is usually installed (macOS has no Gulim).
const KOREAN_SANS: &[&str] = &[
    "Gulim",
    "Adobe Gothic Std",
    "Noto Sans KR",
    "Malgun Gothic",
    "Apple SD Gothic Neo",
    "Source Han Sans K",
    "Noto Sans CJK KR",
    "NanumGothic",
    "Nanum Gothic",
    "Dotum",
];
/// Korean serif (Myeongjo) fonts: InDesign's choice is not known; Source Han Serif K is the
/// closest measured (Nanum Myeongjo, 0.158 pt).
const KOREAN_SERIF: &[&str] =
    &["Source Han Serif K", "Noto Serif KR", "Adobe Myungjo Std", "Nanum Myeongjo", "NanumMyeongjo", "Batang"];
const LATIN: &[&str] = &["Myriad Pro", "Arial", "Helvetica", "Liberation Sans", "DejaVu Sans", "Segoe UI"];
/// Arabic: InDesign takes Adobe Arabic. Of the faces Windows installs, Times New Roman's
/// Naskh is the closest to it (Atlas CHR-016 against InDesign's PDF, mean error of the
/// specimen: Times New Roman 10.4, Arial 11.6, Microsoft Sans Serif 12.8, Dubai 13.2,
/// Segoe UI 14.4, Tahoma 15.3, .notdef boxes 23.0; artifacts/next-20261018/fonts/
/// arabic-candidates).
const ARABIC: &[&str] =
    &["Adobe Arabic", "Times New Roman", "Arial", "Tahoma", "Segoe UI", "Geeza Pro", "Noto Naskh Arabic", "Noto Sans Arabic"];
const HEBREW: &[&str] = &["Adobe Hebrew", "Times New Roman", "Arial", "David", "Segoe UI", "Arial Hebrew", "Noto Sans Hebrew"];
const THAI: &[&str] = &["Leelawadee UI", "Leelawadee", "Tahoma", "Thonburi", "Noto Sans Thai", "Microsoft Sans Serif"];
const DEVANAGARI: &[&str] = &["Adobe Devanagari", "Nirmala UI", "Mangal", "Kohinoor Devanagari", "Noto Sans Devanagari"];
/// Japanese: InDesign takes Kozuka Gothic Pr6N R (#347, Toppan Bunkyu Midashi Gothic,
/// `WritingScript` 1). Adobe's later design, Source Han Sans / Noto Sans CJK JP, is the
/// closest installed (#347 p. 95 error 4.303; Yu Gothic 4.343, MS Gothic 4.328, Myriad Pro
/// 4.338).
const JAPANESE_SANS: &[&str] =
    &["Kozuka Gothic Pr6N", "Source Han Sans", "Noto Sans CJK JP", "Yu Gothic", "Meiryo", "MS Gothic", "Hiragino Sans", "Noto Sans JP"];
const JAPANESE_SERIF: &[&str] = &[
    "Kozuka Mincho Pr6N",
    "Source Han Serif",
    "Noto Serif CJK JP",
    "Yu Mincho",
    "MS Mincho",
    "Hiragino Mincho ProN",
    "Noto Serif JP",
];
const CHINESE: &[&str] = &["Adobe Song Std", "Microsoft YaHei", "SimSun", "PingFang SC", "Noto Sans SC", "Microsoft JhengHei"];
/// Name parts of Japanese and Chinese families (normalised, see `norm`).
const JAPANESE_KEYS: &[&str] = &[
    "kozuka", "kozmin", "kozgo", "mincho", "hiragino", "ryumin", "meiryo", "yugothic", "yumincho", "msgothic", "mspgothic",
    "morisawa", "midashi", "shingo", "futogo", "gothicbbb", "notosanscjkjp", "sourcehansansjp", "sourcehanserifjp",
];
const CHINESE_KEYS: &[&str] = &[
    "adobesong", "adobeming", "adobeheiti", "adobekaiti", "adobefangsong", "simsun", "simhei", "yahei", "jhenghei", "mingliu",
    "pingfang", "songti", "heiti", "kaiti", "notosanssc", "notosanstc", "notoserifsc", "notoseriftc", "sourcehansanssc",
    "sourcehansanstc", "sourcehanserifsc", "sourcehanseriftc",
];

impl FontStore {
    /// Font for an InDesign family + style, using the document's font table (PostScript
    /// name) first, then family + style names, then the nearest weight of the family,
    /// then the substitute for the font's writing system ([`Writing::of_script`] of the
    /// recorded `WritingScript`, else [`Writing::of_font`]).
    pub fn resolve(&self, family: &str, style: &str, table: &[FontInfo]) -> Resolved {
        let declared = table.iter().filter(|f| f.family == family && f.style == style)
            .map(|f| (f.postscript_name.clone(), f.version.clone())).collect();
        let key = (family.to_string(), style.to_string(), declared);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = state.resolved.get(&key) {
            return *r;
        }
        let r = self.resolve_uncached(&mut state, family, style, table);
        state.resolved.insert(key, r);
        r
    }

    fn resolve_uncached(&self, st: &mut State, family: &str, style: &str, table: &[FontInfo]) -> Resolved {
        let (fam, mut want_cff) = clean_family(family);
        let style = style.trim();
        let (nfam, nstyle) = (norm(&fam), norm(style));

        // 1. PostScript names the document recorded for this family + style.
        let declared: Vec<&FontInfo> = table
            .iter()
            .filter(|f| norm(&clean_family(&f.family).0) == nfam && norm(f.style.trim()) == nstyle)
            .collect();
        if want_cff.is_none() {
            want_cff = declared.iter().find_map(|f| format_is_cff(&f.font_type));
        }
        let mut cands: Vec<usize> = Vec::new();
        for f in &declared {
            if let Some(list) = self.by_postscript.get(&f.postscript_name.to_ascii_lowercase()) {
                cands.extend(list);
            }
        }
        let matching_versions: Vec<usize> = cands.iter().copied().filter(|&i| {
            declared.iter().any(|f| !f.version.is_empty() && f.version == self.faces[i].version)
        }).collect();
        if let Some(id) = self.load_best(st, &matching_versions, want_cff)
            .or_else(|| self.load_best(st, &cands, want_cff)) {
            return Resolved { font: id, exact: true };
        }

        // 2. Family + style names (IDs 16/17, 1/2), then full name, then PostScript name
        //    spelled as family + style.
        let mut cands: Vec<usize> = Vec::new();
        if let Some(list) = self.by_family.get(&nfam) {
            cands.extend(
                list.iter().copied().filter(|&i| self.faces[i].pairs.iter().any(|(f, s)| *f == nfam && *s == nstyle)),
            );
        }
        if cands.is_empty() {
            for full in [format!("{nfam}{nstyle}"), if is_regular(&nstyle) { nfam.clone() } else { String::new() }] {
                if let Some(list) = self.by_full.get(&full) {
                    cands.extend(list);
                }
            }
        }
        if cands.is_empty() {
            let key = format!("{nfam}{nstyle}");
            cands.extend(self.faces.iter().enumerate().filter(|(_, f)| norm(&f.postscript) == key).map(|(i, _)| i));
        }
        if let Some(id) = self.load_best(st, &cands, want_cff) {
            return Resolved { font: id, exact: true };
        }

        // 3. Same family, nearest weight and slope.
        if let Some(list) = self.by_family.get(&nfam) {
            let (w, it) = style_weight(style);
            let mut ranked: Vec<usize> = list.clone();
            ranked.sort_by_key(|&i| {
                let f = &self.faces[i];
                let fw = if f.weight == 0 { style_weight(&f.style).0 } else { f.weight };
                (u32::from(f.italic != it) * 10_000 + u32::from(fw.abs_diff(w)), f.source)
            });
            for i in ranked {
                if let Some(id) = self.load_entry(st, i) {
                    return Resolved { font: id, exact: false };
                }
            }
        }

        // 4. The substitute for the font's writing system (the `WritingScript` the document
        //    recorded, else guessed from the name), then a Latin or Korean one.
        let writing = declared
            .iter()
            .find_map(|f| Writing::of_script(f.writing_script?, &fam))
            .unwrap_or_else(|| Writing::of_font(&fam, declared.first().map(|f| f.font_type.as_str())));
        let font = [writing, Writing::Latin, Writing::Korean].into_iter().find_map(|w| self.substitute_in(st, w));
        Resolved { font: font.unwrap_or_else(|| self.synthetic_face(st)), exact: false }
    }

    /// A Korean sans-serif fallback face, if any is installed.
    pub fn korean_fallback(&self) -> Option<FontId> {
        self.substitute(Writing::Korean)
    }

    /// A Latin fallback face (Myriad Pro, else Arial, …), if any is installed.
    pub fn latin_fallback(&self) -> Option<FontId> {
        self.substitute(Writing::Latin)
    }

    /// The face standing in for a missing font of writing system `w`, if one is installed.
    pub fn substitute(&self, w: Writing) -> Option<FontId> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.substitute_in(&mut st, w)
    }

    fn substitute_in(&self, st: &mut State, w: Writing) -> Option<FontId> {
        if let Some(f) = st.substitutes.get(&w) {
            return *f;
        }
        let f = self.first_family(st, w.families()).or_else(|| match w {
            Writing::KoreanSerif => self.substitute_in(st, Writing::Korean),
            Writing::JapaneseSerif => self.substitute_in(st, Writing::Japanese),
            _ => None,
        });
        st.substitutes.insert(w, f);
        f
    }

    /// Faces for characters the face standing in for a missing font lacks: for each of
    /// `chars`, the substitute of its own writing system when that has it, else the first
    /// installed face that has it (document fonts, Adobe Fonts, user, then system fonts;
    /// regular upright faces first). Characters no installed face has are left out: they
    /// stay .notdef boxes and are reported. Loads the faces it returns; results are kept.
    pub fn fallbacks(&self, chars: impl IntoIterator<Item = char>) -> HashMap<char, FontId> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = HashMap::new();
        let mut left: Vec<char> = Vec::new();
        for c in chars {
            match st.char_fallback.get(&c) {
                Some(Some(f)) => {
                    out.insert(c, *f);
                }
                Some(None) => {}
                None => left.push(c),
            }
        }
        left.sort_unstable();
        left.dedup();
        let mut unfound: Vec<char> = Vec::new();
        for c in left {
            let found = Writing::for_char(c)
                .iter()
                .filter_map(|&w| self.substitute_in(&mut st, w))
                .find(|&f| self.loaded_face(f).and_then(|lf| lf.ttf().map(|t| t.glyph_index(c).is_some())).unwrap_or(false));
            match found {
                Some(f) => {
                    st.char_fallback.insert(c, Some(f));
                    out.insert(c, f);
                }
                None => unfound.push(c),
            }
        }
        if !unfound.is_empty() {
            for (c, f) in self.any_face_with(&mut st, &unfound) {
                st.char_fallback.insert(c, f);
                if let Some(f) = f {
                    out.insert(c, f);
                }
            }
        }
        out
    }

    /// The first face (in `fallbacks` order) with each of `chars`, one pass over the faces.
    fn any_face_with(&self, st: &mut State, chars: &[char]) -> Vec<(char, Option<FontId>)> {
        let mut ranked: Vec<usize> = (0..self.faces.len()).filter(|&i| self.faces[i].coords.is_empty()).collect();
        ranked.sort_by_key(|&i| {
            let f = &self.faces[i];
            let regular = is_regular(&norm(&f.style));
            (f.source, u8::from(!regular), u8::from(f.italic), f.weight.abs_diff(400), i)
        });
        let mut found: Vec<Option<usize>> = vec![None; chars.len()];
        let mut seen: std::collections::HashSet<(&Path, u32)> = std::collections::HashSet::new();
        for i in ranked {
            if found.iter().all(Option::is_some) {
                break;
            }
            let info = &self.faces[i];
            if !seen.insert((info.path.as_path(), info.index)) {
                continue;
            }
            let Ok(file) = File::open(info.path.as_path()) else { continue };
            // SAFETY: as in `load_entry`; the map is dropped before this function returns.
            let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else { continue };
            let Ok(face) = ttf_parser::Face::parse(&map, info.index) else { continue };
            for (k, c) in chars.iter().enumerate() {
                if found[k].is_none() && face.glyph_index(*c).is_some() {
                    found[k] = Some(i);
                }
            }
        }
        chars.iter().zip(found).map(|(&c, e)| (c, e.and_then(|e| self.load_entry(st, e)))).collect()
    }

    fn first_family(&self, st: &mut State, families: &[&str]) -> Option<FontId> {
        for fam in families {
            let n = norm(fam);
            let Some(list) = self.by_family.get(&n) else { continue };
            let mut ranked = list.clone();
            ranked.sort_by_key(|&i| {
                let f = &self.faces[i];
                let regular = f.pairs.iter().any(|(pf, ps)| *pf == n && is_regular(ps));
                (u8::from(!regular), u8::from(f.italic), f.weight.abs_diff(400), f.source)
            });
            for i in ranked {
                if let Some(id) = self.load_entry(st, i) {
                    return Some(id);
                }
            }
        }
        None
    }
}

/// InDesign family as it appears in IDML or scripting: drops `$ID/` and the " (OTF)" /
/// " (TT)" / " (T1)" suffix InDesign adds when one family exists in several formats.
/// Returns the family and the outline format the suffix asks for.
pub(super) fn clean_family(family: &str) -> (String, Option<bool>) {
    let f = family.trim().trim_start_matches("$ID/").trim();
    for (suffix, cff) in [(" (OTF)", true), (" (TT)", false), (" (T1)", true)] {
        if let Some(base) = f.strip_suffix(suffix) {
            return (base.trim().to_string(), Some(cff));
        }
    }
    (f.to_string(), None)
}

fn format_is_cff(font_type: &str) -> Option<bool> {
    match font_type {
        "OpenTypeCFF" | "OpenTypeCID" | "Type1" | "CID" => Some(true),
        "TrueType" | "OpenTypeTT" => Some(false),
        _ => None,
    }
}

fn is_regular(nstyle: &str) -> bool {
    matches!(nstyle, "regular" | "roman" | "normal" | "book" | "")
}

/// Weight (100–900) and italic flag a style name implies.
pub(super) fn style_weight(style: &str) -> (u16, bool) {
    let s = norm(style);
    let italic = s.contains("italic") || s.contains("oblique");
    let w = if s.contains("thin") || s.contains("hairline") {
        if s.contains("extrathin") || s.contains("ultrathin") { 50 } else { 100 }
    } else if s.contains("extralight") || s.contains("ultralight") {
        200
    } else if s.contains("light") {
        300
    } else if s.contains("semibold") || s.contains("demibold") || s.contains("demi") {
        600
    } else if s.contains("extrabold") || s.contains("ultrabold") {
        800
    } else if s.contains("bold") {
        700
    } else if s.contains("heavy") || s.contains("black") || s.contains("extrablack") {
        900
    } else if s.contains("medium") {
        500
    } else {
        400
    };
    (w, italic)
}

fn looks_korean(family: &str, font_type: Option<&str>) -> bool {
    if family.chars().any(|c| matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{3130}'..='\u{318F}')) {
        return true;
    }
    if font_type == Some("OpenTypeCID") {
        return true;
    }
    let n = norm(family);
    ["korean", "nanum", "myeongjo", "myungjo", "batang", "dotum", "gulim", "kr", "sandoll", "yoon", "hgg", "kopub"]
        .iter()
        .any(|k| n.contains(k))
        || n.starts_with("sd")
        || n.starts_with("yd")
        || n.starts_with("h2")
}

fn looks_serif(family: &str) -> bool {
    let n = norm(family);
    ["myeongjo", "myungjo", "batang", "serif", "mincho", "song", "ming"].iter().any(|k| n.contains(k))
}
