use folio_edit::{Editor, commands::RegisteredCommand};
use std::collections::BTreeMap;
#[test]
fn numbering_definitions_preserve_unknown_fields_references_and_undo() {
 let original = BTreeMap::from([
 ("designmap.xml".into(),r#"<Document><NumberingList Self="NumberingList/$ID/[Default]" Name="$ID/[Default]"/><NumberingList Self="NumberingList/A" Name="A" ContinueNumbersAcrossStories="true" Vendor="keep"><Future Value="x"/></NumberingList></Document>"#.into()),
 ("Stories/Story_a.xml".into(),r#"<Story Self="a"><ParagraphStyleRange AppliedNumberingList="NumberingList/A"><Properties><AppliedNumberingList type="object">NumberingList/A</AppliedNumberingList></Properties><Content>Unchanged</Content></ParagraphStyleRange></Story>"#.into())]);
 let mut ed=Editor::new(original.clone());
 let changed=ed.apply("options",&[RegisteredCommand::SetNumberingList{id:"NumberingList/A".into(),name:"A".into(),props:BTreeMap::from([("acrossStories".into(),serde_json::json!(false))])}.into()]).unwrap();
 assert_eq!(changed.recompose,vec!["a"]);
 assert!(!ed.numbering_lists().unwrap()[1].across_stories);
 ed.undo().unwrap();assert_eq!(ed.parts(),&original);
 let set=|id:&str,name:&str|RegisteredCommand::SetNumberingList{id:id.into(),name:name.into(),props:BTreeMap::new()}.into();
 ed.apply("rename",&[set("NumberingList/A","한글 & B")]).unwrap();
 assert!(ed.part("designmap.xml").unwrap().contains("Vendor=\"keep\"><Future Value=\"x\"/>"));
 assert!(ed.part("Stories/Story_a.xml").unwrap().contains("NumberingList/한글 &amp; B"));
 assert_eq!(ed.numbering_lists().unwrap()[1].name,"한글 & B");
 let renamed=ed.parts().clone();ed.undo().unwrap();assert_eq!(ed.parts(),&original);ed.redo().unwrap();assert_eq!(ed.parts(),&renamed);
 assert!(ed.apply("duplicate",&[set("","한글 & B")]).is_err());assert_eq!(ed.parts(),&renamed);
 assert!(ed.apply("default",&[RegisteredCommand::DeleteNumberingList{id:folio_idml::numbering::DEFAULT.into()}.into()]).is_err());
 ed.apply("delete",&[RegisteredCommand::DeleteNumberingList{id:"NumberingList/한글 & B".into()}.into()]).unwrap();
 assert!(ed.part("Stories/Story_a.xml").unwrap().contains(folio_idml::numbering::DEFAULT));
 ed.undo().unwrap();assert_eq!(ed.parts(),&renamed);
 ed.apply("new",&[set("","Case")]).unwrap();ed.apply("case",&[set("","case")]).unwrap();
 let list=ed.numbering_lists().unwrap().pop().unwrap();assert!(list.across_stories&&list.across_documents);
}
