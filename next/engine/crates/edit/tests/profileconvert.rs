//! Edit ▸ Convert to Profile on the oracle `convert-profile-before.idml` (InDesign 2026, 2025 not checked): the document has sRGB and U.S. Web Coated
//! (SWOP) v2, 40 random CMYK and 40 random RGB swatches and the default ones; `convert-profile-after.idml` is what InDesign wrote after
//! converting to Adobe RGB (1998) and Coated FOGRA39 (Relative Colorimetric, black point compensation on; `artifacts/profileconv-c1-20261023`).
//! The colour arithmetic is held by `folio-color`'s `convert_reference`; here: what changes in the document, undo, names and references.
use folio_color::ProfileIndex;
use folio_edit::{Editor, commands::RegisteredCommand};
use std::collections::BTreeMap;

const ADOBE_RGB: &str = "Adobe RGB (1998)";
const FOGRA39: &str = "Coated FOGRA39 (ISO 12647-2:2004)";
type Attrs = BTreeMap<String, String>;

fn fixture(name: &str) -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures").join(name);
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if !e.is_dir() {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut e, &mut s).unwrap();
            out.insert(e.name().unwrap().into_owned(), s);
        }
    }
    out
}

fn installed() -> bool {
    let index = ProfileIndex::scan_installed();
    let ok = [ADOBE_RGB, FOGRA39, "sRGB IEC61966-2.1", "U.S. Web Coated (SWOP) v2"].iter().all(|n| index.find(n).is_some());
    if !ok {
        eprintln!("SKIPPED: the ICC profiles of the test are not installed");
    }
    ok
}

/// The attributes of the start tag `tag`.
fn attrs(tag: &str) -> Attrs {
    let mut out = BTreeMap::new();
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].rsplit(|c: char| c.is_whitespace() || c == '<').next().unwrap_or("").to_owned();
        let after = &rest[eq + 2..];
        let end = after.find('"').unwrap();
        out.insert(key, after[..end].replace("&quot;", "\"").replace("&amp;", "&"));
        rest = &after[end + 1..];
    }
    out
}

/// Every `<Color …>` of Graphic.xml, in order.
fn colors(graphic: &str) -> Vec<Attrs> {
    let mut out = Vec::new();
    let mut rest = graphic;
    while let Some(i) = rest.find("<Color ") {
        let end = rest[i..].find('>').unwrap() + i;
        out.push(attrs(&rest[i..=end]));
        rest = &rest[end..];
    }
    out
}

/// A swatch's identity across a rename: its slot in the swatch panel (the hidden unnamed colours have none: their `Self`).
fn key(a: &Attrs) -> String {
    match a["SwatchColorGroupReference"].as_str() {
        "n" => a["Self"].clone(),
        slot => slot.to_owned(),
    }
}

fn convert(rgb: &str, cmyk: &str, intent: &str, bpc: bool) -> RegisteredCommand {
    RegisteredCommand::ConvertToProfile { rgb_profile: rgb.into(), cmyk_profile: cmyk.into(), intent: intent.into(), black_point_compensation: bpc }
}

fn values(a: &Attrs) -> Vec<f64> {
    a["ColorValue"].split_whitespace().map(|v| v.parse().unwrap()).collect()
}

fn document(designmap: &str) -> Attrs {
    let at = designmap.find("<Document ").unwrap();
    attrs(&designmap[at..at + designmap[at..].find('>').unwrap()])
}

#[test]
fn converts_the_swatches_like_indesign_and_undoes_in_one_step() {
    if !installed() {
        return;
    }
    let before = fixture("convert-profile-before.idml");
    let after = fixture("convert-profile-after.idml");
    let mut ed = Editor::new(before.clone());
    let out = ed.apply("Convert to Profile", &[convert(ADOBE_RGB, FOGRA39, "RelativeColorimetric", true).into()]).unwrap();
    assert!(out.parts.iter().any(|p| p == "Resources/Graphic.xml") && out.parts.iter().any(|p| p == "designmap.xml"));
    assert_eq!(out.label, "Convert to Profile");

    // the document is assigned the destination profiles; policies and intents stay
    let (doc, doc_ref) = (document(ed.part("designmap.xml").unwrap()), document(&after["designmap.xml"]));
    for key in ["RGBProfile", "CMYKProfile", "RGBPolicy", "CMYKPolicy", "SolidColorIntent", "DefaultImageIntent", "AfterBlendingIntent"] {
        assert_eq!(doc[key], doc_ref[key], "Document@{key}");
    }

    // every swatch colour agrees with InDesign's
    let mine: BTreeMap<_, _> = colors(ed.part("Resources/Graphic.xml").unwrap()).into_iter().map(|a| (key(&a), a)).collect();
    let theirs: BTreeMap<_, _> = colors(&after["Resources/Graphic.xml"]).into_iter().map(|a| (key(&a), a)).collect();
    let orig: BTreeMap<_, _> = colors(&before["Resources/Graphic.xml"]).into_iter().map(|a| (key(&a), a)).collect();
    assert_eq!(mine.len(), theirs.len());
    let (mut changed, mut worst) = (0, 0.0f64);
    for (k, a) in &mine {
        let b = &theirs[k];
        assert_eq!((&a["Space"], &a["Model"]), (&b["Space"], &b["Model"]));
        let (va, vb) = (values(a), values(b));
        let cmyk = a["Space"] == "CMYK";
        let d = va.iter().zip(&vb).map(|(x, y)| (x - y).abs() * if cmyk { 2.55 } else { 1.0 }).fold(0.0, f64::max);
        assert!(d <= if cmyk { 6.5 } else { 1.0 }, "{} {va:?} InDesign {vb:?}", a["Name"]);
        worst = worst.max(d);
        changed += (va != values(&orig[k])) as usize;
    }
    eprintln!("{changed} swatches changed, worst {worst:.2} levels");
    assert!(changed >= 90, "only {changed} swatches changed");
    // locked swatches are untouched, and every part but the two is
    for locked in ["Color/Black", "Color/Registration"] {
        let (a, o) = (mine.values().find(|a| a["Self"] == locked).unwrap(), orig.values().find(|a| a["Self"] == locked).unwrap());
        assert_eq!(a["ColorValue"], o["ColorValue"]);
    }
    for (name, text) in &before {
        if name != "Resources/Graphic.xml" && name != "designmap.xml" {
            assert_eq!(ed.part(name), Some(text.as_str()), "{name} changed");
        }
    }

    // one undo step brings everything back byte for byte
    ed.undo().unwrap();
    assert_eq!(ed.parts(), &before);
    ed.redo().unwrap();
    assert_ne!(ed.parts(), &before);
}

#[test]
fn automatic_names_follow_the_values_and_references_follow_the_names() {
    if !installed() {
        return;
    }
    let before = fixture("convert-profile-before.idml");
    let mut ed = Editor::new(before.clone());
    ed.apply("Convert to Profile", &[convert(ADOBE_RGB, FOGRA39, "RelativeColorimetric", true).into()]).unwrap();
    let all = colors(ed.part("Resources/Graphic.xml").unwrap());
    let by_slot: BTreeMap<_, _> = all.iter().map(|a| (key(a), a)).collect();
    let auto = |a: &Attrs| {
        let r = values(a).iter().map(|v| v.round() as i64).collect::<Vec<_>>();
        match a["Space"].as_str() {
            "CMYK" => Some(format!("C={} M={} Y={} K={}", r[0], r[1], r[2], r[3])),
            "RGB" => Some(format!("R={} G={} B={}", r[0], r[1], r[2])),
            _ => None,
        }
    };
    // a swatch named by its values is renamed to the values it has now (a name that is taken gets " 2"); other names stay
    let mut renamed = 0;
    for o in colors(&before["Resources/Graphic.xml"]) {
        let now = by_slot[&key(&o)];
        if o["ColorEditable"] == "true" && auto(&o).as_deref() == Some(o["Name"].as_str()) && now["ColorValue"] != o["ColorValue"] {
            let wanted = auto(now).unwrap();
            assert!(now["Name"] == wanted || now["Name"].starts_with(&format!("{wanted} ")), "{} -> {} (values give {wanted})", o["Name"], now["Name"]);
            renamed += 1;
        } else {
            assert_eq!(now["Name"], o["Name"], "only automatic names change");
        }
    }
    assert!(renamed >= 4, "the fixture has automatically named CMYK swatches ({renamed})");
    // names are unique and every reference to a colour finds it
    let mut names: Vec<_> = all.iter().map(|a| a["Self"].clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), all.len());
    for (part, text) in ed.parts() {
        if part.starts_with("Spreads/") || part.starts_with("Stories/") || part == "Resources/Graphic.xml" {
            for attr in ["FillColor=\"", "StrokeColor=\"", "StopColor=\"", "BaseColor=\""] {
                for hit in text.match_indices(attr) {
                    let value = &text[hit.0 + attr.len()..];
                    let value = value[..value.find('"').unwrap()].replace("&amp;", "&");
                    if value.starts_with("Color/") {
                        assert!(names.contains(&value), "{part}: dangling {value}");
                    }
                }
            }
        }
    }
}

#[test]
fn nothing_changes_for_the_documents_own_profiles_and_a_bad_request_is_an_error() {
    if !installed() {
        return;
    }
    let before = fixture("convert-profile-before.idml");
    let mut ed = Editor::new(before.clone());
    ed.apply("same", &[convert("", "", "RelativeColorimetric", true).into()]).unwrap();
    ed.apply("same", &[convert("sRGB IEC61966-2.1", "U.S. Web Coated (SWOP) v2", "Perceptual", false).into()]).unwrap();
    assert_eq!(ed.parts(), &before);
    assert!(ed.apply("missing", &[convert("No Such Profile", "", "RelativeColorimetric", true).into()]).is_err());
    assert!(ed.apply("intent", &[convert(ADOBE_RGB, "", "Wrong", true).into()]).is_err());
    assert_eq!(ed.parts(), &before);
    // one space only: the CMYK swatches stay
    ed.apply("rgb only", &[convert(ADOBE_RGB, "", "RelativeColorimetric", true).into()]).unwrap();
    let (a, b) = (colors(ed.part("Resources/Graphic.xml").unwrap()), colors(&before["Resources/Graphic.xml"]));
    let r0 = |c: &[Attrs]| c.iter().find(|x| x["Self"] == "Color/R0").unwrap()["ColorValue"].clone();
    assert_ne!(r0(&a), r0(&b));
    for (x, y) in a.iter().zip(&b) {
        if y["Space"] == "CMYK" {
            assert_eq!(x["ColorValue"], y["ColorValue"], "{}", y["Self"]);
        }
    }
    assert_eq!(document(ed.part("designmap.xml").unwrap())["CMYKProfile"], "U.S. Web Coated (SWOP) v2");
}

#[test]
fn the_plan_says_what_the_command_would_do_without_doing_it() {
    if !installed() {
        return;
    }
    let before = fixture("convert-profile-before.idml");
    let ed = Editor::new(before.clone());
    let plan = ed.convert_to_profile_plan(ADOBE_RGB, FOGRA39, "RelativeColorimetric", true).unwrap();
    assert_eq!(ed.parts(), &before, "a plan changes nothing");
    assert_eq!((plan["source"]["rgb"].as_str(), plan["source"]["cmyk"].as_str()), (Some("sRGB IEC61966-2.1"), Some("U.S. Web Coated (SWOP) v2")));
    assert_eq!((plan["destination"]["rgb"].as_str(), plan["destination"]["cmyk"].as_str()), (Some(ADOBE_RGB), Some(FOGRA39)));
    let (rgb, cmyk, renamed) = (plan["count"]["rgb"].as_u64().unwrap(), plan["count"]["cmyk"].as_u64().unwrap(), plan["count"]["renamed"].as_u64().unwrap());
    assert!(rgb >= 40 && cmyk >= 40 && renamed >= 4, "{rgb} RGB, {cmyk} CMYK, {renamed} renamed");
    assert_eq!(plan["changed"].as_array().unwrap().len() as u64, rgb + cmyk);
    let red = plan["changed"].as_array().unwrap().iter().find(|c| c["name"] == "R0").unwrap();
    assert_eq!(red["space"], "RGB");
    assert_eq!(red["from"], serde_json::json!([255.0, 0.0, 0.0]));
    assert_eq!(red["to"], serde_json::json!([219.0, 0.0, 0.0]));
    // the same numbers as applying the command
    let mut applied = Editor::new(before);
    applied.apply("Convert to Profile", &[convert(ADOBE_RGB, FOGRA39, "RelativeColorimetric", true).into()]).unwrap();
    let after: BTreeMap<_, _> = colors(applied.part("Resources/Graphic.xml").unwrap()).into_iter().map(|a| (a["Name"].clone(), values(&a))).collect();
    for c in plan["changed"].as_array().unwrap() {
        let now = c["newName"].as_str().unwrap();
        if now == "$ID/" {
            continue; // the hidden unnamed colours share one name
        }
        let to: Vec<f64> = c["to"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(after[now], to, "{now}");
    }
    assert!(ed.convert_to_profile_plan("No Such Profile", "", "RelativeColorimetric", true).is_err());
}
