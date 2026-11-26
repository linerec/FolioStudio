use folio_edit::{Command, Editor, commands::RegisteredCommand, xml::Index};
use folio_model::{ItemKind, PageItem};
use std::{collections::BTreeMap, io::Read, path::Path};

fn fixture() -> (Editor, BTreeMap<String, String>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    let mut labels = BTreeMap::new();
    for k in 0..zip.len() {
        let mut e = zip.by_index(k).unwrap();
        if e.is_dir() {
            continue;
        }
        let name = e.name().unwrap().into_owned();
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        if name.contains("Spread") {
            let ix = Index::scan(&t).unwrap();
            for i in 0..ix.nodes.len() {
                if ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some("oracle.id") {
                    let owner = ix.ancestors(i).find_map(|a| ix.attr(&t, a, "Self")).unwrap();
                    labels.insert(ix.attr(&t, i, "Value").unwrap().to_owned(), owner.to_owned());
                }
            }
        }
        parts.insert(name, t);
    }
    (Editor::new(parts), labels)
}

fn find<'a>(items: &'a [PageItem], id: &str) -> Option<&'a PageItem> {
    for it in items {
        if it.id == id {
            return Some(it);
        }
        if let ItemKind::Group { children } = &it.kind
            && let Some(f) = find(children, id)
        {
            return Some(f);
        }
    }
    None
}


fn value(e: &Editor, id: &str) -> folio_model::image_color::ImageColor {
 let doc=folio_idml::read_parts(e.parts(),Path::new("."),&folio_idml::ReaderData::default()).unwrap();
 let it=doc.spreads.iter().find_map(|s|find(&s.items,id)).unwrap();
 let ItemKind::Rectangle{content:Some(g)}=&it.kind else {panic!("image frame")};
 g.image_color.clone().unwrap()
}
#[test]
fn settings_roundtrip_undo_and_rejection_are_atomic() {
 let (mut e,l)=fixture(); let before=e.parts().clone(); let original=value(&e,&l["img"]);
 let command=|intent:&str| -> Command { RegisteredCommand::SetImageColor{ids:vec![l["img"].clone()],profile:"sRGB IEC61966-2.1".into(),intent:intent.into()}.into() };
 for intent in folio_model::image_color::INTENTS {
  e.apply("color",&[command(intent)]).unwrap();
  assert_eq!(value(&e,&l["img"]),folio_model::image_color::ImageColor{profile:"sRGB IEC61966-2.1".into(),intent:intent.to_string()});
  e.undo().unwrap(); assert_eq!(e.parts(),&before); assert_eq!(value(&e,&l["img"]),original);
 }
 assert!(e.apply("bad",&[command("Unknown")]).is_err()); assert_eq!(e.parts(),&before);
 let bad:Command=RegisteredCommand::SetImageColor{ids:vec![l["img"].clone(),l["oval"].clone()],profile:"sRGB IEC61966-2.1".into(),intent:"Saturation".into()}.into();
 assert!(e.apply("atomic",&[bad]).is_err()); assert_eq!(e.parts(),&before);
}

#[test]
fn self_closing_color_elements_keep_unknown_attributes() {
    for properties in [
        "<Properties extra=\"keep\"/>",
        "<Properties><Profile type=\"string\" extra=\"keep\"/></Properties>",
    ] {
        let (e, labels) = fixture();
        let mut parts = e.parts().clone();
        for text in parts.values_mut() {
            let ix = Index::scan(text).unwrap();
            let Some(image) = (0..ix.nodes.len()).find(|&i| ix.local(text, i) == "Image") else { continue };
            let p = ix.path(text, image, &["Properties"]).unwrap();
            let range = ix.nodes[p].start..ix.nodes[p].end;
            text.replace_range(range, properties);
        }
        let before = parts.clone();
        let mut e = Editor::new(parts);
        let command: Command = RegisteredCommand::SetImageColor {
            ids: vec![labels["img"].clone()], profile: "Custom & RGB".into(), intent: "Perceptual".into(),
        }.into();
        e.apply("color", &[command]).unwrap();
        let changed = e.parts().iter().find(|(name, text)| before.get(*name) != Some(*text)).unwrap().1;
        assert!(changed.contains("extra=\"keep\""));
        assert!(changed.contains(">Custom &amp; RGB</Profile>"));
        assert_eq!(value(&e, &labels["img"]).profile, "Custom & RGB");
        e.undo().unwrap();
        assert_eq!(e.parts(), &before);
    }
}
