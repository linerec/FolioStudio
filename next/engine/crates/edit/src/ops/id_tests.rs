//! Page item ID allocation.

use super::*;
use crate::Editor;
use std::collections::BTreeMap;

#[test]
fn ids_are_never_given_twice() {
    let spread = r#"<idPkg:Spread><Spread Self="s"><Rectangle Self="u10" ItemTransform="1 0 0 1 0 0" ItemLayer="l"/><Rectangle Self="u11" ItemTransform="1 0 0 1 0 0" ItemLayer="l"/></Spread></idPkg:Spread>"#;
    let mut e = Editor::new(BTreeMap::from([("Spreads/Spread_s.xml".to_string(), spread.to_string())]));
    let g = e.apply("group", &[Command::Group { ids: vec!["u10".into(), "u11".into()] }]).unwrap().created;
    assert_eq!(g, vec!["u12".to_string()]);
    e.apply("ungroup", &[Command::Ungroup { id: "u12".into(), remember_layers: None }]).unwrap();
    // u12 is gone from the parts, but stays used
    let d = e.apply("copy", &[Command::Duplicate { ids: vec!["u10".into()], by: [1.0, 0.0] }]).unwrap().created;
    assert_eq!(d, vec!["u13".to_string()]);
}
