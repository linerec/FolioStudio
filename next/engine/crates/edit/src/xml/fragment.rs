use crate::{EditError, xml::{Index, escape}};

pub(crate) fn scan(t: &str) -> Result<Index, EditError> {
    Index::scan(t).map_err(|e| EditError::new("xml", e))
}

pub(crate) fn attribute(t: &mut String, key: &str, value: &str) -> Result<(), EditError> {
    let ix = scan(t)?;
    if let Some(a) = ix.attrs(t, 0).into_iter().find(|a| &t[a.name.0..a.name.1] == key) {
        t.replace_range(a.value.0..a.value.1, &escape(value));
    } else {
        let at = ix.nodes[0].open_end - if ix.nodes[0].empty { 2 } else { 1 };
        t.insert_str(at, &format!(" {key}=\"{}\"", escape(value)));
    }
    Ok(())
}

pub(crate) fn property(t: &mut String, key: &str, value: &str, kind: &str) -> Result<(), EditError> {
    let ix = scan(t)?;
    if ix.nodes[0].empty {
        let name = ix.local(t, 0).to_owned();
        t.replace_range(ix.nodes[0].open_end - 2..ix.nodes[0].open_end, &format!("></{name}>"));
    }
    let ix = scan(t)?;
    if ix.attr(t, 0, key).is_some() { return attribute(t, key, value); }
    let new = format!("<{key} type=\"{kind}\">{}</{key}>", escape(value));
    if let Some(&p) = ix.named(t, 0, "Properties").first() {
        if let Some(&c) = ix.named(t, p, key).first() {
            t.replace_range(ix.nodes[c].start..ix.nodes[c].end, &new);
        } else if ix.nodes[p].empty { t.replace_range(ix.nodes[p].start..ix.nodes[p].end, &format!("<Properties>{new}</Properties>")); }
        else { t.insert_str(ix.nodes[p].close_start, &new); }
    } else { t.insert_str(ix.nodes[0].open_end, &format!("<Properties>{new}</Properties>")); }
    Ok(())
}

pub(crate) fn fragment(t: &str, ix: &Index, i: usize) -> String {
    t[ix.nodes[i].start..ix.nodes[i].end].to_owned()
}
