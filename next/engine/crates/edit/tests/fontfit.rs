//! File ▸ Adjust Layout with "Adjust font size" (layoutadj-c7; rules `next/oracle/rules/layoutadj-c7.ko.md`): the sizes of the text of the stretched frames follow the
//! frame by the scale the search finds, from the facts the host measured (here written by hand: the editor knows no composition).
use folio_edit::{Command, Editor};
use folio_edit::commands::RegisteredCommand;
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> Editor {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/layoutadj7f.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(name, t);
    }
    Editor::new(parts)
}

fn adjust(metrics: Option<String>, on: bool) -> Command {
    Command::Registered(RegisteredCommand::AdjustLayout {
        width: Some(450.0),
        height: Some(500.0),
        allow_locked: None,
        top_margin: None,
        bottom_margin: None,
        left_margin: None,
        right_margin: None,
        auto_adjust_margins: None,
        adjust_font_size: Some(on),
        font_size_min: None,
        font_size_max: None,
        fit_metrics: metrics,
    })
}

/// Facts for every text frame of the fixture: the ones that hold no text have nothing to adjust, `frame` has `fit` (one paragraph of two lines in 12 pt text).
fn metrics(e: &Editor, frame: &str, fit: &str) -> String {
    let mut body = Vec::new();
    for t in e.parts().values() {
        for (i, m) in t.match_indices("<TextFrame Self=\"") {
            let id = t[i + m.len()..].split('"').next().unwrap();
            let v = if id == frame { fit.to_owned() } else { r#"{"Ok":{"story":"","width":228.0,"size":0.0,"leading":null,"e":0.0,"paragraphs":[]}}"#.to_owned() };
            body.push(format!("\"{id}\":{v}"));
        }
    }
    format!("{{\"frames\":{{{}}}}}", body.join(","))
}

fn sizes(e: &Editor) -> Vec<String> {
    let t = &e.parts()["Stories/Story_u16e.xml"];
    t.match_indices("PointSize=\"").map(|(i, m)| t[i + m.len()..].split('"').next().unwrap().to_owned()).collect()
}

#[test]
fn the_text_of_a_stretched_frame_takes_the_scale_the_search_finds() {
    let mut e = fixture();
    assert!(sizes(&e).is_empty(), "the text takes its size from its style");
    let fit = r#"{"Ok":{"story":"u16e","width":240.0,"size":12.0,"leading":18.0,"e":2.8,"paragraphs":[[2,100.0]]}}"#;
    e.apply("adjust", &[adjust(Some(metrics(&e, "u180", fit)), true)]).unwrap();
    let got = sizes(&e);
    assert!(!got.is_empty(), "the story has a local size now");
    let size: f64 = got[0].parse().unwrap();
    // the frame grows 1.5 across and about 1.25 down: the text is between the two scales, a tenth of a point at most
    assert!(size > 12.0 * 1.2 && size < 12.0 * 1.6, "{size}");
    assert!((size * 10.0 - (size * 10.0).round()).abs() < 1e-9, "a tenth of a point: {size}");
    // the fixed leading follows by the same scale (floored to a tenth)
    let t = &e.parts()["Stories/Story_u16e.xml"];
    assert!(t.contains("Leading"), "{t}");
    // one history step, undone with the page size
    e.undo().unwrap();
    assert!(sizes(&e).is_empty());
}

#[test]
fn the_sizes_stay_without_the_option_and_a_frame_that_cannot_be_adjusted_refuses_everything() {
    let mut e = fixture();
    let fit = r#"{"Ok":{"story":"u16e","width":240.0,"size":12.0,"leading":null,"e":2.8,"paragraphs":[[2,100.0]]}}"#;
    e.apply("off", &[adjust(Some(metrics(&e, "u180", fit)), false)]).unwrap();
    assert!(sizes(&e).is_empty());
    // no facts: refused
    let mut e = fixture();
    assert!(e.apply("no facts", &[adjust(None, true)]).is_err());
    // one frame the host could not describe: nothing changes, and the reason is told
    let mut e = fixture();
    let before = e.parts().clone();
    let why = r#"{"Err":"a text frame of several columns is not supported (one column)"}"#;
    let err = e.apply("refused", &[adjust(Some(metrics(&e, "u180", why)), true)]).unwrap_err();
    assert!(err.to_string().contains("several columns"), "{err}");
    assert_eq!(e.parts(), &before, "nothing changed");
}
