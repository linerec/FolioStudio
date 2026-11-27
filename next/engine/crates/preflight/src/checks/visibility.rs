//! Hidden, non-printing and empty items.

use super::{Ctx, visible_text};
use crate::geom::Placed;
use crate::messages::{msg, term};
use crate::rules::{HIDDEN_CONTENT, EMPTY_FRAME};
use crate::Severity;
use folio_model::{Document, ItemKind, Paint};
use serde_json::json;
use std::collections::BTreeMap;

impl Ctx<'_> {
    pub(super) fn check_visibility(&mut self) {
        // layers
        let mut per_layer: BTreeMap<&str, usize> = BTreeMap::new();
        for p in &self.g.items {
            if let Some(l) = p.layer
                && (!l.visible || !l.printable)
                && p.parent.is_none()
                && !p.anchored
            {
                *per_layer.entry(l.id.as_str()).or_default() += 1;
            }
        }
        for (lid, n) in per_layer {
            let Some(l) = self.doc.layers.iter().find(|l| l.id == lid) else { continue };
            let why = if !l.visible { "숨긴 레이어" } else { "인쇄하지 않는 레이어" };
            let name = l.name.clone();
            self.push(HIDDEN_CONTENT, Severity::Info, None, Some(lid), msg("preflight.hidden.layer", format!("{why} '{name}'에 개체 {n}개"), json!({"why":if !l.visible { term("preflight.hiddenLayer") } else { term("preflight.nonprintingLayer") },"name":name,"count":n})), json!({"layer": lid, "name": name, "visible": l.visible, "printable": l.printable, "items": n}));
        }
        // items (not already explained by their layer or a hidden parent)
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            let parent_hidden = p.parent.is_some_and(|q| !self.g.items[q].visible || self.g.items[q].nonprinting);
            if parent_hidden || !p.layer_prints {
                continue;
            }
            let hidden = !p.item.visible;
            let nonprinting = p.item.print.nonprinting;
            if !hidden && !nonprinting {
                continue;
            }
            if !has_content(p, self.doc) {
                continue;
            }
            let page = self.page_of(p);
            let id = p.item.id.clone();
            let master = self.master_data(p);
            let why = if hidden { "숨긴 개체" } else { "인쇄하지 않는 개체" };
            self.push(HIDDEN_CONTENT, Severity::Info, page, Some(&id), msg("preflight.hidden.item", format!("{why}: 출력되지 않음"), json!({"why":if hidden { term("preflight.hiddenObject") } else { term("preflight.nonprintingObject") }})), json!({"hidden": hidden, "nonprinting": nonprinting, "master": master}));
        }
    }

    pub(super) fn check_empty_frames(&mut self) {
        for i in 0..self.g.items.len() {
            let p = &self.g.items[i];
            if !self.checked(p) {
                continue;
            }
            let item = p.item;
            let no_paint = item.fill == Paint::None && (item.stroke.paint == Paint::None || item.stroke.weight <= 0.0);
            let empty = match &item.kind {
                ItemKind::Rectangle { content: None } | ItemKind::Oval { content: None } | ItemKind::Polygon { content: None } => item.clipped.is_empty() && no_paint,
                ItemKind::TextFrame(tf) => {
                    no_paint && self.doc.stories.get(&tf.story).is_none_or(|s| !s.paragraphs.iter().flat_map(|p| &p.runs).any(|r| visible_text(&r.text) || !r.inline.is_empty()))
                }
                _ => false,
            };
            if empty {
                let page = self.page_of(p);
                let id = item.id.clone();
                let master = self.master_data(p);
                let what = if matches!(item.kind, ItemKind::TextFrame(_)) { "빈 텍스트 프레임" } else { "빈 프레임" };
                self.push(EMPTY_FRAME, Severity::Info, page, Some(&id), msg("preflight.frame.empty", what.into(), json!({"what":if matches!(item.kind, ItemKind::TextFrame(_)) { term("preflight.emptyTextFrame") } else { term("preflight.emptyFrame") }})), json!({"text": matches!(item.kind, ItemKind::TextFrame(_)), "master": master}));
            }
        }
    }
}

/// An item that would draw something: paint, a graphic, pasted content, text, children.
fn has_content(p: &Placed<'_>, doc: &Document) -> bool {
    let i = p.item;
    if i.fill != Paint::None || (i.stroke.paint != Paint::None && i.stroke.weight > 0.0) || !i.clipped.is_empty() {
        return true;
    }
    match &i.kind {
        ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => content.is_some(),
        ItemKind::TextFrame(tf) => doc.stories.get(&tf.story).is_some_and(|s| s.paragraphs.iter().flat_map(|p| &p.runs).any(|r| visible_text(&r.text))),
        ItemKind::Group { children } => !children.is_empty(),
        ItemKind::Other { .. } => true,
        ItemKind::GraphicLine => false,
    }
}
