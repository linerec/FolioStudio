//! The Glyphs panel's Show list as InDesign 2025 (20.5.5.119) offers it (rules/glyphs-c3.ko.md): Recent Glyphs, Alternates for Selection,
//! Entire Font, the Unicode ranges the face has glyphs in, then the OpenType features the face has (GSUB). Item names, order and presence
//! were read from the panel in Arial, Times New Roman, Malgun Gothic, D2Coding and Segoe UI Symbol; the contents of a range or feature view
//! were matched cell by cell (screen shots against the glyph outlines) or cell count by cell count in Arial, Times New Roman, Segoe UI, Segoe UI
//! Symbol and Malgun Gothic (rules/glyphs-c4.ko.md).
//!
//! A feature view is the By Unicode order of the whole font (unicode_order) cut down to the glyphs that feature substitutes (GSUB single
//! and alternate substitutions): exact for Small Capitals, Small Capitals From Capitals, Stylistic Alternates, Stylistic Sets 2 and 3, Unicase.
//! A range view lists the glyphs whose lowest Unicode value (control codes below U+0020 do not count) is in the range, each glyph once. The
//! categories follow InDesign's Unicode table (Unicode 15.1 era): a character assigned later is in no range in InDesign, in Folio it is.
use super::Mapping;
use icu_properties::{CodePointMapData, props::GeneralCategory as Gc};
use std::collections::BTreeMap;
use ttf_parser::Face;

/// (id, name, exact): the ranges, in the panel's order. `exact` = the contents agree with the panel in every face measured (rules/glyphs-c4.ko.md).
pub const RANGES: [(&str, &str, bool); 13] = [
    ("latin", "Basic Latin and Latin 1", true), ("latin-a", "Extended Latin A", true), ("latin-b", "Extended Latin B", true),
    ("punctuation", "Punctuation", true), ("open-close", "Open & Close Punctuation", true), ("dashes-quotes", "Dashes & Quotes", true),
    ("scripts", "Superscripts & Subscripts", true), ("numbers", "Numbers", true), ("currency", "Currency", true),
    ("symbols", "Symbols", true), ("math", "Math Symbols", true), ("greek", "Greek", true), ("cyrillic", "Cyrillic", true),
];

/// The OpenType features the panel names (tag, name); other tags have no entry in the panel's Show list.
pub fn feature_name(tag: &[u8; 4]) -> Option<String> {
    Some(match tag {
        b"ccmp" => "Glyph Composition / Decomposition", b"init" => "Initial Forms", b"isol" => "Isolated Forms", b"medi" => "Medial Forms",
        b"rlig" => "Required Ligatures", b"liga" => "Standard Ligatures", b"fina" => "Terminal Forms", b"dnom" => "Denominators",
        b"frac" => "Fractions", b"lnum" => "Lining Figures", b"numr" => "Numerators", b"onum" => "Oldstyle Figures",
        b"pnum" => "Proportional Figures", b"smcp" => "Small Capitals", b"c2sc" => "Small Capitals From Capitals",
        b"salt" => "Stylistic Alternates", b"tnum" => "Tabular Figures", b"unic" => "Unicase", b"vert" => "Vertical Writing",
        b"subs" => "Subscript", b"sups" => "Superscript",
        // listed in nearly every face that has them (case 13 of 14 faces, sinf 14 of 14, ordn 10 of 13, zero, hist, aalt, afrc, hlig, ...: rules/glyphs-c4.ko.md)
        b"case" => "Case-Sensitive Forms", b"sinf" => "Scientific Inferiors", b"ordn" => "Ordinals", b"zero" => "Slashed Zero", b"hist" => "Historical Forms",
        b"aalt" => "Access All Alternates", b"afrc" => "Alternative Fractions", b"hlig" => "Historical Ligatures", b"ornm" => "Ornaments",
        b"falt" => "Final Glyph on Line Alternates", b"swsh" => "Swash", b"fwid" => "Full Widths", b"hwid" => "Half Widths", b"pwid" => "Proportional Widths",
        b"ruby" => "Ruby Notation Forms", b"vrt2" => "Vertical Alternates and Rotation", b"dlig" => "Discretionary Ligatures",
        [b's', b's', a @ b'0'..=b'2', b @ b'0'..=b'9'] => {
            let n = u32::from(a - b'0') * 10 + u32::from(b - b'0');
            return (1..=20).contains(&n).then(|| format!("Stylistic Set {n}"));
        }
        _ => return None,
    }.to_owned())
}

fn gc(cp: u32) -> Option<Gc> { char::from_u32(cp).map(|c| CodePointMapData::<Gc>::new().get(c)) }

/// Is a glyph whose lowest Unicode value is `cp` in the range `id`? (rules/glyphs-c4.ko.md: every range was matched cell by cell or by cell
/// count in Arial, Times New Roman, Segoe UI, Segoe UI Symbol and Malgun Gothic)
pub fn in_range(id: &str, cp: u32) -> bool {
    let c = gc(cp);
    match id {
        "latin" => (0x20..=0xFF).contains(&cp),
        "latin-a" => (0x100..=0x17F).contains(&cp),
        "latin-b" => (0x180..=0x24F).contains(&cp),
        "greek" => matches!(cp, 0x370..=0x3FF | 0x1F00..=0x1FFF),
        "cyrillic" => (0x400..=0x52F).contains(&cp),
        // the superscript and subscript block, the Latin-1 superscript digits and the Phonetic Extensions subscript letters (not the
        // modifier letters and capitals that also decompose to <super>, nor the ordinal indicators)
        "scripts" => matches!(cp, 0x2070..=0x209F | 0xB2 | 0xB3 | 0xB9 | 0x1D62..=0x1D6A),
        // Unicode general categories, the panel's counts agree in every face measured: Numbers = N*, Currency = Sc, Punctuation = P* and all
        // marks (Mn, Mc, Me), Symbols = Sc + Sk + So without the Specials block (U+FFFC, U+FFFD), Math = Sm without the Hebrew presentation
        // forms (U+FB29), Open & Close = Ps + Pe, Dashes & Quotes = Pd + Pi + Pf (any block: the ASCII brackets and hyphen are in them)
        "numbers" => matches!(c, Some(Gc::DecimalNumber | Gc::LetterNumber | Gc::OtherNumber)),
        "currency" => c == Some(Gc::CurrencySymbol),
        "math" => c == Some(Gc::MathSymbol) && !matches!(cp, 0xFB1D..=0xFB4F),
        "punctuation" => matches!(c, Some(Gc::ConnectorPunctuation | Gc::DashPunctuation | Gc::OpenPunctuation | Gc::ClosePunctuation
            | Gc::InitialPunctuation | Gc::FinalPunctuation | Gc::OtherPunctuation | Gc::NonspacingMark | Gc::SpacingMark | Gc::EnclosingMark)),
        "symbols" => matches!(c, Some(Gc::CurrencySymbol | Gc::ModifierSymbol | Gc::OtherSymbol)) && !matches!(cp, 0xFFF0..=0xFFFF),
        "open-close" => matches!(c, Some(Gc::OpenPunctuation | Gc::ClosePunctuation)),
        "dashes-quotes" => matches!(c, Some(Gc::DashPunctuation | Gc::InitialPunctuation | Gc::FinalPunctuation)),
        _ => false,
    }
}

/// The lowest Unicode value a glyph is listed under (control codes below U+0020 do not count).
pub fn lowest(row: &Mapping) -> Option<u32> { row.unicode.iter().copied().find(|cp| *cp >= 0x20) }

/// Is the OpenType script tag a right-to-left script (the panel's views read no feature record of those)?
pub fn rtl_script(tag: &[u8; 4]) -> bool { matches!(tag, b"arab" | b"hebr" | b"syrc" | b"thaa" | b"nko " | b"adlm") }

/// A GSUB feature of the face: `seq` = its place in the panel's list (see `features`), `rtl` = the script that lists it first is right-to-left
/// (arab, hebr, ...: the panel's first divider group), `any_rtl` = a right-to-left script registers it at all, `cjk` = a CJK script registers it,
/// `ligature_only` = every lookup of every record of the tag is a ligature substitution, `usable` = some script's default record starts with a
/// lookup that does not read the neighbours.
#[derive(Clone, Copy, Debug)]
pub struct Feature { pub seq: usize, pub rtl: bool, pub any_rtl: bool, pub cjk: bool, pub ligature_only: bool, pub usable: bool }

fn contextual(t: &ttf_parser::gsub::SubstitutionSubtable<'_>) -> bool {
    use ttf_parser::gsub::SubstitutionSubtable as S;
    matches!(t, S::Context(_) | S::ChainContext(_) | S::ReverseChainSingle(_))
}

/// The GSUB features of the face the panel can list, with their place in its list: the order of the font's scripts, in each script the order of
/// its default language system's feature array, a feature standing where it first appears (rules/glyphs-c4.ko.md: Arial's arab list is
/// calt dlig ccmp init isol medi rclt rlig liga ss01 fina and the panel lists init isol medi rlig liga ss01 fina; Lato's DFLT list is the
/// alphabetical one; a feature only a language-specific system registers, as Segoe UI's isol, is not listed). `seq` is `usize::MAX` for a
/// feature no default system has.
pub fn features(face: &Face<'_>) -> BTreeMap<[u8; 4], Feature> {
    let mut out: BTreeMap<[u8; 4], Feature> = BTreeMap::new();
    let Some(gsub) = face.tables().gsub else { return out };
    let tags: Vec<[u8; 4]> = gsub.features.into_iter().map(|f| f.tag.to_bytes()).collect();
    let fresh = Feature { seq: usize::MAX, rtl: false, any_rtl: false, cjk: false, ligature_only: true, usable: false };
    for t in &tags { out.entry(*t).or_insert(fresh); }
    let mut next = 0usize;
    for script in gsub.scripts.into_iter() {
        let rtl = rtl_script(&script.tag.to_bytes());
        let cjk = matches!(&script.tag.to_bytes(), b"hani" | b"kana" | b"hang" | b"bopo" | b"jamo");
        for (is_default, lang) in script.default_language.into_iter().map(|l| (true, l)).chain(script.languages.into_iter().map(|l| (false, l))) {
            for index in lang.feature_indices.into_iter().chain(lang.required_feature) {
                let Some(tag) = tags.get(usize::from(index)) else { continue };
                let f = out.entry(*tag).or_insert(fresh);
                f.any_rtl |= rtl;
                f.cjk |= cjk;
                if !is_default { continue; }
                if f.seq == usize::MAX { f.seq = next; next += 1; f.rtl = rtl; }
                // Hebrew records do not make a feature usable (Segoe UI's rlig: arab chaining only, hebr ligatures: not listed)
                let first_ok = gsub.features.get(index).and_then(|ft| ft.lookup_indices.into_iter().next()).and_then(|li| gsub.lookups.get(li))
                    .is_some_and(|l| l.subtables.into_iter::<ttf_parser::gsub::SubstitutionSubtable>().next().is_some_and(|t| !contextual(&t)));
                if first_ok && script.tag.to_bytes() != *b"hebr" { f.usable = true; }
            }
        }
    }
    for feature in gsub.features.into_iter() {
        let tag = feature.tag.to_bytes();
        let all_ligature = feature.lookup_indices.into_iter().all(|i| gsub.lookups.get(i).is_some_and(|l| l.subtables.into_iter::<ttf_parser::gsub::SubstitutionSubtable>().all(|t| matches!(t, ttf_parser::gsub::SubstitutionSubtable::Ligature(_)))));
        if let Some(f) = out.get_mut(&tag) { f.ligature_only &= all_ligature; }
    }
    out
}

/// (group, id, name) of the Show list of one face: fixed items, the ranges, features of the face in the font's own order.
pub fn items(rows: &[Mapping], features: &BTreeMap<[u8; 4], Feature>) -> Vec<(u8, String, String)> {
    let mut out = vec![(0, "recent".to_owned(), "Recent Glyphs".to_owned()), (1, "alternates".to_owned(), "Alternates for Selection".to_owned()),
        (1, "entire".to_owned(), "Entire Font".to_owned())];
    // Open & Close Punctuation and Dashes & Quotes come together, for faces with CJK brackets (Malgun Gothic, D2Coding: yes; Arial, Times New
    // Roman, Segoe UI, Segoe UI Symbol: no)
    let cjk_brackets = rows.iter().any(|r| lowest(r).is_some_and(|cp| (0x3008..=0x3011).contains(&cp)));
    for (id, name, _) in RANGES {
        // every other range is listed for every face, even one with no glyph in it (37 faces read: Javanese Text, Myanmar Text and Segoe UI Emoji list
        // Greek and Cyrillic with nothing in them; Segoe UI Symbol lists Cyrillic over an empty grid)
        let here = if matches!(id, "open-close" | "dashes-quotes") { cjk_brackets } else { true };
        if here { out.push((2, format!("range:{id}"), name.to_owned())); }
    }
    // Glyph Composition / Decomposition (ccmp) is listed in Segoe UI Symbol, not in Arial and Times New Roman, which register it under arab and
    // hebr too (inferred from these three faces; Segoe UI registers it under arab and hebr and lists it: the rule behind that is not known)
    let mut named: Vec<(usize, u8, String, String)> = features.iter().filter(|(tag, f)| f.seq != usize::MAX && f.usable && match *tag {
        b"ccmp" => !f.any_rtl,
        // Vertical Writing: listed for faces that register it under a CJK script (Malgun Gothic yes; Segoe UI Symbol has vert but no)
        b"vert" => f.cjk,
        // Discretionary Ligatures: listed when the face's dlig lookups are ligatures only (Georgia, Verdana, ...: 10 of 10), mostly not when chaining
        // lookups come with them (Arial, Times New Roman, Tahoma, ...: 6 of 9; Segoe UI, Calibri are the exceptions)
        b"dlig" => f.ligature_only,
        _ => true,
    }).filter_map(|(tag, f)| {
        // Access All Alternates stands last
        feature_name(tag).map(|name| (if tag == b"aalt" { usize::MAX } else { f.seq }, if f.rtl { 3 } else { 4 }, format!("feature:{}", String::from_utf8_lossy(tag)), name))
    }).collect();
    named.sort_by_key(|entry| entry.0);
    out.extend(named.into_iter().map(|(_, group, id, name)| (group, id, name)));
    out
}

/// Does glyph `id` belong to the range view `show` ("range:<id>")? `None` = not a range view (a feature view is `feature_view::targets`).
pub fn view(show: &str) -> Option<Box<dyn Fn(&Mapping) -> bool>> {
    let id = show.strip_prefix("range:")?.to_owned();
    Some(Box::new(move |row| lowest(row).is_some_and(|cp| in_range(&id, cp))))
}
