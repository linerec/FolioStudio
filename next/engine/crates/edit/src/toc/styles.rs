use super::xml::*;
use crate::{EditError, Work};
use folio_model::toc::TocStyle;

pub(super) fn all(parts: &std::collections::BTreeMap<String,String>) -> Result<Vec<TocStyle>, EditError> {
    let mut out = Vec::new();
    for t in parts.values().filter(|t| t.contains("<TOCStyle ")) {
        out.extend(folio_idml::toc::styles(t).map_err(|e| EditError::new("xml", e))?);
    }
    Ok(out)
}

pub(crate) fn save(w: &mut Work<'_>, style: &TocStyle) -> Result<(), EditError> {
    let doc=folio_idml::read_parts(w.ed.parts(),std::path::Path::new("toc.idml"),&Default::default()).map_err(|e|EditError::new("xml",e.to_string()))?;
    let mut normalized=style.clone();
    for entry in &mut normalized.entries {
        if let Some(p)=doc.styles.paragraph.iter().find(|p|p.id==entry.name) {entry.name=p.name.clone();}
        if let Some(p)=doc.styles.paragraph.iter().find(|p|p.name==entry.format_style) {entry.format_style=p.id.clone();}
        for value in [&mut entry.page_number_style,&mut entry.separator_style] {
            if let Some(c)=doc.styles.character.iter().find(|c|c.name==*value) {*value=c.id.clone();}
        }
    }
    if let Some(p)=doc.styles.paragraph.iter().find(|p|p.name==normalized.title_style) {normalized.title_style=p.id.clone();}
    if normalized.title_style.is_empty() { normalized.title_style="ParagraphStyle/$ID/[No paragraph style]".into(); }
    let style=&normalized;
    if style.name.trim().is_empty() || style.entries.iter().any(|e| e.name.is_empty() || !(1..=10).contains(&e.level)
        || !["BeforeEntry", "AfterEntry", "None"].contains(&e.page_number_position.as_str())) {
        return Err(EditError::new("invalid", "목차 스타일 이름, 단계 또는 쪽 번호 위치가 올바르지 않습니다"));
    }
    let all = all(w.ed.parts())?;
    if all.iter().any(|s| s.name == style.name && s.id != style.id) {
        return Err(EditError::new("invalid", "같은 이름의 목차 스타일이 있습니다"));
    }
    let id = if style.id.is_empty() { w.new_id() } else { style.id.clone() };
    let found = if style.id.is_empty() { None } else { Some(w.find(&id)?) };
    let mut t = if let Some((part, ix, i)) = &found {
        if ix.local(w.text(part)?, *i) != "TOCStyle" { return Err(EditError::new("invalid", "목차 스타일이 아닙니다")); }
        fragment(w.text(part)?, ix, *i)
    } else { "<TOCStyle></TOCStyle>".into() };
    for (k,v) in [("Self",id.clone()), ("Name",style.name.clone()), ("Title",style.title.clone()),
        ("IncludeHidden",style.include_hidden.to_string()), ("IncludeBookDocuments",style.include_book_documents.to_string()),
        ("CreateBookmarks",style.create_bookmarks.to_string()), ("MakeAnchor",style.make_anchor.to_string()),
        ("RunIn",style.run_in.to_string()), ("RemoveForcedLineBreak",style.remove_forced_line_break.to_string()),
        ("NumberedParagraphs",if style.numbered_paragraphs.is_empty() { "IncludeFullParagraph".into() } else { style.numbered_paragraphs.clone() })] {
        attribute(&mut t,k,&v)?;
    }
    property(&mut t,"TitleStyle",&style.title_style,"object")?;
    let ix = scan(&t)?;
    let old: Vec<_> = ix.named(&t,0,"TOCStyleEntry").into_iter().map(|i| (ix.attr(&t,i,"Name").unwrap_or_default().to_owned(), fragment(&t,&ix,i))).collect();
    let mut entries = String::new();
    for (n,e) in style.entries.iter().enumerate() {
        let mut c = old.iter().find(|(name,_)| crate::xml::unescape(name)==e.name).map(|(_,t)|t.clone()).unwrap_or_else(|| "<TOCStyleEntry></TOCStyleEntry>".into());
        for (k,v) in [("Self",format!("{id}TOCStyleEntry{n}")),("Name",e.name.clone()),("Level",e.level.to_string()),
            ("PageNumberPosition",e.page_number_position.clone()),("Separator",e.separator.clone()),("SortAlphabet",e.sort_alphabet.to_string())] { attribute(&mut c,k,&v)?; }
        for (k,v) in [("FormatStyle",&e.format_style),("PageNumberStyle",&e.page_number_style),("SeparatorStyle",&e.separator_style)] {
            property(&mut c,k,v,if v.starts_with("ParagraphStyle/") || v.starts_with("CharacterStyle/") { "object" } else { "string" })?;
        }
        entries.push_str(&c);
    }
    for i in ix.named(&t,0,"TOCStyleEntry").into_iter().rev() { t.replace_range(ix.nodes[i].start..ix.nodes[i].end,""); }
    let ix=scan(&t)?;
    t.insert_str(ix.nodes[0].close_start,&entries);
    if let Some((part,ix,i))=found { w.splice(&part,vec![(ix.nodes[i].start..ix.nodes[i].end,t)]) }
    else {
        let part="Resources/Styles.xml";
        let ix=w.index(part)?;
        let at=ix.nodes[0].close_start;
        w.splice(part,vec![(at..at,t)])
    }
}

pub(crate) fn delete(w: &mut Work<'_>, id: &str) -> Result<(), EditError> {
    if w.ed.parts().values().any(|t| t.contains(&format!("AppliedTOCStyle=\"{}\"",crate::xml::escape(id)))) {
        return Err(EditError::new("inUse", "이 목차 스타일을 사용하는 이야기가 있습니다"));
    }
    let (part,ix,i)=w.find(id)?;
    if ix.local(w.text(&part)?,i)!="TOCStyle" { return Err(EditError::new("invalid","목차 스타일이 아닙니다")); }
    w.splice(&part,vec![(ix.nodes[i].start..ix.nodes[i].end,String::new())])
}
