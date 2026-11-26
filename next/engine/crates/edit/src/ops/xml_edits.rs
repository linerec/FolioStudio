//! XML attribute edits and element fragments.

use super::*;

pub(super) fn bool_text(v: bool) -> String {
    if v { "true".into() } else { "false".into() }
}

/// Range of an attribute's value, or where a new attribute goes (with its leading space).
pub(crate) fn attr_edit(ix: &Index, t: &str, i: usize, name: &str, value: &str) -> (std::ops::Range<usize>, String) {
    match ix.attrs(t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == name) {
        Some(a) => (a.value.0..a.value.1, escape(value)),
        None => {
            let n = &ix.nodes[i];
            let at = if n.empty { n.open_end - 2 } else { n.open_end - 1 };
            (at..at, format!(" {name}=\"{}\"", escape(value)))
        }
    }
}

/// Removes an attribute with the space before it.
pub(super) fn attr_removal(ix: &Index, t: &str, i: usize, name: &str) -> Option<(std::ops::Range<usize>, String)> {
    let a = ix.attrs(t, i).into_iter().find(|a| &t[a.name.0..a.name.1] == name)?;
    let mut s = a.name.0;
    while s > 0 && t.as_bytes()[s - 1].is_ascii_whitespace() {
        s -= 1;
    }
    Some((s..a.value.1 + 1, String::new()))
}

pub(super) fn set_attrs(w: &mut Work<'_>, id: &str, set: &[(&str, String)]) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    let edits = set.iter().map(|(k, v)| attr_edit(&ix, t, i, k, v)).collect();
    w.splice(&part, edits)
}

pub(super) fn element_text<'t>(ix: &Index, t: &'t str, i: usize) -> &'t str {
    let n = &ix.nodes[i];
    t[n.open_end..n.close_start].trim()
}

/// The element's text with the indentation before it, so the part keeps its layout.
pub(super) fn element_span(ix: &Index, t: &str, i: usize) -> std::ops::Range<usize> {
    let n = &ix.nodes[i];
    let b = t.as_bytes();
    let mut s = n.start;
    while s > 0 && (b[s - 1] == b'\t' || b[s - 1] == b' ') {
        s -= 1;
    }
    if s > 0 && b[s - 1] == b'\n' {
        s -= 1;
        if s > 0 && b[s - 1] == b'\r' {
            s -= 1;
        }
    } else {
        s = n.start;
    }
    s..n.end
}

/// An element's text with some attributes set and others removed (its own start tag only).
pub(super) fn element_with(ix: &Index, t: &str, i: usize, set: &[(&str, String)], remove: &[&str]) -> String {
    let n = &ix.nodes[i];
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for (k, v) in set {
        edits.push(attr_edit(ix, t, i, k, v));
    }
    for k in remove {
        if let Some(e) = attr_removal(ix, t, i, k) {
            edits.push(e);
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut out = t[n.start..n.end].to_owned();
    for (r, v) in edits {
        out.replace_range(r.start - n.start..r.end - n.start, &v);
    }
    out
}

pub(super) fn indent_before(t: &str, at: usize) -> &str {
    let line = t[..at].rfind('\n').map_or(0, |p| p + 1);
    let ws = &t[line..at];
    if ws.chars().all(|c| c == '\t' || c == ' ') { ws } else { "" }
}

/// A script label of element `i` (`Properties/Label/KeyValuePair`).
pub(super) fn label_value(ix: &Index, t: &str, i: usize, key: &str) -> Option<String> {
    let props = ix.named(t, i, "Properties").first().copied()?;
    let label = ix.named(t, props, "Label").first().copied()?;
    ix.named(t, label, "KeyValuePair").into_iter().find(|&k| ix.value(t, k, "Key").as_deref() == Some(key)).and_then(|k| ix.value(t, k, "Value"))
}

/// `Properties/Label/KeyValuePair` as InDesign writes labels; a pair with the same key is
/// replaced.
pub(super) fn set_label(w: &mut Work<'_>, id: &str, key: &str, value: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let pair = format!("<KeyValuePair Key=\"{}\" Value=\"{}\" />", escape(key), escape(value));
    let indent = indent_before(&t, ix.nodes[i].start).to_owned();
    let edit = if let Some(props) = ix.named(&t, i, "Properties").first().copied() {
        if let Some(label) = ix.named(&t, props, "Label").first().copied() {
            if let Some(kv) = ix.named(&t, label, "KeyValuePair").into_iter().find(|&k| ix.value(&t, k, "Key").as_deref() == Some(key)) {
                attr_edit(&ix, &t, kv, "Value", value)
            } else {
                let at = ix.nodes[label].close_start;
                (at..at, format!("\t{pair}\n{indent}\t\t"))
            }
        } else {
            // InDesign writes Label before PathGeometry (basic-create/create-*).
            let at = ix.nodes[props].open_end;
            (at..at, format!("\t<Label>\n{indent}\t\t\t{pair}\n{indent}\t\t</Label>\n{indent}\t"))
        }
    } else {
        let at = ix.nodes[i].open_end;
        (at..at, format!("\n{indent}\t<Properties>\n{indent}\t\t<Label>\n{indent}\t\t\t{pair}\n{indent}\t\t</Label>\n{indent}\t</Properties>"))
    };
    w.splice(&part, vec![edit])
}
