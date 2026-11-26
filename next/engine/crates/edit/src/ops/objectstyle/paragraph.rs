use super::*;

/// The paragraph style an object style gives a text frame's story (styles-c1, InDesign 2025 DOM, oracle os-next-*):
/// - a story threaded through more than one frame takes none (the frame is not alone: measured with the style applied to the first and
///   to a later frame, with and without "apply next style", a click and Alt-click);
/// - "apply next style" gives the first paragraph the style and every later one the next style of the one before it;
///   otherwise every paragraph takes the style. Overrides of the paragraphs are cleared either way.
pub(super) fn apply(w: &mut Work<'_>, story: &str, style: &str, threaded: bool) -> Result<(), EditError> {
    if threaded { return Ok(()) }
    let Some(styles) = Styles::load(w) else { return Ok(()) };
    if !styles.enabled(style, "EnableParagraphStyle") { return Ok(()) }
    let Some(paragraph) = styles.value(style, &[], "AppliedParagraphStyle", "EnableParagraphStyle") else { return Ok(()) };
    if paragraph.ends_with("/$ID/[No paragraph style]") { return Ok(()) }
    let next = styles.value(style, &[], "ApplyNextParagraphStyle", "EnableParagraphStyle").as_deref() == Some("true");
    crate::text::format::apply_style_to_story(w, story, &paragraph, next)
}
