use super::*;
use folio_edit::commands::RegisteredCommand as Toc;

// Source-story and source-style changes are setup for the TOC measurement. Only
// existing source parts are overlaid; no generated story, links or frames are input.
pub fn check(base:&BTreeMap<String,String>,golden:&Path)->Vec<String> {
    check_with_vector(base, golden, None)
}

pub fn check_with_vector(base: &BTreeMap<String, String>, golden: &Path, vector: Option<&Value>) -> Vec<String> {
    let mut input=base.clone();
    let after=read_dir_parts(&golden.join("after"));
    let mut expected=base.clone();expected.extend(after.clone());
    let expected_doc=read(&expected,Path::new("toc.idml"));
    let expected_style=expected_doc.toc_styles.iter().find(|s|s.name=="Oracle TOC").unwrap();
    for (name,text) in &after {
        if name.starts_with("Stories/") && base.contains_key(name) {input.insert(name.clone(),text.clone());}
    }
    let mut styles=after["Resources/Styles.xml"].clone();
    let ix=Index::scan(&styles).unwrap();
    let i=ix.by_self(&styles,&expected_style.id).unwrap();
    styles.replace_range(ix.nodes[i].start..ix.nodes[i].end,"");
    input.insert("Resources/Styles.xml".into(),styles);
    let mut ed=Editor::new(input.clone());
    let mut style=expected_style.clone();style.id.clear();
    let mut problems=Vec::new();
    let result=(||->Result<(),String> {
        ed.apply("style",&[Toc::SaveTOCStyle{style}.into()]).map_err(|e|e.to_string())?;
        let actual_style=ed.toc_styles().unwrap().into_iter().find(|s|s.name=="Oracle TOC").unwrap();
        let mut normalized=actual_style.clone();normalized.id=expected_style.id.clone();
        if &normalized!=expected_style {problems.push("TOC style differs".into());}
        let doc=read(ed.parts(),Path::new("toc.idml"));
        static FONTS:std::sync::OnceLock<folio_text::FontStore>=std::sync::OnceLock::new();
        let layout=folio_text::layout_document(&doc,FONTS.get_or_init(||folio_text::FontStore::discover(&[])));
        let sources=folio_text::layout::toc::sources(&doc,&layout);
        ed.apply("create",&[Toc::CreateTOC{style:actual_style.id,frame:String::new(),replace:false,sources:sources.clone()}.into()]).map_err(|e|e.to_string())?;
        let doc=read(ed.parts(),Path::new("toc.idml"));
        let story=doc.toc_stories.keys().next().unwrap().clone();
        if let Some(vector) = vector {
            check_vector(&doc, &story, ed.parts(), vector, &mut problems);
        }
        let wanted=expected_doc.toc_stories.keys().next().unwrap();
        let paragraphs=|d:&Document,id:&String| d.stories[id].paragraphs.iter().map(|p|(p.style.clone(),p.runs.iter().map(|r|r.text.as_str()).collect::<String>())).collect::<Vec<_>>();
        if paragraphs(&doc,&story)!=paragraphs(&expected_doc,wanted) {problems.push(format!("paragraphs: {:?} vs {:?}",paragraphs(&doc,&story),paragraphs(&expected_doc,wanted)));}
        let character_styles=|d:&Document,id:&String| d.stories[id].paragraphs.iter().map(|p|p.runs.iter().flat_map(|r|r.text.chars().map(|c|(c,r.attrs.character_style.clone()))).collect::<Vec<_>>()).collect::<Vec<_>>();
        if character_styles(&doc,&story)!=character_styles(&expected_doc,wanted) {problems.push("character styles differ".into());}
        let graph=|parts:&BTreeMap<String,String>| {
            let text=&parts["designmap.xml"];let ix=Index::scan(text).unwrap();
            (0..ix.nodes.len()).filter_map(|i|match ix.local(text,i) {
                "HyperlinkPageDestination"=>Some(format!("page:{}",ix.attr(text,i,"DestinationPage").unwrap_or_default())),
                "Bookmark"=>{
                    let mut names=vec![unescape(ix.attr(text,i,"Name").unwrap_or_default())];
                    let mut parent=ix.nodes[i].parent;
                    while let Some(p)=parent {
                        if ix.local(text,p)=="Bookmark" {names.push(unescape(ix.attr(text,p,"Name").unwrap_or_default()));}
                        parent=ix.nodes[p].parent;
                    }
                    names.reverse();Some(format!("bookmark:{names:?}"))
                },
                "Hyperlink"=>Some("link".into()),_=>None,
            }).collect::<Vec<_>>()
        };
        let mut actual=graph(ed.parts());actual.sort();let mut wanted=graph(&expected);wanted.sort();
        if actual!=wanted {problems.push(format!("links/bookmarks: {actual:?} vs {wanted:?}"));}
        let created=ed.parts().clone();
        ed.apply("update",&[Toc::UpdateTOC{story,sources}.into()]).map_err(|e|e.to_string())?;
        let mut updated=graph(ed.parts());updated.sort();
        if updated!=actual {problems.push("update duplicated or lost link targets".into());}
        let final_parts=ed.parts().clone();
        ed.undo();if ed.parts()!=&created {problems.push("update undo changed bytes".into());}
        ed.redo();if ed.parts()!=&final_parts {problems.push("update redo changed bytes".into());}
        for _ in 0..3 {ed.undo();}if ed.parts()!=&input {problems.push("undo did not restore input".into());}
        Ok(())
    })();
    if let Err(e)=result {problems.push(e);}
    problems
}

fn check_vector(doc: &Document, story: &str, parts: &BTreeMap<String, String>, vector: &Value, problems: &mut Vec<String>) {
    let projected = &vector["output"]["projectedAfter"];
    let wanted = projected.as_object().unwrap().values()
        .flat_map(|rows| rows.as_array().unwrap())
        .find(|row| {
            row["element"] == "Story" &&
                row["AppliedTOCStyle"].as_str().is_some_and(|value| value.starts_with("TOCStyle/"))
        })
        .unwrap();
    let style = doc.toc_styles.iter().find(|style| style.id == doc.toc_stories[story]).unwrap();
    if format!("TOCStyle/{}", style.name) != wanted["AppliedTOCStyle"].as_str().unwrap() {
        problems.push("vector: applied TOC style differs".into());
    }
    let paragraphs = &doc.stories[story].paragraphs;
    let mut ranges: Vec<(&str, String)> = Vec::new();
    for (i, paragraph) in paragraphs.iter().enumerate() {
        let style = doc.styles.paragraph.iter()
            .find(|style| style.name == paragraph.style)
            .map_or(paragraph.style.as_str(), |style| style.id.as_str());
        if ranges.last().is_none_or(|(previous, _)| *previous != style) {
            ranges.push((style, String::new()));
        }
        let text = &mut ranges.last_mut().unwrap().1;
        text.extend(paragraph.runs.iter().map(|run| run.text.as_str()));
        if i + 1 < paragraphs.len() {
            text.push('\n');
        }
    }
    let rows = wanted["paragraphs"].as_array().unwrap().iter()
        .map(|row| (row["style"].as_str().unwrap(), row["text"].as_str().unwrap().to_owned()))
        .collect::<Vec<_>>();
    if ranges != rows {
        problems.push(format!("vector paragraphs: {ranges:?} vs {rows:?}"));
    }
    let design = &parts["designmap.xml"];
    let index = Index::scan(design).unwrap();
    for name in ["HyperlinkPageDestination", "Hyperlink", "Bookmark"] {
        let actual = (0..index.nodes.len()).filter(|&i| index.local(design, i) == name).count();
        let expected = projected["designmap.xml"].as_array().unwrap().iter()
            .filter(|row| row["element"] == name).count();
        if actual != expected {
            problems.push(format!("vector {name}: {actual} vs {expected}"));
        }
    }
}
