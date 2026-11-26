//! Resolves object styles and the category-specific properties they supply.

use super::{ROOT_OBJECT, StyleSheet, qualify};
use crate::props::{Props, Val, is_none_ref};
use crate::warn::Warnings;
use crate::xml::Element;
use std::sync::Arc;

impl<'a> StyleSheet<'a> {
    /// Object style chain for lookups: applied style first, root "[None]" last.
    pub fn object_chain(&self, applied: Option<&str>, w: &mut Warnings) -> Arc<Vec<&'a str>> {
        let id = match applied {
            Some(a) if !is_none_ref(a) => qualify("ObjectStyle", a),
            _ => ROOT_OBJECT.to_owned(),
        };
        if let Some(c) = self.obj_cache.lock().ok().and_then(|c| c.get(&id).cloned()) {
            return c;
        }
        let mut ids: Vec<&'a str> = Vec::new();
        if let Some((k, _)) = self.obj.get_key_value(ROOT_OBJECT) {
            ids.push(k);
        }
        for d in Self::chain(&self.obj, &id, ROOT_OBJECT, w) {
            if let Some(s) = d.el.attr("Self") {
                ids.push(s);
            }
        }
        ids.reverse();
        let ids = Arc::new(ids);
        if let Ok(mut c) = self.obj_cache.lock() {
            c.insert(id, ids.clone());
        }
        ids
    }

    pub fn object_style(&self, id: &str) -> Option<&'a Element<'a>> {
        self.obj.get(id).map(|d| d.el)
    }
}

/// What part of an object style a property belongs to (its `Enable…` switch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cat {
    Fitting,
    Fill,
    Stroke,
    StrokeCorner,
    FrameGeneral,
    FrameBaseline,
    FrameAutoSize,
    Wrap,
    Transparency,
    Anchor,
}

fn enabled(style: &Element<'_>, cat: Cat) -> bool {
    let key = match cat {
        Cat::Fitting => "EnableFrameFittingOptions",
        Cat::Fill => "EnableFill",
        Cat::Stroke => "EnableStroke",
        Cat::StrokeCorner => "EnableStrokeAndCornerOptions",
        Cat::FrameGeneral => "EnableTextFrameGeneralOptions",
        Cat::FrameBaseline => "EnableTextFrameBaselineOptions",
        Cat::FrameAutoSize => "EnableTextFrameAutoSizingOptions",
        Cat::Wrap => "EnableTextWrapAndOthers",
        Cat::Anchor => "EnableAnchoredObjectOptions",
        Cat::Transparency => {
            return style
                .child("ObjectStyleObjectEffectsCategorySettings")
                .and_then(|c| c.attr("EnableTransparency"))
                != Some("false");
        }
    };
    style.attr(key) != Some("false")
}

/// Property lookup for a page item: the item itself, then its object style chain.
pub struct ObjectProps<'a, 's> {
    pub item: &'a Element<'a>,
    pub sheet: &'s StyleSheet<'a>,
    pub chain: Arc<Vec<&'a str>>,
}

impl<'a> ObjectProps<'a, '_> {
    fn descend(e: &'a Element<'a>, path: &[&str]) -> Option<&'a Element<'a>> {
        let mut cur = e;
        for p in path {
            cur = cur.child(p)?;
        }
        Some(cur)
    }

    /// `key` on the element at `path` below the item (or the style), e.g.
    /// `(["TextFramePreference"], "InsetSpacing")`. The first style of the chain with the
    /// category on settles it: a value it does not write is InDesign's default, not its parent's
    /// (a style based on one with 70% opacity and no TransparencySetting of its own prints at
    /// 100%: basic-styles ostyle-based-on).
    pub fn get(&self, path: &[&str], key: &str, cat: Cat) -> Option<Val<'a>> {
        if let Some(v) = Self::descend(self.item, path).and_then(|e| e.get(key)) {
            return Some(v);
        }
        for id in self.chain.iter() {
            let Some(style) = self.sheet.object_style(id) else { continue };
            if !enabled(style, cat) {
                continue;
            }
            return Self::descend(style, path).and_then(|e| e.get(key));
        }
        None
    }
    pub fn str(&self, path: &[&str], key: &str, cat: Cat) -> Option<&'a str> {
        self.get(path, key, cat)?.as_str()
    }
    pub fn f64(&self, path: &[&str], key: &str, cat: Cat) -> Option<f64> {
        self.get(path, key, cat)?.f64()
    }
    pub fn bool(&self, path: &[&str], key: &str, cat: Cat) -> Option<bool> {
        self.get(path, key, cat)?.bool()
    }
}
