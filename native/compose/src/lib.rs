pub mod breaks;
pub mod compose;
pub mod cost;
pub mod hyphenate;
mod measure;
mod placement;

pub use compose::compose;
pub use placement::{PlacedLine, place_line};

pub const SOFT_HYPHEN: char = '\u{ad}';

pub fn is_trailing_blank(ch: char) -> bool {
    matches!(ch, ' ' | '\n' | '\u{3000}' | '\u{2000}'..='\u{200a}')
}
pub fn is_word_space(ch: char) -> bool {
    matches!(ch, ' ' | '\u{a0}')
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub ch: char,
    pub advance: f64,
    pub end_advance: f64,
    pub point_size: f64,
    pub unit: f64,
}
impl Item {
    pub fn new(ch: char, advance: f64, tracking: f64, point_size: f64) -> Self {
        Self {
            ch,
            advance,
            end_advance: advance - tracking,
            point_size,
            unit: 0.0,
        }
    }
    pub fn soft_hyphen(hyphen_width: f64, point_size: f64) -> Self {
        Self {
            end_advance: hyphen_width,
            ..Self::new(SOFT_HYPHEN, 0.0, 0.0, point_size)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
    LeftJustified,
    CenterJustified,
    RightJustified,
    FullyJustified,
}
impl Alignment {
    pub fn is_justified(self) -> bool {
        !matches!(self, Self::Left | Self::Center | Self::Right)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Search {
    #[default]
    InDesign,
    Exhaustive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Composer {
    #[default]
    RomanParagraph,
    JapaneseParagraph,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub min: f64,
    pub desired: f64,
    pub max: f64,
}
impl Range {
    pub const fn new(min: f64, desired: f64, max: f64) -> Self {
        Self { min, desired, max }
    }
    fn allowance(self, shrink: bool) -> f64 {
        if shrink {
            (self.desired - self.min) / 100.0
        } else {
            (self.max - self.desired) / 100.0
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HyphenSettings {
    pub words_longer_than: usize,
    pub after_first: usize,
    pub before_last: usize,
    pub capitalized: bool,
    pub last_word: bool,
    pub ladder_limit: u32,
    pub zone: f64,
    pub across_columns: bool,
    pub weight: f64,
    pub quality: f64,
}
impl Default for HyphenSettings {
    fn default() -> Self {
        Self {
            words_longer_than: 5,
            after_first: 2,
            before_last: 2,
            capitalized: true,
            last_word: true,
            ladder_limit: 3,
            zone: 36.0,
            across_columns: true,
            weight: cost::HYPHEN_WEIGHT_AT_5,
            quality: cost::DICTIONARY_QUALITY,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub alignment: Alignment,
    pub single_word: Alignment,
    pub balance_ragged_lines: bool,
    pub word_spacing: Range,
    pub letter_spacing: Range,
    pub glyph_scaling: Range,
    pub composer: Composer,
    pub hyphenation: bool,
    pub hyphen: HyphenSettings,
    pub search: Search,
    pub last_line_indent: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            alignment: Alignment::LeftJustified,
            single_word: Alignment::FullyJustified,
            balance_ragged_lines: false,
            word_spacing: Range::new(80.0, 100.0, 133.0),
            letter_spacing: Range::new(0.0, 0.0, 0.0),
            glyph_scaling: Range::new(100.0, 100.0, 100.0),
            composer: Composer::default(),
            hyphenation: true,
            hyphen: HyphenSettings::default(),
            search: Search::default(),
            last_line_indent: 0.0,
        }
    }
}
