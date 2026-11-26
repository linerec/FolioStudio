use super::*;

#[test]
fn unmeasured_generation_is_atomic() {
    let base=read_zip(&oracle().join("fixtures/index-c2.idml"));
    let mut ed=Editor::new(base.clone());
    let ids=labels(&base);
    let topic=serde_json::json!({"op":"addIndexTopic","parent":"","name":"\u{1D400}x","sort_order":"","target":"format"});
    run_step(&mut ed,&topic,&ids,&base).unwrap();
    let before=ed.parts().clone();
    let command=serde_json::json!({"op":"generateIndex","page":"","frame":"","replace":true,"title":"Index","headings":true,"font":"Malgun Gothic","locations":[]});
    let error=run_step(&mut ed,&command,&ids,&base).unwrap_err();
    assert!(error.contains("unsupported"),"{error}");
    assert_eq!(ed.parts(),&before);
    ed.undo();assert_eq!(ed.parts(),&base);
}

fn stories(parts: &BTreeMap<String,String>) -> Vec<String> {
    let t=&parts["designmap.xml"];let ix=Index::scan(t).unwrap();
    (0..ix.nodes.len()).filter(|&i|ix.local(t,i)=="IndexInstanceType")
        .filter_map(|i|ix.attr(t,i,"Story").map(str::to_owned)).collect()
}

// Generated IDs and XML whitespace differ; compare the semantic IDML graph and
// parsed paragraphs, including character overrides. No golden parts are input.
pub fn check(base:&BTreeMap<String,String>, golden:&Path, scenario:&Value, ids:&HashMap<String,String>) -> Vec<String> {
    let mut expected=base.clone();expected.extend(read_dir_parts(&golden.join("after")));
    let mut ed=Editor::new(base.clone());let mut problems=Vec::new();
    let result=(||->Result<(),String>{
        let mut snapshots=vec![base.clone()];
        for step in scenario["steps"].as_array().unwrap() {
            // a step that changes nothing (adding a topic whose name is equal to an existing one) leaves no undo step
            let before=ed.parts().clone();
            // a step marked `tolerate` may fail in both (InDesign records the error and goes on: a second cross reference of the same kind to a topic)
            match run_step(&mut ed,step,ids,base) {Err(_) if step["tolerate"]==true => continue, other => {other?;}}
            if ed.parts()!=&before {snapshots.push(ed.parts().clone());}
        }
        let actual_doc=read(ed.parts(),Path::new("index.idml"));let wanted_doc=read(&expected,Path::new("index.idml"));
        let actual=stories(ed.parts());let wanted=stories(&expected);
        if actual.len()!=wanted.len(){return Err(format!("index story count {} != {}",actual.len(),wanted.len()));}
        let paragraphs=|d:&Document,id:&String|d.stories[id].paragraphs.iter().map(|p|(
            p.style.clone(),p.runs.iter().map(|r|r.text.as_str()).collect::<String>(),
            p.runs.iter().flat_map(|r|r.text.chars().map(|c|(c,r.attrs.clone()))).collect::<Vec<_>>()
        )).collect::<Vec<_>>();
        for (a,b) in actual.iter().zip(&wanted) {
            if paragraphs(&actual_doc,a)!=paragraphs(&wanted_doc,b){problems.push(format!("paragraphs differ: {:?} vs {:?}",paragraphs(&actual_doc,a),paragraphs(&wanted_doc,b)));}
        }
        let graph=|parts:&BTreeMap<String,String>,ids:&[String]| {
            let mut out=Vec::new();
            let mut markers=BTreeMap::new();
            for (part,t) in parts.iter().filter(|(p,_)|p.starts_with("Stories/")) {
                let ix=Index::scan(t).unwrap();let mut ordinal=0;
                for i in 0..ix.nodes.len(){if ix.local(t,i)=="PageReference" {
                    markers.insert(ix.attr(t,i,"Id").unwrap_or_default().to_owned(),format!("{part}:{ordinal}:{}",ix.value(t,i,"ReferencedTopic").unwrap_or_default()));ordinal+=1;
                }}
            }
            for id in ids {
                let t=&parts[&format!("Stories/Story_{id}.xml")];let ix=Index::scan(t).unwrap();
                for i in 0..ix.nodes.len() {if ix.local(t,i)=="AlternativeDestination" {
                    let mut attrs=["Type","IndexMarkerId","TextAnchorName","TocEntryPageNumberString","TocEntryLevel"].map(|a|ix.value(t,i,a).unwrap_or_default().to_owned());
                    attrs[1]=markers.get(&attrs[1]).cloned().unwrap_or_else(||format!("MISSING {}",attrs[1]));out.push(attrs);
                }}
            }out
        };
        if graph(ed.parts(),&actual)!=graph(&expected,&wanted){problems.push(format!("index marker destinations differ: {:?} vs {:?}",graph(ed.parts(),&actual),graph(&expected,&wanted)));}
        let options=|parts:&BTreeMap<String,String>|{
            let t=&parts["Resources/Preferences.xml"];let ix=Index::scan(t).unwrap();
            let i=(0..ix.nodes.len()).find(|&i|ix.local(t,i)=="IndexOptions").unwrap();
            ["Title","TitleStyle","SectionHeadingStyle","Level1Style","Level2Style","Level3Style","Level4Style","ReplaceExistingIndex","IncludeSectionHeadings"].map(|a|ix.attr(t,i,a).unwrap_or_default().to_owned())
        };
        if options(ed.parts())!=options(&expected){problems.push(format!("index options differ: {:?} vs {:?}",options(ed.parts()),options(&expected)));}
        let styles=|parts:&BTreeMap<String,String>|{
            let t=&parts["Resources/Styles.xml"];let ix=Index::scan(t).unwrap();let mut out=BTreeMap::new();
            for i in 0..ix.nodes.len(){let id=ix.attr(t,i,"Self").unwrap_or_default();if id.starts_with("ParagraphStyle/$ID/Index "){
                let attrs=["FontStyle","PointSize","SpaceBefore","SpaceAfter","Justification","LeftIndent","FirstLineIndent"].map(|a|ix.value(t,i,a).unwrap_or_default().to_owned());
                let props=ix.nodes.iter().enumerate().filter(|(_,n)| n.start>ix.nodes[i].start && n.end<ix.nodes[i].end)
                    .filter(|(j,_)|matches!(ix.local(t,*j),"BasedOn"|"AppliedFont"))
                    .map(|(j,n)|(ix.local(t,j).to_owned(),t[n.open_end..n.close_start].to_owned())).collect::<Vec<_>>();
                out.insert(id.to_owned(),(attrs,props));
            }}out
        };
        if styles(ed.parts())!=styles(&expected){problems.push(format!("generated styles differ: {:?} vs {:?}",styles(ed.parts()),styles(&expected)));}
        for previous in snapshots[..snapshots.len()-1].iter().rev(){ed.undo();if ed.parts()!=previous {problems.push("one-command undo did not restore exact bytes".into());}}
        for next in &snapshots[1..]{ed.redo();if ed.parts()!=next{problems.push("redo did not restore exact bytes".into());}}
        Ok(())
    })();
    if let Err(e)=result{problems.push(e);}problems
}
