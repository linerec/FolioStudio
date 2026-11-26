//! Alternates for a selection (Glyphs panel ▸ Alternates for Selection): the glyphs a face gives a character, or a short
//! run of characters, through its OpenType substitution (GSUB) features: stylistic alternates and sets, swashes,
//! ligatures, small caps, figures, fractions, ordinals, locale forms and the like. Each feature is applied alone to the
//! text and the glyphs it gives are listed with the feature tags that give them. The shaping of every feature is the
//! shaper's own, which is held against InDesign's PDF by the `opentype` oracle scenarios (ot-ss01…ot-ss20, ot-salt,
//! ot-swsh, ot-dlig, ot-frac, ot-ordn, ot-zero, ot-swash-gabriola, …); the list itself is Folio's (the panel's own
//! list cannot be read without driving its window, see rules/glyphs.toml).
use rustybuzz::{Face, Feature, UnicodeBuffer, ttf_parser::Tag};
use super::{Mapping, unicode_order::{self, Edges}};
use std::collections::BTreeSet;

/// Longest selection the alternates are looked up for (a ligature is a few characters).
const MAX_CHARS: usize = 8;
/// Alternate substitutions (GSUB type 3) number their choices 1, 2, …; more than this is not listed.
const MAX_CHOICES: u32 = 32;
/// Features the shaper switches on by itself. The baseline has them off, so a ligature, a contextual or a locale form is
/// an alternate like any other.
const ON_BY_DEFAULT: [&[u8; 4]; 6] = [b"kern", b"liga", b"clig", b"calt", b"locl", b"rclt"];

/// One glyph the selection can show and the feature tags that give it (none: the character's own glyph).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alternate {
    pub glyph: u16,
    pub features: Vec<String>,
}

fn shape(face: &Face<'_>, text: &str, on: Option<(Tag, u32)>) -> Vec<u16> {
    let mut features: Vec<Feature> = ON_BY_DEFAULT.iter().map(|t| Feature::new(Tag::from_bytes(t), 0, ..)).collect();
    if let Some((tag, value)) = on {
        features.retain(|f| f.tag != tag);
        features.push(Feature::new(tag, value, ..));
    }
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    rustybuzz::shape(face, &features, buffer).glyph_infos().iter().map(|i| i.glyph_id as u16).collect()
}

/// The glyphs of `text` in `face`, the character's own glyph first, then the glyphs each substitution feature gives in
/// the order of the feature tags and, within a feature, of its choices. Only results of one glyph are listed (a
/// panel cell is one glyph); a glyph reached by several features lists all of them.
pub fn list(face: &Face<'_>, text: &str) -> Vec<Alternate> {
    let text: String = text.chars().take(MAX_CHARS).collect();
    if text.is_empty() {
        return Vec::new();
    }
    let base = shape(face, &text, None);
    let mut out: Vec<(Vec<u16>, Vec<String>)> = vec![(base.clone(), Vec::new())];
    let tags: BTreeSet<[u8; 4]> = face.tables().gsub.map(|g| g.features.into_iter().map(|f| f.tag.to_bytes()).collect()).unwrap_or_default();
    for bytes in tags {
        // the panel lists no locale, contextual or composition results (it has no Show entry for them)
        if unicode_order::NOT_LISTED.iter().any(|n| **n == bytes) { continue; }
        let tag = Tag::from_bytes(&bytes);
        let name = tag.to_string();
        let mut previous = base.clone();
        for choice in 1..=MAX_CHOICES {
            let got = shape(face, &text, Some((tag, choice)));
            // unchanged: the feature does nothing here, or its choices are used up (a choice past the last leaves the glyph)
            if got == previous || got == base {
                break;
            }
            match out.iter_mut().find(|(seq, _)| *seq == got) {
                Some((_, names)) => {
                    if !names.contains(&name) {
                        names.push(name.clone());
                    }
                }
                None => out.push((got.clone(), vec![name.clone()])),
            }
            previous = got;
        }
    }
    let listed: Vec<Alternate> = out.into_iter().filter(|(seq, _)| seq.len() == 1).map(|(seq, features)| Alternate { glyph: seq[0], features }).collect();
    // Several characters that no feature joins into one glyph: the panel shows the alternates of the first glyph of the selection
    // (InDesign 2025, Arial "A1" selected: the list of A; rules/glyphs-c3.ko.md)
    if listed.is_empty() && base.len() > 1 {
        if let Some(first) = text.chars().next() {
            return list(face, &first.to_string());
        }
    }
    listed
}

/// The order and the members InDesign's panel gives Alternates for Selection (measured in Arial: A, a, g, I, Z, 1, &, and in Segoe UI: I, 1 with
/// the tooltips of the cells; rules/glyphs.toml glyphs.alternates, glyphs-c4.ko.md): the selected character's own glyph first; for one selected
/// character also the glyphs that give it by a substitution (the reverse of an alternate: Arial's unicase `i` is listed for `I`); then the rest
/// ordered by the Unicode value the panel files a glyph under: its own, or for a glyph without one the owner's (the glyph that gives it, as in
/// the By Unicode order: Segoe UI's small capital I is filed under `i`, so Stylistic Alternates `I` comes before it), a glyph no substitution
/// the panel counts gives last; equal values: glyphs without a Unicode value first, then by glyph id.
pub(super) fn arrange(list: &mut Vec<Alternate>, rows: &[Mapping], edges: &Edges, text: &str) {
    let Some(base) = list.first().map(|a| a.glyph) else { return };
    let owners = unicode_order::owners(rows, edges);
    let mut chars = text.chars();
    let (Some(_), None) = (chars.next(), chars.next()) else {
        let key = |a: &Alternate| rank(rows, &owners, a.glyph);
        list[1..].sort_by_key(key);
        return;
    };
    let mut reverse: Vec<(u16, BTreeSet<[u8; 4]>)> = edges.iter().filter(|((s, t), _)| *t == base && *s != base).map(|((s, _), tags)| (*s, tags.clone())).collect();
    reverse.sort_by_key(|(s, _)| *s);
    for (glyph, tags) in reverse {
        if !list.iter().any(|a| a.glyph == glyph) {
            list.push(Alternate { glyph, features: tags.iter().map(|t| String::from_utf8_lossy(t).into_owned()).collect() });
        }
    }
    list[1..].sort_by_key(|a| rank(rows, &owners, a.glyph));
}

fn lowest(rows: &[Mapping], glyph: u16) -> Option<u32> {
    rows.get(usize::from(glyph)).and_then(|r| r.unicode.iter().copied().find(|cp| *cp >= 0x20))
}

fn rank(rows: &[Mapping], owners: &std::collections::BTreeMap<u16, (bool, u16)>, glyph: u16) -> (u32, u8, u16) {
    match lowest(rows, glyph) {
        Some(cp) => (cp, 1, glyph),
        None => (owners.get(&glyph).and_then(|(_, source)| lowest(rows, *source)).unwrap_or(u32::MAX), 0, glyph),
    }
}
