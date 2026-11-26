use crate::{EditError, Work, xml::{escape, fragment::*}};

/// The paragraph style a generated paragraph of `role` (`Title`, `Section Head`, `Level n`) uses: the
/// user's own style when the options name one, else the measured `Index <role>` style.
pub(super) fn style_for(w: &mut Work<'_>, name: &str, font: &str) -> Result<String, EditError> {
    let role = match name { "Title" => "TitleStyle".to_owned(), "Section Head" => "SectionHeadingStyle".to_owned(), n => format!("{}Style", n.replace(' ', "")) };
    match super::generation_styles::user_style(w, &role)? { Some(id) => Ok(id), None => ensure(w, name, font) }
}

pub(super) fn ensure(w: &mut Work<'_>, name: &str, font: &str) -> Result<String, EditError> {
    let id = format!("ParagraphStyle/$ID/Index {name}");
    let part = "Resources/Styles.xml";
    let ix = w.index(part)?;
    let text = w.text(part)?;
    if let Some(i)=ix.by_self(text, &id) {
        let mut raw=fragment(text,&ix,i);
        property(&mut raw,"AppliedFont",font,"string")?;
        w.splice(part,vec![(ix.nodes[i].start..ix.nodes[i].end,raw)])?;
        return Ok(id);
    }
    let group = (0..ix.nodes.len()).find(|&i| ix.local(text,i) == "RootParagraphStyleGroup")
        .ok_or_else(|| EditError::new("xml", "missing paragraph styles"))?;
    let attrs = match name {
        "Title" => "FontStyle=\"Bold\" PointSize=\"30\" SpaceAfter=\"18\" Justification=\"CenterAlign\"",
        "Section Head" => "FontStyle=\"Bold\" SpaceBefore=\"7.2\" SpaceAfter=\"7.2\"",
        "Level 1" => "PointSize=\"10\" LeftIndent=\"28\" FirstLineIndent=\"-28\"",
        "Level 2" => "PointSize=\"10\" LeftIndent=\"28\" FirstLineIndent=\"-21\"",
        // measured in InDesign 2025 (index-c3): each level hangs 7 pt less
        "Level 3" => "PointSize=\"10\" LeftIndent=\"28\" FirstLineIndent=\"-14\"",
        "Level 4" => "PointSize=\"10\" LeftIndent=\"28\" FirstLineIndent=\"-7\"",
        _ => return Err(EditError::new("unsupported", "unmeasured index level")),
    };
    let new = format!("<ParagraphStyle Self=\"{}\" Name=\"$ID/Index {name}\" Imported=\"false\" NextStyle=\"{}\" SplitDocument=\"false\" EmitCss=\"true\" IncludeClass=\"true\" ExtendedKeyboardShortcut=\"0 0 0\" EmptyNestedStyles=\"true\" EmptyLineStyles=\"true\" EmptyGrepStyles=\"true\" KeyboardShortcut=\"0 0\" {attrs}><Properties><BasedOn type=\"object\">ParagraphStyle/$ID/NormalParagraphStyle</BasedOn><PreviewColor type=\"enumeration\">Nothing</PreviewColor><AppliedFont type=\"string\">{}</AppliedFont></Properties></ParagraphStyle>", escape(&id), escape(&id), escape(font));
    let at = ix.nodes[group].close_start;
    w.splice(part,vec![(at..at,new)])?;
    Ok(id)
}

pub(super) fn options(w: &mut Work<'_>, title: &str, headings: bool, replace: bool, levels: usize) -> Result<(), EditError> {
    let part = "Resources/Preferences.xml";
    let ix = w.index(part)?;
    let text = w.text(part)?;
    let i = (0..ix.nodes.len()).find(|&i| ix.local(text,i) == "IndexOptions")
        .ok_or_else(|| EditError::new("xml", "missing index options"))?;
    let mut raw = fragment(text, &ix, i);
    for (name,value) in [("Title",title.to_owned()),("IncludeSectionHeadings",headings.to_string()),("ReplaceExistingIndex",replace.to_string())] { attribute(&mut raw,name,&value)?; }
    for (name,value) in [("TitleStyle","Title"),("SectionHeadingStyle","Section Head")] {
        if name == "TitleStyle" && title.is_empty() || name == "SectionHeadingStyle" && !headings || super::generation_styles::user_style(w,name)?.is_some() {continue;}
        attribute(&mut raw,name,&format!("ParagraphStyle/$ID/Index {value}"))?;
    }
    for n in 1..=levels {
        let role = format!("Level{n}Style");
        if super::generation_styles::user_style(w,&role)?.is_some() {continue;}
        attribute(&mut raw,&role,&format!("ParagraphStyle/$ID/Index Level {n}"))?;
    }
    w.splice(part,vec![(ix.nodes[i].start..ix.nodes[i].end,raw)])
}
