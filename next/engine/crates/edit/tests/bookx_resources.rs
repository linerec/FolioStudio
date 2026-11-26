use folio_edit::{Editor, commands::RegisteredCommand, xml::Index};
use std::{collections::{BTreeMap, BTreeSet}, path::Path};

fn measured(name: &str) -> BTreeMap<String,String> {
    ["designmap.xml", "Resources/Styles.xml", "Resources/Graphic.xml"].into_iter().map(|part| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/bookx-c2/resources").join(name).join(part);
        (part.into(), std::fs::read_to_string(path).unwrap())
    }).collect()
}
fn definition(t: &str, tag: &str, name: &str) -> String {
    let ix=Index::scan(t).unwrap();
    let n=(0..ix.nodes.len()).find(|&i| ix.local(t,i)==tag && ix.attr(t,i,"Name")==Some(name)).unwrap();
    let mut xml=t[ix.nodes[n].start..ix.nodes[n].end].to_owned();
    let ix=Index::scan(&xml).unwrap();
    let mut remove=Vec::new();
    for i in 0..ix.nodes.len() { for a in ix.attrs(&xml,i) {
        if ["Self","SwatchColorGroupReference"].contains(&&xml[a.name.0..a.name.1]) { remove.push(a.name.0..a.value.1+1); }
    }}
    for range in remove.into_iter().rev() { xml.replace_range(range, ""); }
    xml.split_whitespace().collect::<Vec<_>>().join(" ")
}
#[test]
fn seven_resource_kinds_match_indesign_and_undo_exactly() {
    let before=measured("before-1");
    let mut ed=Editor::new(before.clone());
    let kinds=["swatch","numberingList","conditionalText","textVariable","trapStyle","tableOfContentStyle","crossReferenceFormat"].map(String::from).to_vec();
    let command=RegisteredCommand::SyncBookResources{kinds,parts:measured("before-0")};
    ed.apply("sync", &[command.clone().into()]).unwrap();
    let expected=measured("after-1");
    for (part,tag) in [("Resources/Graphic.xml","Color"),("designmap.xml","NumberingList"),("designmap.xml","Condition"),("designmap.xml","TextVariable"),("Resources/Styles.xml","TrapPreset"),("Resources/Styles.xml","TOCStyle"),("designmap.xml","CrossReferenceFormat")] {
        for name in ["Shared","SourceOnly","TargetOnly"] {
            assert_eq!(definition(ed.part(part).unwrap(),tag,name),definition(&expected[part],tag,name),"{tag}/{name}");
        }
    }
    let after=ed.parts().clone();
    let mut ids=BTreeSet::new();
    for t in after.values() { let ix=Index::scan(t).unwrap();for i in 0..ix.nodes.len() {if let Some(id)=ix.attr(t,i,"Self") {assert!(ids.insert(id),"duplicate {id}");}} }
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);
    ed.redo().unwrap();assert_eq!(ed.parts(),&after);
    ed.apply("again", &[command.into()]).unwrap();
    // InDesign's second Synchronize unsets the colour it gave a source-only condition of its own colour (bookx-c4-cond17 after-2); nothing else changes
    let plain=|t:&str|{let mut t=t.to_owned();while let Some(a)=t.find("<IndicatorColor"){let b=t[a..].find("</IndicatorColor>").unwrap()+a+"</IndicatorColor>".len();t.replace_range(a..b,"");}t.replace("<Properties>","").replace("</Properties>","").split_whitespace().collect::<Vec<_>>().join(" ")};
    assert_eq!(ed.parts().keys().collect::<Vec<_>>(),after.keys().collect::<Vec<_>>());
    for (part,text) in ed.parts() { assert_eq!(plain(text),plain(&after[part]),"{part} after a second synchronization"); }
}
