use folio_edit::{Command,Editor,xml::Index};
use std::{collections::BTreeMap,io::Read,path::Path};

fn editor() -> Editor {
    let path=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/basic.idml");
    let mut zip=zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts=BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry=zip.by_index(i).unwrap();
        if entry.is_dir() {continue;}
        let name=entry.name().unwrap().into_owned();
        let mut text=String::new();entry.read_to_string(&mut text).unwrap();parts.insert(name,text);
    }
    Editor::new(parts)
}
fn labelled(ed:&Editor,label:&str)->String {
    for t in ed.parts().values() {
        if !t.contains(&format!("Value=\"{label}\"")) {continue;}
        let ix=Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.attr(t,i,"Value")==Some(label) {
                return ix.ancestors(i).find_map(|a|ix.attr(t,a,"Self").map(str::to_owned)).unwrap();
            }
        }
    }
    panic!("missing label")
}
fn link(from:&str,to:Option<&str>)->Command {
    Command::ThreadText{id:from.into(),next:to.map(str::to_owned)}
}
fn new_frame(ed:&Editor,id:&str)->Command {
    let page=ed.parts().iter().filter(|(p,_)|p.starts_with("Spreads/"))
        .find_map(|(_,t)|{let ix=Index::scan(t).unwrap();(0..ix.nodes.len()).find(|&i|ix.local(t,i)=="Page").map(|i|ix.attr(t,i,"Self").unwrap().to_owned())}).unwrap();
    Command::CreateThreadFrame{id:id.into(),page,rect:[40.,40.,200.,240.],layer:None}
}
#[test]
fn cycles_and_nonframes_are_atomic() {
    let mut ed=editor();
    let (a,b,c,r)=(labelled(&ed,"tf"),labelled(&ed,"tf-1"),labelled(&ed,"tf-2"),labelled(&ed,"r-plain"));
    let before=ed.parts().clone();
    ed.apply("detach",&[link(&b,None)]).unwrap();ed.undo().unwrap();
    for cmd in [link(&a,Some(&a)),link(&c,Some(&b)),link(&a,Some(&r))] {
        assert!(ed.apply("refused",&[cmd]).is_err());
        assert_eq!(ed.parts(),&before);
        assert_eq!(ed.redo_label(),Some("detach"));
    }
}
#[test]
fn create_and_thread_is_one_undo_and_failed_source_leaves_no_story() {
    let mut ed=editor();
    let before=ed.parts().clone();let from=labelled(&ed,"tf-2");
    let result=ed.apply("create and thread",&[new_frame(&ed,&from)]).unwrap();
    assert_eq!(result.created.len(),1);
    let after=ed.parts().clone();
    ed.undo().unwrap();assert_eq!(ed.parts(),&before);assert_eq!(ed.undo_label(),None);
    let bad=labelled(&ed,"r-plain");
    assert!(ed.apply("refused",&[new_frame(&ed,&bad)]).is_err());
    assert_eq!(ed.parts(),&before);
    ed.redo().unwrap();assert_eq!(ed.parts(),&after);
}
#[test]
fn locked_downstream_frame_prevents_thread_mutation() {
    let mut ed=editor();let a=labelled(&ed,"tf");let b=labelled(&ed,"tf-1");let c=labelled(&ed,"tf-2");
    ed.apply("lock",&[Command::SetItem{id:c,locked:Some(true),visible:None}]).unwrap();
    let before=ed.parts().clone();
    assert!(ed.apply("join",&[link(&a,Some(&b))]).is_err());
    assert!(ed.apply("detach",&[link(&b,None)]).is_err());
    assert_eq!(ed.parts(),&before);
}
#[test]
fn unsupported_inline_merge_preserves_both_stories() {
    let ed=editor();let a=labelled(&ed,"tf");let b=labelled(&ed,"tf-1");
    let mut parts=ed.parts().clone();
    let story=parts.values().find_map(|t| {
        let ix=Index::scan(t).ok()?;let i=ix.by_self(t,&a)?;
        ix.attr(t,i,"ParentStory").map(str::to_owned)
    }).unwrap();
    let t=parts.get_mut(&format!("Stories/Story_{story}.xml")).unwrap();
    *t=t.replacen("<Content>", "<Footnote Self=\"u999f\"><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>note</Content></CharacterStyleRange></ParagraphStyleRange></Footnote><Content>",1);
    let mut ed=Editor::new(parts);let before=ed.parts().clone();
    assert!(ed.apply("join",&[link(&b,Some(&a))]).is_err());
    assert_eq!(ed.parts(),&before);
}
