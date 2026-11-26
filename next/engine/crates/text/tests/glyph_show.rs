//! The Glyphs panel's Show list (rules/glyphs-c3.ko.md, glyphs-c4.ko.md) against what InDesign 2025 lists, in the faces installed on this
//! machine (a face that is not installed skips its part): item names and order in Arial, Times New Roman, Segoe UI, Segoe UI Symbol and
//! Malgun Gothic, and the number of cells of every range view that fits one screen. The golden values are in tests/data/glyphs-show.json.
use folio_text::{FontStore, glyphs};

fn page(store: &FontStore, family: &str, show: &str) -> Option<glyphs::Page> {
    let req: glyphs::Request = serde_json::from_value(serde_json::json!({"family": family, "style": "Regular", "show": show, "limit": 128})).unwrap();
    glyphs::query(store, &[], &req).ok()
}

fn face_is_system_file(store: &FontStore, family: &str) -> bool {
    let resolved = store.resolve(family, "Regular", &[]);
    store.face_info(resolved.font).is_some_and(|i| i.path.to_string_lossy().to_lowercase().replace('\\', "/").contains("/windows/fonts/"))
}

fn golden() -> serde_json::Value { serde_json::from_str(include_str!("data/glyphs-show.json")).unwrap() }

#[test]
fn the_show_list_has_the_items_and_the_order_of_the_panel() {
    let store = FontStore::discover(&[]);
    for (family, want) in golden()["items"].as_object().unwrap() {
        let Some(page) = page(&store, family, "") else { eprintln!("skipped: {family} not installed"); continue };
        // the feature items follow the order of the font's own feature arrays: only the Windows system file is the one InDesign read (an Adobe Fonts
        // copy of Segoe UI that Folio finds first has its arrays sorted)
        if !face_is_system_file(&store, family) { eprintln!("skipped: {family} is not the system file"); continue; }
        let mut names: Vec<&str> = page.show_items.iter().map(|i| i.name.as_str()).collect();
        let mut want: Vec<&str> = want.as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        // the items whose presence rule is not known (Segoe UI): left out on both sides
        let diff = &golden()["itemDifferences"][family];
        for key in ["missing", "extra"] {
            for n in diff[key].as_array().into_iter().flatten() { let n = n.as_str().unwrap(); want.retain(|w| *w != n); names.retain(|w| *w != n); }
        }
        // the face on another Windows build may have a few more or fewer glyphs in a range, but never another item list
        assert_eq!(names, want, "{family}: Show list");
    }
}

#[test]
fn a_view_lists_the_glyphs_of_its_range_or_feature() {
    let store = FontStore::discover(&[]);
    for (show, cells) in golden()["arialViewCells"].as_object().unwrap() {
        let Some(page) = page(&store, "Arial", show) else { return eprintln!("skipped: Arial not installed") };
        let want = cells.as_u64().unwrap() as usize;
        // glyph counts move a little between Arial versions (this machine's Arial 7.x is the one measured): allow 3 either way
        assert!(page.total.abs_diff(want) <= 3, "{show}: {} glyphs, InDesign shows {want}", page.total);
    }
}

#[test]
fn the_latin_range_skips_the_controls_and_lists_a_shared_glyph_once() {
    let store = FontStore::discover(&[]);
    let Some(page) = page(&store, "Arial", "range:latin") else { return eprintln!("skipped: Arial not installed") };
    assert_eq!(page.entries[0].unicode.first(), Some(&0x20), "starts with the space");
    assert!(page.entries.iter().all(|e| e.unicode.iter().any(|cp| (0x20..=0xFF).contains(cp))));
    let mut ids: Vec<u16> = page.entries.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), page.entries.len());
}

/// The version string (name id 5) of the installed face: the cell counts below are exact for the version that was measured and within a few
/// glyphs for another Windows build.
fn version(store: &FontStore, family: &str) -> Option<String> {
    let resolved = store.resolve(family, "Regular", &[]);
    if !resolved.exact { return None; }
    store.with_face(resolved.font, |face| face.names().into_iter().find(|n| n.name_id == 5).and_then(|n| n.to_string())).flatten()
}

/// The lowest Unicode value (>= U+0020) of every glyph of a view, read page by page.
fn lowest_values(store: &FontStore, family: &str, show: &str) -> Vec<u32> {
    let mut out = Vec::new();
    loop {
        let req: glyphs::Request = serde_json::from_value(serde_json::json!({"family": family, "style": "Regular", "show": show, "limit": 128, "offset": out.len()})).unwrap();
        let page = glyphs::query(store, &[], &req).unwrap();
        out.extend(page.entries.iter().map(|e| e.unicode.iter().copied().find(|cp| *cp >= 0x20).unwrap_or(0)));
        if page.entries.is_empty() || out.len() >= page.total { return out; }
    }
}

#[test]
fn every_range_view_has_the_cells_the_panel_draws() {
    let store = FontStore::discover(&[]);
    for (family, entry) in golden()["viewCells"].as_object().unwrap() {
        let Some(have) = version(&store, family) else { eprintln!("skipped: {family} not installed"); continue };
        let same = Some(have.as_str()) == entry["version"].as_str();
        // InDesign 2025 files a character under a range by its own (older) Unicode table: a glyph whose character is unassigned in Unicode 15.1
        // (Arial 7.07 has U+20C1, U+20C3 and U+20C4, new currency signs) is in no range, while Folio's ICU data already knows it
        let newer: std::collections::HashSet<u32> = entry["newer"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
        for (show, cells) in entry["cells"].as_object().unwrap() {
            let want = cells.as_u64().unwrap() as usize;
            let ink = entry["ink"][show].as_u64().unwrap() as usize;
            // `cells` = the cells of the rule checked against the screen shots (a trailing blank or clipped cell is not drawn: ink <= cells)
            assert!(ink <= want && want <= ink + 3, "{family} {show}: golden ink {ink} cells {want}");
            let got = lowest_values(&store, family, show).into_iter().filter(|cp| !newer.contains(cp)).count();
            if same {
                assert_eq!(got, want, "{family} {show}: cells of the view (font version {have}, InDesign draws {ink})");
            } else {
                // another build of the face: a few glyphs more or fewer
                assert!(got.abs_diff(want) <= 3, "{family} {show}: {got} cells, InDesign draws {ink} (font version {have}, measured {})", entry["version"]);
            }
        }
    }
}

#[test]
fn every_feature_view_has_the_cells_the_panel_draws() {
    let store = FontStore::discover(&[]);
    let known = &golden()["featureDifferences"];
    for (family, views) in golden()["featureInk"].as_object().unwrap() {
        let Some(have) = version(&store, family) else { eprintln!("skipped: {family} not installed"); continue };
        let same = Some(have.as_str()) == golden()["viewCells"][family]["version"].as_str();
        let listed: Vec<String> = page(&store, family, "").unwrap().show_items.into_iter().map(|i| i.id).collect();
        for (show, ink) in views.as_object().unwrap() {
            // the feature items InDesign lists that Folio's list does not have are the item differences (Segoe UI), not this test's business
            if !listed.contains(show) || known[family].as_array().into_iter().flatten().any(|k| k == show) { continue; }
            let ink = ink.as_u64().unwrap() as usize;
            let total = page(&store, family, show).unwrap().total;
            // a cell at the end can be blank (nothing drawn): ink <= cells <= ink + 1
            if same { assert!(ink <= total && total <= ink + 1, "{family} {show}: {total} cells, InDesign draws {ink}"); }
            else { assert!(total.abs_diff(ink) <= 3, "{family} {show}: {total} cells, InDesign draws {ink} (font version {have})"); }
        }
    }
}
