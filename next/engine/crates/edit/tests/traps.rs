//! Trap presets: definitions and page assignments change only the IDML Styles.xml and Page attributes;
//! failed commands change nothing; undo and redo give the bytes back; the model reads the result back.
use folio_edit::{Editor, commands::RegisteredCommand};
use folio_model::traps::{Preset, DEFAULT_PRESET, NO_TRAP_PRESET};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, std::path::PathBuf) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/pages.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for n in 0..z.len() {
        let mut entry = z.by_index(n).unwrap();
        if entry.is_dir() { continue; }
        let mut t = String::new();
        entry.read_to_string(&mut t).unwrap();
        parts.insert(entry.name().unwrap().into_owned(), t);
    }
    (Editor::new(parts), path)
}

fn settings(ed: &Editor, path: &Path) -> folio_model::traps::Settings {
    folio_idml::read_parts(ed.parts(), path, &folio_idml::ReaderData::default()).unwrap().traps
}

fn save(preset: Preset) -> folio_edit::Command { RegisteredCommand::SaveTrapPreset { preset }.into() }

fn named(name: &str) -> Preset { Preset { name: name.into(), ..Preset::default() } }

#[test]
fn create_edit_rename_duplicate_delete_assign_and_undo() {
    let (mut ed, path) = fixture();
    let original = ed.parts().clone();
    let pages: Vec<String> = settings(&ed, &path).assignments.iter().map(|a| a.page.clone()).collect();
    assert!(pages.len() >= 3, "the pages fixture has document pages");
    assert!(settings(&ed, &path).assignments.iter().all(|a| a.preset == DEFAULT_PRESET));

    // create: only the name and one width given, the rest are InDesign's defaults; the new id comes back
    let out = ed.apply("new", &[save(Preset { default_width: Some(0.4), ..named("Magazine") })]).unwrap();
    assert_eq!(out.created, vec!["TrapPreset/Magazine".to_string()]);
    let made = settings(&ed, &path).presets.into_iter().find(|p| p.name == "Magazine").unwrap();
    assert_eq!((made.default_width, made.black_width, made.join.as_str(), made.image_placement.as_str()), (Some(0.4), Some(0.5), "MiterEndJoin", "CenterEdges"));
    assert_eq!((made.step_threshold, made.black_color_threshold, made.black_density, made.sliding_threshold, made.color_reduction),
        (Some(10.0), Some(100.0), Some(1.6), Some(70.0), Some(100.0)));
    let created = ed.parts().clone();

    // a second preset with the same name, an empty name and values outside the range change nothing
    for bad in [named("Magazine"), named("  "), Preset { default_width: Some(-1.0), ..named("Bad") }, Preset { join: "No pe".into(), ..named("Bad") }] {
        assert!(ed.apply("bad", &[save(bad)]).is_err());
        assert_eq!(ed.parts(), &created);
    }

    // edit by id: unnamed fields keep their values
    ed.apply("edit", &[save(Preset { id: "TrapPreset/Magazine".into(), black_width: Some(0.75), join: "RoundEndJoin".into(), internal_images: Some(true), ..Preset::default() })]).unwrap();
    let edited = settings(&ed, &path).presets.into_iter().find(|p| p.name == "Magazine").unwrap();
    assert_eq!((edited.default_width, edited.black_width, edited.join.as_str(), edited.internal_images), (Some(0.4), Some(0.75), "RoundEndJoin", Some(true)));

    // assign two pages; a parent page or an unknown id changes nothing
    ed.apply("assign", &[RegisteredCommand::AssignTrapPreset { pages: pages[..2].to_vec(), preset: "TrapPreset/Magazine".into() }.into()]).unwrap();
    let state = settings(&ed, &path);
    assert_eq!(state.assignments.iter().filter(|a| a.preset == "TrapPreset/Magazine").map(|a| a.page.clone()).collect::<Vec<_>>(), pages[..2].to_vec());
    let assigned = ed.parts().clone();
    for bad in [RegisteredCommand::AssignTrapPreset { pages: vec![pages[2].clone(), "nope".into()], preset: "TrapPreset/Magazine".into() },
                RegisteredCommand::AssignTrapPreset { pages: pages.clone(), preset: "TrapPreset/Missing".into() },
                RegisteredCommand::AssignTrapPreset { pages: vec![], preset: "TrapPreset/Magazine".into() }] {
        assert!(ed.apply("bad", &[bad.into()]).is_err());
        assert_eq!(ed.parts(), &assigned);
    }

    // rename moves the pages along
    ed.apply("rename", &[save(Preset { id: "TrapPreset/Magazine".into(), name: "Cover".into(), ..Preset::default() })]).unwrap();
    let state = settings(&ed, &path);
    assert!(state.presets.iter().any(|p| p.id == "TrapPreset/Cover" && p.name == "Cover"));
    assert_eq!(state.assignments.iter().filter(|a| a.preset == "TrapPreset/Cover").count(), 2);
    assert!(!ed.part("Resources/Styles.xml").unwrap().contains("TrapPreset/Magazine"));

    // duplicate: the copy has the same values
    ed.apply("copy", &[RegisteredCommand::DuplicateTrapPreset { id: "TrapPreset/Cover".into(), name: "Cover B".into() }.into()]).unwrap();
    let state = settings(&ed, &path);
    let (a, b) = (state.presets.iter().find(|p| p.name == "Cover").unwrap(), state.presets.iter().find(|p| p.name == "Cover B").unwrap());
    assert_eq!((a.black_width, &a.join, a.internal_images), (b.black_width, &b.join, b.internal_images));
    assert!(ed.apply("same", &[RegisteredCommand::DuplicateTrapPreset { id: "TrapPreset/Cover".into(), name: "Cover B".into() }.into()]).is_err());

    // delete needs a replacement; the built-ins stay; the pages take the replacement
    let before_delete = ed.parts().clone();
    for bad in [("TrapPreset/Cover", ""), ("TrapPreset/Cover", "TrapPreset/Cover"), ("TrapPreset/Cover", "TrapPreset/Missing"),
                (DEFAULT_PRESET, NO_TRAP_PRESET), (NO_TRAP_PRESET, DEFAULT_PRESET)] {
        assert!(ed.apply("bad", &[RegisteredCommand::DeleteTrapPreset { id: bad.0.into(), replacement: bad.1.into() }.into()]).is_err(), "{bad:?}");
        assert_eq!(ed.parts(), &before_delete);
    }
    ed.apply("delete", &[RegisteredCommand::DeleteTrapPreset { id: "TrapPreset/Cover".into(), replacement: DEFAULT_PRESET.into() }.into()]).unwrap();
    let state = settings(&ed, &path);
    assert!(state.presets.iter().all(|p| p.name != "Cover"));
    assert!(state.assignments.iter().all(|a| a.preset == DEFAULT_PRESET));

    // undo and redo of every step give the bytes back
    let end = ed.parts().clone();
    let mut steps = 0;
    while ed.undo().is_some() { steps += 1; }
    assert_eq!(steps, 6);
    assert_eq!(ed.parts(), &original);
    for _ in 0..steps { ed.redo().unwrap(); }
    assert_eq!(ed.parts(), &end);
}

/// What a script or an LLM sends: a partial preset as JSON, and the structured answer (the made id).
#[test]
fn commands_read_from_json_and_report_the_made_id() {
    let (mut ed, path) = fixture();
    let wire = serde_json::json!({"op": "saveTrapPreset", "preset": {"name": "Wire", "black_width": 0.9}});
    let command: RegisteredCommand = serde_json::from_value(wire).unwrap();
    let out = ed.apply("wire", &[command.into()]).unwrap();
    assert_eq!(out.created, vec!["TrapPreset/Wire".to_string()]);
    let made = settings(&ed, &path).presets.into_iter().find(|p| p.name == "Wire").unwrap();
    assert_eq!((made.default_width, made.black_width), (Some(0.25), Some(0.9)));
    assert!(RegisteredCommand::recognizes("assignTrapPreset") && RegisteredCommand::has_field("deleteTrapPreset", "replacement"));
}

/// The limits InDesign enforces (measured with trapx-rules-probe): a value outside is refused and nothing changes.
#[test]
fn measured_limits_are_enforced_and_copies_are_named_like_indesign() {
    let (mut ed, _path) = fixture();
    let before = ed.parts().clone();
    for bad in [Preset { default_width: Some(8.001), ..named("L") }, Preset { black_width: Some(-0.001), ..named("L") }, Preset { step_threshold: Some(0.0), ..named("L") },
                Preset { black_density: Some(0.0), ..named("L") }, Preset { black_density: Some(10.5), ..named("L") }, Preset { color_reduction: Some(100.001), ..named("L") }] {
        assert!(ed.apply("bad", &[save(bad)]).is_err());
        assert_eq!(ed.parts(), &before);
    }
    ed.apply("edge", &[save(Preset { default_width: Some(8.0), black_width: Some(0.0), step_threshold: Some(1.0), black_density: Some(10.0), sliding_threshold: Some(0.0), ..named("Edge") })]).unwrap();
    for (id, want) in [("TrapPreset/Edge", "Edge Copy 1"), ("TrapPreset/Edge", "Edge Copy 2")] {
        let out = ed.apply("copy", &[RegisteredCommand::DuplicateTrapPreset { id: id.into(), name: String::new() }.into()]).unwrap();
        assert_eq!(out.created, vec![format!("TrapPreset/{want}")]);
    }
    let out = ed.apply("copy of a copy", &[RegisteredCommand::DuplicateTrapPreset { id: "TrapPreset/Edge Copy 1".into(), name: String::new() }.into()]).unwrap();
    assert_eq!(out.created, vec!["TrapPreset/Edge Copy 3".to_string()]);
    let out = ed.apply("copy default", &[RegisteredCommand::DuplicateTrapPreset { id: NO_TRAP_PRESET.into(), name: String::new() }.into()]).unwrap();
    assert_eq!(out.created, vec!["TrapPreset/[No trap preset] Copy 1".to_string()]);
}

#[test]
fn built_ins_take_values_but_keep_names_and_parent_pages_take_presets() {
    // InDesign (trapx-builtin-probe, oracle trapx-edit-values/-duplicate/-enum-words): the two built-in presets can be edited and
    // copied, not renamed or deleted; parent pages take a preset like document pages; the IDML words are the measured ones.
    let (mut ed, path) = fixture();
    ed.apply("edit built-in", &[save(Preset { id: DEFAULT_PRESET.into(), default_width: Some(0.9), black_width: Some(1.1), ..Preset::default() })]).unwrap();
    let default = settings(&ed, &path).presets.into_iter().find(|p| p.id == DEFAULT_PRESET).unwrap();
    assert_eq!((default.default_width, default.black_width), (Some(0.9), Some(1.1)));
    let before = ed.parts().clone();
    assert!(ed.apply("rename built-in", &[save(Preset { id: NO_TRAP_PRESET.into(), name: "Mine".into(), ..Preset::default() })]).is_err());
    for id in [DEFAULT_PRESET, NO_TRAP_PRESET] {
        let other = if id == DEFAULT_PRESET { NO_TRAP_PRESET } else { DEFAULT_PRESET };
        assert!(ed.apply("delete built-in", &[RegisteredCommand::DeleteTrapPreset { id: id.into(), replacement: other.into() }.into()]).is_err());
    }
    assert_eq!(ed.parts(), &before);

    for (id, copy) in [(DEFAULT_PRESET, "[Default] Copy 1"), (NO_TRAP_PRESET, "[No trap preset] Copy 1")] {
        ed.apply("copy", &[RegisteredCommand::DuplicateTrapPreset { id: id.into(), name: String::new() }.into()]).unwrap();
        assert!(settings(&ed, &path).presets.iter().any(|p| p.id == format!("TrapPreset/{copy}")), "{copy}");
    }

    // measured IDML words are accepted, a scripting name or a guess is not
    {
        let ok = Preset { join: "BevelEndJoin".into(), end: "OverlapTrapEnds".into(), image_placement: "ImageNeutralDensity".into(), ..named("Words") };
        ed.apply("words", &[save(ok)]).unwrap();
    }
    for bad in [Preset { join: "ROUND_END_JOIN".into(), ..named("Bad1") }, Preset { end: "OverlappedTrapEnds".into(), ..named("Bad2") }, Preset { image_placement: "ChokeEdges".into(), ..named("Bad3") }] {
        assert!(ed.apply("bad word", &[save(bad)]).is_err());
    }

    // a parent page takes a preset (InDesign accepts it); the document page list of the model stays document pages only
    let master = ed.parts().iter().find(|(name, _)| name.starts_with("MasterSpreads/")).map(|(name, text)| {
        let at = text.find("<Page Self=\"").unwrap() + "<Page Self=\"".len();
        (name.clone(), text[at..at + text[at..].find('"').unwrap()].to_string())
    }).unwrap();
    ed.apply("assign parent", &[RegisteredCommand::AssignTrapPreset { pages: vec![master.1.clone()], preset: NO_TRAP_PRESET.into() }.into()]).unwrap();
    let part = ed.part(&master.0).unwrap();
    let at = part.find(&format!("<Page Self=\"{}\"", master.1)).unwrap();
    let tag = &part[at..at + part[at..].find('>').unwrap()];
    assert!(tag.contains(&format!("AppliedTrapPreset=\"{NO_TRAP_PRESET}\"")), "{tag}");
}
