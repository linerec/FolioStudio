//! Paragraph styles an index generation uses (index-c3). Measured in InDesign 2025: when the
//! document's index options name a paragraph style of the user (`doc.indexGenerationOptions.level1Style`
//! and the others), the generated paragraphs use it and no `Index …` style is created for that role;
//! the roles left at "[No paragraph style]" still get the measured `Index Title/Section Head/Level n`.
use super::invalid;
use crate::{EditError, Work, xml::fragment::*};

const NONE: &str = "ParagraphStyle/$ID/[No paragraph style]";
/// The option attributes of the roles: title, section heading, levels 1 to 4 (InDesign has no fifth level).
const ROLES: [&str; 6] = ["TitleStyle", "SectionHeadingStyle", "Level1Style", "Level2Style", "Level3Style", "Level4Style"];

fn options_node(w: &Work<'_>) -> Result<usize, EditError> {
    let t = w.text("Resources/Preferences.xml")?;
    let ix = w.index("Resources/Preferences.xml")?;
    (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "IndexOptions").ok_or_else(|| EditError::new("xml", "missing index options"))
}

/// The user style an option names; `None` when it is unset or one of the generated `Index …` styles.
pub(super) fn user_style(w: &Work<'_>, role: &str) -> Result<Option<String>, EditError> {
    let t = w.text("Resources/Preferences.xml")?;
    let ix = w.index("Resources/Preferences.xml")?;
    let i = options_node(w)?;
    Ok(ix.value(t, i, role).filter(|s| s != NONE && !s.starts_with("ParagraphStyle/$ID/Index ")))
}

/// Checks (before anything is written) that every user style the options name exists.
pub(super) fn check(w: &Work<'_>) -> Result<(), EditError> {
    for role in ROLES {
        if let Some(id) = user_style(w, role)? {
            let ix = w.index("Resources/Styles.xml")?;
            let t = w.text("Resources/Styles.xml")?;
            if ix.by_self(t, &id).is_none() { return Err(invalid(format!("색인 옵션의 문단 스타일이 없습니다: {id}"))); }
        }
    }
    Ok(())
}

/// `setIndexGenerationStyles`: the paragraph styles of the Generate Index options (title, section
/// heading, levels 1 to 4). An empty id puts the role back to "[No paragraph style]", so the next
/// generation makes the measured `Index …` style for it. Roles beyond `levels.len()` are left alone.
pub(crate) fn set(w: &mut Work<'_>, title: &String, section_heading: &String, levels: &[String]) -> Result<(), EditError> {
    if levels.len() > 4 { return Err(invalid("색인에는 4단계까지 있습니다")); }
    let wanted: Vec<&String> = [title, section_heading].into_iter().chain(levels.iter()).collect();
    let styles_ix = w.index("Resources/Styles.xml")?;
    let styles_t = w.text("Resources/Styles.xml")?.to_owned();
    for id in wanted.iter().filter(|s| !s.is_empty()) {
        if !id.starts_with("ParagraphStyle/") || styles_ix.by_self(&styles_t, id).is_none() {
            return Err(invalid(format!("문단 스타일을 찾을 수 없습니다: {id}")));
        }
    }
    super::generation_input::ensure_options(w)?;
    let part = "Resources/Preferences.xml";
    let i = options_node(w)?;
    let ix = w.index(part)?;
    let text = w.text(part)?;
    let mut raw = fragment(text, &ix, i);
    for (role, id) in ROLES.iter().zip(&wanted) {
        attribute(&mut raw, role, if id.is_empty() { NONE } else { id })?;
    }
    w.splice(part, vec![(ix.nodes[i].start..ix.nodes[i].end, raw)])
}
