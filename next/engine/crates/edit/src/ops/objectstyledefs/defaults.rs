//! Default Text Frame Style and Default Graphic Frame Style of the Object Styles panel menu (oracle `objectstyles` os-default-*).
//! Measured by the panel menu's own actions (`app.menuActions`): the document keeps them in `Resources/Preferences.xml`:
//! - Default Text Frame Style is `PageItemDefault/@AppliedTextObjectStyle`, Default Graphic Frame Style `@AppliedGraphicObjectStyle`
//!   (os-default-a-none, os-default-b-text). The choices are `[None]`, `[Basic Graphics Frame]` and `[Basic Text Frame]` (the document's
//!   `[None]`, `[Normal Graphics Frame]` and `[Normal Text Frame]`); a style of the document is taken the same way.
//! - Choosing a default text frame style also sets the text default paragraph style (`TextDefault/@AppliedParagraphStyle`): choosing
//!   `[Basic Text Frame]` when it is the default already took `NormalParagraphStyle` to `[No paragraph style]` (os-default-a-text), as did
//!   `[None]` (os-default-a-none). Both have no paragraph style of their own in force (`EnableParagraphStyle` off); a style that has one
//!   in force would give its own: that case is not measured.
use super::*;

const PREFS: &str = "Resources/Preferences.xml";
const NO_PARAGRAPH_STYLE: &str = "ParagraphStyle/$ID/[No paragraph style]";

/// Sets the default object style of new text frames (`kind` "text") or of new graphic frames ("graphic").
pub(super) fn set_default(w: &mut Work<'_>, kind: &String, style: &str) -> Result<(), EditError> {
    let attr = match kind.as_str() {
        "text" => "AppliedTextObjectStyle",
        "graphic" => "AppliedGraphicObjectStyle",
        other => return Err(EditError::new("invalid", format!("default frame style of {other}"))),
    };
    let (t, ix) = scan(w)?;
    let s = find(&t, &ix, style)?;
    let paragraph = if ix.attr(&t, s, "EnableParagraphStyle") == Some("true") {
        ix.value(&t, s, "AppliedParagraphStyle").unwrap_or_else(|| NO_PARAGRAPH_STYLE.to_owned())
    } else {
        NO_PARAGRAPH_STYLE.to_owned()
    };
    let prefs = w.text(PREFS)?.to_owned();
    let px = Index::scan(&prefs).map_err(|e| EditError::new("xml", e))?;
    let item = (0..px.nodes.len()).find(|&i| px.local(&prefs, i) == "PageItemDefault").ok_or_else(|| EditError::new("xml", "no PageItemDefault"))?;
    let mut edits = Vec::new();
    if px.value(&prefs, item, attr).as_deref() != Some(style) {
        edits.push(attr_edit(&px, &prefs, item, attr, style));
    }
    if kind == "text" {
        let text = (0..px.nodes.len()).find(|&i| px.local(&prefs, i) == "TextDefault").ok_or_else(|| EditError::new("xml", "no TextDefault"))?;
        if px.value(&prefs, text, "AppliedParagraphStyle").as_deref() != Some(paragraph.as_str()) {
            edits.push(attr_edit(&px, &prefs, text, "AppliedParagraphStyle", &paragraph));
        }
    }
    if edits.is_empty() { Ok(()) } else { w.splice(PREFS, edits) }
}

impl crate::Editor {
    /// The default object styles of new frames (`Self`s): text frames, graphic frames.
    pub fn default_frame_styles(&self) -> (Option<String>, Option<String>) {
        let Some(prefs) = self.parts.get(PREFS) else { return (None, None) };
        let Ok(px) = Index::scan(prefs) else { return (None, None) };
        let Some(item) = (0..px.nodes.len()).find(|&i| px.local(prefs, i) == "PageItemDefault") else { return (None, None) };
        (px.value(prefs, item, "AppliedTextObjectStyle"), px.value(prefs, item, "AppliedGraphicObjectStyle"))
    }
}
