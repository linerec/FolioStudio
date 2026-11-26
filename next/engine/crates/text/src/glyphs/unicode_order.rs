//! The Glyphs panel's By Unicode order, as InDesign 2025 (20.5.5.119) lists "Entire Font" (rules/glyphs.toml glyphs.sort).
//! Measured by reading the panel's cells (screenshots matched cell by cell against the font's glyph outlines) in Arial, Times New
//! Roman, Malgun Gothic and D2Coding: the panel starts with By Unicode checked, for every face.
//!
//! The order: glyphs with a Unicode value (control codes below U+0020 do not count) by their lowest Unicode value, each glyph once
//! (the no-break space is not listed a second time). Right after such a glyph come the glyphs without a Unicode value that one of
//! the font's substitutions (GSUB single and alternate substitutions: oldstyle and proportional figures, small capitals, stylistic
//! sets, denominators, ...) gives it, lowest glyph id first. A glyph several glyphs give belongs to the one that gives it by a
//! feature other than Small Capitals From Capitals (`c2sc`), and among those to the one with the lowest glyph id (Agrave's and
//! agrave's alternate follows agrave, glyph id 106 against 172). The results of `locl`, `ccmp`, `calt` and `rclt` are not listed
//! (the panel's Show menu has no entry for them). Glyphs nothing leads to (no Unicode value, no substitution) come last, by glyph id.
use super::Mapping;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use ttf_parser::{Face, gsub::{SingleSubstitution, SubstitutionSubtable as S}, opentype_layout::Coverage};

/// Features whose substitutions the panel does not list as alternates (it has no Show entry for them).
pub(super) const NOT_LISTED: [&[u8; 4]; 4] = [b"locl", b"ccmp", b"calt", b"rclt"];
/// Features that give way to every other feature when several glyphs give the same glyph.
const LOW: [&[u8; 4]; 1] = [b"c2sc"];

/// (source glyph, target glyph) → the feature tags whose single or alternate substitutions give it.
pub(super) type Edges = HashMap<(u16, u16), BTreeSet<[u8; 4]>>;

pub(super) fn covered(c: &Coverage<'_>) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    match c {
        Coverage::Format1 { glyphs } => out.extend(glyphs.into_iter().enumerate().map(|(i, g)| (g.0, i as u16))),
        Coverage::Format2 { records } => {
            for r in records.into_iter() {
                out.extend((r.start.0..=r.end.0).map(|g| (g, r.value.saturating_add(g - r.start.0))));
            }
        }
    }
    out
}

/// Every one-glyph-to-one-glyph substitution of the face (GSUB lookup types 1 and 3) with the features that use it.
pub(super) fn edges(face: &Face<'_>) -> Edges {
    let mut out = Edges::new();
    let Some(gsub) = face.tables().gsub else { return out };
    for feature in gsub.features.into_iter() {
        let tag = feature.tag.to_bytes();
        for index in feature.lookup_indices.into_iter() {
            let Some(lookup) = gsub.lookups.get(index) else { continue };
            // the lookups after the first one that reads the neighbours do not count (the panel never files their results under the character:
            // Segoe UI's fraction numerators come after a chaining lookup and are listed last, without a Unicode value; rules/glyphs-c4.ko.md)
            if lookup.subtables.into_iter::<S>().any(|t| matches!(t, S::Context(_) | S::ChainContext(_) | S::ReverseChainSingle(_))) { break; }
            for subtable in lookup.subtables.into_iter::<S>() {
                match subtable {
                    S::Single(SingleSubstitution::Format1 { coverage, delta }) => {
                        for (g, _) in covered(&coverage) {
                            out.entry((g, (i32::from(g) + i32::from(delta)) as u16)).or_default().insert(tag);
                        }
                    }
                    S::Single(SingleSubstitution::Format2 { coverage, substitutes }) => {
                        for (g, i) in covered(&coverage) {
                            if let Some(t) = substitutes.get(i) { out.entry((g, t.0)).or_default().insert(tag); }
                        }
                    }
                    S::Alternate(alt) => {
                        for (g, i) in covered(&alt.coverage) {
                            let Some(set) = alt.alternate_sets.get(i) else { continue };
                            for t in set.alternates { out.entry((g, t.0)).or_default().insert(tag); }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out.retain(|_, tags| { tags.retain(|t| !NOT_LISTED.contains(&t)); !tags.is_empty() });
    out
}

fn listed(row: &Mapping) -> bool { row.unicode.iter().any(|cp| *cp >= 0x20) }

/// For every glyph without a Unicode value that a substitution reaches: (the glyph is given only by `c2sc`, the lowest-id glyph with a Unicode
/// value that gives it, directly or through other alternates). The glyph is listed right after that owner, and the panel files it under the
/// owner's Unicode value (its tooltip shows that value).
pub(super) fn owners(rows: &[Mapping], edges: &Edges) -> BTreeMap<u16, (bool, u16)> {
    let mut mapped: Vec<&Mapping> = rows.iter().filter(|r| listed(r)).collect();
    mapped.sort_by_key(|r| (r.unicode.first().copied().unwrap_or(0), r.id));
    let is_mapped: Vec<bool> = {
        let mut v = vec![false; rows.len()];
        for r in &mapped { v[usize::from(r.id)] = true; }
        v
    };
    let mut targets: HashMap<u16, Vec<(u16, bool)>> = HashMap::new(); // source → (target, a feature other than c2sc gives it)
    for ((s, t), tags) in edges {
        targets.entry(*s).or_default().push((*t, tags.iter().any(|g| !LOW.contains(&g))));
    }
    // owner of a glyph without a Unicode value: (the glyph is given by a feature that is not low, its lowest-id source)
    let mut owner: BTreeMap<u16, (bool, u16)> = BTreeMap::new();
    for source in &mapped {
        let mut seen: BTreeSet<u16> = BTreeSet::from([source.id]);
        let mut frontier = vec![source.id];
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for s in frontier {
                for &(t, strong) in targets.get(&s).into_iter().flatten() {
                    if usize::from(t) >= rows.len() || is_mapped[usize::from(t)] || !seen.insert(t) { continue; }
                    let candidate = (!strong, source.id);
                    owner.entry(t).and_modify(|o| if candidate < *o { *o = candidate }).or_insert(candidate);
                    next.push(t);
                }
            }
            frontier = next;
        }
    }
    owner
}

/// Glyph ids in By Unicode order (every glyph of `rows` once).
pub(super) fn order(rows: &[Mapping], edges: &Edges) -> Vec<u16> {
    let mut mapped: Vec<&Mapping> = rows.iter().filter(|r| listed(r)).collect();
    mapped.sort_by_key(|r| (r.unicode.first().copied().unwrap_or(0), r.id));
    let owner = owners(rows, edges);
    let mut group: HashMap<u16, Vec<u16>> = HashMap::new();
    for (t, (_, source)) in &owner { group.entry(*source).or_default().push(*t); }
    let mut out = Vec::with_capacity(rows.len());
    for source in &mapped {
        out.push(source.id);
        if let Some(g) = group.get_mut(&source.id) { g.sort_unstable(); out.extend(g.iter().copied()); }
    }
    let placed: BTreeSet<u16> = out.iter().copied().collect();
    out.extend(rows.iter().map(|r| r.id).filter(|id| !placed.contains(id)));
    out
}
