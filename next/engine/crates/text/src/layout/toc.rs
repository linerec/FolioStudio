//! Paragraph locations supplied to TOC editing by the composition owner.
use super::*;
use folio_model::toc::{TocSource, TextLocation};

pub fn sources(doc: &Document, layout: &DocumentLayout) -> Vec<TocSource> {
    let mut seen = BTreeSet::new();
    locations(doc, layout).into_iter().filter(|l| l.paragraph_start)
        .filter(|l| seen.insert((l.source.story.clone(), l.source.paragraph)))
        .map(|l| l.source).collect()
}

pub fn locations(doc: &Document, layout: &DocumentLayout) -> Vec<TextLocation> {
    let frames = DocumentFrames::new(doc);
    let mut refs: Vec<_> = frames.frames.values().collect();
    refs.sort_by_key(|f| f.z);
    let mut starts = BTreeMap::new();
    let plan = super::listplan::Plan::new(doc, &frames);
    for story in doc.stories.values() {
        let mut state = TextState::new(String::new(), 1, None);
        state.list_labels = plan.0.get(&story.id).cloned();
        let mut at = 0;
        let offsets: Vec<_> = story.paragraphs.iter().map(|p| {
            let here = at;
            state.char_start = here;
            at += p.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() + 1;
            (here, state.list_label(&p.attrs).map(|v| v.0).unwrap_or_default())
        }).collect();
        starts.insert(story.id.as_str(), offsets);
    }
    let mut out = Vec::new();
    for f in refs {
        let (Some(si), Some(pi)) = (f.spread, f.page_in_spread) else { continue };
        let Some(page) = doc.spreads.get(si).and_then(|s| s.pages.get(pi)) else { continue };
        let Some(lines) = layout.frames.get(&f.item.id) else { continue };
        let Some(offsets) = starts.get(f.frame.story.as_str()) else { continue };
        if doc.toc_stories.contains_key(&f.frame.story) { continue }
        for line in lines.lines.iter().filter(|l| l.table.is_none() && l.cell.is_none()) {
            let p = match offsets.binary_search_by_key(&line.start, |v| v.0) {
                Ok(p) => p, Err(0) => continue, Err(p) => p - 1,
            };
            out.push(TextLocation { start: line.start, end: line.end, paragraph_start: offsets[p].0 == line.start,
                source: TocSource { story: f.frame.story.clone(), paragraph: p, page: page.name.clone(), page_index: page.index,
                hidden: !f.item.visible || doc.layers.iter().any(|l| Some(&l.id) == f.item.layer.as_ref() && !l.visible),
                number: offsets[p].1.clone(), book: None } });
        }
    }
    out.sort_by_key(|s| s.source.page_index);
    out
}
