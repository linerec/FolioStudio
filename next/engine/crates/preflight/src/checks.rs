//! Shared preflight state, rule order and finding aggregation.

mod fonts;
mod graphics;
mod margins;
mod pages;
mod paint;
mod spots;
mod text;
mod transparency;
mod visibility;

use crate::colors::InkTable;
use crate::geom::{Doc, Placed};
use crate::messages::Message;
use crate::{Finding, Options, Severity};
use folio_links::LinkReport;
use folio_model::layout::DocumentLayout;
use folio_model::{Document, ItemKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub(super) struct Ctx<'d> {
    doc: &'d Document,
    layout: &'d DocumentLayout,
    pub(super) links: &'d LinkReport,
    pub(super) o: &'d Options,
    pub(super) g: &'d Doc<'d>,
    inks: InkTable<'d>,
    pub(super) findings: Vec<Finding>,
    pub(super) limitations: Vec<String>,
    /// Graphic id → effective resolution (see [`crate::ImageFact`]).
    pub(super) images: BTreeMap<String, crate::ImageFact>,
    /// (rule, page, subject) → finding index, for aggregated findings.
    groups: HashMap<(String, Option<usize>, String), usize>,
    /// Story id → text frame item indices in thread order.
    story_frames: HashMap<&'d str, Vec<usize>>,
    /// Story id → laid character ranges (start, end, frame item index), sorted.
    laid: HashMap<&'d str, Vec<(usize, usize, usize)>>,
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Where a run of text is: frame item indices whose lines show part of `[start, end)`.
fn frames_showing(laid: &[(usize, usize, usize)], start: usize, end: usize) -> Vec<usize> {
    let mut out: Vec<usize> = laid.iter().filter(|(s, e, _)| *s < end && start < *e).map(|x| x.2).collect();
    out.dedup();
    out
}

fn visible_text(s: &str) -> bool {
    s.chars().any(|c| !c.is_whitespace() && c != '\u{FFFC}' && !c.is_control())
}

impl<'d> Ctx<'d> {
    pub(super) fn new(g: &'d Doc<'d>, doc: &'d Document, layout: &'d DocumentLayout, links: &'d LinkReport, o: &'d Options) -> Self {
        let mut story_frames: HashMap<&'d str, Vec<usize>> = HashMap::new();
        for (i, p) in g.items.iter().enumerate() {
            if let ItemKind::TextFrame(tf) = &p.item.kind {
                story_frames.entry(tf.story.as_str()).or_default().push(i);
            }
        }
        // thread order: head (no previous in the set), then `next`
        for frames in story_frames.values_mut() {
            if frames.len() < 2 {
                continue;
            }
            let ids: HashMap<&str, usize> = frames.iter().map(|&i| (g.items[i].item.id.as_str(), i)).collect();
            let tf = |i: usize| match &g.items[i].item.kind {
                ItemKind::TextFrame(t) => Some(t),
                _ => None,
            };
            let head = frames.iter().copied().find(|&i| tf(i).and_then(|t| t.previous.as_deref()).is_none_or(|p| !ids.contains_key(p)));
            let mut ordered = Vec::with_capacity(frames.len());
            let mut cur = head;
            while let Some(c) = cur {
                if ordered.contains(&c) {
                    break;
                }
                ordered.push(c);
                cur = tf(c).and_then(|t| t.next.as_deref()).and_then(|n| ids.get(n).copied());
            }
            for &f in frames.iter() {
                if !ordered.contains(&f) {
                    ordered.push(f);
                }
            }
            *frames = ordered;
        }
        let mut laid: HashMap<&'d str, Vec<(usize, usize, usize)>> = HashMap::new();
        for (fid, ft) in &layout.frames {
            let Some(&fi) = g.by_id.get(fid.as_str()) else { continue };
            let ItemKind::TextFrame(tf) = &g.items[fi].item.kind else { continue };
            let v = laid.entry(tf.story.as_str()).or_default();
            for l in &ft.lines {
                v.push((l.start, l.end.max(l.start + 1), fi));
            }
        }
        for v in laid.values_mut() {
            v.sort_unstable();
        }
        Ctx {
            doc,
            layout,
            links,
            o,
            inks: InkTable::new(doc),
            g,
            findings: Vec::new(),
            limitations: Vec::new(),
            images: BTreeMap::new(),
            groups: HashMap::new(),
            story_frames,
            laid,
        }
    }

    pub(super) fn run(&mut self) {
        self.check_fonts();
        self.check_missing_glyphs();
        self.check_links_and_images();
        self.check_item_colors();
        self.check_text();
        self.check_spots();
        self.check_overset();
        self.check_bleed_and_safety();
        self.check_visibility();
        self.check_empty_frames();
        self.check_transparency();
        self.check_placed_fonts();
        self.check_blank_pages();
        self.check_document();
        if self.o.image_ink {
            crate::image_ink::check(self);
        }
    }

    // -----------------------------------------------------------------------------------
    // Output helpers

    fn page_name(&self, page: Option<usize>) -> Option<String> {
        page.and_then(|p| self.g.page_box(p)).map(|b| b.name.clone())
    }

    pub(super) fn push(&mut self, rule: &str, severity: Severity, page: Option<usize>, item: Option<&str>, message: Message, data: Value) {
        // items on masters: the master page's name (e.g. "A"), with no document page index
        let page_name = self.page_name(page).or_else(|| {
            let p = &self.g.items[*self.g.by_id.get(item?)?];
            let s = &self.g.spreads[p.spread];
            if !s.master {
                return None;
            }
            s.pages.iter().find(|b| Some(b.index) == p.page).map(|b| b.name.clone())
        });
        self.findings.push(Finding { rule: rule.into(), severity, page, page_name, item: item.map(str::to_owned), message: message.text, message_id: message.id, message_args: message.args, data });
    }

    /// One finding per (rule, page, subject); later items are added to `data.items`.
    // Each argument supplies a separate finding field or its lazy payload.
    #[allow(clippy::too_many_arguments)]
    fn push_grouped(&mut self, rule: &str, severity: Severity, page: Option<usize>, subject: &str, item: &str, message: impl FnOnce() -> Message, data: impl FnOnce() -> Value) {
        let key = (rule.to_owned(), page, subject.to_owned());
        if let Some(&i) = self.groups.get(&key) {
            let f = &mut self.findings[i];
            if let Some(items) = f.data.get_mut("items").and_then(Value::as_array_mut)
                && !items.iter().any(|v| v.as_str() == Some(item))
            {
                items.push(json!(item));
            }
            if let Some(c) = f.data.get_mut("count") {
                *c = json!(c.as_u64().unwrap_or(1) + 1);
            }
            if f.severity > severity {
                f.severity = severity;
            }
            return;
        }
        let mut d = data();
        if let Value::Object(m) = &mut d {
            m.insert("items".into(), json!([item]));
            m.insert("count".into(), json!(1));
        }
        self.groups.insert(key, self.findings.len());
        self.push(rule, severity, page, Some(item), message(), d);
    }

    /// Page to report an item on (document page; masters and pasteboard → None).
    fn page_of(&self, p: &Placed<'_>) -> Option<usize> {
        self.g.doc_page(p)
    }

    /// Items the print checks look at: printing, on a page (or anywhere when the pasteboard
    /// is included).
    fn checked(&self, p: &Placed<'_>) -> bool {
        crate::scope::checked(p,self.g,self.doc,self.o)
    }

    fn master_data(&self, p: &Placed<'_>) -> Value {
        match self.g.master_name(p) {
            Some(m) => json!(m),
            None => Value::Null,
        }
    }
}
