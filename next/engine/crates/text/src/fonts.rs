//! Font discovery, matching and glyph outlines.
//!
//! `FontStore::discover` reads only the table directory, `name`, `OS/2` and `fvar` of every
//! font file it finds (TrueType, OpenType, `.ttc`/`.otc` collections and the extension-less
//! OpenType files of Adobe Fonts) and keeps a name index. Faces are memory-mapped when a
//! document first uses them.
//!
//! The store is internally synchronised (`Send + Sync`): resolution, outlines and the
//! shaping caches take `&self`, so layout threads and render threads can share one store
//! behind an `Arc`. Only `add_dirs` needs `&mut self`.

use crate::shape::{PlanCache, ShapeCache};
use folio_model::FontInfo;
use folio_model::layout::{FontId, GlyphSource, OutlineCmd};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use unicode_normalization::UnicodeNormalization;
mod resource;

mod cache;
mod discovery;
mod fallback;
mod reader;

pub use fallback::Writing;
use fallback::style_weight;
use reader::{parse_names, read_faces};
#[cfg(test)]
use fallback::clean_family;
#[cfg(test)]
use reader::mac_roman;

/// Result of matching an InDesign family + style to a face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub font: FontId,
    /// The face is the one the document asked for (not a substitute).
    pub exact: bool,
}

/// Where a font file was found. Earlier sources win when the same face is in several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontSource {
    /// A caller-supplied folder such as the document's "Document fonts".
    Extra,
    /// Adobe Fonts (`%APPDATA%\Adobe\CoreSync\plugins\livetype\…`), ranked before a
    /// user-installed copy of the same face. On #347 InDesign's Pretendard has the first
    /// baseline of the 1000-unit Adobe Fonts file (CID 69 at 922) but the advances of a
    /// 2048-unit copy in the user folder (probably a third, encrypted Adobe build);
    /// this order reproduces more of InDesign's line ends (1329 vs 1324 of 1389).
    Adobe,
    /// Per-user fonts (`%LOCALAPPDATA%\Microsoft\Windows\Fonts`).
    User,
    /// System fonts (`%WINDIR%\Fonts`).
    System,
}

/// A face found by discovery. Only its names and a few `OS/2` values are read.
#[derive(Clone, Debug)]
pub struct FaceInfo {
    pub path: Arc<PathBuf>,
    /// Face index inside a collection (0 for single-face files).
    pub index: u32,
    pub source: FontSource,
    pub postscript: String,
    /// Preferred family (name ID 16, else 1), English when present.
    pub family: String,
    /// Preferred style (name ID 17, else 2).
    pub style: String,
    pub full_name: String,
    pub version: String,
    /// `OS/2.usWeightClass` (400 when absent).
    pub weight: u16,
    pub italic: bool,
    /// CFF/CFF2 outlines (OpenType PostScript flavour) rather than `glyf`.
    pub cff: bool,
    /// Variation coordinates of a variable font's named instance (empty: default).
    pub coords: Vec<([u8; 4], f32)>,
    /// (family, style) keys from name IDs 16/17 and 1/2 in every language, normalised.
    pairs: Vec<(String, String)>,
    /// Families from name IDs 16 and 1 in every language, normalised.
    families: Vec<String>,
    /// Full names (ID 4) in every language, normalised.
    fulls: Vec<String>,
}

impl FaceInfo {
    /// Normalised (family, style) keys and full names this face answers to.
    #[doc(hidden)]
    pub fn name_keys(&self) -> (&[(String, String)], &[String]) {
        (&self.pairs, &self.fulls)
    }
}

/// Font-wide metrics in em (1 em = 1.0), y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetrics {
    pub units_per_em: f64,
    /// `hhea` ascender / descender (descender negative) and line gap.
    pub ascender: f64,
    pub descender: f64,
    pub line_gap: f64,
    /// `OS/2` typographic ascender / descender when present, else hhea.
    pub typo_ascender: f64,
    pub typo_descender: f64,
    /// What InDesign's "Ascent" first-baseline offset uses (measured on #347, 28 faces):
    /// `OS/2.sTypoAscender` for Roman fonts; the top of "d" for fonts with Hangul
    /// (Apple SD Gothic Neo 0.758, NanumSquare_ac 0.73, NEXON Lv1 Gothic 0.78, Noto Sans
    /// KR 0.798), where a CID-keyed font's "d" is CID 69 (its Adobe-Korea1 CID) whatever
    /// glyph that is: Pretendard's CID 69 is a circumflex capital, 0.922 em, as measured.
    /// Japanese/Chinese-only faces use the exact outline top of 'd'
    /// (text-multilingual: Yu Gothic and Microsoft YaHei).
    pub ascent: f64,
    /// Height of capitals (`OS/2.sCapHeight`, else the top of "H").
    pub cap_height: f64,
    /// Height of lowercase (`OS/2.sxHeight`, else the top of "x").
    pub x_height: f64,
    /// Top of the ideographic em box (from `BASE` 'ideo' when present).
    pub embox_top: f64,
    /// Advance of U+0020 (0.25 em when the font has no space).
    pub space_advance: f64,
    /// Underline position (negative: below the baseline) and thickness (`post`).
    pub underline_position: f64,
    pub underline_thickness: f64,
    /// Strike-through position above the baseline and thickness (`OS/2`).
    pub strikeout_position: f64,
    pub strikeout_thickness: f64,
    /// The face's own bounding box (`head` yMax, and -yMin: both usually positive): the height of a
    /// condition's highlight indicator is the largest of these over the runs of a line
    /// (InDesign 2025, scenarios condx-hl-*, rules next/oracle/rules/condx-c4.ko.md).
    pub bbox_top: f64,
    pub bbox_bottom: f64,
}

impl FaceMetrics {
    pub const SYNTHETIC: FaceMetrics = FaceMetrics {
        units_per_em: 1000.0,
        ascender: 0.8,
        descender: -0.2,
        line_gap: 0.0,
        typo_ascender: 0.8,
        typo_descender: -0.2,
        ascent: 0.8,
        cap_height: 0.7,
        x_height: 0.5,
        embox_top: 0.88,
        space_advance: 0.25,
        underline_position: -0.1,
        underline_thickness: 0.05,
        strikeout_position: 0.25,
        strikeout_thickness: 0.05,
        bbox_top: 0.9,
        bbox_bottom: 0.3,
    };
}

enum FontData {
    Mapped(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl FontData {
    fn bytes(&self) -> &[u8] {
        match self {
            FontData::Mapped(m) => m,
            FontData::Owned(v) => v,
        }
    }
}

/// A face in use: its file mapped into memory, or the built-in synthetic face.
pub struct LoadedFace {
    /// Index into the store's discovered faces; `None` for the synthetic face.
    entry: Option<usize>,
    data: Option<FontData>,
    index: u32,
    coords: Vec<([u8; 4], f32)>,
    metrics: FaceMetrics,
}

impl LoadedFace {
    pub fn metrics(&self) -> FaceMetrics {
        self.metrics
    }

    /// Font bytes (`None` for the synthetic face).
    pub fn bytes(&self) -> Option<&[u8]> {
        self.data.as_ref().map(FontData::bytes)
    }

    /// A parsed face with its variation coordinates applied.
    pub fn ttf(&self) -> Option<ttf_parser::Face<'_>> {
        let mut face = ttf_parser::Face::parse(self.bytes()?, self.index).ok()?;
        for (tag, v) in &self.coords {
            face.set_variation(ttf_parser::Tag::from_bytes(tag), *v);
        }
        Some(face)
    }

    /// A rustybuzz face with its variation coordinates applied.
    pub fn shaping_face(&self) -> Option<rustybuzz::Face<'_>> {
        let mut face = rustybuzz::Face::from_slice(self.bytes()?, self.index)?;
        if !self.coords.is_empty() {
            let vars: Vec<rustybuzz::Variation> = self
                .coords
                .iter()
                .map(|(t, v)| rustybuzz::Variation { tag: ttf_parser::Tag::from_bytes(t), value: *v })
                .collect();
            face.set_variations(&vars);
        }
        Some(face)
    }
}

/// The faces loaded so far, for borrowing font data during a layout (see
/// `shape::FaceTable`).
pub struct Faces(pub(crate) Vec<Arc<LoadedFace>>);

impl Faces {
    pub fn get(&self, font: FontId) -> Option<&LoadedFace> {
        self.0.get(font as usize).map(|f| &**f)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

type OutlineCache = HashMap<(FontId, u16), Option<Arc<Vec<OutlineCmd>>>>;
const OUTLINE_SHARDS: usize = 32;

type FontLookupKey = (String, String, Vec<(String, String)>);

#[derive(Default)]
struct State {
    resolved: HashMap<FontLookupKey, Resolved>,
    loaded_entry: HashMap<usize, FontId>,
    /// Substitute face per writing system (`None`: none of its families is installed).
    substitutes: HashMap<Writing, Option<FontId>>,
    /// Face per character for text a substitute face lacks (`None`: no installed face has it).
    char_fallback: HashMap<char, Option<FontId>>,
    synthetic: Option<FontId>,
}

/// Installed and document fonts, loaded on demand. `Send + Sync`.
pub struct FontStore {
    faces: Vec<FaceInfo>,
    by_postscript: HashMap<String, Vec<usize>>,
    by_family: HashMap<String, Vec<usize>>,
    by_full: HashMap<String, Vec<usize>>,
    /// Folders scanned so far (as given and canonical).
    scanned: Vec<PathBuf>,
    /// Loaded faces, indexed by `FontId`; append-only.
    loaded: RwLock<Vec<Arc<LoadedFace>>>,
    state: Mutex<State>,
    outlines: Vec<RwLock<OutlineCache>>,
    pub(crate) shapes: ShapeCache,
    pub(crate) plans: PlanCache,
    pub(crate) glyph_catalogs: crate::glyphs::CatalogCache,
    /// Time spent scanning, for diagnostics.
    pub scan_ms: u64,
}

const _: () = {
    const fn send_sync<T: Send + Sync>() {}
    send_sync::<FontStore>();
};

impl std::fmt::Debug for FontStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loaded = self.loaded.read().map_or(0, |l| l.len());
        f.debug_struct("FontStore").field("faces", &self.faces.len()).field("loaded", &loaded).finish()
    }
}

impl FontStore {
    /// A store without any font file: every face is the synthetic one (0.5 em glyphs,
    /// 0.25 em spaces). Useful for tests and headless runs.
    pub fn empty() -> FontStore {
        FontStore {
            faces: Vec::new(),
            by_postscript: HashMap::new(),
            by_family: HashMap::new(),
            by_full: HashMap::new(),
            scanned: Vec::new(),
            loaded: RwLock::new(Vec::new()),
            state: Mutex::new(State::default()),
            outlines: (0..OUTLINE_SHARDS).map(|_| RwLock::new(HashMap::new())).collect(),
            shapes: ShapeCache::default(),
            plans: PlanCache::default(),
            glyph_catalogs: Default::default(),
            scan_ms: 0,
        }
    }
}

/// Lower-case and drop spaces, hyphens and underscores: "Noto Sans KR" → "notosanskr".
pub(crate) fn norm(s: &str) -> String {
    s.nfc().filter(|c| !matches!(c, ' ' | '-' | '_' | '\u{00A0}')).flat_map(char::to_lowercase).collect::<String>().nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_suffixes_and_weights() {
        assert_eq!(clean_family("Noto Sans KR (OTF)"), ("Noto Sans KR".into(), Some(true)));
        assert_eq!(clean_family("$ID/SD Gothic"), ("SD Gothic".into(), None));
        assert_eq!(style_weight("Bold Italic"), (700, true));
        assert_eq!(style_weight("ExtraBold"), (800, false));
        assert_eq!(style_weight("Light"), (300, false));
        assert_eq!(norm("Apple SD Gothic Neo"), "applesdgothicneo");
        assert_eq!(mac_roman(0x80), 'Ä');
        assert_eq!(mac_roman(0xFF), 'ˇ');
    }

    #[test]
    fn writing_scripts() {
        assert_eq!(Writing::of_script(0, "FolioMissing Gothic"), Some(Writing::Latin));
        assert_eq!(Writing::of_script(3, "FolioMissing Gothic"), Some(Writing::Korean));
        assert_eq!(Writing::of_script(3, "FolioMissing Myungjo"), Some(Writing::KoreanSerif));
        assert_eq!(Writing::of_script(1, "Toppan Bunkyu Midashi Gothic"), Some(Writing::Japanese));
        assert_eq!(Writing::of_script(25, "Some Song"), Some(Writing::Chinese));
        assert_eq!(Writing::of_script(4, "Adobe Arabic"), Some(Writing::Arabic));
        assert_eq!(Writing::of_script(32, "Symbols"), None);
    }

    #[test]
    fn empty_store_falls_back_to_synthetic() {
        let s = FontStore::empty();
        let r = s.resolve("Minion Pro", "Regular", &[]);
        assert!(!r.exact);
        assert!(s.is_synthetic(r.font));
        assert!(s.outline(r.font, 1).is_none());
    }

    #[test]
    fn korean_font_names_compare_across_normalisation_forms() {
        for name in ["맑은 고딕", "애플 산돌고딕 Neo", "Café Regular"] {
            assert_eq!(norm(name), norm(&name.nfd().collect::<String>()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn decomposed_korean_font_file_and_family() {
        let windows = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let source = windows.join("Fonts/malgun.ttf");
        if !source.is_file() { return; } // Optional system fixture; never use Adobe fonts.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).unwrap().join("artifacts/mac-i18n-20261020/unit-tests").join(format!("folio-font-nfd-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("한글 글꼴.ttf".nfd().collect::<String>());
        std::fs::copy(source, &file).unwrap();
        {
            let store = FontStore::from_dirs(&[(root.clone(), FontSource::Extra)]);
            let nfc = store.resolve("맑은 고딕", "Regular", &[]);
            let nfd = store.resolve(&"맑은 고딕".nfd().collect::<String>(), "Regular", &[]);
            assert!(nfc.exact && nfd.exact);
            assert_eq!(nfc.font, nfd.font);
            assert_eq!(store.face_info(nfc.font).unwrap().path.as_ref(), &file);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
