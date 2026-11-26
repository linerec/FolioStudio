use super::{BTreeMap, EditError, Value, Work, format_story_range};
use std::collections::BTreeSet;

pub(crate) fn stories(w: &mut Work<'_>, ids: &[String]) -> Result<Vec<String>, EditError> {
    let mut seen = BTreeSet::new();
    let mut stories = Vec::new();
    for id in ids {
        let (part, index, item) = w.find(id)?;
        let xml = w.text(&part)?;
        let frames = std::iter::once(item).filter(|&node| {
            matches!(index.local(xml, node), "TextFrame" | "TextPath")
        });
        let paths = index.named(xml, item, "TextPath").into_iter();
        for node in frames.chain(paths) {
            if let Some(story) = index.value(xml, node, "ParentStory")
                && seen.insert(story.clone())
            {
                stories.push(story);
            }
        }
    }
    Ok(stories)
}

pub(crate) fn run(
    w: &mut Work<'_>,
    ids: &[String],
    props: &BTreeMap<String, Value>,
) -> Result<(), EditError> {
    let stories = stories(w, ids)?;
    if stories.is_empty() {
        return Err(EditError::new("notText", "no text frame or text path selected"));
    }
    for id in stories {
        format_story_range(w, &id, None, props)?;
    }
    Ok(())
}
