//! Failure atomicity and redundant-selection normalization for group/spread operations.
use folio_edit::{Anchor, Command, Editor, xml::Index};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, BTreeMap<String,String>) {
    let p=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip=zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts=BTreeMap::new(); let mut labels=BTreeMap::new();
    for k in 0..zip.len() {
        let mut e=zip.by_index(k).unwrap(); if e.is_dir() {continue;}
        let name=e.name().unwrap().into_owned();let mut t=String::new();e.read_to_string(&mut t).unwrap();
        if name.starts_with("Spreads/") {
            let ix=Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t,i)=="KeyValuePair" && ix.attr(&t,i,"Key")==Some("oracle.id") {
                    let owner=ix.ancestors(i).find(|&a|ix.attr(&t,a,"Self").is_some()).unwrap();
                    labels.insert(ix.value(&t,i,"Value").unwrap(),ix.attr(&t,owner,"Self").unwrap().into());
                }
            }
        }
        parts.insert(name,t);
    }
    (Editor::new(parts),labels)
}
fn resize(ids:Vec<String>)->Command {Command::ResizeMany{ids,size:[350.0,240.0],anchor:Anchor::Center,include_stroke:true}}

#[test]
fn selected_group_child_and_duplicate_ids_are_not_resized_twice() {
    let (mut a,l)=fixture();let mut b=Editor::new(a.parts().clone());
    a.apply("resize",&[resize(vec![l["grp"].clone(),l["r-plain"].clone()])]).unwrap();
    b.apply("resize",&[resize(vec![l["grp-a"].clone(),l["grp"].clone(),l["r-plain"].clone(),l["grp"].clone()])]).unwrap();
    assert_eq!(a.parts(),b.parts());
}
#[test]
fn invalid_group_size_rolls_back_an_earlier_move() {
    let (mut a,l)=fixture();let before=a.parts().clone();
    let result=a.apply("batch",&[
        Command::Move{ids:vec![l["r-plain"].clone()],by:[20.0,0.0]},
        Command::Resize{id:l["grp"].clone(),size:[-1.0,50.0],anchor:Anchor::Center,include_stroke:true},
    ]);
    assert!(result.is_err());assert_eq!(a.parts(),&before);assert_eq!(a.rev(),0);
}
#[test]
fn invalid_destination_restores_both_spread_parts() {
    let (mut a,l)=fixture();let before=a.parts().clone();
    let result=a.apply("batch",&[
        Command::MoveToSpread{ids:vec![l["tf"].clone()],spread:"uf3".into(),by:[600.0,20.0]},
        Command::MoveToSpread{ids:vec![l["r-fill"].clone()],spread:"missing".into(),by:[0.0,0.0]},
    ]);
    assert!(result.is_err());assert_eq!(a.parts(),&before);assert!(a.recompose().is_empty());
}
#[test]
fn spread_move_keeps_all_selected_members_and_normalizes_group_children() {
    let (mut a,l)=fixture();let mut b=Editor::new(a.parts().clone());
    let ids=vec![l["r-plain"].clone(),l["grp"].clone()];
    a.apply("move",&[Command::MoveToSpread{ids,spread:"u10c".into(),by:[-300.0,20.0]}]).unwrap();
    let ids=vec![l["grp-a"].clone(),l["grp"].clone(),l["r-plain"].clone(),l["grp"].clone()];
    b.apply("move",&[Command::MoveToSpread{ids,spread:"u10c".into(),by:[-300.0,20.0]}]).unwrap();
    assert_eq!(a.parts(),b.parts());
}

#[test]
fn rotating_about_the_middle_of_the_bounds_is_the_same_as_rotating_about_the_centre() {
    let (mut a, l) = fixture();
    let id = l["r-plain"].clone();
    let mut b = Editor::new(a.parts().clone());
    let mut c = Editor::new(a.parts().clone());
    let bounds = a.bounds(&id).unwrap();
    let centre = [(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0];
    a.apply("anchor", &[Command::Rotate { ids: vec![id.clone()], by: 35.0, anchor: Anchor::Center }]).unwrap();
    b.apply("point", &[Command::RotateAbout { ids: vec![id.clone()], by: 35.0, point: centre }]).unwrap();
    let (ta, tb) = (a.bounds(&id).unwrap(), b.bounds(&id).unwrap());
    assert!(ta.iter().zip(&tb).all(|(x, y)| (x - y).abs() < 1e-9), "{ta:?} {tb:?}");
    // about another point: a quarter turn counter-clockwise about the top left corner puts the box above it, from x to x + its height
    c.apply("far", &[Command::RotateAbout { ids: vec![id.clone()], by: 90.0, point: [bounds[0], bounds[1]] }]).unwrap();
    let tc = c.bounds(&id).unwrap();
    let (w, h) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
    assert!((tc[0] - bounds[0]).abs() < 1e-6 && (tc[2] - (bounds[0] + h)).abs() < 1e-6, "{tc:?}");
    assert!((tc[3] - bounds[1]).abs() < 1e-6 && (tc[1] - (bounds[1] - w)).abs() < 1e-6, "{tc:?}");
}

#[test]
fn compound_paths_refuse_what_cannot_join_and_undo_takes_everything_back() {
    let (mut a, l) = fixture();
    let before = a.parts().clone();
    let one = a.apply("one", &[Command::MakeCompoundPath { ids: vec![l["r-plain"].clone()] }]).unwrap_err().to_string();
    assert!(one.contains("at least two"), "{one}");
    // a group's member and a group
    let member = a.apply("member", &[Command::MakeCompoundPath { ids: vec![l["r-plain"].clone(), l["grp-a"].clone()] }]).unwrap_err().to_string();
    assert!(member.contains("inside a group") || member.contains("Group"), "{member}");
    let group = a.apply("group", &[Command::MakeCompoundPath { ids: vec![l["r-plain"].clone(), l["grp"].clone()] }]).unwrap_err().to_string();
    assert!(group.contains("only rectangles, ovals and polygons"), "{group}");
    // a plain shape has nothing to release
    let nothing = a.apply("release", &[Command::ReleaseCompoundPath { id: l["r-plain"].clone() }]).unwrap_err().to_string();
    assert!(nothing.contains("not a compound path"), "{nothing}");
    assert_eq!(a.parts(), &before, "a refused command changes nothing");
    // make, release, undo twice
    a.apply("make", &[Command::MakeCompoundPath { ids: vec![l["oval"].clone(), l["r-plain"].clone(), l["poly"].clone()] }]).unwrap();
    a.apply("release", &[Command::ReleaseCompoundPath { id: l["r-plain"].clone() }]).unwrap();
    a.undo().unwrap();
    a.undo().unwrap();
    assert_eq!(a.parts(), &before);
}
