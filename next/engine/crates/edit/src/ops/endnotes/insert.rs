//! Type ▸ Insert Endnote (InDesign 20.5, notes3-insert.ko.md, window document): a reference at the
//! insertion point (the selected text stays), its body among the endnote story's bodies in
//! reference order, and, the first time, a new last page with an endnote text frame and the endnote
//! story (title, then the body). One command, so one undo.
use super::story::{self, At, NO_STYLE};
use crate::xml::{escape, Index};
use crate::{Command, EditError, Work};

pub(crate) fn run(w: &mut Work<'_>, story: &str, at: &usize) -> Result<(), EditError> {
    let opts = story::options(w)?;
    let separator = opts.get("EndnoteSeparatorText").cloned().unwrap_or_else(|| "\t".into());
    // Endnote Options ▸ Scope Story (rest-c1, oracle endnotes-story en-story-scope): each story has an endnote story of its own, on a new
    // page right after the page where the story ends; Document: the one endnote story on a new last page
    let scoped = opts.get("ScopeValue").is_some_and(|v| v == "StoryScope");
    let source = if scoped { Some(story::source_part(w, story)?) } else { None };
    let part = match &source {
        Some(src) => match story::endnote_part_for(w, src)? {
            Some(p) => p,
            None => { let before = story::page_after_story(w, src)?; create_story(w, before)? }
        },
        None => match story::endnote_part(w)? { Some(p) => p, None => create_story(w, None)? },
    };
    let (reference, range) = (w.new_id(), w.new_id());
    crate::text::endnote_ref::insert(w, story, *at, &reference, &range)?;
    // the references in reading order (of this story when each story has its own endnotes): where the new one stands decides where its body goes
    let order: Vec<(String, String)> = match &source { Some(src) => references_in(w, src)?, None => references(w)? };
    let k = order.iter().position(|(r, _)| *r == reference).ok_or_else(|| EditError::new("xml", "the new reference is not in the text"))?;
    let body = story::range_xml(&range, &reference, "", &separator);
    let place = match (order.get(k + 1), k.checked_sub(1).and_then(|p| order.get(p))) {
        (Some((_, next)), _) => At::Before(next),
        (None, Some((_, previous))) => At::After(previous),
        (None, None) => At::First,
    };
    story::place_range(w, &part, &body, place)?;
    w.created.push(reference);
    w.created.push(range);
    Ok(())
}

/// What an endnote reference looks like in a story: its `Position` and its character style (the document's endnote options).
pub(crate) fn marker_look(w: &Work<'_>) -> Result<(Option<String>, Option<String>), EditError> {
    let opts = story::options(w)?;
    let superscript = opts.get("EndnoteMarkerPositioning").is_none_or(|p| p == "SuperscriptMarker");
    let style = opts.get("EndnoteMarkerStyle").filter(|cs| !cs.ends_with("/[No character style]")).cloned();
    Ok((superscript.then(|| "Superscript".to_owned()), style))
}

/// The bodies of endnotes whose references a placed Word file brought (`(reference, range, text)`, in reading order, the text already
/// without the number): each goes among the bodies in reference order, and the first one of the document makes the endnote story
/// (oracle docxtext-c2-en*).
pub(crate) fn add_bodies(w: &mut Work<'_>, items: &[(String, String, String, Vec<story::RichRun>)], tail_style: Option<&str>) -> Result<(), EditError> {
    if items.is_empty() {
        return Ok(());
    }
    let opts = story::options(w)?;
    let separator = opts.get("EndnoteSeparatorText").cloned().unwrap_or_else(|| "\t".into());
    let part = match story::endnote_part(w)? { Some(p) => p, None => create_story(w, None)? };
    for (reference, range, text, rich) in items {
        // the references in reading order, less those whose bodies are not in the story yet (the later ones of this very text)
        let there = {
            let t = w.text(&part)?;
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "EndnoteRange").filter_map(|i| ix.value(t, i, "Self")).collect::<Vec<_>>()
        };
        let order: Vec<(String, String)> = references(w)?.into_iter().filter(|(r, b)| r == reference || there.contains(b)).collect();
        let k = order.iter().position(|(r, _)| r == reference).ok_or_else(|| EditError::new("xml", "the new reference is not in the text"))?;
        // runs with character formatting keep it (docxtext3-q04); plain text is one content
        let body = if rich.is_empty() {
            story::range_xml(range, reference, &story::text_escape(text).replace('\r', "</Content><Br /><Content>"), &separator)
        } else {
            story::rich_range_xml(range, reference, rich, &separator)
        };
        let place = match (order.get(k + 1), k.checked_sub(1).and_then(|p| order.get(p))) {
            (Some((_, next)), _) => At::Before(next),
            (None, Some((_, previous))) => At::After(previous),
            (None, None) => At::First,
        };
        story::place_range(w, &part, &body, place)?;
        w.created.push(range.clone());
    }
    if let Some(style) = tail_style {
        story::wrap_bodies(w, &part, style)?;
    }
    Ok(())
}

/// `(Self, EndnoteTextRange)` of the endnote references of one story part, in reading order.
fn references_in(w: &Work<'_>, part: &str) -> Result<Vec<(String, String)>, EditError> {
    let t = w.text(part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    Ok((0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Endnote")
        .filter_map(|i| Some((ix.value(t, i, "Self")?, ix.value(t, i, "EndnoteTextRange")?))).collect())
}

/// `(Self, EndnoteTextRange)` of every endnote reference of the main stories, in reading order.
fn references(w: &Work<'_>) -> Result<Vec<(String, String)>, EditError> {
    let mut out = Vec::new();
    for part in story::story_parts(w)? {
        let t = w.text(&part)?;
        if !t.contains("<Endnote ") || t.contains("IsEndnoteStory=\"true\"") { continue; }
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "Endnote"
                && let (Some(r), Some(b)) = (ix.value(t, i, "Self"), ix.value(t, i, "EndnoteTextRange")) { out.push((r, b)); }
        }
    }
    Ok(out)
}

/// The first endnote of a document (of a story, with the scope Story): a new page of the document's parent page (the last page, or in
/// front of page `before`), the endnote text frame in its margins, the endnote story with its title. Returns the story's part.
pub(super) fn create_story(w: &mut Work<'_>, before: Option<String>) -> Result<String, EditError> {
    let opts = story::options(w)?;
    let master = last_master(w)?;
    let made = w.created.len();
    crate::ops::pages::run(w, &Command::InsertPages { count: 1, before: None, master })?;
    let page_id = w.created.get(made..).and_then(|c| c.last()).cloned().ok_or_else(|| EditError::new("xml", "no new page"))?;
    // in front of a page: the new last page moves there (its own spread; the spreads after it keep theirs, as InDesign writes it)
    if let Some(before) = before {
        crate::ops::pages::run(w, &Command::MovePages { pages: vec![page_id.clone()], before: Some(before) })?;
        keep_spread_names(w, &page_id)?;
    }
    let needle = format!("Self=\"{page_id}\"");
    let spread_part = w.parts_named("Spreads/").into_iter().find(|p| w.text(p).is_ok_and(|t| t.contains(&needle)))
        .ok_or_else(|| EditError::new("xml", "the new page has no spread"))?;
    let (story_id, frame_id) = (w.new_id(), w.new_id());
    let t = w.text(&spread_part)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let page = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Page" && ix.attr(&t, i, "Self") == Some(page_id.as_str()))
        .ok_or_else(|| EditError::new("xml", "no page"))?;
    let number = |text: &str, i: usize, key: &str| -> Vec<f64> {
        ix.attr(text, i, key).unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect()
    };
    let (bounds, transform) = (number(&t, page, "GeometricBounds"), number(&t, page, "ItemTransform"));
    let margins = ix.path(&t, page, &["MarginPreference"]).ok_or_else(|| EditError::new("xml", "page without margins"))?;
    let margin = |key: &str| ix.attr(&t, margins, key).and_then(|v| v.parse::<f64>().ok()).unwrap_or(36.0);
    if bounds.len() != 4 || transform.len() != 6 { return Err(EditError::new("xml", "page geometry")); }
    // the page (top, left, bottom, right) in spread coordinates, less its margins
    let (top, left) = (bounds[0] + transform[5] + margin("Top"), bounds[1] + transform[4] + margin("Left"));
    let (bottom, right) = (bounds[2] + transform[5] - margin("Bottom"), bounds[3] + transform[4] - margin("Right"));
    let layer = {
        let dm = w.text("designmap.xml")?;
        let dx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
        let root = (0..dx.nodes.len()).find(|&i| dx.local(dm, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document"))?;
        (dx.attr(dm, root, "ActiveLayer").unwrap_or("").to_owned(), dx.attr(dm, root, "DOMVersion").unwrap_or("20.4").to_owned())
    };
    let frame = frame_xml(&frame_id, &story_id, &layer.0, [top, left, bottom, right]);
    let spread = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Spread" && ix.attr(&t, i, "Self").is_some()).ok_or_else(|| EditError::new("xml", "no spread"))?;
    // InDesign writes the frame after the page; a page of a spread that is not facing has binding 0 (measured)
    let at = ix.nodes[spread].close_start;
    let binding = crate::ops::attr_edit(&ix, &t, spread, "BindingLocation", "0");
    let rule = crate::ops::attr_edit(&ix, &t, page, "LayoutRule", "UseMaster");
    w.splice(&spread_part, vec![(at..at, frame), binding, rule])?;
    // the story: the title (its own paragraph when the title style differs from the body style)
    let title = opts.get("EndnoteTitle").cloned().unwrap_or_else(|| "Endnotes".into());
    let title_style = opts.get("EndnoteTitleStyle").cloned().unwrap_or_else(|| "ParagraphStyle/$ID/NormalParagraphStyle".into());
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Story xmlns:idPkg=\"http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging\" DOMVersion=\"{}\"><Story Self=\"{story_id}\" AppliedTOCStyle=\"n\" UserText=\"true\" IsEndnoteStory=\"true\" TrackChanges=\"false\" StoryTitle=\"$ID/\" AppliedNamedGrid=\"n\"><StoryPreference OpticalMarginAlignment=\"false\" OpticalMarginSize=\"12\" FrameType=\"TextFrameType\" StoryOrientation=\"Horizontal\" StoryDirection=\"LeftToRightDirection\" /><GridDataInformation FontStyle=\"$ID/\" PointSize=\"9.21259842519685\" CharacterAki=\"0\" LineAki=\"6.909448818897637\" HorizontalScale=\"100\" VerticalScale=\"100\" LineAlignment=\"LeftOrTopLineJustify\" GridAlignment=\"AlignEmCenter\" CharacterAlignment=\"AlignEmCenter\" /><InCopyExportOption IncludeGraphicProxies=\"true\" IncludeAllResources=\"false\" /><ParagraphStyleRange AppliedParagraphStyle=\"{}\"><CharacterStyleRange AppliedCharacterStyle=\"{NO_STYLE}\"><Content>{}</Content><Br /></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>", escape(&layer.1), escape(&title_style), escape(&title));
    let part = format!("Stories/Story_{story_id}.xml");
    w.add_part(part.clone(), xml)?;
    crate::ops::story_copy::register(w, &story_id)?;
    w.created.push(frame_id);
    Ok(part)
}

/// The page operations keep the spread parts in their places and move the pages through them, so the new page's spread took the name
/// (part and `Self`) of the spread it was put in front of. InDesign gives the new spread a new name and the spread after it keeps its
/// own (oracle endnotes-story en-story-scope): the two names are exchanged, each spread staying where it is.
fn keep_spread_names(w: &mut Work<'_>, page_id: &str) -> Result<(), EditError> {
    let dm = w.text("designmap.xml")?.to_owned();
    let srcs: Vec<(usize, String)> = dm.match_indices("<idPkg:Spread src=\"").filter_map(|(i, m)| {
        let at = i + m.len();
        dm[at..].split('"').next().map(|s| (at, s.to_owned()))
    }).collect();
    let needle = format!("Self=\"{page_id}\"");
    let Some(k) = srcs.iter().position(|(_, p)| w.text(p).is_ok_and(|t| t.contains(&needle))) else { return Ok(()) };
    let Some((_, next)) = srcs.get(k + 1).cloned() else { return Ok(()) };
    let (at, here) = srcs[k].clone();
    let self_of = |p: &str| p.trim_start_matches("Spreads/Spread_").trim_end_matches(".xml").to_owned();
    let (a, b) = (self_of(&here), self_of(&next));
    let (ta, tb) = (w.text(&here)?.to_owned(), w.text(&next)?.to_owned());
    let (sa, sb) = (format!("<Spread Self=\"{a}\""), format!("<Spread Self=\"{b}\""));
    if !ta.contains(&sa) || !tb.contains(&sb) { return Ok(()); }
    w.remove_part(&here);
    w.remove_part(&next);
    w.add_part(next.clone(), ta.replacen(&sa, &sb, 1))?;
    w.add_part(here.clone(), tb.replacen(&sb, &sa, 1))?;
    // the designmap lists the new spread under its new name, the next one under its own
    let next_at = srcs[k + 1].0;
    w.splice("designmap.xml", vec![(at..at + here.len(), next), (next_at..next_at + srcs[k + 1].1.len(), here)])
}

fn frame_xml(id: &str, story: &str, layer: &str, [top, left, bottom, right]: [f64; 4]) -> String {
    let point = |x: f64, y: f64| format!("<PathPointType Anchor=\"{x} {y}\" LeftDirection=\"{x} {y}\" RightDirection=\"{x} {y}\" />");
    format!("<EndnoteTextFrame Self=\"{id}\" ParentStory=\"{story}\" PreviousTextFrame=\"n\" NextTextFrame=\"n\" ContentType=\"TextType\" ParentInterfaceChangeCount=\"\" TargetInterfaceChangeCount=\"\" LastUpdatedInterfaceChangeCount=\"\" OverriddenPageItemProps=\"\" HorizontalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" VerticalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" GradientFillStart=\"0 0\" GradientFillLength=\"0\" GradientFillAngle=\"0\" GradientStrokeStart=\"0 0\" GradientStrokeLength=\"0\" GradientStrokeAngle=\"0\" ItemLayer=\"{layer}\" Locked=\"false\" LocalDisplaySetting=\"Default\" GradientFillHiliteLength=\"0\" GradientFillHiliteAngle=\"0\" GradientStrokeHiliteLength=\"0\" GradientStrokeHiliteAngle=\"0\" AppliedObjectStyle=\"ObjectStyle/$ID/[Normal Text Frame]\" Visible=\"true\" Name=\"$ID/\" ItemTransform=\"1 0 0 1 0 0\"><Properties><PathGeometry><GeometryPathType PathOpen=\"false\"><PathPointArray>{}{}{}{}</PathPointArray></GeometryPathType></PathGeometry></Properties><TextFramePreference TextColumnCount=\"1\" TextColumnFixedWidth=\"{}\" TextColumnMaxWidth=\"0\"><Properties><InsetSpacing type=\"list\"><ListItem type=\"unit\">0</ListItem><ListItem type=\"unit\">0</ListItem><ListItem type=\"unit\">0</ListItem><ListItem type=\"unit\">0</ListItem></InsetSpacing></Properties></TextFramePreference><TextWrapPreference Inverse=\"false\" ApplyToMasterPageOnly=\"false\" TextWrapSide=\"BothSides\" TextWrapMode=\"None\"><Properties><TextWrapOffset Top=\"0\" Left=\"0\" Bottom=\"0\" Right=\"0\" /></Properties></TextWrapPreference><ObjectExportOption EpubType=\"$ID/\" SizeType=\"DefaultSize\" CustomSize=\"$ID/\" PreserveAppearanceFromLayout=\"PreserveAppearanceDefault\" AltTextSourceType=\"SourceXMLStructure\" ActualTextSourceType=\"SourceXMLStructure\" CustomAltText=\"$ID/\" CustomActualText=\"$ID/\" ApplyTagType=\"TagFromStructure\" ImageConversionType=\"JPEG\" ImageExportResolution=\"Ppi300\" GIFOptionsPalette=\"AdaptivePalette\" GIFOptionsInterlaced=\"true\" JPEGOptionsQuality=\"High\" JPEGOptionsFormat=\"BaselineEncoding\" ImageAlignment=\"AlignLeft\" ImageSpaceBefore=\"0\" ImageSpaceAfter=\"0\" UseImagePageBreak=\"false\" ImagePageBreak=\"PageBreakBefore\" CustomImageAlignment=\"false\" SpaceUnit=\"CssPixel\" CustomLayout=\"false\" CustomLayoutType=\"AlignmentAndSpacing\"><Properties><AltMetadataProperty NamespacePrefix=\"$ID/\" PropertyPath=\"$ID/\" /><ActualMetadataProperty NamespacePrefix=\"$ID/\" PropertyPath=\"$ID/\" /></Properties></ObjectExportOption></EndnoteTextFrame>",
        point(left, top), point(left, bottom), point(right, bottom), point(right, top), right - left)
}

/// The parent page applied to the document's last page: the new page gets the same one (measured).
fn last_master(w: &Work<'_>) -> Result<Option<String>, EditError> {
    let dm = w.text("designmap.xml")?;
    let last = dm.match_indices("<idPkg:Spread src=\"").map(|(i, m)| i + m.len()).filter_map(|i| dm[i..].split('"').next()).last();
    let Some(part) = last else { return Ok(None) };
    let t = w.text(part)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let page = (0..ix.nodes.len()).rev().find(|&i| ix.local(t, i) == "Page");
    Ok(page.and_then(|p| ix.attr(t, p, "AppliedMaster")).filter(|m| *m != "n").map(str::to_owned))
}
