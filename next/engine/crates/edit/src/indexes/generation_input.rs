use crate::{EditError, Work};

pub(super) fn ensure_options(w: &mut Work<'_>) -> Result<(), EditError> {
    let part = "Resources/Preferences.xml";
    let text = w.text(part)?;
    let ix = w.index(part)?;
    if (0..ix.nodes.len()).any(|i| ix.local(text, i) == "IndexOptions") {
        return Ok(());
    }
    let root = ix.roots().find(|&i| ix.local(text, i) == "Preferences")
        .ok_or_else(|| EditError::new("xml", "missing Preferences"))?;
    let at = ix.nodes[root].close_start;
    w.splice(part, vec![(at..at, "<IndexOptions IncludeBookDocuments=\"false\" />\n".into())])
}

pub(super) fn check(w: &Work<'_>) -> Result<(), EditError> {
    let unsupported = || EditError::new("unsupported", "측정하지 않은 색인 생성 옵션이 있습니다(사용자 글자 스타일은 지원하지 않습니다)");
    let t=w.text("Resources/Preferences.xml")?;
    let ix=w.index("Resources/Preferences.xml")?;
    let i=(0..ix.nodes.len()).find(|&i|ix.local(t,i)=="IndexOptions")
        .ok_or_else(||EditError::new("xml","missing index options"))?;
    // (IncludeBookDocuments: the book-wide index, build-b3)
    for (key,expected) in [("IncludeHiddenEntries","false"),
        ("IndexFormat","NestedFormat"),("IncludeEmptyIndexSections","false"),
        ("FollowingTopicSeparator","  "),("BetweenPageNumbersSeparator",", "),
        ("PageRangeSeparator","^="),("EntryEndSeparator","")]
    { if ix.value(t,i,key).is_some_and(|s|s!=expected) {return Err(unsupported());} }
    // the paragraph styles may be the user's own (index-c3); they must exist
    super::generation_styles::check(w)?;
    // the character styles of the page numbers and of the cross references (the words and the topic they name) are not applied
    for key in ["PageNumberStyle","CrossReferenceStyle","CrossReferenceTopicStyle"] {
        // (the style of the words is the generated `Index Cross-reference` after a generation with cross references)
        if ix.value(t,i,key).is_some_and(|s|s!="CharacterStyle/$ID/[No character style]" && !(key=="CrossReferenceStyle" && s=="CharacterStyle/$ID/Index Cross-reference")){return Err(unsupported());}
    }
    for part in w.parts_named("Stories/") {
        let t=w.text(&part)?; let ix=w.index(&part)?;
        for i in 0..ix.nodes.len() {
            if ix.local(t,i)=="PageReference" && ix.value(t,i,"PageNumberStyleOverride").is_some_and(|s|s!="n" && s!="CharacterStyle/$ID/[No character style]") {
                return Err(unsupported());
            }
        }
    }
    Ok(())
}
