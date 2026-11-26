//! Shaping a paragraph into clusters: one composer item per character cluster.
//!
//! A paragraph is cut into segments of one style run, one face (after per-character
//! fallback for substituted runs), one script and one bidi direction; each segment is
//! shaped with rustybuzz. Clusters stay in logical (typed) order whatever the direction;
//! paragraphs with right-to-left text carry bidi levels (`ShapedParagraph::levels`, Unicode
//! bidi algorithm, left-to-right paragraph as InDesign's default `ParagraphDirection`) and
//! layout reorders each line by them.
//! Widths are in points: font units / units per em in InDesign's 1/65536 em precision
//! ([`em_fixed`]), × point size, times the horizontal scale. Tracking (1/1000 em) is kept
//! separately because InDesign drops it after the last glyph of a line.
//!
//! Shaped segments are cached in the `FontStore` size-free (advances in ems, offsets in
//! font units), keyed by face, script,
//! language, features and text, so a size, tracking or scale change and a re-layout of
//! an edited story reuse them. Shape plans are shared the same way.

use crate::fonts::{FaceMetrics, Faces, FontStore, Resolved};
use folio_model::layout::FontId;
use folio_model::{Capitalization, CharacterAttrs, KerningMethod, Paragraph, Position, TextPreferences};
use rustybuzz::ttf_parser::{GlyphId, Tag};
use rustybuzz::{Direction, Face, Feature, Language, Script, ShapePlan, UnicodeBuffer};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterKind {
    /// Glyphs from the font.
    Glyphs,
    /// A word space (U+0020, U+00A0): justification widens or narrows it.
    Space,
    /// A fixed-width space (em, en, thin, hair, …): InDesign sizes it from the point size.
    FixedSpace,
    Tab,
    /// Right indent tab (U+0008): text after it is set flush to the line's right edge.
    RightTab,
    /// Indent to here (U+0007): later lines of the paragraph start at this position.
    IndentHere,
    /// Forced line break.
    Break,
    /// An anchored or inline object (U+FFFC).
    Object,
    /// A discretionary hyphen (U+00AD): invisible unless a line ends there.
    SoftHyphen,
}

#[derive(Clone, Debug)]
pub struct Cluster {
    /// First character of the cluster (as typed, before case mapping).
    pub ch: char,
    /// Paragraph-relative character range.
    pub chars: Range<usize>,
    pub run: usize,
    pub font: FontId,
    /// Point size the glyphs are drawn at (after superscript / small caps).
    pub size: f64,
    /// Natural advance in points, horizontal scale applied, without tracking. For word
    /// spaces: the 100% space.
    pub advance: f64,
    /// Tracking after the cluster, in points.
    pub tracking: f64,
    /// Glyphs of the cluster in `ShapedParagraph::glyphs`.
    pub glyphs: Range<usize>,
    pub kind: ClusterKind,
    /// Baseline shift up, in points (baseline shift plus superscript / subscript offset).
    pub rise: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub font: FontId,
    pub id: u16,
    /// Offset from the cluster's origin, in points (x right, y up).
    pub dx: f64,
    pub dy: f64,
}

/// The hyphen InDesign draws at a hyphenated line end, per run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunHyphen {
    pub font: FontId,
    pub glyph: u16,
    /// Advance without tracking, in points.
    pub width: f64,
    pub size: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ShapedParagraph {
    pub clusters: Vec<Cluster>,
    pub glyphs: Vec<ShapedGlyph>,
    /// Hyphen per run.
    pub hyphens: Vec<RunHyphen>,
    /// Characters in the paragraph (without its return).
    pub char_count: usize,
    /// Bidi embedding level of every character; empty when the paragraph is left to right
    /// and has no right-to-left text (every level 0).
    pub levels: Vec<u8>,
    /// The paragraph's base direction is right to left (`ParagraphAttrs::right_to_left`).
    pub rtl: bool,
    /// Characters set as the .notdef glyph (an empty box) because their face has no glyph
    /// for them: (character, run). A face the document asked for keeps such characters, as
    /// InDesign does; a substitute face lacks them only when no installed face has them.
    pub missing: Vec<(usize, usize)>,
    /// Vertical text: how each cluster sits (`crate::vertical::adjust`); empty for horizontal text.
    pub vertical: Vec<crate::vertical::Orientation>,
}

impl ShapedParagraph {
    /// Bidi level of character `i` (0 = left to right).
    pub fn level(&self, i: usize) -> u8 {
        self.levels.get(i).copied().unwrap_or(0)
    }
}

/// Synthesised small caps and superscript / subscript geometry (document text
/// preferences, as fractions of the point size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapePrefs {
    pub small_cap: f64,
    pub superscript_size: f64,
    pub superscript_position: f64,
    pub subscript_size: f64,
    pub subscript_position: f64,
}

impl Default for ShapePrefs {
    fn default() -> Self {
        ShapePrefs::from(&TextPreferences::default())
    }
}

impl From<&TextPreferences> for ShapePrefs {
    fn from(p: &TextPreferences) -> Self {
        ShapePrefs {
            small_cap: p.small_cap / 100.0,
            superscript_size: p.superscript_size / 100.0,
            superscript_position: p.superscript_position / 100.0,
            subscript_size: p.subscript_size / 100.0,
            subscript_position: p.subscript_position / 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Scr {
    Common,
    Latin,
    Greek,
    Cyrillic,
    Hangul,
    Han,
    Kana,
    /// Any other script (Arabic, Hebrew, Thai, Devanagari, …).
    Other(Script),
}

impl Scr {
    fn of(c: char) -> Scr {
        match c {
            '\u{AC00}'..='\u{D7A3}'
            | '\u{1100}'..='\u{11FF}'
            | '\u{3130}'..='\u{318F}'
            | '\u{A960}'..='\u{A97F}'
            | '\u{D7B0}'..='\u{D7FF}' => Scr::Hangul,
            '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FFFF}' => {
                Scr::Han
            }
            '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => Scr::Kana,
            '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}' => Scr::Greek,
            '\u{0400}'..='\u{052F}' => Scr::Cyrillic,
            '\u{1E00}'..='\u{1EFF}' | '\u{FB00}'..='\u{FB06}' => Scr::Latin,
            c if c.is_ascii() => {
                if c.is_ascii_alphabetic() {
                    Scr::Latin
                } else {
                    Scr::Common
                }
            }
            c => {
                use unicode_script::{Script as U, UnicodeScript};
                // Letters below U+2000 without a script of their own (µ, ª) count as Latin,
                // later Latin-script characters (fullwidth forms) as common.
                let latin_or_common = || if c.is_alphabetic() && (c as u32) < 0x2000 { Scr::Latin } else { Scr::Common };
                match c.script() {
                    U::Latin | U::Common | U::Inherited | U::Unknown => latin_or_common(),
                    U::Greek => Scr::Greek,
                    U::Cyrillic => Scr::Cyrillic,
                    U::Hangul => Scr::Hangul,
                    U::Han => Scr::Han,
                    U::Hiragana | U::Katakana => Scr::Kana,
                    s => {
                        let tag = s.short_name().as_bytes();
                        match <[u8; 4]>::try_from(tag).ok().and_then(|t| Script::from_iso15924_tag(Tag::from_bytes(&t))) {
                            Some(script) => Scr::Other(script),
                            None => Scr::Common,
                        }
                    }
                }
            }
        }
    }

    fn script(self) -> Script {
        match self {
            Scr::Hangul => rustybuzz::script::HANGUL,
            Scr::Han => rustybuzz::script::HAN,
            Scr::Kana => rustybuzz::script::KATAKANA,
            Scr::Greek => rustybuzz::script::GREEK,
            Scr::Cyrillic => rustybuzz::script::CYRILLIC,
            Scr::Latin | Scr::Common => rustybuzz::script::LATIN,
            Scr::Other(s) => s,
        }
    }

    fn is_roman(self) -> bool {
        matches!(self, Scr::Latin | Scr::Greek | Scr::Cyrillic | Scr::Common)
    }
}

/// InDesign's widths of fixed spaces, in em.
fn fixed_space_em(c: char) -> Option<f64> {
    Some(match c {
        '\u{2003}' => 1.0,        // em space
        '\u{2002}' => 0.5,        // en space
        '\u{2004}' => 1.0 / 3.0,  // third space
        '\u{2005}' => 0.25,       // quarter space
        '\u{2006}' => 1.0 / 6.0,  // sixth space
        '\u{2009}' => 0.125,      // thin space
        '\u{200A}' => 1.0 / 24.0, // hair space
        '\u{202F}' => 0.125,      // fixed-width nonbreaking space (approximation)
        '\u{3000}' => 1.0,        // ideographic space
        _ => return None,
    })
}

fn special_kind(c: char) -> Option<ClusterKind> {
    match c {
        '\n' => Some(ClusterKind::Break),
        '\t' => Some(ClusterKind::Tab),
        '\u{0008}' => Some(ClusterKind::RightTab),
        '\u{0007}' => Some(ClusterKind::IndentHere),
        '\u{FFFC}' => Some(ClusterKind::Object),
        '\u{00AD}' => Some(ClusterKind::SoftHyphen),
        c if fixed_space_em(c).is_some() => Some(ClusterKind::FixedSpace),
        _ => None,
    }
}

fn is_blank(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}' | '\u{FEFF}' | '\u{2060}'..='\u{2064}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{061C}'
        )
}

/// A combining mark or joiner (Unicode script "Inherited": it takes its base's script).
fn is_mark(c: char) -> bool {
    use unicode_script::UnicodeScript;
    (c as u32) >= 0x300 && c.script() == unicode_script::Script::Inherited
}

/// Characters that may make a paragraph's bidi levels non-zero (strong right-to-left
/// letters and the right-to-left controls): a cheap test before running the bidi algorithm.
fn maybe_rtl(c: char) -> bool {
    matches!(
        c,
        '\u{0590}'..='\u{08FF}'
            | '\u{FB1D}'..='\u{FDFF}'
            | '\u{FE70}'..='\u{FEFF}'
            | '\u{10800}'..='\u{10FFF}'
            | '\u{1E800}'..='\u{1EFFF}'
            | '\u{200F}'
            | '\u{202B}'
            | '\u{202E}'
            | '\u{2067}'
    )
}

/// Bidi level of every character (Unicode bidi algorithm, paragraph level 1 when
/// `rtl`, else 0); empty when every level is 0.
fn bidi_levels(chars: &[(char, usize)], rtl: bool) -> Vec<u8> {
    if !rtl && !chars.iter().any(|&(c, _)| maybe_rtl(c)) {
        return Vec::new();
    }
    let text: String = chars.iter().map(|&(c, _)| c).collect();
    let base = if rtl { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() };
    let info = unicode_bidi::ParagraphBidiInfo::new(&text, Some(base));
    let levels: Vec<u8> = text.char_indices().map(|(b, _)| info.levels.get(b).map_or(0, |l| l.number())).collect();
    if levels.iter().all(|&l| l == 0) { Vec::new() } else { levels }
}

/// Language tag for an InDesign language name.
fn language_tag(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    for (prefix, tag) in [
        ("english", "en"),
        ("korean", "ko"),
        ("japanese", "ja"),
        ("chinese", "zh"),
        ("german", "de"),
        ("french", "fr"),
        ("spanish", "es"),
        ("italian", "it"),
        ("arabic", "ar"),
        ("hebrew", "he"),
        ("thai", "th"),
        ("hindi", "hi"),
    ] {
        if n.starts_with(prefix) {
            return tag;
        }
    }
    "en"
}

/// Tracking in points after every character of a run: 1/1000 em of the run's point size
/// (also for synthesised small capitals, measured: NanumSquare_ac 12 pt small caps), scaled
/// by the horizontal scale like the glyphs (measured: Apple SD Gothic Neo Medium 10.47 pt,
/// tracking −40, 104% gives −0.4354 pt, not −0.4186). Manual kerning (`kerning_value`)
/// counts the same way.
pub fn tracking_pt(a: &CharacterAttrs) -> f64 {
    (a.tracking + a.kerning_value) / 1000.0 * a.point_size * a.horizontal_scale / 100.0
}

// ---------------------------------------------------------------------------------------
// Caches shared through the FontStore

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlanKey {
    font: FontId,
    script: Script,
    /// Shaped right to left (an odd bidi level).
    rtl: bool,
    lang: &'static str,
    features: Vec<(Tag, u32)>,
}

/// Shape plans per face, script, language and features.
#[derive(Default)]
pub(crate) struct PlanCache {
    map: RwLock<HashMap<PlanKey, Arc<ShapePlan>>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct SegKey {
    plan: PlanKey,
    text: Box<str>,
}

/// One shaped glyph: cluster = byte offset into the segment text; offsets in font units.
#[derive(Clone, Copy, Debug)]
struct RawGlyph {
    id: u16,
    cluster: u32,
    /// Advance in ems as InDesign computes it ([`em_fixed`]).
    advance_em: f64,
    x_offset: i32,
    y_offset: i32,
}

/// A width in font units as a fraction of the em in 16.16 fixed point, truncated toward
/// zero. InDesign's glyph advances and kerning adjustments have this precision: its widths
/// are whole multiples of 1/65536 em (#347: NEXON Lv1 Gothic Bold 944/1000 em at
/// 42.7107 pt is 61865/65536 em = 40.31828 pt, not 40.31892), the kerning separately.
/// Tracking is not quantised. Without this, a frame fitted to its text in InDesign is
/// up to ~0.001 pt too narrow for the same line here.
pub fn em_fixed(units: i32, units_per_em: f64) -> f64 {
    (f64::from(units) / units_per_em * 65536.0).trunc() / 65536.0
}

const SHAPE_SHARDS: usize = 32;
/// Segments kept per shard before the shard is cleared (bounds memory to roughly
/// 32 × 8192 segments).
const SHAPE_SHARD_LIMIT: usize = 8192;

type ShapeShard = Mutex<HashMap<SegKey, Arc<[RawGlyph]>>>;

/// Shaped segments in font units, shared by layout threads and across layouts.
pub(crate) struct ShapeCache {
    shards: Vec<ShapeShard>,
}

impl Default for ShapeCache {
    fn default() -> Self {
        ShapeCache { shards: (0..SHAPE_SHARDS).map(|_| Mutex::new(HashMap::new())).collect() }
    }
}

impl ShapeCache {
    fn shard(&self, key: &SegKey) -> &ShapeShard {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        &self.shards[(h.finish() as usize) % SHAPE_SHARDS]
    }

    fn get(&self, key: &SegKey) -> Option<Arc<[RawGlyph]>> {
        self.shard(key).lock().ok()?.get(key).cloned()
    }

    pub(crate) fn clear(&self) {
        for shard in &self.shards {
            if let Ok(mut s) = shard.lock() {
                s.clear();
            }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.shards.iter().map(|s| s.lock().map_or(0, |s| s.len())).sum()
    }

    fn put(&self, key: SegKey, glyphs: Arc<[RawGlyph]>) {
        if let Ok(mut shard) = self.shard(&key).lock() {
            if shard.len() >= SHAPE_SHARD_LIMIT {
                shard.clear();
            }
            shard.insert(key, glyphs);
        }
    }
}

// ---------------------------------------------------------------------------------------
// Faces for one layout

/// Shaping faces for the faces loaded in a `FontStore`, created on first use and shared by
/// threads. Borrows a `Faces` snapshot, which must hold every face the runs resolve to.
pub struct FaceTable<'a> {
    store: &'a FontStore,
    faces: &'a Faces,
    shaping: Vec<OnceLock<Option<Face<'a>>>>,
    smcp: Vec<OnceLock<bool>>,
}

impl<'a> FaceTable<'a> {
    pub fn new(store: &'a FontStore, faces: &'a Faces) -> Self {
        FaceTable {
            store,
            faces,
            shaping: (0..faces.len()).map(|_| OnceLock::new()).collect(),
            smcp: (0..faces.len()).map(|_| OnceLock::new()).collect(),
        }
    }

    /// The face of `font` for reading its tables (vertical metrics and forms).
    pub(crate) fn ttf_face(&self, font: FontId) -> Option<&Face<'a>> {
        self.face(font)
    }

    fn face(&self, font: FontId) -> Option<&Face<'a>> {
        let faces: &'a Faces = self.faces;
        self.shaping.get(font as usize)?.get_or_init(|| faces.get(font)?.shaping_face()).as_ref()
    }

    fn units_per_em(&self, font: FontId) -> f64 {
        self.metrics(font).units_per_em
    }

    pub fn metrics(&self, font: FontId) -> FaceMetrics {
        self.faces.get(font).map_or(FaceMetrics::SYNTHETIC, |f| f.metrics())
    }

    pub fn is_synthetic(&self, font: FontId) -> bool {
        self.faces.get(font).is_none_or(|f| f.bytes().is_none())
    }

    /// The glyph of `c` in `font` and its advance in ems (unshaped: emphasis marks).
    pub fn glyph(&self, font: FontId, c: char) -> Option<(u16, f64)> {
        let face = self.face(font)?;
        let g = face.glyph_index(c)?;
        Some((g.0, f64::from(face.glyph_hor_advance(g)?) / self.units_per_em(font)))
    }

    /// Top of the outline of `c` in ems (the height of a capital 'H': a drop cap is sized by it).
    pub fn glyph_top(&self, font: FontId, c: char) -> Option<f64> {
        let face = self.face(font)?;
        let b = face.glyph_bounding_box(face.glyph_index(c)?)?;
        Some(f64::from(b.y_max) / self.units_per_em(font))
    }

    /// Bottom of the outline of `c` in ems (negative below the baseline: a drop cap's descender).
    pub fn glyph_bottom(&self, font: FontId, c: char) -> Option<f64> {
        let face = self.face(font)?;
        let b = face.glyph_bounding_box(face.glyph_index(c)?)?;
        Some(f64::from(b.y_min) / self.units_per_em(font))
    }

    /// Left side bearing of a glyph in ems: the left edge of its outline (a drop cap aligns it
    /// with the text's left edge).
    pub fn left_bearing(&self, font: FontId, glyph: u16) -> Option<f64> {
        let face = self.face(font)?;
        let b = face.glyph_bounding_box(ttf_parser::GlyphId(glyph))?;
        Some(f64::from(b.x_min) / self.units_per_em(font))
    }

    /// Whether `font` has a glyph for `c` (the synthetic face has every glyph; a face loaded
    /// after this table's snapshot has none).
    pub fn covers(&self, font: FontId, c: char) -> bool {
        match self.faces.get(font) {
            None => false,
            Some(f) if f.bytes().is_none() => true,
            Some(_) => self.face(font).is_some_and(|face| face.glyph_index(c).is_some()),
        }
    }

    fn has_smcp(&self, font: FontId) -> bool {
        let Some(slot) = self.smcp.get(font as usize) else { return false };
        *slot.get_or_init(|| {
            let tag = Tag::from_bytes(b"smcp");
            self.face(font).is_some_and(|f| f.tables().gsub.is_some_and(|g| g.features.index(tag).is_some()))
        })
    }
}

/// Shapes paragraphs. Cheap to create; all state lives in the `FaceTable` and the store.
pub struct Shaper<'t, 'a> {
    table: &'t FaceTable<'a>,
    prefs: ShapePrefs,
    /// Faces for characters a substituted run's face lacks ([`FontStore::fallbacks`]).
    fallbacks: &'t HashMap<char, FontId>,
}

impl<'t, 'a> Shaper<'t, 'a> {
    /// `fallbacks` must be loaded before the `FaceTable`'s snapshot was taken.
    pub fn new(table: &'t FaceTable<'a>, fallbacks: &'t HashMap<char, FontId>) -> Self {
        Shaper { table, prefs: ShapePrefs::default(), fallbacks }
    }

    pub fn with_prefs(mut self, prefs: ShapePrefs) -> Self {
        self.prefs = prefs;
        self
    }

    pub fn table(&self) -> &'t FaceTable<'a> {
        self.table
    }

    /// Point size and rise of a run.
    fn run_geometry(&self, a: &CharacterAttrs) -> (f64, f64) {
        let size = a.point_size.max(0.1);
        let p = &self.prefs;
        // The superscript / subscript offset follows the vertical scale (oracle charpanel-c3 cp-super-vscale: 12 pt at 150 % rises 5.994 pt, not 3.996).
        let vscale = a.vertical_scale / 100.0;
        match a.position {
            Position::Superscript => (size * p.superscript_size, a.baseline_shift + size * p.superscript_position * vscale),
            Position::Subscript => (size * p.subscript_size, a.baseline_shift - size * p.subscript_position * vscale),
            _ => (size, a.baseline_shift),
        }
    }

    /// The face for character `c` of a run resolved to `primary`. A run set in the font
    /// the document asked for keeps it even for characters it lacks: InDesign then sets
    /// the font's .notdef glyph with its advance (measured: Avenir with Hangul; Arial,
    /// Georgia and Malgun Gothic with Hangul, Arabic and Thai, InDesign 2025 PDF/X-4). A
    /// substituted run takes such characters from a fallback face instead (InDesign would
    /// print the substitute's .notdef box).
    fn font_for(&self, c: char, primary: Resolved) -> FontId {
        let Resolved { font: primary, exact } = primary;
        if exact || is_blank(c) || self.table.covers(primary, c) {
            return primary;
        }
        self.fallbacks.get(&c).copied().filter(|&f| self.table.covers(f, c)).unwrap_or(primary)
    }

    /// The glyph a U+FFFD character of a run shows when the run carries `CustomGlyph` (a glyph of the run's own face
    /// that has no Unicode value; InDesign stores it as U+FFFD plus the property). A face the document did not get,
    /// or a glyph id the face does not have, leaves the character to the ordinary path.
    fn custom_glyph(&self, c: char, a: &CharacterAttrs, primary: Resolved) -> Option<u16> {
        let g = a.custom_glyph.filter(|_| c == '\u{FFFD}' && primary.exact)?;
        let face = self.table.face(primary.font)?;
        (g < face.number_of_glyphs()).then_some(g)
    }

    /// One custom glyph: a cluster of its own, the glyph's nominal advance (no kerning with its neighbours).
    fn shape_custom_glyph(&self, at: usize, run: usize, a: &CharacterAttrs, font: FontId, (size, rise): (f64, f64), out: &mut ShapedParagraph) {
        let Some(g) = a.custom_glyph else { return };
        let upem = self.table.units_per_em(font);
        let advance = self.table.face(font).and_then(|f| f.glyph_hor_advance(GlyphId(g))).map_or(0, i32::from);
        let first = out.glyphs.len();
        out.glyphs.push(ShapedGlyph { font, id: g, dx: 0.0, dy: 0.0 });
        out.clusters.push(Cluster {
            ch: '\u{FFFD}',
            chars: at..at + 1,
            run,
            font,
            size,
            advance: em_fixed(advance, upem) * size * a.horizontal_scale / 100.0,
            tracking: tracking_pt(a) * size / a.point_size.max(0.1),
            glyphs: first..out.glyphs.len(),
            kind: ClusterKind::Glyphs,
            rise,
        });
    }

    /// Hyphen glyph and width for a run.
    fn hyphen(&self, font: FontId, a: &CharacterAttrs, size: f64) -> RunHyphen {
        match self.table.face(font) {
            Some(face) => {
                let glyph = face.glyph_index('-').or_else(|| face.glyph_index('\u{2010}'));
                let adv = glyph.and_then(|g| face.glyph_hor_advance(g)).map_or(0, i32::from);
                let width = em_fixed(adv, self.table.units_per_em(font)) * size * a.horizontal_scale / 100.0;
                RunHyphen { font, glyph: glyph.map_or(0, |g| g.0), width, size }
            }
            None => RunHyphen { font, glyph: 0, width: size * 0.33 * a.horizontal_scale / 100.0, size },
        }
    }

    fn synth_small_caps(&self, a: &CharacterAttrs, c: char, font: FontId) -> bool {
        matches!(a.capitalization, Capitalization::SmallCaps)
            && c.is_lowercase()
            && !self.table.has_smcp(font)
    }

    /// Shapes `para`; `run_fonts[i]` is the face resolved for run `i`. `object_width(run,
    /// k)` is the width of the `k`-th U+FFFC of a run.
    pub fn shape_paragraph(
        &self,
        para: &Paragraph,
        run_fonts: &[Resolved],
        object_width: &dyn Fn(usize, usize) -> f64,
    ) -> ShapedParagraph {
        let mut chars: Vec<(char, usize)> = Vec::new();
        for (r, run) in para.runs.iter().enumerate() {
            chars.extend(run.text.chars().map(|c| (c, r)));
        }
        let rtl = para.attrs.right_to_left;
        let composer = para.attrs.composer.as_str();
        let cjk = matches!(composer, "HL Composer J" | "HL Single J");
        let logical_order = !rtl && (cjk || matches!(composer, "HL Composer" | "HL Single"));
        let levels = if logical_order { Vec::new() } else { bidi_levels(&chars, rtl) };
        let level = |i: usize| levels.get(i).copied().unwrap_or(0);
        let mut out = ShapedParagraph { char_count: chars.len(), rtl, ..ShapedParagraph::default() };
        for (r, run) in para.runs.iter().enumerate() {
            let (size, _) = self.run_geometry(&run.attrs);
            out.hyphens.push(self.hyphen(run_fonts[r].font, &run.attrs, size));
        }
        let mut objects_seen = vec![0usize; para.runs.len()];
        let n = chars.len();
        let mut i = 0;
        while i < n {
            let (c, r) = chars[i];
            let a = &para.runs[r].attrs;
            let primary = run_fonts[r];
            let (size, rise) = self.run_geometry(a);
            if let Some(kind) = special_kind(c) {
                let advance = match kind {
                    ClusterKind::FixedSpace => fixed_space_em(c).unwrap_or(0.0) * size * a.horizontal_scale / 100.0,
                    ClusterKind::Object => {
                        let k = objects_seen[r];
                        objects_seen[r] += 1;
                        object_width(r, k)
                    }
                    ClusterKind::SoftHyphen => out.hyphens[r].width,
                    _ => 0.0,
                };
                let tracking = if kind == ClusterKind::FixedSpace { tracking_pt(a) * size / a.point_size.max(0.1) } else { 0.0 };
                let g = out.glyphs.len();
                out.clusters.push(Cluster {
                    ch: c,
                    chars: i..i + 1,
                    run: r,
                    font: primary.font,
                    size,
                    advance,
                    tracking,
                    glyphs: g..g,
                    kind,
                    rise,
                });
                i += 1;
                continue;
            }
            if self.custom_glyph(c, a, primary).is_some() {
                self.shape_custom_glyph(i, r, a, primary.font, (size, rise), &mut out);
                i += 1;
                continue;
            }
            let font = self.font_for(c, primary);
            let synth_caps = self.synth_small_caps(a, c, font);
            let mut script = Scr::of(c);
            let lv = level(i);
            let mut j = i + 1;
            while j < n {
                let (c2, r2) = chars[j];
                if r2 != r || special_kind(c2).is_some() || level(j) != lv || self.custom_glyph(c2, a, primary).is_some() {
                    break;
                }
                let s2 = Scr::of(c2);
                if s2 != Scr::Common && script != Scr::Common && s2 != script {
                    break;
                }
                // A combining mark stays with its base when that face has it.
                let f2 = self.font_for(c2, primary);
                let mark_here = f2 != font && is_mark(c2) && self.table.covers(font, c2);
                if (f2 != font && !mark_here) || self.synth_small_caps(a, c2, font) != synth_caps {
                    break;
                }
                if script == Scr::Common {
                    script = s2;
                }
                j += 1;
            }
            if logical_order && script == Scr::Other(rustybuzz::script::ARABIC) {
                script = Scr::Common;
            }
            self.shape_segment(&chars, i..j, a, font, script, lv % 2 == 1, synth_caps, cjk, (size, rise), &mut out);
            i = j;
        }
        if !rtl {
            self.kern_run_boundaries(para, &mut out);
            self.kern_inline_objects(para, &mut out);
        }
        out.levels = levels;
        out
    }

    /// Restore metric pair spacing across compatible character-format boundaries.
    fn kern_run_boundaries(&self, para: &Paragraph, out: &mut ShapedParagraph) {
        for i in 0..out.clusters.len().saturating_sub(1) {
            let (left, right) = (&out.clusters[i], &out.clusters[i + 1]);
            let (a, b) = (&para.runs[left.run].attrs, &para.runs[right.run].attrs);
            // charpanel-c3 (oracle cp-super-word, cp-vscale-word, cp-skew-word): a Superscript/Subscript boundary keeps the pair
            // (the nominal size is the same; the left glyph's size scales it), a different vertical scale or skew does not.
            let position_only = a.position != b.position
                && a.point_size == b.point_size
                && a.capitalization == b.capitalization;
            if left.run == right.run
                || left.font != right.font
                || ((left.size - right.size).abs() > 1e-8 && !position_only)
                || a.horizontal_scale != b.horizontal_scale
                || a.vertical_scale != b.vertical_scale
                || a.skew != b.skew
                || a.baseline_shift != b.baseline_shift
                || a.kerning_manual
                || a.kerning_value != 0.0
                || !matches!(
                    a.kerning_method,
                    KerningMethod::Metrics | KerningMethod::MetricsRomanOnly
                )
                || !matches!(left.kind, ClusterKind::Glyphs | ClusterKind::Space)
                || !matches!(right.kind, ClusterKind::Glyphs | ClusterKind::Space)
                || left.chars.len() != 1
                || right.chars.len() != 1
                || !left.ch.is_ascii()
                || !right.ch.is_ascii()
            {
                continue;
            }
            let Some(face) = self.table.face(left.font) else {
                continue;
            };
            let case = |c: char, a: &CharacterAttrs| {
                if matches!(a.capitalization, Capitalization::AllCaps) {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            };
            let pair = format!("{}{}", case(left.ch, a), case(right.ch, b));
            let widths: Vec<f64> = [0, 1]
                .into_iter()
                .map(|kern| {
                    let key = PlanKey {
                        font: left.font,
                        script: Scr::Latin.script(),
                        rtl: false,
                        lang: language_tag(&a.language),
                        features: vec![
                            (Tag::from_bytes(b"kern"), kern),
                            (Tag::from_bytes(b"liga"), 0),
                            (Tag::from_bytes(b"clig"), 0),
                        ],
                    };
                    self.raw_glyphs(face, key, &pair)
                        .iter()
                        .map(|g| g.advance_em)
                        .sum()
                })
                .collect();
            let adjustment = (widths[1] - widths[0]) * left.size * a.horizontal_scale / 100.0;
            out.clusters[i].advance += adjustment;
        }
    }

    /// Pair kerning around an inline object (outlines-c3, measured with `TextRange.createOutlines` in InDesign 2025:
    /// `artifacts/outlines-c3-20261023/measure/k-*.jsonl`, `n-*.jsonl`). InDesign kerns the object character as if it were
    /// a word space: the letter before it by the font's pair (letter, space) — Arial A -113, L -76, P T Y -37, Times New
    /// Roman also V W -37, P Y -76 (units of 2048) — and the letter after it by (space, letter). Fonts without such pairs
    /// (Georgia, Calibri, Verdana, Tahoma, Malgun Gothic) are not kerned. The first moves the object, the second the text
    /// after it. Only objects that take room in the line (inline) count.
    fn kern_inline_objects(&self, para: &Paragraph, out: &mut ShapedParagraph) {
        let plain = |a: &CharacterAttrs| !a.kerning_manual && a.kerning_value == 0.0 && matches!(a.kerning_method, KerningMethod::Metrics | KerningMethod::MetricsRomanOnly);
        let latin = |c: char| matches!(Scr::of(c), Scr::Latin | Scr::Common);
        let adjust = |font: FontId, a: &CharacterAttrs, pair: &str, size: f64| -> f64 {
            let Some(face) = self.table.face(font) else { return 0.0 };
            let widths: Vec<f64> = [0, 1]
                .into_iter()
                .map(|kern| {
                    let key = PlanKey { font, script: Scr::Latin.script(), rtl: false, lang: language_tag(&a.language), features: vec![(Tag::from_bytes(b"kern"), kern), (Tag::from_bytes(b"liga"), 0), (Tag::from_bytes(b"clig"), 0)] };
                    self.raw_glyphs(face, key, pair).iter().map(|g| g.advance_em).sum()
                })
                .collect();
            (widths[1] - widths[0]) * size * a.horizontal_scale / 100.0
        };
        let case = |c: char, a: &CharacterAttrs| if matches!(a.capitalization, Capitalization::AllCaps) { c.to_ascii_uppercase() } else { c };
        for i in 0..out.clusters.len() {
            if out.clusters[i].kind != ClusterKind::Object || out.clusters[i].advance <= 0.0 {
                continue;
            }
            let obj_attrs = &para.runs[out.clusters[i].run].attrs;
            if i > 0 {
                let l = &out.clusters[i - 1];
                let a = &para.runs[l.run].attrs;
                if l.kind == ClusterKind::Glyphs && l.chars.len() == 1 && latin(l.ch) && plain(a) {
                    let adj = adjust(l.font, a, &format!("{} ", case(l.ch, a)), l.size);
                    out.clusters[i - 1].advance += adj;
                }
            }
            if let Some(r) = out.clusters.get(i + 1) {
                let o = &out.clusters[i];
                if r.kind == ClusterKind::Glyphs && r.chars.len() == 1 && latin(r.ch) && r.font == o.font && plain(obj_attrs) {
                    let adj = adjust(o.font, obj_attrs, &format!(" {}", case(r.ch, &para.runs[r.run].attrs)), o.size);
                    out.clusters[i].advance += adj;
                }
            }
        }
    }

    fn features(a: &CharacterAttrs, script: Scr, synth_caps: bool) -> Vec<(Tag, u32)> {
        let kern = !a.kerning_manual && a.kerning_value==0.0 && match a.kerning_method {
            KerningMethod::None => false,
            KerningMethod::MetricsRomanOnly => script.is_roman(),
            KerningMethod::Metrics | KerningMethod::Optical => true,
        };
        let mut features: Vec<(Tag, u32)> = vec![
            (Tag::from_bytes(b"kern"), u32::from(kern)),
            (Tag::from_bytes(b"liga"), u32::from(a.ligatures)),
            (Tag::from_bytes(b"clig"), u32::from(a.ligatures)),
        ];
        match a.capitalization {
            // oracle format-caps: InDesign's All Caps also enables OpenType capital
            // spacing (cpsp). Uppercasing alone makes Arial capitals too tightly set.
            Capitalization::AllCaps => features.push((Tag::from_bytes(b"cpsp"), 1)),
            Capitalization::SmallCaps if !synth_caps => features.push((Tag::from_bytes(b"smcp"), 1)),
            Capitalization::CapToSmallCap if !synth_caps => {
                features.push((Tag::from_bytes(b"smcp"), 1));
                features.push((Tag::from_bytes(b"c2sc"), 1));
            }
            _ => {}
        }
        match a.position {
            Position::OtSuperscript => features.push((Tag::from_bytes(b"sups"), 1)),
            Position::OtSubscript => features.push((Tag::from_bytes(b"subs"), 1)),
            Position::OtNumerator => features.push((Tag::from_bytes(b"numr"), 1)),
            Position::OtDenominator => features.push((Tag::from_bytes(b"dnom"), 1)),
            _ => {}
        }
        crate::opentype::features(&a.opentype, &mut features);
        features
    }

    /// Shaped glyphs of `text` in font units, from the store's cache when possible.
    fn raw_glyphs(&self, face: &Face<'a>, plan_key: PlanKey, text: &str) -> Arc<[RawGlyph]> {
        let store = self.table.store;
        let key = SegKey { plan: plan_key, text: text.into() };
        if let Some(hit) = store.shapes.get(&key) {
            return hit;
        }
        let script = key.plan.script;
        let direction = if key.plan.rtl { Direction::RightToLeft } else { Direction::LeftToRight };
        let language = Language::from_str(key.plan.lang).ok();
        let cached = store.plans.map.read().ok().and_then(|m| m.get(&key.plan).cloned());
        let plan = match cached {
            Some(p) => p,
            None => {
                let feats: Vec<Feature> = key.plan.features.iter().map(|&(t, v)| Feature::new(t, v, ..)).collect();
                let p = Arc::new(ShapePlan::new(face, direction, Some(script), language.as_ref(), &feats));
                if let Ok(mut m) = store.plans.map.write() {
                    m.insert(key.plan.clone(), p.clone());
                }
                p
            }
        };
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(direction);
        buffer.set_script(script);
        if let Some(l) = language {
            buffer.set_language(l);
        }
        let shaped = rustybuzz::shape_with_plan(face, &plan, buffer);
        let upem = f64::from(face.units_per_em());
        let glyphs: Arc<[RawGlyph]> = shaped
            .glyph_infos()
            .iter()
            .zip(shaped.glyph_positions())
            .map(|(i, p)| {
                // The nominal advance and the shaper's adjustment (kerning) separately.
                let nominal = face.glyph_hor_advance(GlyphId(i.glyph_id as u16)).map_or(p.x_advance, i32::from);
                RawGlyph {
                    id: i.glyph_id as u16,
                    cluster: i.cluster,
                    advance_em: em_fixed(nominal, upem) + em_fixed(p.x_advance - nominal, upem),
                    x_offset: p.x_offset,
                    y_offset: p.y_offset,
                }
            })
            .collect();
        store.shapes.put(key, glyphs.clone());
        glyphs
    }

    fn segment_text(
        face: &Face<'_>,
        chars: &[(char, usize)],
        range: &Range<usize>,
        a: &CharacterAttrs,
        script: Scr,
        synth_caps: bool,
    ) -> (String, Vec<usize>) {
        // Text as shaped (case-mapped), with the source character of every byte.
        let upper = matches!(a.capitalization, Capitalization::AllCaps) || synth_caps;
        let mut text = String::new();
        let mut source: Vec<usize> = Vec::new();
        // style-ligatures-{0,22,23}, style-paragraph-font: legacy Roman fonts
        // expose ff/fi/fl/ffi/ffl through cmap without a Latin GSUB liga feature.
        // InDesign still uses those glyphs; positive tracking 23 disables them
        // (22 keeps them at both 12 and 18 pt). Keep source scalar coverage.
        let latin_liga = face.tables().gsub.is_some_and(|g| {
            g.scripts
                .find(Tag::from_bytes(b"latn"))
                .and_then(|s| s.default_language)
                .is_some_and(|l| {
                    l.feature_indices.into_iter().any(|i| {
                        g.features
                            .get(i)
                            .is_some_and(|f| f.tag == Tag::from_bytes(b"liga"))
                    })
                })
        });
        let legacy_liga =
            script == Scr::Latin && !upper && a.ligatures && a.tracking < 23.0 && !latin_liga;
        let mut k = range.start;
        while k < range.end {
            let c = chars[k].0;
            if legacy_liga && c == 'f' {
                let mut replacement = None;
                for (letters, ligature) in [
                    ("ffi", '\u{fb03}'),
                    ("ffl", '\u{fb04}'),
                    ("ff", '\u{fb00}'),
                    ("fi", '\u{fb01}'),
                    ("fl", '\u{fb02}'),
                ] {
                    if k + letters.len() <= range.end
                        && letters
                            .chars()
                            .enumerate()
                            .all(|(j, c)| chars[k + j].0 == c)
                        && face.glyph_index(ligature).is_some()
                    {
                        replacement = Some((letters.len(), ligature));
                        break;
                    }
                }
                if let Some((count, c)) = replacement {
                    text.push(c);
                    source.resize(text.len(), k);
                    k += count;
                    continue;
                }
            }
            if upper {
                text.extend(c.to_uppercase());
            } else if a.capitalization == Capitalization::LowerCase {
                text.extend(c.to_lowercase());
            } else {
                text.push(c);
            }
            source.resize(text.len(), k);
            k += 1;
        }
        (text, source)
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_segment(
        &self,
        chars: &[(char, usize)],
        range: Range<usize>,
        a: &CharacterAttrs,
        font: FontId,
        script: Scr,
        rtl: bool,
        synth_caps: bool,
        cjk: bool,
        (run_size, rise): (f64, f64),
        out: &mut ShapedParagraph,
    ) {
        let size = if synth_caps { run_size * self.prefs.small_cap } else { run_size };
        let hscale = a.horizontal_scale / 100.0;
        // 1/1000 em of the glyphs' size: smaller under Superscript / Subscript (measured:
        // text-format-char char-superscript, tracking 20 on 15 pt text drawn at 8.745 pt)
        let tracking = tracking_pt(a) * run_size / a.point_size.max(0.1);
        let r = chars[range.start].1;
        let upem = self.table.units_per_em(font);
        let (sx, sy) = (size / upem * hscale, size / upem);

        let Some(face) = self.table.face(font) else {
            // Synthetic face: 0.5 em glyphs, 0.25 em spaces.
            for k in range {
                let c = chars[k].0;
                let space = c == ' ' || c == '\u{00A0}';
                let g = out.glyphs.len();
                if !is_blank(c) {
                    out.glyphs.push(ShapedGlyph { font, id: 0, dx: 0.0, dy: 0.0 });
                }
                out.clusters.push(Cluster {
                    ch: c,
                    chars: k..k + 1,
                    run: r,
                    font,
                    size,
                    advance: if space { 0.25 } else { 0.5 } * size * hscale,
                    tracking,
                    glyphs: g..out.glyphs.len(),
                    kind: if space { ClusterKind::Space } else { ClusterKind::Glyphs },
                    rise,
                });
            }
            return;
        };

        let (text, source) = Self::segment_text(face, chars, &range, a, script, synth_caps);
        let lang = if script == Scr::Hangul { "ko" } else { language_tag(&a.language) };
        let mut features=Self::features(a, script, synth_caps);
        // text-multilingual/ja: CJK Composer + Metrics uses the font's palt
        // single positioning (Yu Gothic 日 begins at -0.02em), even with the
        // OTFProportionalMetrics flag false. Roman-only metrics leave it off.
        if (cjk || matches!(script,Scr::Han|Scr::Kana)) && matches!(a.kerning_method,KerningMethod::Metrics) && !a.kerning_manual {
            features.push((Tag::from_bytes(b"palt"),1));
        }
        let plan_key = PlanKey { font, script: script.script(), rtl, lang, features };
        let glyphs = self.raw_glyphs(face, plan_key, &text);

        // Glyph groups of one source character, in glyph (visual) order; right-to-left
        // output runs backwards through the text, so its groups are reversed into logical
        // order. A group's glyphs keep their visual order and offsets from its left edge.
        let mut groups: Vec<(usize, usize, usize)> = Vec::new();
        let mut g = 0;
        while g < glyphs.len() {
            let k = source[glyphs[g].cluster as usize];
            let mut e = g + 1;
            while e < glyphs.len() && source[glyphs[e].cluster as usize] == k {
                e += 1;
            }
            groups.push((g, e, k));
            g = e;
        }
        if rtl {
            groups.reverse();
        }
        for (n, &(g, e, k)) in groups.iter().enumerate() {
            // Characters up to the next cluster's source (ligatures cover several).
            let next = groups.get(n + 1).map_or(range.end, |x| x.2);
            let c = chars[k].0;
            let first = out.glyphs.len();
            let mut pen = 0.0;
            for gl in &glyphs[g..e] {
                if !is_blank(c) {
                    out.glyphs.push(ShapedGlyph {
                        font,
                        id: gl.id,
                        dx: pen + f64::from(gl.x_offset) * sx,
                        dy: f64::from(gl.y_offset) * sy,
                    });
                }
                pen += gl.advance_em * size * hscale;
            }
            if !is_blank(c) && glyphs[g..e].iter().any(|gl| gl.id == 0) {
                out.missing.push((k, r));
            }
            let kind = if c == ' ' || c == '\u{00A0}' { ClusterKind::Space } else { ClusterKind::Glyphs };
            out.clusters.push(Cluster {
                ch: c,
                chars: k..next.max(k + 1),
                run: r,
                font,
                size,
                advance: pen,
                tracking,
                glyphs: first..out.glyphs.len(),
                kind,
                rise,
            });
        }
    }
}
