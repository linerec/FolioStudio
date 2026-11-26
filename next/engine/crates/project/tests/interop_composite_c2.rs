//! Composite model rows (documents, spreads, items, stories, tables ...) of interop.tsv.
//!
//! A composite row does not claim that every value inside it equals InDesign's. It claims that the structure (which parts
//! the object has, in which order, which are present) is read from IDML and comes back unchanged from Folio's own IDML
//! export. The evidence is the whole checked-in IDML corpus (every `oracle/fixtures/*.idml`, real InDesign output): each is
//! read, written through the real .folio writer and IDML exporter, and read again; the two models and every IDML part must be
//! equal. The census proves the corpus really contains each composite (a row nothing in the corpus exercises is not
//! claimed here). Differences of a single value (approximate / Folio-only fields) stay in the rows of those fields.
#[path = "support/interop.rs"]
mod support;
use serde_json::Value;
use std::collections::BTreeMap;
use support::{fixture, root, roundtrip};

fn fixture_names() -> Vec<String> {
    fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                walk(&path, &format!("{prefix}{name}/"), out);
            } else if let Some(stem) = name.strip_suffix(".idml") {
                out.push(format!("{prefix}{stem}"));
            }
        }
    }
    let mut names = Vec::new();
    walk(&root().join("oracle/fixtures"), "", &mut names);
    names.sort();
    names
}

/// Values the reader invents for the current run (path of the opened file, caller's file facts): never stored in IDML.
fn without_runtime(mut doc: Value) -> Value {
    let map = doc.as_object_mut().unwrap();
    map.remove("source");
    if let Some(facts) = map.get_mut("facts").and_then(Value::as_object_mut) {
        facts.remove("file_name");
        facts.remove("folder");
    }
    doc
}

/// What the census counts: a value that carries something (not null, false, empty text, list or object).
fn meaningful(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

/// `*` = every list element or object value, `**` = any depth (also zero), `=text` at the end = the value is that text.
fn count(value: &Value, pattern: &[&str]) -> usize {
    let Some((head, rest)) = pattern.split_first() else {
        return usize::from(meaningful(value));
    };
    if let Some(text) = head.strip_prefix('=') {
        return usize::from(value.as_str() == Some(text));
    }
    let children: Vec<&Value> = match value {
        Value::Array(a) => a.iter().collect(),
        Value::Object(o) => o.values().collect(),
        _ => vec![],
    };
    match *head {
        "**" => {
            count(value, rest)
                + children.iter().map(|c| count(c, pattern)).sum::<usize>()
        }
        "*" => children.iter().map(|c| count(c, rest)).sum(),
        key => value.get(key).map_or(0, |c| count(c, rest)),
    }
}

/// Row of interop.tsv -> JSON patterns that show up in a document that really has it.
fn census_rows() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Document.spreads", vec!["spreads/*"]),
        ("Document.master_spreads", vec!["master_spreads/*"]),
        ("Document.stories", vec!["stories/*"]),
        ("Document.swatches", vec!["swatches/*"]),
        ("Document.layers", vec!["layers/*"]),
        ("Document.font_table", vec!["font_table/*"]),
        ("Document.gradients", vec!["gradients/*"]),
        ("Document.swatch_order", vec!["swatch_order/*"]),
        ("Document.text_variables", vec!["text_variables/*"]),
        ("Document.styles", vec!["styles/paragraph/*", "styles/character/*", "styles/object/*"]),
        ("Spread.pages", vec!["spreads/*/pages/*"]),
        ("Spread.items", vec!["spreads/*/items/*"]),
        ("Spread.guides", vec!["spreads/*/guides/*"]),
        ("Spread.id", vec!["spreads/*/id"]),
        ("Page", vec!["spreads/*/pages/*/bounds"]),
        ("PageColumns", vec!["spreads/*/pages/*/columns/positions", "spreads/*/pages/*/columns/gutter"]),
        ("Guide", vec!["spreads/*/guides/*/location"]),
        ("Guide.position", vec!["spreads/*/guides/*/position"]),
        ("Guide.page", vec!["spreads/*/guides/*/page"]),
        ("PageItem", vec!["spreads/*/items/*/transform"]),
        ("PageItem.kind", vec!["**/items/*/kind"]),
        ("PageItem.stroke", vec!["**/items/*/stroke/paint"]),
        ("PageItem.clipped", vec!["**/clipped/*"]),
        ("PageItem.fill_gradient", vec!["**/fill_gradient"]),
        ("PageItem.stroke_gradient", vec!["**/stroke_gradient"]),
        ("PageItem.text_wrap", vec!["**/text_wrap"]),
        ("PageItem.print", vec!["**/items/*/print/fx"]),
        ("ItemKind", vec!["**/items/*/kind"]),
        ("ItemKind::Rectangle", vec!["**/kind/Rectangle"]),
        ("ItemKind::Oval", vec!["**/kind/Oval"]),
        ("ItemKind::Polygon", vec!["**/kind/Polygon"]),
        ("ItemKind::GraphicLine", vec!["**/kind/=GraphicLine"]),
        ("ItemKind::TextFrame", vec!["**/kind/TextFrame"]),
        ("ItemKind::TextFrame.0", vec!["**/kind/TextFrame/story"]),
        ("ItemKind::Group", vec!["**/kind/Group"]),
        ("ItemKind::Group.children", vec!["**/kind/Group/children/*"]),
        ("ItemKind::Other", vec!["**/kind/Other"]),
        ("ItemKind::Other.element", vec!["**/kind/Other/element"]),
        ("TextFrame", vec!["**/kind/TextFrame/columns"]),
        ("TextFrame.auto_size", vec!["**/kind/TextFrame/auto_size"]),
        ("AutoSize", vec!["**/auto_size/kind"]),
        ("TextWrap", vec!["**/text_wrap/mode"]),
        ("Graphic", vec!["**/content/element"]),
        ("Graphic.clip", vec!["**/content/clip"]),
        ("Rect", vec!["spreads/*/pages/*/bounds/left"]),
        ("Stroke", vec!["**/items/*/stroke/weight"]),
        ("ItemPrint", vec!["**/items/*/print/fx"]),
        ("ItemPrint.fx", vec!["**/items/*/print/fx"]),
        ("ItemPrint.level_fx", vec!["**/items/*/print/level_fx"]),
        ("ItemPrint.media_link", vec!["**/print/media_link"]),
        ("ItemPrint.placed_art", vec!["**/print/placed_art"]),
        ("ItemPrint.poster", vec!["**/print/poster"]),
        ("ObjectEffects", vec!["**/print/fx/drop_shadow", "**/print/fx/feather", "**/print/fx/inner_shadow"]),
        ("Story", vec!["stories/*/paragraphs"]),
        ("Story.id", vec!["stories/*/id"]),
        ("Story.paragraphs", vec!["stories/*/paragraphs/*"]),
        ("Paragraph", vec!["stories/*/paragraphs/*/runs"]),
        ("Paragraph.attrs", vec!["stories/*/paragraphs/*/attrs/justification"]),
        ("Paragraph.runs", vec!["stories/*/paragraphs/*/runs/*"]),
        ("ParagraphAttrs", vec!["stories/*/paragraphs/*/attrs/left_indent", "stories/*/paragraphs/*/attrs/space_after"]),
        ("ParagraphAttrs.tabs", vec!["stories/*/paragraphs/*/attrs/tabs/*"]),
        ("ParagraphAttrs.rule_above", vec!["**/attrs/rule_above"]),
        ("ParagraphAttrs.rule_below", vec!["**/attrs/rule_below"]),
        ("ParagraphAttrs.shading", vec!["**/attrs/shading"]),
        ("ParagraphAttrs.border", vec!["**/attrs/border"]),
        ("ParagraphAttrs.list", vec!["**/attrs/list"]),
        ("TextRun", vec!["**/runs/*/text"]),
        ("TextRun.attrs", vec!["**/runs/*/attrs/font_family"]),
        ("TextRun.inline", vec!["**/runs/*/inline/*"]),
        ("TextRun.tables", vec!["**/runs/*/tables/*"]),
        ("TextRun.variable", vec!["**/runs/*/variable"]),
        ("Table", vec!["**/runs/*/tables/*/rows/*"]),
        ("Table.rows", vec!["**/tables/*/rows/*"]),
        ("Table.cells", vec!["**/tables/*/cells/*"]),
        ("Table.style", vec!["**/tables/*/style"]),
        ("TableRow", vec!["**/tables/*/rows/*/height"]),
        ("TableCell", vec!["**/tables/*/cells/*/id"]),
        ("TableCell.paragraphs", vec!["**/tables/*/cells/*/paragraphs/*"]),
        ("TableCell.edges", vec!["**/tables/*/cells/*/edges/*/weight"]),
        ("TableCell.diagonal", vec!["**/tables/*/cells/*/diagonal/paint"]),
        ("Id", vec!["**/id"]),
        ("Document", vec!["spreads/*/id"]),
        ("ViewPreferences", vec!["view/horizontal_units"]),
        ("ViewPreferences.grid", vec!["view/grid"]),
        ("GridPreferences", vec!["view/grid/*"]),
        ("TextPreferences", vec!["text_preferences/small_cap"]),
        ("Pasteboard", vec!["pasteboard/vertical"]),
        ("PrintSetup", vec!["print/bleed"]),
        ("PrintSetup.inks", vec!["print/inks/*"]),
        ("PrintSetup.mixed_inks", vec!["print/mixed_inks/*"]),
        ("Ink", vec!["print/inks/*/name"]),
        ("FootnoteOptions", vec!["footnotes/start_at"]),
        ("FootnoteOptions.rule", vec!["footnotes/rule"]),
        ("FootnoteRule", vec!["footnotes/rule/weight"]),
        ("StyleCatalog", vec!["styles/paragraph/*"]),
        ("StyleEntry", vec!["styles/paragraph/*/id", "styles/character/*/id"]),
        ("VariableDef", vec!["text_variables/*/kind"]),
        ("FontInfo", vec!["font_table/*/family"]),
        ("SourceInfo", vec!["source/dom_version"]),
        ("Document.source", vec!["source/reader"]),
        ("Document.text_preferences", vec!["text_preferences/small_cap"]),
        ("Document.print", vec!["print/bleed"]),
        ("Document.pasteboard", vec!["pasteboard/vertical"]),
        ("Document.footnotes", vec!["footnotes/start_at"]),
        ("Document.view", vec!["view/horizontal_units"]),
        ("Swatch", vec!["swatches/*/id"]),
        ("Gradient", vec!["gradients/*/stops/*"]),
        ("GradientStop", vec!["gradients/*/stops/*"]),
        ("GradientGeometry", vec!["**/fill_gradient/angle", "**/stroke_gradient/angle"]),
        ("CharacterAttrs", vec!["**/runs/*/attrs/font_family"]),
        ("CharacterAttrs.kenten", vec!["**/runs/*/attrs/kenten"]),
        ("Kenten", vec!["**/runs/*/attrs/kenten"]),
        ("KerningMethod", vec!["**/runs/*/attrs/kerning_method"]),
        ("ListStyle", vec!["**/attrs/list/kind"]),
        ("ListStyle.alignment", vec!["**/attrs/list/alignment"]),
        ("ListStyle.marker", vec!["**/attrs/list/marker/horizontal_scale"]),
        ("ParagraphRule", vec!["**/attrs/rule_above/weight", "**/attrs/rule_below/weight"]),
        ("ParagraphShading", vec!["**/attrs/shading"]),
        ("ParagraphBorder", vec!["**/attrs/border"]),
        ("SpacingRange", vec!["**/attrs/word_spacing/min"]),
        ("Note", vec!["**/runs/*/note"]),
        ("TextVariable", vec!["**/runs/*/variable"]),
        ("CellStroke", vec!["**/tables/*/cells/*/edges/*/weight"]),
        ("ShadowEffect", vec!["**/fx/drop_shadow", "**/fx/outer_glow", "**/fx/inner_shadow"]),
        ("FeatherEffect", vec!["**/fx/feather"]),
        ("DirectionalFeather", vec!["**/fx/directional_feather"]),
        ("GradientFeather", vec!["**/fx/gradient_feather"]),
        ("GradientFeather.stops", vec!["**/fx/gradient_feather/stops/*"]),
        ("InnerGlow", vec!["**/fx/inner_glow"]),
        ("BevelEmboss", vec!["**/fx/bevel"]),
        ("Satin", vec!["**/fx/satin"]),
        ("LevelEffects", vec!["**/level_fx/fill/*", "**/level_fx/stroke/*", "**/level_fx/content/*"]),
        ("LevelEffects.fill", vec!["**/level_fx/fill/*"]),
        ("LevelEffects.stroke", vec!["**/level_fx/stroke/*"]),
        ("LevelEffects.content", vec!["**/level_fx/content/*"]),
    ]
}

/// One state of the corpus: a checked-in fixture, or a fixture with the parts InDesign wrote for one oracle case laid over it.
struct Case {
    fixture: String,
    label: String,
    parts: std::collections::BTreeMap<String, String>,
}

fn golden_cases(fixtures: &[String]) -> Vec<Case> {
    let mut cases = Vec::new();
    let goldens = root().join("oracle/goldens");
    for fixture in fixtures {
        let base = fixture_parts_names(fixture);
        let Ok(dirs) = std::fs::read_dir(goldens.join(fixture)) else { continue };
        let mut dirs: Vec<_> = dirs.filter_map(Result::ok).map(|d| d.path()).collect();
        dirs.sort();
        for dir in dirs {
            let after = dir.join("after");
            if !after.is_dir() {
                continue;
            }
            let mut parts = fixture_map(fixture);
            let mut laid = 0;
            let mut stack = vec![after.clone()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap().filter_map(Result::ok) {
                    let path = e.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().is_some_and(|x| x == "xml") {
                        let rel = path.strip_prefix(&after).unwrap().to_string_lossy().replace(char::from(92), "/");
                        if base.contains(&rel) {
                            if let Ok(text) = std::fs::read_to_string(&path) {
                                parts.insert(rel, text);
                                laid += 1;
                            }
                        }
                    }
                }
            }
            if laid > 0 {
                cases.push(Case {
                    fixture: fixture.clone(),
                    label: format!("{fixture}/{}", dir.file_name().unwrap().to_string_lossy()),
                    parts,
                });
            }
        }
    }
    cases
}

fn fixture_map(name: &str) -> std::collections::BTreeMap<String, String> {
    fixture(name)
}

fn fixture_parts_names(name: &str) -> std::collections::BTreeSet<String> {
    fixture(name).into_keys().collect()
}

struct Tally {
    census: BTreeMap<&'static str, usize>,
    failures: Vec<String>,
    unreadable: Vec<String>,
    cases: usize,
}

fn check_case(case: &Case, rows: &[(&'static str, Vec<&'static str>)], tally: &mut Tally) {
    let result = std::panic::catch_unwind(|| roundtrip(&case.fixture, &case.parts));
    let (before, after, written) = match result {
        Ok(r) => r,
        Err(_) => {
            tally.unreadable.push(case.label.clone());
            return;
        }
    };
    tally.cases += 1;
    for (part, xml) in &case.parts {
        if written.get(part) != Some(xml) {
            tally.failures.push(format!("{}: IDML part {part} changed or vanished", case.label));
        }
    }
    let whole = before.clone();
    let (b, a) = (without_runtime(before), without_runtime(after));
    if b != a {
        let key = b
            .as_object()
            .unwrap()
            .iter()
            .find(|(k, v)| a.get(k.as_str()) != Some(*v))
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        tally.failures.push(format!("{}: model differs after export in `{key}`", case.label));
    }
    for (row, patterns) in rows {
        let n: usize = patterns.iter().map(|p| count(&whole, &p.split('/').collect::<Vec<_>>())).sum();
        *tally.census.entry(row).or_default() += n;
    }
}

#[test]
fn every_corpus_and_indesign_golden_state_survives_idml_export_unchanged() {
    let names = fixture_names();
    assert!(names.len() >= 190, "the checked-in IDML corpus is missing: {}", names.len());
    let rows: Vec<_> = census_rows().into_iter().collect::<BTreeMap<_, _>>().into_iter().collect();
    let mut tally = Tally { census: BTreeMap::new(), failures: Vec::new(), unreadable: Vec::new(), cases: 0 };
    let mut cases: Vec<Case> = names
        .iter()
        .map(|n| Case { fixture: n.clone(), label: n.clone(), parts: fixture(n) })
        .collect();
    let fixtures = cases.len();
    cases.extend(golden_cases(&names));
    println!("fixtures: {fixtures}, golden states: {}", cases.len() - fixtures);
    // The states are independent (each round trip has its own folder), so the corpus runs on all cores.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    let chunk = cases.len().div_ceil(threads);
    let parts: Vec<Tally> = std::thread::scope(|scope| {
        let handles: Vec<_> = cases
            .chunks(chunk)
            .map(|chunk| {
                let rows = &rows;
                scope.spawn(move || {
                    let mut t = Tally { census: BTreeMap::new(), failures: Vec::new(), unreadable: Vec::new(), cases: 0 };
                    for case in chunk {
                        check_case(case, rows, &mut t);
                    }
                    t
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for t in parts {
        for (row, n) in t.census {
            *tally.census.entry(row).or_default() += n;
        }
        tally.failures.extend(t.failures);
        tally.unreadable.extend(t.unreadable);
        tally.cases += t.cases;
    }
    println!("read and exported: {} of {}; unreadable: {}", tally.cases, cases.len(), tally.unreadable.len());
    for label in tally.unreadable.iter().take(20) {
        println!("unreadable {label}");
    }
    for (row, n) in &tally.census {
        println!("census {row} = {n}");
    }
    assert!(tally.failures.is_empty(), "{:#?}", &tally.failures[..tally.failures.len().min(40)]);
    assert!(tally.unreadable.len() * 50 <= cases.len(), "too many states cannot be read as documents: {}", tally.unreadable.len());
    let empty: Vec<_> = tally.census.iter().filter(|(_, n)| **n == 0).map(|(r, _)| *r).collect();
    println!("rows the corpus never exercises: {empty:?}");
    // These composites are in no fixture and no InDesign state; the tests below build them (Kenten, movies and sounds, ramp points).
    const BY_THE_TESTS_BELOW: [&str; 7] = [
        "CharacterAttrs.kenten", "Kenten", "GradientFeather.stops", "ItemKind::Other", "ItemKind::Other.element",
        "ItemPrint.media_link", "ItemPrint.poster",
    ];
    let missing: Vec<_> = empty.iter().filter(|r| !BY_THE_TESTS_BELOW.contains(r)).collect();
    assert!(missing.is_empty(), "rows the corpus never exercises: {missing:?}");
}

#[test]
#[ignore = "development helper: FOLIO_DUMP=<fixture> writes the model JSON"]
fn dump_model_json() {
    let name = std::env::var("FOLIO_DUMP").unwrap();
    let (before, _, _) = roundtrip(&name, &fixture(&name));
    std::fs::write(root().join("../artifacts/interop-c2-20261023/dump.json"), serde_json::to_string_pretty(&before).unwrap()).unwrap();
}

fn corner(page: &Value) -> (f64, f64) {
    let n = |v: &Value, k: &str| v[k].as_f64().unwrap();
    let (t, b) = (&page["transform"], &page["bounds"]);
    let mut min = (f64::MAX, f64::MAX);
    for (x, y) in [(n(b, "left"), n(b, "top")), (n(b, "right"), n(b, "top")), (n(b, "right"), n(b, "bottom")), (n(b, "left"), n(b, "bottom"))] {
        let p = (n(t, "a") * x + n(t, "c") * y + n(t, "tx"), n(t, "b") * x + n(t, "d") * y + n(t, "ty"));
        min = (min.0.min(p.0), min.1.min(p.1));
    }
    min
}

/// `Guide.position` (spread coordinates) and `Guide.page` are not IDML attributes: IDML stores `Location` measured from the ruler origin and
/// puts the guide into a `Page` element. Folio derives both when it reads. They must follow the stored values for every ruler origin and
/// come out the same again after Folio's own export.
#[test]
fn guide_position_and_page_are_derived_from_location_ruler_origin_and_page_element() {
    let mut checked = 0;
    let mut on_page = 0;
    for name in fixture_names() {
        let base = fixture(&name);
        if !base.iter().any(|(p, x)| p.starts_with("Spreads/") && x.contains("<Guide ")) {
            continue;
        }
        for origin in ["SpreadOrigin", "PageOrigin", "SpineOrigin"] {
            let mut parts = base.clone();
            let Some(xml) = parts.get_mut("Resources/Preferences.xml") else { continue };
            if !xml.contains("<ViewPreference ") {
                continue;
            }
            support::attr(xml, "ViewPreference", 0, "RulerOrigin", origin);
            let (before, after, written) = roundtrip(&name, &parts);
            assert_eq!(written["Resources/Preferences.xml"], parts["Resources/Preferences.xml"], "{name}");
            assert_eq!(before["view"]["ruler_origin"], origin, "{name}");
            for doc in [&before, &after] {
                for spread in doc["spreads"].as_array().unwrap().iter().chain(doc["master_spreads"].as_array().unwrap()) {
                    let pages = spread["pages"].as_array().unwrap();
                    let corners: Vec<(f64, f64)> = pages.iter().map(corner).collect();
                    let spread_origin = corners.iter().fold((f64::MAX, f64::MAX), |a, c| (a.0.min(c.0), a.1.min(c.1)));
                    for g in spread["guides"].as_array().unwrap() {
                        let horizontal = g["horizontal"].as_bool().unwrap();
                        let mut o = spread_origin;
                        match origin {
                            "PageOrigin" => {
                                if let Some(c) = g["page"].as_u64().and_then(|i| corners.get(i as usize)) {
                                    o = *c;
                                }
                            }
                            "SpineOrigin" if !horizontal && corners.len() == 2 => o.0 = corners[0].0.max(corners[1].0),
                            _ => {}
                        }
                        let expected = g["location"].as_f64().unwrap() + if horizontal { o.1 } else { o.0 };
                        assert!((g["position"].as_f64().unwrap() - expected).abs() < 1e-9, "{name} {origin}: guide position");
                        if let Some(i) = g["page"].as_u64() {
                            assert!((i as usize) < pages.len(), "{name}: guide page index in range");
                            on_page += 1;
                        }
                        checked += 1;
                    }
                }
            }
            assert_eq!(without_runtime(before), without_runtime(after), "{name} {origin}");
        }
    }
    assert!(checked >= 30 && on_page >= 6, "guides checked: {checked}, on a page: {on_page}");
}

fn insert_before_spread_end(xml: &mut String, snippet: &str) {
    let at = xml.rfind("</Spread>").unwrap();
    xml.insert_str(at, snippet);
}

fn path_geometry(xml: &str) -> String {
    let start = xml.find("<PathGeometry>").unwrap();
    let end = start + xml[start..].find("</PathGeometry>").unwrap() + "</PathGeometry>".len();
    xml[start..end].to_owned()
}

/// Page items the model does not draw (`ItemKind::Other`) keep their place, their media link and their poster image; the XML stays as written.
#[test]
fn unmodelled_page_items_keep_element_link_and_poster() {
    const PART: &str = "Spreads/Spread_uf3.xml";
    let mut parts = fixture("basic");
    let geometry = path_geometry(&parts[PART]);
    let items = format!(
        "<Movie Self=\"c2movie\" ItemLayer=\"ub8\" ItemTransform=\"1 0 0 1 0 0\" Visible=\"true\"><Properties>{geometry}</Properties><Image Self=\"c2poster\" ItemTransform=\"1 0 0 1 0 0\" ImageTypeName=\"$ID/PNG\" Space=\"$ID/#Links_RGB\"><Properties><GraphicBounds Left=\"0\" Top=\"0\" Right=\"20\" Bottom=\"20\" /></Properties><Link Self=\"c2posterlink\" LinkResourceURI=\"file:/media/poster.png\" LinkResourceFormat=\"$ID/PNG\" StoredState=\"Normal\" /></Image><Link Self=\"c2movielink\" LinkResourceURI=\"file:/media/clip.mp4\" LinkResourceFormat=\"$ID/MPEG\" StoredState=\"Normal\" /></Movie><Sound Self=\"c2sound\" ItemLayer=\"ub8\" ItemTransform=\"1 0 0 1 0 0\" Visible=\"true\"><Properties>{geometry}</Properties><Link Self=\"c2soundlink\" LinkResourceURI=\"file:/media/sound.mp3\" LinkResourceFormat=\"$ID/MP3\" StoredState=\"Normal\" /></Sound><Button Self=\"c2button\" ItemLayer=\"ub8\" ItemTransform=\"1 0 0 1 0 0\" Visible=\"true\"><Properties>{geometry}</Properties></Button>"
    );
    insert_before_spread_end(parts.get_mut(PART).unwrap(), &items);
    let (before, after, written) = roundtrip("basic", &parts);
    assert_eq!(written[PART], parts[PART]);
    assert_eq!(without_runtime(before.clone()), without_runtime(after.clone()));
    for doc in [&before, &after] {
        let find = |id: &str| support::by_id(doc, id).clone();
        let movie = find("c2movie");
        assert_eq!(movie["kind"]["Other"]["element"], "Movie");
        assert!(movie["print"]["media_link"]["uri"].as_str().unwrap().ends_with("clip.mp4"));
        assert!(movie["print"]["poster"]["link"]["uri"].as_str().unwrap().ends_with("poster.png"));
        let sound = find("c2sound");
        assert_eq!(sound["kind"]["Other"]["element"], "Sound");
        assert!(sound["print"]["media_link"]["uri"].as_str().unwrap().ends_with("sound.mp3"));
        assert!(sound["print"]["poster"].is_null());
        let button = find("c2button");
        assert_eq!(button["kind"]["Other"]["element"], "Button");
        assert!(button["print"]["media_link"].is_null() && button["print"]["poster"].is_null());
    }
}

/// Every Kenten field together (a character range with its own kind, size, placement, side, colour and custom character).
#[test]
fn kenten_fields_together_survive_idml_export() {
    const PART: &str = "Stories/Story_u118.xml";
    let mut parts = fixture("basic");
    let xml = parts.get_mut(PART).unwrap();
    for (key, value) in [
        ("KentenKind", "KentenFisheye"), ("KentenPosition", "BelowLeft"), ("KentenPlacement", "3.5"), ("KentenFontSize", "9"),
        ("KentenCustomCharacter", "★"), ("KentenFillColor", "Color/Black"), ("KentenTint", "40"),
    ] {
        support::attr(xml, "CharacterStyleRange", 0, key, value);
    }
    let (before, after, written) = roundtrip("basic", &parts);
    assert_eq!(written[PART], parts[PART]);
    let pointer = "/stories/u118/paragraphs/0/runs/0/attrs/kenten";
    let expected = serde_json::json!({
        "kind": "Fisheye", "size": 9.0, "placement": 3.5, "below": true,
        "paint": {"Swatch": {"id": "Color/Black", "tint": 40.0}}, "custom": "★",
    });
    support::check(&before, &after, pointer, expected);
    assert_eq!(without_runtime(before), without_runtime(after));
}

/// The ramp points of a gradient feather are not read: the model keeps the settings that it draws (type, angle, length, start) and an empty point list,
/// whatever child elements the setting has. The XML of the setting, children included, stays as written.
#[test]
fn gradient_feather_settings_are_kept_and_ramp_points_are_not_read() {
    const PART: &str = "Spreads/Spread_uf3.xml";
    let golden = root().join("oracle/goldens/basic/fx-grad-feather-values/after").join(PART);
    for children in ["", "<OpacityGradientStop Location=\"0\" Opacity=\"100\" /><OpacityGradientStop Location=\"60\" Opacity=\"20\" />"] {
        let mut parts = fixture("basic");
        let mut xml = std::fs::read_to_string(&golden).unwrap();
        if !children.is_empty() {
            xml = xml.replace("<GradientFeatherSetting Applied=\"true\" Type=\"Radial\" Angle=\"30\" />",
                &format!("<GradientFeatherSetting Applied=\"true\" Type=\"Radial\" Angle=\"30\">{children}</GradientFeatherSetting>"));
            assert!(xml.contains("OpacityGradientStop"));
        }
        parts.insert(PART.into(), xml);
        let (before, after, written) = roundtrip("basic", &parts);
        assert_eq!(written[PART], parts[PART]);
        for doc in [&before, &after] {
            let g = &support::by_id(doc, "uff")["print"]["fx"]["gradient_feather"];
            assert_eq!(g["kind"], "Radial");
            assert_eq!(g["angle"], 30.0);
            assert_eq!(g["stops"], serde_json::json!([]));
        }
    }
}
