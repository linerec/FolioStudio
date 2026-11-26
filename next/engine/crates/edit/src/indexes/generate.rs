use super::{entries, generated_styles, invalid};
use crate::{Command, EditError, Work, xml::{escape, fragment::*}};
use folio_model::toc::TextLocation;

// The registry passes each index command field separately; retain that signature.
#[allow(clippy::too_many_arguments)]
/// `include_book_documents`: make the book-wide index (none: as the document's index options say); `book_documents`: the
/// book's documents in order with the other documents' entries (the server fills them from the project's book).
pub(crate) fn generate(w: &mut Work<'_>, page: &String, frame: &str, replace: &bool, title: &str, headings: &bool, font: &str, locations: &[TextLocation], include_book_documents: &Option<bool>, book_documents: &Option<Vec<folio_model::indexes::BookIndexDocument>>) -> Result<(), EditError> {
    if font.is_empty() || title.chars().any(char::is_control) { return Err(invalid("font is required and title must be one line")); }
    super::generation_input::ensure_options(w)?;
    super::generation_input::check(w)?;
    let doc = folio_idml::read_parts(w.ed.parts(), std::path::Path::new("index.idml"), &Default::default())
        .map_err(|e| EditError::new("xml",e.to_string()))?;
    if doc.indexes.len() != 1 { return Err(invalid("문서에 색인이 하나 있어야 합니다")); }
    let include_book = include_book_documents.unwrap_or_else(|| w.ed.index_includes_book_documents());
    // Index panel ▸ Sort Options: the language groups (sort_options.rs)
    let opts = super::sort_options::SortOptions::read(w)?;
    let mut entries = entries::collect(w, &doc, &doc.indexes[0].topics, locations, &opts)?;
    if include_book && let Some(book) = book_documents.as_ref().filter(|b| b.iter().any(|d| !d.own)) {
        entries = entries::merge_book(entries, book, &opts)?;
    }
    let levels = entries.iter().map(|e| e.level).max().unwrap_or(0);
    let existing = instances(w)?;
    if *replace && existing.len() > 1 { return Err(invalid("교체할 색인 이야기가 여러 개입니다")); }
    let story = if *replace && !existing.is_empty() { existing[0].clone() } else {
        let frame = if frame.is_empty() {
            let page = doc.spreads.iter().flat_map(|s| &s.pages).filter(|p| page.is_empty() || p.id == *page).min_by_key(|p| p.index)
                .ok_or_else(|| invalid("쪽을 찾을 수 없습니다"))?;
            crate::ops::run(w, &Command::CreateItem { page: page.id.clone(), kind: crate::ops::ItemKind::TextFrame,
                rect: [36.,400.,page.bounds.width()-36.,page.bounds.height()-36.],layer:None,sides:6 })?;
            w.created.last().cloned().ok_or_else(|| invalid("글 상자를 만들지 못했습니다"))?
        } else {frame.to_owned()};
        let (part,ix,i) = w.find(&frame)?;
        if ix.local(w.text(&part)?,i) != "TextFrame" {return Err(invalid("글 상자를 선택하십시오"));}
        let story = ix.value(w.text(&part)?,i,"ParentStory").ok_or_else(|| invalid("missing story"))?;
        if doc.stories.get(&story).is_some_and(|s| s.paragraphs.iter().any(|p| p.runs.iter().any(|r| !r.text.is_empty()))) {
            return Err(invalid("색인을 놓을 글 상자가 비어 있지 않습니다"));
        }
        register(w,&story)?;
        story
    };
    let mut body = String::new();
    if !title.is_empty() {
        let style = generated_styles::style_for(w,"Title",font)?;
        body.push_str(&paragraph(&style, &format!("<Content>{}</Content>",escape(title)), true));
    }
    let mut heading = String::new();
    for entry in entries {
        if *headings && entry.level == 1 && heading != entry.heading {
            heading = entry.heading.clone();
            let style = generated_styles::style_for(w,"Section Head",font)?;
            body.push_str(&paragraph(&style,&format!("<Content>{}</Content>",escape(&heading)),true));
        }
        let style = generated_styles::style_for(w,&format!("Level {}",entry.level),font)?;
        let mut runs = format!("<Content>{}{}</Content>",escape(&entry.printed),if entry.pages.is_empty() {""} else {"  "});
        let (before_cross, between) = cross_separators(w)?;
        for (n,page) in entry.pages.iter().enumerate() {
            if n > 0 {runs.push_str("<Content>, </Content>");}
            // (a page of another book document is a link source with no destination in this document)
            let marker = if page.marker.is_empty() { None } else { Some((page.marker.as_str(),entry.name.as_str())) };
            runs.push_str(&link(w,&page.first,marker));
            if page.index != page.end {
                runs.push_str("<Content>–</Content>");
                runs.push_str(&link(w,&page.last,None));
            }
        }
        for (n,cross) in entry.cross.iter().enumerate() {
            cross_style(w,font)?;
            runs.push_str(&format!("<Content>{}{}</Content>",escape(if n == 0 {&before_cross} else {&between}),escape(&cross.lead)));
            // the words have the character style (the range closes and opens again, as InDesign writes it); FontStyle and font are the measuring script's (index6-see-*)
            runs.push_str(&format!("</CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"{CROSS_STYLE}\" FontStyle=\"Regular\"><Properties><AppliedFont type=\"string\">{}</AppliedFont></Properties><Content>{}</Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\">",escape(font),escape(&cross.words)));
            if !cross.tail.is_empty() { runs.push_str(&format!("<Content>{}</Content>",escape(&cross.tail))); }
        }
        body.push_str(&paragraph(&style,&runs,false));
    }
    if body.is_empty() {return Err(invalid("생성할 색인 참조가 없습니다"));}
    let (part,ix,i) = w.find(&story)?;
    let mut raw = fragment(w.text(&part)?,&ix,i);
    let si = scan(&raw)?;
    for &p in si.named(&raw,0,"ParagraphStyleRange").iter().rev() {raw.replace_range(si.nodes[p].start..si.nodes[p].end,"");}
    let si = scan(&raw)?;
    raw.insert_str(si.nodes[0].close_start,&body);
    w.splice(&part,vec![(ix.nodes[i].start..ix.nodes[i].end,raw)])?;
    generated_styles::options(w,title,*headings,*replace,levels)?;
    include_book_option(w, include_book)?;
    w.recompose(&story);
    w.created.push(story);
    Ok(())
}

impl crate::Editor {
    /// Whether this document's index options include the book's other documents.
    pub fn index_includes_book_documents(&self) -> bool {
        let part = "Resources/Preferences.xml";
        let (Some(text), Ok(ix)) = (self.part(part), self.cached_index(part)) else { return false };
        (0..ix.nodes.len())
            .find(|&i| ix.local(text, i) == "IndexOptions")
            .and_then(|i| ix.value(text, i, "IncludeBookDocuments"))
            .is_some_and(|v| v == "true")
    }
}

/// Generate Index keeps Include Book Documents in the document's index options, as InDesign does.
fn include_book_option(w: &mut Work<'_>, on: bool) -> Result<(), EditError> {
    let part = "Resources/Preferences.xml";
    let ix = w.index(part)?;
    let text = w.text(part)?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(text, i) == "IndexOptions").ok_or_else(|| EditError::new("xml", "missing index options"))?;
    if ix.value(text, i, "IncludeBookDocuments").is_none_or(|v| v == "false") && !on { return Ok(()); }
    let mut raw = fragment(text, &ix, i);
    attribute(&mut raw, "IncludeBookDocuments", &on.to_string())?;
    w.splice(part, vec![(ix.nodes[i].start..ix.nodes[i].end, raw)])
}

const CROSS_STYLE: &str = "CharacterStyle/$ID/Index Cross-reference";

/// The character style of the words "See also" (measured: italic, 10 pt; InDesign's font is Minion Pro, here the font of the index) and the index option that names it.
fn cross_style(w: &mut Work<'_>, font: &str) -> Result<(), EditError> {
    let part = "Resources/Styles.xml";
    let ix = w.index(part)?;
    let text = w.text(part)?;
    if let Some(i) = ix.by_self(text, CROSS_STYLE) {
        let mut raw = fragment(text, &ix, i);
        property(&mut raw, "AppliedFont", font, "string")?;
        w.splice(part, vec![(ix.nodes[i].start..ix.nodes[i].end, raw)])?;
    } else {
        let group = (0..ix.nodes.len()).find(|&i| ix.local(text, i) == "RootCharacterStyleGroup").ok_or_else(|| EditError::new("xml", "missing character styles"))?;
        let new = format!("<CharacterStyle Self=\"{CROSS_STYLE}\" Imported=\"false\" SplitDocument=\"false\" EmitCss=\"true\" IncludeClass=\"true\" ExtendedKeyboardShortcut=\"0 0 0\" KeyboardShortcut=\"0 0\" Name=\"$ID/Index Cross-reference\" FontStyle=\"Italic\" PointSize=\"10\"><Properties><BasedOn type=\"string\">$ID/[No character style]</BasedOn><PreviewColor type=\"enumeration\">Nothing</PreviewColor><AppliedFont type=\"string\">{}</AppliedFont></Properties></CharacterStyle>", escape(font));
        let at = ix.nodes[group].close_start;
        w.splice(part, vec![(at..at, new)])?;
    }
    let part = "Resources/Preferences.xml";
    let ix = w.index(part)?;
    let text = w.text(part)?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(text, i) == "IndexOptions").ok_or_else(|| EditError::new("xml", "missing index options"))?;
    let mut raw = fragment(text, &ix, i);
    attribute(&mut raw, "CrossReferenceStyle", CROSS_STYLE)?;
    w.splice(part, vec![(ix.nodes[i].start..ix.nodes[i].end, raw)])
}

/// The text between a topic's page numbers (or its name) and its cross references, and between two cross references
/// (`BeforeCrossReferenceSeparator` ". " and `BetweenEntriesSeparator` "; " by default, measured: index6-see-*).
fn cross_separators(w: &Work<'_>) -> Result<(String, String), EditError> {
    let t = w.text("Resources/Preferences.xml")?;
    let ix = w.index("Resources/Preferences.xml")?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "IndexOptions").ok_or_else(|| EditError::new("xml", "missing index options"))?;
    Ok((ix.value(t, i, "BeforeCrossReferenceSeparator").unwrap_or_else(|| ". ".into()), ix.value(t, i, "BetweenEntriesSeparator").unwrap_or_else(|| "; ".into())))
}

fn paragraph(style: &str, runs: &str, regular: bool) -> String {
    format!("<ParagraphStyleRange AppliedParagraphStyle=\"{}\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"{}>{runs}<Br/></CharacterStyleRange></ParagraphStyleRange>",escape(style),if regular {" FontStyle=\"Regular\""} else {""})
}
fn link(w: &mut Work<'_>, text: &str, marker: Option<(&str,&str)>) -> String {
    let id = w.new_id();
    let decimal = u64::from_str_radix(id.trim_start_matches('u'),16).unwrap_or(0);
    let properties = marker.map(|(id,topic)| format!("<Properties><AlternativeDestination Type=\"IndexMarker\" IndexMarkerId=\"{}\" TextAnchorName=\"\" TocEntryPageNumberString=\"{}\" TocEntryLevel=\"0\"/></Properties>",escape(id),escape(topic))).unwrap_or_default();
    format!("<HyperlinkTextSource Self=\"{id}\" Name=\".{decimal}\" Hidden=\"true\" AppliedCharacterStyle=\"n\">{properties}<Content>{}</Content></HyperlinkTextSource>",escape(text))
}
fn instances(w: &Work<'_>) -> Result<Vec<String>,EditError> {
    let ix=w.index("designmap.xml")?;let t=w.text("designmap.xml")?;
    Ok((0..ix.nodes.len()).filter(|&i|ix.local(t,i)=="IndexInstanceType").filter_map(|i|ix.value(t,i,"Story")).collect())
}
fn register(w: &mut Work<'_>, story: &str) -> Result<(),EditError> {
    let part="designmap.xml";let ix=w.index(part)?;let t=w.text(part)?;
    let root=ix.roots().next().ok_or_else(||invalid("missing document"))?;
    let p=*ix.named(t,root,"Properties").first().ok_or_else(||invalid("missing document properties"))?;
    let new=format!("<IndexInstanceType Name=\"$ID/Index\" Story=\"{}\"/>",escape(story));
    if let Some(&list)=ix.named(t,p,"InstanceList").first() {
        let n=&ix.nodes[list];
        if n.empty {w.splice(part,vec![(n.start..n.end,format!("<InstanceList>{new}</InstanceList>"))])}
        else {w.splice(part,vec![(n.close_start..n.close_start,new)])}
    } else {let at=ix.nodes[p].open_end;w.splice(part,vec![(at..at,format!("<InstanceList>{new}</InstanceList>"))])}
}
