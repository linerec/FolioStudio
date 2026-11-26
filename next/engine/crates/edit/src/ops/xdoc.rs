//! Paste of objects copied in another document: the swatches and styles they use come along, as
//! InDesign 2025 brings them (window documents; oracle basic-xdoc, fixture basic-other):
//! - a swatch the document lacks is added as the other document defines it — in its sorted
//!   place in Graphic.xml, with a new last Swatches panel slot (`SwatchColorGroupReference`),
//!   a spot colour with its Ink (the next TrapOrder) — xdoc-card, xdoc-spot;
//! - a swatch whose name the document has with other values comes in as "<name> 2" (the first
//!   free number) and the pasted objects use it — xdoc-blue; with equal values the document's
//!   own is used;
//! - a style (paragraph, character, object, cell, table) the document lacks is added at the end
//!   of its root group; a style the document has keeps the document's definition —
//!   xdoc-text, xdoc-basic-para;
//! - what comes in follows the same rules for its own references (a style's swatch, a style's
//!   based-on), in the order the pasted objects and their stories name them;
//! - layers do not come: the objects go on the active layer, as any paste (xdoc-two-layers).
use std::collections::BTreeMap;
use std::ops::Range;

use super::clipboard::Source;
use super::*;
use crate::xml::{escape, unescape};

const GRAPHIC: &str = "Resources/Graphic.xml";
const STYLES: &str = "Resources/Styles.xml";
const SWATCHES: [&str; 5] = ["Color", "Tint", "Gradient", "MixedInk", "MixedInkGroup"];
const STYLE_KINDS: [&str; 5] = ["ParagraphStyle", "CharacterStyle", "ObjectStyle", "CellStyle", "TableStyle"];
pub(crate) fn bring_resources(w: &mut Work<'_>, parts: &BTreeMap<String,String>, xml: &str) -> Result<String, EditError> {
    let source = Source::of(&crate::Editor::new(parts.clone()));
    Importer::new(&source).bring(w, xml)
}
/// Attributes that do not define a swatch (its identity and its place in the panel).
const NOT_DEFINING: [&str; 7] = ["Self", "Name", "SwatchColorGroupReference", "SwatchCreatorID", "ColorEditable", "ColorRemovable", "Visible"];

/// Brings another document's resources into this one while its objects are pasted.
pub(super) struct Importer<'s> {
    src: &'s Source,
    /// Source reference → the reference in this document.
    done: BTreeMap<String, String>,
    /// Book synchronization: nested ids (gradient stops…) of what is added are new, so none collides with the document's.
    book: bool,
}

/// The swatch and style references in `xml`: attribute values (`FillColor="Color/x"`) and
/// element texts (`<BasedOn type="object">ParagraphStyle/x</BasedOn>`), with their ranges.
fn references(xml: &str) -> Vec<(Range<usize>, String)> {
    let b = xml.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' || c == b'>' {
            let end_ch = if c == b'"' { b'"' } else { b'<' };
            let start = i + 1;
            let rest = &xml[start..];
            if SWATCHES.iter().chain(STYLE_KINDS.iter()).any(|k| rest.starts_with(k) && rest[k.len()..].starts_with('/'))
                && let Some(len) = rest.bytes().position(|x| x == end_ch)
            {
                out.push((start..start + len, xml[start..start + len].to_owned()));
                i = start + len;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// An element's text in a part (its whole subtree).
fn element_text(t: &str, ix: &Index, i: usize) -> String {
    t[ix.nodes[i].start..ix.nodes[i].end].to_owned()
}

/// The element's own attributes that define it (for comparing two swatches).
fn defining(t: &str, ix: &Index, i: usize) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = ix.attrs(t, i).iter()
        .map(|a| (t[a.name.0..a.name.1].to_owned(), t[a.value.0..a.value.1].to_owned()))
        .filter(|(k, _)| !NOT_DEFINING.contains(&k.as_str()))
        .collect();
    v.sort();
    // children (gradient stops, mixed inks) as they are
    let n = &ix.nodes[i];
    if !n.empty {
        v.push((String::from("#children"), t[n.open_end..n.close_start].trim().to_owned()));
    }
    v
}

impl<'s> Importer<'s> {
    /// Book > Synchronize, Swatches (oracle `bookx-c3-swatch`, InDesign 2025): every swatch the style source has goes in by
    /// name. A swatch the document has is replaced by the source's (a spot colour's Ink too, the document's trap order
    /// kept); one it lacks is added (a spot colour with its Ink at the next trap order); one only the document has stays.
    /// All kinds go: process, spot, RGB, LAB colours, tints, gradients, mixed inks and their groups. The special swatches
    /// ([Paper] is editable but never carried; [Black], [Registration] and the hidden process inks cannot be edited)
    /// and the hidden unnamed ones stay the document's own.
    pub(super) fn book_colors(&mut self, w: &mut Work<'_>) -> Result<(), EditError> {
        self.book = true;
        let st = self.src.text(GRAPHIC)?.to_owned();
        let sx = Index::scan(&st).map_err(|e| EditError::new("xml", e))?;
        let wanted: Vec<usize> = (0..sx.nodes.len()).filter(|&i| SWATCHES.contains(&sx.local(&st, i)) && sx.attr(&st, i, "Self").is_some()).collect();
        // what the document has stands as itself while the others are replaced or added (a tint's base colour…)
        {
            let (target, tx) = swatches::scan(w, GRAPHIC)?;
            for &i in &wanted {
                let id = sx.attr(&st, i, "Self").unwrap_or_default();
                if tx.by_self(&target, id).is_some() { self.preset(id, id); }
            }
        }
        for i in wanted {
            let kind = sx.local(&st, i);
            let name = sx.attr(&st, i, "Name").unwrap_or_default();
            if name.starts_with("$ID/") || sx.attr(&st, i, "ColorEditable") == Some("false")
                || sx.attr(&st, i, "ColorOverride").is_some_and(|o| o.starts_with("Special")) { continue; }
            let Some(id) = sx.attr(&st, i, "Self") else { continue };
            let (target, tx) = swatches::scan(w, GRAPHIC)?;
            if let Some(j) = tx.by_self(&target, id) {
                let mut xml = self.reuse_children(w, &element_text(&st, &sx, i), &element_text(&target, &tx, j))?;
                super::book_resources::keep_slot(&mut xml, &target, &tx, j)?;
                w.splice(GRAPHIC, vec![(tx.nodes[j].start..tx.nodes[j].end, xml)])?;
                if kind == "Color" && sx.attr(&st, i, "Model") == Some("Spot") { self.replace_ink(w, name)?; }
            } else { self.swatch(w, kind, id)?; }
        }
        Ok(())
    }

    /// The replaced element's nested ids (gradient stops…) stay when its shape is the same, so a repeated synchronization writes the same bytes.
    fn reuse_children(&mut self, w: &mut Work<'_>, xml: &str, old: &str) -> Result<String, EditError> {
        let nested = |t: &str| -> Result<Vec<String>, EditError> {
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            Ok((1..ix.nodes.len()).filter_map(|j| ix.attr(t, j, "Self").map(str::to_owned)).collect())
        };
        let (ids, new_ids) = (nested(old)?, nested(xml)?);
        if ids.len() != new_ids.len() { return self.fresh_children(w, xml); }
        let ex = Index::scan(xml).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        let mut at = 0;
        for j in 1..ex.nodes.len() {
            if ex.attr(xml, j, "Self").is_some() { edits.push(attr_edit(&ex, xml, j, "Self", &crate::xml::unescape(&ids[at]))); at += 1; }
        }
        let mut out = xml.to_owned();
        for (range, value) in edits.into_iter().rev() { out.replace_range(range, &value); }
        Ok(out)
    }

    /// The element with a new Self for every nested element that has one (gradient stops…), so nothing collides.
    fn fresh_children(&mut self, w: &mut Work<'_>, xml: &str) -> Result<String, EditError> {
        let ex = Index::scan(xml).map_err(|e| EditError::new("xml", e))?;
        let mut edits = Vec::new();
        for j in 1..ex.nodes.len() {
            if ex.attr(xml, j, "Self").is_some() { edits.push(attr_edit(&ex, xml, j, "Self", &w.new_id())); }
        }
        let mut out = xml.to_owned();
        for (range, value) in edits.into_iter().rev() { out.replace_range(range, &value); }
        Ok(out)
    }

    /// A spot colour the document has: its Ink is the source's, with the document's own trap order.
    fn replace_ink(&mut self, w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
        let st = self.src.text(GRAPHIC)?.to_owned();
        let sx = Index::scan(&st).map_err(|e| EditError::new("xml", e))?;
        let ink = escape(&swatches::self_of("Ink", &unescape(name)));
        let Some(si) = sx.by_self(&st, &ink) else { return Ok(()) };
        let (tt, tx) = swatches::scan(w, GRAPHIC)?;
        let Some(ti) = tx.by_self(&tt, &ink) else { return Ok(()) };
        let mut e = element_text(&st, &sx, si);
        if let Some(order) = tx.attr(&tt, ti, "TrapOrder") { e = set_attr(&e, "TrapOrder", order)?; }
        w.splice(GRAPHIC, vec![(tx.nodes[ti].start..tx.nodes[ti].end, e)])
    }
    pub(super) fn new(src: &'s Source) -> Self {
        Importer { src, done: BTreeMap::new(), book: false }
    }

    /// `from` (a reference in the other document) already stands as `to` in this one (Load Styles decided it).
    pub(super) fn preset(&mut self, from: &str, to: &str) {
        self.done.insert(from.to_owned(), to.to_owned());
    }

    /// A pasted object's XML (new ids given, its stories already copied in) with what it and
    /// then its stories name resolved in this document.
    pub(super) fn bring(&mut self, w: &mut Work<'_>, xml: &str) -> Result<String, EditError> {
        let out = self.rewrite(w, xml)?;
        let ix = Index::scan(&out).map_err(|e| EditError::new("xml", e))?;
        let mut stories: Vec<String> = Vec::new();
        for k in 0..ix.nodes.len() {
            if let Some(s) = ix.attr(&out, k, "ParentStory") && !stories.iter().any(|x| x == s) {
                stories.push(s.to_owned());
            }
        }
        for s in stories {
            let part = format!("Stories/Story_{s}.xml");
            let Ok(t) = w.text(&part) else { continue };
            let t = t.to_owned();
            let new = self.rewrite(w, &t)?;
            if new != t {
                w.splice(&part, vec![(0..t.len(), new)])?;
            }
        }
        Ok(out)
    }

    /// `xml` with every swatch and style it names resolved in this document (brought in when
    /// needed).
    pub(super) fn rewrite(&mut self, w: &mut Work<'_>, xml: &str) -> Result<String, EditError> {
        let refs = references(xml);
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for (range, r) in refs {
            let to = self.resolve(w, &r)?;
            if to != r {
                edits.push((range, to));
            }
        }
        let mut out = xml.to_owned();
        for (range, to) in edits.into_iter().rev() {
            out.replace_range(range, &to);
        }
        Ok(out)
    }

    fn resolve(&mut self, w: &mut Work<'_>, r: &str) -> Result<String, EditError> {
        if let Some(to) = self.done.get(r) {
            return Ok(to.clone());
        }
        let kind = r.split('/').next().unwrap_or_default();
        let to = if r.contains("$ID/") {
            r.to_owned()
        } else if SWATCHES.contains(&kind) {
            self.swatch(w, kind, r)?
        } else {
            self.style(w, kind, r)?
        };
        self.done.insert(r.to_owned(), to.clone());
        Ok(to)
    }

    fn swatch(&mut self, w: &mut Work<'_>, kind: &str, r: &str) -> Result<String, EditError> {
        let Ok(st) = self.src.text(GRAPHIC) else { return Ok(r.to_owned()) };
        let st = st.to_owned();
        let sx = Index::scan(&st).map_err(|e| EditError::new("xml", e))?;
        let Some(si) = sx.by_self(&st, r) else { return Ok(r.to_owned()) };
        let name = unescape(sx.attr(&st, si, "Name").unwrap_or_default());
        let (tt, tx) = swatches::scan(w, GRAPHIC)?;
        // unnamed colours (InDesign's hidden black…) match by definition, whatever their id
        let unnamed = name == "$ID/";
        let same_def = |i: usize| defining(&tt, &tx, i) == defining(&st, &sx, si);
        if unnamed
            && let Some(i) = swatches::of_kind(&tx, &tt, kind).into_iter().find(|&i| tx.attr(&tt, i, "Name") == Some("$ID/") && same_def(i))
        {
            return Ok(tx.attr(&tt, i, "Self").unwrap_or(r).to_owned());
        }
        // the new Self as written in XML (escaped) and the new name (plain)
        let (new_name, new_self) = match tx.by_self(&tt, r) {
            Some(i) if same_def(i) => return Ok(r.to_owned()),
            Some(_) if unnamed => (name.clone(), format!("{kind}/{}", w.new_id())),
            Some(_) => {
                let mut n = 2;
                loop {
                    let candidate = format!("{name} {n}");
                    let me = escape(&swatches::self_of(kind, &candidate));
                    if tx.by_self(&tt, &me).is_none() {
                        break (candidate, me);
                    }
                    n += 1;
                }
            }
            None => (name.clone(), r.to_owned()),
        };
        // its own references (a tint's base colour…) resolve to the new self first
        self.done.insert(r.to_owned(), new_self.clone());
        let mut element = self.rewrite(w, &element_text(&st, &sx, si))?;
        if self.book { element = self.fresh_children(w, &element)?; }
        element = set_attr(&element, "Name", &new_name)?;
        if attr_of(&element, "SwatchColorGroupReference").is_some() && !unnamed {
            let slot = swatches::add_slot(w, &unescape(&new_self))?;
            element = set_attr(&element, "SwatchColorGroupReference", &slot)?;
        }
        swatches::insert(w, kind, &new_self, &element, "Ink")?;
        // a spot colour's Ink comes too, with the next trap order
        if kind == "Color" && attr_of(&element, "Model").as_deref() == Some("Spot") {
            let ink = escape(&swatches::self_of("Ink", &unescape(sx.attr(&st, si, "Name").unwrap_or_default())));
            if let Some(ii) = sx.by_self(&st, &ink) {
                let new_ink = swatches::self_of("Ink", &new_name);
                let (tt, tx) = swatches::scan(w, GRAPHIC)?;
                if tx.by_self(&tt, &escape(&new_ink)).is_none() {
                    // (a document with no Ink at all gets the four process inks first: File > New made it before it had them)
                    let (at, ind, order) = swatches::next_ink_slot(w)?;
                    let mut e = element_text(&st, &sx, ii);
                    e = set_attr(&e, "Self", &new_ink)?;
                    e = set_attr(&e, "Name", &new_name)?;
                    e = set_attr(&e, "TrapOrder", &order.to_string())?;
                    w.splice(GRAPHIC, vec![(at..at, format!("\n{ind}{e}"))])?;
                }
            }
        }
        Ok(new_self)
    }

    fn style(&mut self, w: &mut Work<'_>, kind: &str, r: &str) -> Result<String, EditError> {
        let t = w.text(STYLES)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        if ix.by_self(&t, r).is_some() {
            return Ok(r.to_owned()); // the document's own definition wins
        }
        let Ok(st) = self.src.text(STYLES) else { return Ok(r.to_owned()) };
        let st = st.to_owned();
        let sx = Index::scan(&st).map_err(|e| EditError::new("xml", e))?;
        let Some(si) = sx.by_self(&st, r) else { return Ok(r.to_owned()) };
        self.done.insert(r.to_owned(), r.to_owned());
        let element = self.rewrite(w, &element_text(&st, &sx, si))?;
        // at the end of its root group (after the group's last element)
        let t = w.text(STYLES)?.to_owned();
        let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
        let group = format!("Root{kind}Group");
        let g = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == group).ok_or_else(|| EditError::new("notFound", group.clone()))?;
        let at = match ix.children(g).last() {
            Some(&last) => {
                let ind = indent_before(&t, ix.nodes[last].start).to_owned();
                (ix.nodes[last].end, ind)
            }
            None => (ix.nodes[g].close_start, format!("{}\t", indent_before(&t, ix.nodes[g].start))),
        };
        w.splice(STYLES, vec![(at.0..at.0, format!("\n{}{element}", at.1))])?;
        Ok(r.to_owned())
    }
}

/// The value of an attribute of an element's start tag.
fn attr_of(element: &str, name: &str) -> Option<String> {
    let ix = Index::scan(element).ok()?;
    ix.attr(element, 0, name).map(str::to_owned)
}

/// The element with an attribute of its start tag set (added when absent).
fn set_attr(element: &str, name: &str, value: &str) -> Result<String, EditError> {
    let ix = Index::scan(element).map_err(|e| EditError::new("xml", e))?;
    let (range, text) = attr_edit(&ix, element, 0, name, value);
    let mut out = element.to_owned();
    out.replace_range(range, &text);
    Ok(out)
}
