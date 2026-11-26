//! Document-order list labels shared by composition and text conversion.
use super::{DocumentFrames, listnumbers::NumberingState};
use folio_model::{Document, ItemKind, ListKind, PageItem, Paragraph};
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Labels = BTreeMap<usize, String>;
#[derive(Default)]
pub(super) struct Plan(pub BTreeMap<String, Labels>);
type Counters = BTreeMap<String, NumberingState>;

impl Plan {
    pub fn new(doc: &Document, frames: &DocumentFrames<'_>) -> Self {
        let mut builder = Builder { doc, plan: Self::default(), seen: BTreeSet::new(), shared: Counters::new() };
        let mut roots: Vec<_> = frames.chains.iter().filter_map(|(id, chain)| {
            let f = frames.frame(chain.first()?)?;
            let si = f.spread?;
            let order = doc.story_order.iter().position(|s| s == id).unwrap_or(usize::MAX);
            Some(((si, f.page_in_spread.unwrap_or(usize::MAX), order, *id), *id))
        }).collect();
        roots.sort_by_key(|(key, _)| *key);
        let mut domain = None;
        for ((si, _, _, _), id) in roots {
            // Parent spreads have their own numbering sequences.
            let next = if si < doc.spreads.len() { 0 } else { si + 1 };
            if domain != Some(next) { builder.shared.clear(); domain = Some(next); }
            builder.story(id);
        }
        for id in doc.stories.keys() {
            if !builder.seen.contains(id) { builder.shared.clear(); builder.story(id); }
        }
        builder.plan
    }
}

struct Builder<'a> { doc: &'a Document, plan: Plan, seen: BTreeSet<String>, shared: Counters }
impl Builder<'_> {
    fn story(&mut self, id: &str) {
        if !self.seen.insert(id.to_owned()) { return; }
        if let Some(story) = self.doc.stories.get(id) {
            self.text(id, &story.paragraphs, &mut Counters::new());
        }
    }
    fn text(&mut self, id: &str, paragraphs: &[Paragraph], local: &mut Counters) {
        let mut labels = Labels::new();
        let mut at = 0;
        for p in paragraphs {
            if let Some(l) = p.attrs.list.as_ref().filter(|_| p.runs.iter().flat_map(|r|r.text.chars()).next() != Some('\u{16}')) {
                let value = match l.kind {
                    ListKind::Bullet => format!("{}{}", l.bullet, l.bullet_text_after),
                    ListKind::Numbered => {
                        let across = !l.list.is_empty() && self.doc.numbering_lists.iter().any(|n| n.name == l.list && n.across_stories);
                        let counters = if across { &mut self.shared } else { &mut *local };
                        counters.entry(l.list.clone()).or_default().label(l)
                    }
                };
                labels.insert(at, value);
            }
            for run in &p.runs {
                for item in &run.inline { self.inline(item); }
                for table in &run.tables {
                    let mut cells: Vec<_> = table.cells.iter().collect();
                    cells.sort_by_key(|c| (c.row, c.column));
                    for cell in cells { self.text(&cell.id, &cell.paragraphs, local); }
                }
            }
            at += p.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() + 1;
        }
        self.plan.0.insert(id.to_owned(), labels);
    }
    fn inline(&mut self, item: &PageItem) {
        match &item.kind {
            ItemKind::TextFrame(frame) => self.story(&frame.story),
            ItemKind::Group { children } => for child in children { self.inline(child); },
            _ => {}
        }
    }
}
