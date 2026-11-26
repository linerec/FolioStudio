//! Alternates for Selection (rules/glyphs.toml glyphs.alternates) against the fonts installed on this machine. A font
//! that is not installed skips its part. The panel's own list cannot be read from InDesign without driving its window;
//! each feature's glyph is the shaper's, which the opentype oracle scenarios hold against InDesign's PDF.
use folio_text::{FontStore, glyphs};

fn page(store: &FontStore, family: &str, style: &str, text: &str) -> Option<glyphs::Page> {
    let req: glyphs::Request = serde_json::from_value(serde_json::json!({"family": family, "style": style, "filter": "alternates", "text": text})).unwrap();
    glyphs::query(store, &[], &req).ok()
}

#[test]
fn the_characters_own_glyph_comes_first_and_every_other_glyph_has_a_feature() {
    let store = FontStore::discover(&[]);
    let Some(page) = page(&store, "Gabriola", "Regular", "a") else { return eprintln!("skipped: Gabriola not installed") };
    let first = &page.entries[0];
    assert!(first.unicode.contains(&0x61) && first.features.is_empty(), "the cmap glyph of a, no feature: {first:?}");
    assert!(page.entries.len() > 3, "Gabriola gives many alternates: {}", page.entries.len());
    let mut ids: Vec<u16> = page.entries.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), page.entries.len(), "a glyph is listed once");
    assert!(page.entries[1..].iter().all(|e| !e.features.is_empty()), "every alternate names the features that give it");
    assert_eq!(page.total, page.entries.len());
}

#[test]
fn stylistic_sets_and_alternates_of_the_face_are_found() {
    let store = FontStore::discover(&[]);
    let Some(page) = page(&store, "Gabriola", "Regular", "a") else { return eprintln!("skipped: Gabriola not installed") };
    let tags: Vec<&str> = page.entries.iter().flat_map(|e| e.features.iter().map(String::as_str)).collect();
    assert!(tags.iter().any(|t| t.starts_with("ss")), "a stylistic set: {tags:?}");
    assert!(tags.contains(&"smcp") && tags.contains(&"ss01"), "small capitals and the first set (Gabriola a): {tags:?}");
}

#[test]
fn a_ligature_is_an_alternate_of_the_letters_it_replaces() {
    let store = FontStore::discover(&[]);
    let Some(page) = page(&store, "Calibri", "Regular", "fi") else { return eprintln!("skipped: Calibri not installed") };
    assert!(page.entries.iter().any(|e| e.features.iter().any(|t| t == "liga" || t == "clig")), "fi ligature: {:?}", page.entries);
}

#[test]
fn nothing_selected_lists_nothing_and_a_face_without_features_lists_one_glyph() {
    let store = FontStore::discover(&[]);
    if let Some(none) = page(&store, "Arial", "Regular", "") {
        assert_eq!((none.total, none.entries.len()), (0, 0));
    }
    if let Some(one) = page(&store, "Courier New", "Regular", "#") {
        assert!(!one.entries.is_empty() && one.entries[0].features.is_empty());
    }
}

#[test]
fn by_cid_gid_and_by_unicode_are_the_two_orders_and_by_unicode_is_the_default() {
    let store = FontStore::discover(&[]);
    let ask = |order: &str| -> Option<glyphs::Page> {
        let req: glyphs::Request = serde_json::from_value(serde_json::json!({"family": "Arial", "style": "Regular", "limit": 128, "order": order})).ok()?;
        glyphs::query(&store, &[], &req).ok()
    };
    let (Some(by_gid), Some(by_unicode), Some(default)) = (ask("gid"), ask("unicode"), ask("")) else { return eprintln!("skipped: Arial not installed") };
    assert_eq!(by_gid.entries.iter().map(|e| e.id).take(5).collect::<Vec<_>>(), [0, 1, 2, 3, 4], "By CID / GID is glyph id order");
    // InDesign opens the Glyphs panel with By Unicode checked (measured): that is the default
    assert_eq!(default.entries.iter().map(|e| e.id).collect::<Vec<_>>(), by_unicode.entries.iter().map(|e| e.id).collect::<Vec<_>>());
    assert_eq!(default.entries[0].unicode.first(), Some(&0x20), "starts at the space: control codes are not listed first");
    assert_eq!(by_gid.total, by_unicode.total, "the same glyphs, another order");
    let bad: glyphs::Request = serde_json::from_value(serde_json::json!({"family": "Arial", "style": "Regular", "order": "name"})).unwrap();
    assert!(glyphs::query(&store, &[], &bad).is_err());
}

/// The first 702 cells of "Entire Font" in InDesign 2025's Glyphs panel (By Unicode) for four installed faces, read from the panel
/// and matched cell by cell against the outlines (tests/data/glyphs-by-unicode.json). A face with another glyph count is another version.
#[test]
fn entire_font_order_is_the_one_the_indesign_panel_shows() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("data/glyphs-by-unicode.json")).unwrap();
    let store = FontStore::discover(&[]);
    let mut checked = 0;
    for (face, measured) in fixture["faces"].as_object().unwrap() {
        let (family, style) = face.split_once('/').unwrap();
        let ask = |offset: usize| -> Option<glyphs::Page> {
            let req: glyphs::Request = serde_json::from_value(serde_json::json!({"family": family, "style": style, "limit": 128, "offset": offset})).ok()?;
            glyphs::query(&store, &[], &req).ok()
        };
        let Some(first) = ask(0) else { eprintln!("skipped: {face} not installed"); continue };
        if first.glyph_count as u64 != measured["glyphs"].as_u64().unwrap() { eprintln!("skipped: {face} has {} glyphs (measured another version)", first.glyph_count); continue; }
        let mut ids: Vec<u16> = Vec::new();
        for offset in (0..702).step_by(128) { ids.extend(ask(offset).unwrap().entries.iter().map(|e| e.id)); }
        ids.truncate(702);
        let expected: Vec<u16> = measured["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u16).collect();
        let first_difference = ids.iter().zip(&expected).position(|(a, b)| a != b);
        assert_eq!(first_difference, None, "{face}: cell {first_difference:?}: Folio {:?} InDesign {:?}", first_difference.map(|i| ids[i]), first_difference.map(|i| expected[i]));
        checked += 1;
    }
    eprintln!("compared {checked} faces");
}

/// Arial 7.00 (4651 glyphs): Alternates for Selection as the panel shows it with one character selected (cells read from the panel).
#[test]
fn alternates_for_selection_are_the_members_and_the_order_the_indesign_panel_shows() {
    let store = FontStore::discover(&[]);
    let ask = |text: &str| -> Option<Vec<u16>> {
        let req: glyphs::Request = serde_json::from_value(serde_json::json!({"family": "Arial", "style": "Regular", "filter": "alternates", "text": text})).ok()?;
        let page = glyphs::query(&store, &[], &req).ok()?;
        (page.glyph_count == 4651).then(|| page.entries.iter().map(|e| e.id).collect())
    };
    let Some(a) = ask("A") else { return eprintln!("skipped: Arial (4651 glyphs) not installed") };
    assert_eq!(a, [36, 3445, 3465], "A, the a-shaped stylistic alternate, the small capital");
    assert_eq!(ask("a").unwrap(), [68, 3445, 3453, 3465]);
    assert_eq!(ask("g").unwrap(), [74, 42, 3471], "a unicase G sorts by its own Unicode value, the small capital counts as g");
    assert_eq!(ask("1").unwrap(), [20, 3426, 3436, 3675, 4548, 239], "oldstyle, proportional, denominator and tabular one, then the superscript one (U+00B9)");
    assert_eq!(ask("&").unwrap(), [9, 3773]);
    let mut capital_i = ask("I").unwrap();
    assert_eq!(capital_i[..4], [44, 3450, 3462, 3473], "I, its serif forms and the small capital");
    capital_i.sort_unstable();
    assert_eq!(capital_i, [44, 76, 3450, 3462, 3473], "the unicase i that gives I is listed too");
    let mut capital_z = ask("Z").unwrap();
    assert_eq!(capital_z[0], 61);
    capital_z.sort_unstable();
    assert_eq!(capital_z, [61, 93, 3490], "z (unicase) and the small capital");
}

#[test]
fn a_feature_glyph_in_the_list_is_the_glyph_the_layout_shaper_sets_for_that_feature() {
    use folio_model::{Capitalization, CharacterAttrs, Paragraph, TextRun, opentype::{OpenType, OpenTypeFeature}};
    use folio_text::shape::{FaceTable, Shaper};
    let store = FontStore::discover(&[]);
    let Some(list) = page(&store, "Gabriola", "Regular", "a") else { return eprintln!("skipped: Gabriola not installed") };
    let resolved = store.resolve("Gabriola", "Regular", &[]);
    assert!(resolved.exact);
    let faces = store.snapshot();
    let table = FaceTable::new(&store, &faces);
    let fallbacks = std::collections::HashMap::new();
    let shaper = Shaper::new(&table, &fallbacks);
    let glyph_of = |attrs: CharacterAttrs| -> u16 {
        let mut para = Paragraph::default();
        para.runs.push(TextRun { text: "a".into(), attrs, ..TextRun::default() });
        let shaped = shaper.shape_paragraph(&para, &[resolved], &|_, _| 0.0);
        shaped.glyphs[0].id
    };
    // the list applies one feature to the plain text (ligatures and contextual alternates, on by default, switched off): the layout is set up alike
    let mut base = CharacterAttrs { font_family: "Gabriola".into(), font_style: "Regular".into(), point_size: 24.0, ligatures: false, ..CharacterAttrs::default() };
    base.opentype.features.remove(&OpenTypeFeature::ContextualAlternate);
    let with = |f: &dyn Fn(&mut OpenType)| { let mut a = base.clone(); f(&mut a.opentype); a };
    let mut checked = 0;
    for n in 1..=20u32 {
        let tag = format!("ss{n:02}");
        let got = glyph_of(with(&|o| o.stylistic_sets = 1 << n));
        let listed: Vec<u16> = list.entries.iter().filter(|e| e.features.contains(&tag)).map(|e| e.id).collect();
        if !listed.is_empty() {
            assert!(listed.contains(&got), "{tag}: the layout sets glyph {got}, the list has {listed:?}");
            checked += 1;
        }
    }
    for (tag, feature) in [("swsh", OpenTypeFeature::Swash), ("salt", OpenTypeFeature::StylisticAlternate)] {
        let got = glyph_of(with(&|o| { o.features.insert(feature); }));
        let listed: Vec<u16> = list.entries.iter().filter(|e| e.features.iter().any(|f| f == tag)).map(|e| e.id).collect();
        if !listed.is_empty() {
            assert!(listed.contains(&got), "{tag}: the layout sets glyph {got}, the list has {listed:?}");
            checked += 1;
        }
    }
    let small = glyph_of(CharacterAttrs { capitalization: Capitalization::SmallCaps, ..base.clone() });
    let listed: Vec<u16> = list.entries.iter().filter(|e| e.features.iter().any(|f| f == "smcp")).map(|e| e.id).collect();
    assert!(listed.contains(&small), "smcp: the layout sets glyph {small}, the list has {listed:?}");
    assert!(checked >= 4, "several stylistic sets were compared: {checked}");
}

#[test]
fn several_selected_characters_that_do_not_join_show_the_first_ones_alternates() {
    // InDesign 2025, Arial: selecting "A1" lists A's alternates (A, small capital A, ...), not nothing (rules/glyphs-c3.ko.md)
    let store = FontStore::discover(&[]);
    let (Some(two), Some(one)) = (page(&store, "Arial", "Regular", "A1"), page(&store, "Arial", "Regular", "A")) else { return eprintln!("skipped: Arial not installed") };
    let ids = |p: &glyphs::Page| p.entries.iter().map(|e| e.id).collect::<Vec<_>>();
    assert!(ids(&one).len() >= 3 && ids(&two) == ids(&one), "A1 -> {:?}, A -> {:?}", ids(&two), ids(&one));
}

#[test]
fn times_new_roman_lists_the_number_of_alternates_the_panel_shows() {
    // InDesign 2025 (glyphs-c3, real caret/selection session): A -> 2 cells, a -> 2, 1 -> 5, f -> 2
    let store = FontStore::discover(&[]);
    for (text, cells) in [("A", 2), ("a", 2), ("1", 5), ("f", 2)] {
        let Some(p) = page(&store, "Times New Roman", "Regular", text) else { return eprintln!("skipped: Times New Roman not installed") };
        assert_eq!(p.entries.len(), cells, "Times New Roman {text}: {:?}", p.entries.iter().map(|e| (e.id, e.features.clone())).collect::<Vec<_>>());
    }
}

#[test]
fn alternates_of_one_character_have_the_cells_and_the_order_of_the_panel_in_every_face_measured() {
    // InDesign 2025, Arial / Segoe UI / Malgun Gothic: the cells of A a I Z 1 f g y h, and the glyph ids of I, Z (Arial) and I, 1 (Segoe UI) read from
    // the cells' tooltips (rules/glyphs-c4.ko.md; tests/data/glyphs-alternates.json). The glyph ids are those of the face version measured.
    let golden: serde_json::Value = serde_json::from_str(include_str!("data/glyphs-alternates.json")).unwrap();
    let store = FontStore::discover(&[]);
    for family in ["Arial", "Segoe UI", "Malgun Gothic"] {
        let entry = &golden[family];
        let resolved = store.resolve(family, "Regular", &[]);
        if !resolved.exact { eprintln!("skipped: {family} not installed"); continue; }
        let version = store.with_face(resolved.font, |face| face.names().into_iter().find(|n| n.name_id == 5).and_then(|n| n.to_string())).flatten();
        let same = version.as_deref() == entry["version"].as_str();
        for (text, cells) in entry["cells"].as_object().unwrap() {
            let p = page(&store, family, "Regular", text).unwrap();
            let want = cells.as_u64().unwrap() as usize;
            if same { assert_eq!(p.entries.len(), want, "{family} {text}"); } else { assert!(p.entries.len().abs_diff(want) <= 1, "{family} {text}: {} cells, InDesign {want}", p.entries.len()); }
        }
        if !same { continue; }
        for (text, ids) in entry["order"].as_object().unwrap() {
            let want: Vec<u64> = ids.as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
            let got: Vec<u64> = page(&store, family, "Regular", text).unwrap().entries.iter().map(|e| u64::from(e.id)).collect();
            assert_eq!(got, want, "{family} {text}: glyph ids in the order of the panel");
        }
    }
}
