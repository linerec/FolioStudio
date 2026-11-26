//! Layer restoration when ungrouping page items.

    use super::*;
    use crate::Editor;
    use std::collections::BTreeMap;

    fn editor() -> Editor {
        let dm = r#"<Document Self="d" ActiveLayer="l2"><Layer Self="l1" Name="Base"/><Layer Self="l2" Name="Top"/></Document>"#;
        let spread = r#"<idPkg:Spread><Spread Self="s">
<Page Self="p" Name="1" GeometricBounds="0 0 100 100" ItemTransform="1 0 0 1 0 0"/>
<Rectangle Self="a" ItemTransform="1 0 0 1 0 0" ItemLayer="l1" Locked="false"/>
<Rectangle Self="b" ItemTransform="1 0 0 1 0 0" ItemLayer="l1" Locked="false"/>
<Rectangle Self="c" ItemTransform="1 0 0 1 0 0" ItemLayer="l2" Locked="false"/>
</Spread></idPkg:Spread>"#;
        Editor::new(BTreeMap::from([("designmap.xml".to_string(), dm.to_string()), ("Spreads/Spread_s.xml".to_string(), spread.to_string())]))
    }

    /// (Self, layer) of the spread's items in order.
    fn items(e: &Editor) -> Vec<(String, String)> {
        let t = e.parts()["Spreads/Spread_s.xml"].clone();
        let mut out = Vec::new();
        for tag in t.split('<').filter(|s| s.starts_with("Rectangle")) {
            let get = |k: &str| tag.split(&format!("{k}=\"")).nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_owned();
            out.push((get("Self"), get("ItemLayer")));
        }
        out
    }

    fn group_ac(e: &mut Editor) -> String {
        e.apply("group", &[Command::Group { ids: vec!["a".into(), "c".into()] }]).unwrap().created[0].clone()
    }

    /// basic-layers3 ungroup-mixed-remembers: the member of the lower layer goes back to where it was.
    #[test]
    fn ungroup_puts_members_back_on_their_layers_and_places() {
        let mut e = editor();
        let g = group_ac(&mut e);
        assert!(e.parts()["Spreads/Spread_s.xml"].contains(MEMBER_LAYERS), "the group carries the memory");
        e.apply("ungroup", &[Command::Ungroup { id: g, remember_layers: None }]).unwrap();
        let got = items(&e);
        assert_eq!(got, vec![("a".into(), "l1".into()), ("b".into(), "l1".into()), ("c".into(), "l2".into())]);
        assert!(!e.parts()["Spreads/Spread_s.xml"].contains(MEMBER_LAYERS));
    }

    /// Ungroup Remembers Layers off: every member goes to the group's layer (the frontmost member's).
    #[test]
    fn ungroup_without_memory_uses_the_groups_layer() {
        let mut e = editor();
        let g = group_ac(&mut e);
        e.apply("ungroup", &[Command::Ungroup { id: g, remember_layers: Some(false) }]).unwrap();
        let layers: Vec<String> = items(&e).into_iter().map(|(_, l)| l).collect();
        assert_eq!(items(&e).iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>(), vec!["b", "a", "c"]);
        assert_eq!(layers, vec!["l1", "l2", "l2"]);
    }

    /// Paste Remembers Layers: off, the copy goes on the active layer; on, on the layer it came from.
    #[test]
    fn paste_remembers_the_layer_it_was_copied_from() {
        let mut e = editor();
        e.copy(&["c".into()]).unwrap();
        e.apply("active", &[Command::ActiveLayer { id: "l1".into() }]).unwrap();
        let paste = |remember_layers| Command::Paste { page: Some("p".into()), at: None, layer: None, into: None, remember_layers };
        let layer_of = |e: &Editor, id: &str| items(e).into_iter().find(|(i, _)| i == id).unwrap().1;
        let off = e.apply("paste", &[paste(false)]).unwrap().created[0].clone();
        assert_eq!(layer_of(&e, &off), "l1");
        let on = e.apply("paste", &[paste(true)]).unwrap().created[0].clone();
        assert_eq!(layer_of(&e, &on), "l2");
        assert_eq!(items(&e).len(), 5);
    }

    /// A group moved to another layer since forgets what its members had.
    #[test]
    fn a_group_moved_to_another_layer_forgets() {
        let mut e = editor();
        let g = group_ac(&mut e);
        e.apply("move", &[Command::ToLayer { ids: vec![g.clone()], layer: "l1".into() }]).unwrap();
        e.apply("ungroup", &[Command::Ungroup { id: g, remember_layers: None }]).unwrap();
        assert!(items(&e).iter().all(|(_, l)| l == "l1"));
    }
