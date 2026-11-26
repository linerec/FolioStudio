//! Paragraph, character and object styles (Resources/Styles.xml) and attribute resolution.
//!
//! Text attributes resolve bottom-up: the root paragraph style "[No paragraph style]" (which
//! defines every attribute) → the applied paragraph style's `BasedOn` chain → the applied
//! style → the ParagraphStyleRange's own overrides. Character attributes start from the same
//! paragraph layers (paragraph styles carry PointSize, AppliedFont, Leading, …), then the root
//! character style → the CharacterStyle `BasedOn` chain → the CharacterStyleRange overrides.
//! Each layer only sets the attributes it defines.
//!
//! Object styles cascade the same way from "[None]"; a style contributes a category (fill,
//! stroke, text frame options, wrap, transparency) only when its `Enable…` flag allows it.

mod character;
mod object;
mod paragraph;
mod table;

pub use character::{CharState, apply_char};
pub use object::{Cat, ObjectProps};
pub use paragraph::{ListRaw, ParaBase, apply_para, stroke_type_name};

use crate::props::{PropMap, Props, clean_name, is_none_ref};
use crate::warn::Warnings;
use crate::xml::Element;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub const ROOT_PARAGRAPH: &str = "ParagraphStyle/$ID/[No paragraph style]";
pub const ROOT_CHARACTER: &str = "CharacterStyle/$ID/[No character style]";
pub const ROOT_OBJECT: &str = "ObjectStyle/$ID/[None]";
const MAX_CHAIN: usize = 64;

struct StyleDef<'a> {
    el: &'a Element<'a>,
    props: PropMap<'a>,
    based_on: Option<String>,
    name: String,
}

/// `BasedOn` may be a full id (`ParagraphStyle/가격`) or a bare `$ID/[No paragraph style]`.
fn qualify(kind: &str, r: &str) -> String {
    if r.starts_with(kind) && r[kind.len()..].starts_with('/') { r.to_owned() } else { format!("{kind}/{r}") }
}

fn collect<'a>(roots: &[&'a Element<'a>], kind: &str, w: &mut Warnings) -> HashMap<&'a str, StyleDef<'a>> {
    let mut found = Vec::new();
    for r in roots {
        r.descendants_named(kind, &mut found);
    }
    let mut map = HashMap::with_capacity(found.len());
    for el in found {
        let Some(id) = el.attr("Self") else { continue };
        let props = PropMap::of(el);
        let based_on = (&props)
            .str("BasedOn")
            .filter(|b| !is_none_ref(b))
            .map(|b| qualify(kind, b));
        let name = clean_name(el.attr("Name").unwrap_or(id)).to_owned();
        if map.insert(id, StyleDef { el, props, based_on, name }).is_some() {
            w.add(format!("duplicate style id {id}"));
        }
    }
    map
}

pub struct StyleSheet<'a> {
    para: HashMap<&'a str, StyleDef<'a>>,
    chr: HashMap<&'a str, StyleDef<'a>>,
    obj: HashMap<&'a str, StyleDef<'a>>,
    tbl: HashMap<&'a str, StyleDef<'a>>,
    cell: HashMap<&'a str, StyleDef<'a>>,
    // Resolved once per style and shared by the reading threads.
    para_cache: Mutex<HashMap<String, Arc<ParaBase>>>,
    obj_cache: Mutex<HashMap<String, Arc<Vec<&'a str>>>>,
}

impl<'a> StyleSheet<'a> {
    pub fn new(roots: &[&'a Element<'a>], w: &mut Warnings) -> Self {
        let sheet = StyleSheet {
            para: collect(roots, "ParagraphStyle", w),
            chr: collect(roots, "CharacterStyle", w),
            obj: collect(roots, "ObjectStyle", w),
            tbl: collect(roots, "TableStyle", w),
            cell: collect(roots, "CellStyle", w),
            para_cache: Mutex::default(),
            obj_cache: Mutex::default(),
        };
        if !sheet.para.contains_key(ROOT_PARAGRAPH) {
            w.add("root paragraph style [No paragraph style] missing; using model defaults");
        }
        sheet
    }

    /// Style chain root-most first, excluding the root style itself.
    fn chain<'s>(map: &'s HashMap<&'a str, StyleDef<'a>>, id: &str, root: &str, w: &mut Warnings) -> Vec<&'s StyleDef<'a>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = Some(id.to_owned());
        while let Some(c) = cur {
            if c == root {
                break;
            }
            if !seen.insert(c.clone()) || out.len() >= MAX_CHAIN {
                w.add(format!("style BasedOn cycle at {c}"));
                break;
            }
            match map.get(c.as_str()) {
                Some(d) => {
                    out.push(d);
                    cur = d.based_on.clone();
                }
                None => {
                    w.add(format!("missing style {c}"));
                    break;
                }
            }
        }
        out.reverse();
        out
    }
}

pub const ROOT_TABLE: &str = "TableStyle/$ID/[No table style]";
pub const ROOT_CELL: &str = "CellStyle/$ID/[None]";
