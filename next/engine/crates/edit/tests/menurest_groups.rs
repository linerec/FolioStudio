//! Duplicate Style Group (five style kinds), and Style Group Options… / Delete Group… of an object style group, against what InDesign 2025
//! measured (oracle/measured/menurest-groups-2025/, made by artifacts/menurest-c1-20261023/make_measured.py from DOM probes and captured
//! scenarios). Each case starts from the `before` parts InDesign wrote, runs the commands and compares the outline of Resources/Styles.xml
//! (every group and style in order: depth, tag, Self, Name, NextStyle, BasedOn) and the Applied*Style attributes of the other parts.
use folio_edit::xml::{Index, unescape};
use folio_edit::{Command, Editor};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn dir() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/measured/menurest-groups-2025")
}

fn is_style(tag: &str) -> bool {
    ["Paragraph", "Character", "Object", "Table", "Cell"].iter().any(|k| tag == format!("{k}Style") || tag == format!("{k}StyleGroup"))
}

/// depth|tag|Self|Name|NextStyle|BasedOn of every style group and style, in document order (the root groups with no Self).
fn outline(t: &str) -> Vec<String> {
    let ix = Index::scan(t).unwrap();
    let mut out = Vec::new();
    let depth = |i: usize| ix.ancestors(i).filter(|&a| { let l = ix.local(t, a); is_style(l) || (l.starts_with("Root") && l.ends_with("StyleGroup")) }).count();
    for i in 0..ix.nodes.len() {
        let tag = ix.local(t, i);
        if tag.starts_with("Root") && tag.ends_with("StyleGroup") {
            out.push(format!("{}|{tag}||||", depth(i)));
        } else if is_style(tag) {
            let based = ix.children(i).iter().find(|&&c| ix.local(t, c) == "Properties")
                .and_then(|&p| ix.children(p).iter().find(|&&c| ix.local(t, c) == "BasedOn").map(|&b| unescape(&t[ix.nodes[b].open_end..ix.nodes[b].close_start])))
                .unwrap_or_default();
            let get = |k: &str| ix.attr(t, i, k).map(unescape).unwrap_or_default();
            out.push(format!("{}|{tag}|{}|{}|{}|{based}", depth(i), get("Self"), get("Name"), get("NextStyle")));
        }
    }
    out
}

fn applied(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = t;
    // in document order, whichever of the three names comes first
    loop {
        let found = [" AppliedObjectStyle=\"", " AppliedGraphicObjectStyle=\"", " AppliedTextObjectStyle=\""].iter()
            .filter_map(|k| rest.find(k).map(|at| (at, k.len()))).min();
        let Some((at, len)) = found else { break };
        let value = &rest[at + len..];
        out.push(unescape(value.split('"').next().unwrap()));
        rest = &rest[at + len..];
    }
    out
}

#[test]
fn style_group_duplicate_rename_and_delete_match_indesign() {
    let root = dir();
    let all: Value = serde_json::from_str(&std::fs::read_to_string(root.join("cases.json")).unwrap()).unwrap();
    let mut failed = Vec::new();
    let cases = all["cases"].as_array().unwrap();
    assert!(cases.len() > 20);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let parts: BTreeMap<String, String> = case["before"].as_object().unwrap().iter()
            .map(|(part, file)| (part.clone(), std::fs::read_to_string(root.join(file.as_str().unwrap())).unwrap())).collect();
        let mut ed = Editor::new(parts);
        let mut result = Ok(());
        for step in case["commands"].as_array().unwrap() {
            let command = match folio_edit::commands::oracle::command(step, None) {
                Ok(Some(c)) => c,
                _ => match serde_json::from_value::<folio_edit::ObjectStyleDefinition>(step.clone()) {
                    Ok(d) => Command::ObjectStyleDefinition(d),
                    Err(_) => serde_json::from_value::<Command>(step.clone()).unwrap_or_else(|e| panic!("{name}: {step}: {e}")),
                },
            };
            if let Err(e) = ed.apply("step", &[command]) {
                result = Err(format!("{step}: {} {}", e.code, e.message));
                break;
            }
        }
        if let Err(e) = result {
            failed.push(format!("{name}: {e}"));
            continue;
        }
        let want: Vec<String> = case["outline"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect();
        let got = outline(ed.part("Resources/Styles.xml").unwrap());
        if got != want {
            let first = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
            failed.push(format!("{name}: Styles.xml differs at line {first}: engine {:?} / InDesign {:?} ({} vs {} lines)", got.get(first), want.get(first), got.len(), want.len()));
            continue;
        }
        for (part, list) in case["applied"].as_object().unwrap() {
            let want: Vec<String> = list.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect();
            let got = applied(ed.part(part).unwrap());
            if got != want {
                let first = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(0);
                failed.push(format!("{name}: {part}: Applied*Style differs at {first}: engine {:?} / InDesign {:?}", got.get(first), want.get(first)));
            }
        }
    }
    assert!(failed.is_empty(), "{} of {} cases differ from InDesign:\n{}", failed.len(), cases.len(), failed.join("\n"));
}
