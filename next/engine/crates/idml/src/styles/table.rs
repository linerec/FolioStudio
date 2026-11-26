//! Resolves property layers for table and cell styles.

use super::{ROOT_CELL, ROOT_TABLE, StyleDef, StyleSheet, qualify};
use crate::props::{PropMap, is_none_ref};
use crate::warn::Warnings;
use std::collections::HashMap;

impl<'a> StyleSheet<'a> {
    /// Property layers of a table style: the root "[No table style]" first, the applied
    /// style last; and the applied style's name.
    pub fn table_layers(&self, applied: Option<&str>, w: &mut Warnings) -> (Vec<&PropMap<'a>>, String) {
        Self::layers(&self.tbl, "TableStyle", ROOT_TABLE, applied, w)
    }

    /// Property layers of a cell style: "[None]" first, the applied style last.
    pub fn cell_layers(&self, applied: Option<&str>, w: &mut Warnings) -> (Vec<&PropMap<'a>>, String) {
        Self::layers(&self.cell, "CellStyle", ROOT_CELL, applied, w)
    }

    fn layers<'s>(
        map: &'s HashMap<&'a str, StyleDef<'a>>,
        kind: &str,
        root: &str,
        applied: Option<&str>,
        w: &mut Warnings,
    ) -> (Vec<&'s PropMap<'a>>, String) {
        let mut out: Vec<&PropMap<'a>> = Vec::new();
        if let Some(r) = map.get(root) {
            out.push(&r.props);
        }
        let id = match applied {
            Some(a) if !is_none_ref(a) => qualify(kind, a),
            _ => return (out, String::new()),
        };
        if id == root {
            return (out, String::new());
        }
        out.extend(Self::chain(map, &id, root, w).into_iter().map(|d| &d.props));
        let name = map.get(id.as_str()).map(|d| d.name.clone()).unwrap_or_default();
        (out, name)
    }
}
