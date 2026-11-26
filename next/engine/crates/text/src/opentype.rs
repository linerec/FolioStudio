use crate::FontStore;
use folio_model::{FontInfo, opentype::{FigureStyle, OpenType, OpenTypeFeature, PositionalForm}};
use rustybuzz::ttf_parser::Tag;
use std::collections::BTreeSet;

const FEATURES: &[(OpenTypeFeature, &[u8; 4])] = &[
    (OpenTypeFeature::DiscretionaryLigature, b"dlig"),
    (OpenTypeFeature::Fraction, b"frac"),
    (OpenTypeFeature::Ordinal, b"ordn"),
    (OpenTypeFeature::Swash, b"swsh"),
    (OpenTypeFeature::Titling, b"titl"),
    (OpenTypeFeature::ContextualAlternate, b"calt"),
    (OpenTypeFeature::SlashedZero, b"zero"),
    (OpenTypeFeature::Historical, b"hist"),
    (OpenTypeFeature::OverlapSwash, b"cswh"),
    (OpenTypeFeature::StylisticAlternate, b"salt"),
    (OpenTypeFeature::JustificationAlternate, b"jalt"),
    (OpenTypeFeature::StretchedAlternate, b"stch"),
];

pub(crate) fn features(a: &OpenType, out: &mut Vec<(Tag, u32)>) {
    for &(feature, tag) in FEATURES {
        out.push((Tag::from_bytes(tag), u32::from(a.features.contains(&feature))));
    }
    for n in 1..=20 {
        let tag = [b's', b's', b'0' + n / 10, b'0' + n % 10];
        out.push((Tag::from_bytes(&tag), u32::from(a.stylistic_sets & (1 << n) != 0)));
    }
    let figures: &[&[u8; 4]] = match a.figure_style {
        FigureStyle::Default => &[],
        FigureStyle::TabularLining => &[b"tnum", b"lnum"],
        FigureStyle::ProportionalLining => &[b"pnum", b"lnum"],
        FigureStyle::TabularOldstyle => &[b"tnum", b"onum"],
        FigureStyle::ProportionalOldstyle => &[b"pnum", b"onum"],
    };
    for &tag in figures { out.push((Tag::from_bytes(tag), 1)); }
    let form = match a.positional_form {
        PositionalForm::Initial => Some(b"init"),
        PositionalForm::Medial => Some(b"medi"),
        PositionalForm::Final => Some(b"fina"),
        PositionalForm::Isolated => Some(b"isol"),
        PositionalForm::None | PositionalForm::Calculate => None,
    };
    if let Some(tag) = form { out.push((Tag::from_bytes(tag), 1)); }
}

/// GSUB and GPOS both contain user-selectable features (some sets only position glyphs).
pub fn supported(store: &FontStore, family: &str, style: &str, table: &[FontInfo]) -> Vec<String> {
    let font = store.resolve(family, style, table);
    store.with_face(font.font, |face| {
        let mut tags = BTreeSet::new();
        for layout in [face.tables().gsub, face.tables().gpos].into_iter().flatten() {
            for feature in layout.features {
                tags.insert(feature.tag.to_string());
            }
        }
        tags.into_iter().collect()
    }).unwrap_or_default()
}
