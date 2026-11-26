//! A feature view of the Glyphs panel: the glyphs the feature gives, as InDesign 2025 (20.5.5.119) lists them (rules/glyphs-c4.ko.md).
//! Measured in Arial, Times New Roman and Segoe UI by reading the cells of every feature view:
//! - only the feature records of scripts that are not right-to-left count (Arial's Stylistic Set 1 has 29 substitutions, 9 of them registered
//!   under arab: the view has 20; Initial / Medial / Terminal Forms, Standard / Required Ligatures are arab-only: empty views);
//! - the lookups of one record run in order, each on the result of the one before (Tabular Figures in Arial: oldstyle and proportional zero..nine
//!   give zero..nine, then `one` gives the tabular one: ten glyphs, not eleven);
//! - the lookups after the first one that reads the neighbours (contextual and chaining substitutions) do not count (Fractions in Arial and
//!   Segoe UI: the slash gives the fraction slash, the digits' superscript lookup comes after the chaining one: one glyph);
//! - the glyphs of the ligature lookups before that count as given when the ligature has more than one component (Segoe UI's Stylistic Set 18:
//!   61 single substitutions and 11 ligatures, 72 cells; Segoe UI Symbol's Stylistic Set 20: 4 cells).
use super::{show::rtl_script, unicode_order::covered};
use std::collections::{BTreeSet, HashMap, HashSet};
use ttf_parser::{Face, gsub::{SingleSubstitution, SubstitutionSubtable as S}, opentype_layout::LayoutTable};

/// One lookup of a feature: glyph → the glyphs it is replaced by, the ligature glyphs it makes, and whether it reads the neighbours.
struct Step { map: HashMap<u16, Vec<u16>>, ligatures: Vec<u16>, contextual: bool }

fn step(gsub: &LayoutTable<'_>, index: u16) -> Option<Step> {
    let lookup = gsub.lookups.get(index)?;
    let mut out = Step { map: HashMap::new(), ligatures: Vec::new(), contextual: false };
    for subtable in lookup.subtables.into_iter::<S>() {
        match subtable {
            S::Single(SingleSubstitution::Format1 { coverage, delta }) => {
                for (g, _) in covered(&coverage) { out.map.entry(g).or_default().push((i32::from(g) + i32::from(delta)) as u16); }
            }
            S::Single(SingleSubstitution::Format2 { coverage, substitutes }) => {
                for (g, i) in covered(&coverage) { if let Some(t) = substitutes.get(i) { out.map.entry(g).or_default().push(t.0); } }
            }
            S::Alternate(alt) => {
                for (g, i) in covered(&alt.coverage) {
                    let Some(set) = alt.alternate_sets.get(i) else { continue };
                    for t in set.alternates { out.map.entry(g).or_default().push(t.0); }
                }
            }
            S::Ligature(lig) => {
                // a ligature of one glyph is a one-to-one substitution the panel does not list (Segoe UI Symbol's Stylistic Set 20: 16 ligatures, 4 of more than one glyph, 4 cells)
                for set in lig.ligature_sets.into_iter() { for l in set { if !l.components.is_empty() { out.ligatures.push(l.glyph.0); } } }
            }
            S::Context(_) | S::ChainContext(_) | S::ReverseChainSingle(_) => out.contextual = true,
            S::Multiple(_) => {}
        }
    }
    Some(out)
}

/// The glyph ids the feature `tag` gives (the panel's view of that feature, before it is put in By Unicode order).
pub(super) fn targets(face: &Face<'_>, tag: [u8; 4]) -> HashSet<u16> {
    let mut out = HashSet::new();
    let Some(gsub) = face.tables().gsub else { return out };
    let tags: Vec<[u8; 4]> = gsub.features.into_iter().map(|f| f.tag.to_bytes()).collect();
    let mut records: BTreeSet<u16> = BTreeSet::new();
    for script in gsub.scripts.into_iter() {
        if rtl_script(&script.tag.to_bytes()) { continue; }
        for lang in script.default_language.into_iter().chain(script.languages) {
            for index in lang.feature_indices.into_iter().chain(lang.required_feature) {
                if tags.get(usize::from(index)) == Some(&tag) { records.insert(index); }
            }
        }
    }
    for index in records {
        let Some(feature) = gsub.features.get(index) else { continue };
        let mut state: HashMap<u16, BTreeSet<u16>> = HashMap::new(); // glyph the lookups started from → the glyphs it has become
        for li in feature.lookup_indices {
            let Some(step) = step(&gsub, li) else { continue };
            if step.contextual { break; }
            out.extend(step.ligatures.iter().copied());
            for g in step.map.keys() { state.entry(*g).or_insert_with(|| BTreeSet::from([*g])); }
            for now in state.values_mut() {
                *now = now.iter().flat_map(|g| step.map.get(g).cloned().unwrap_or_else(|| vec![*g])).collect();
            }
        }
        for (from, now) in &state { out.extend(now.iter().copied().filter(|g| g != from)); }
    }
    out
}
