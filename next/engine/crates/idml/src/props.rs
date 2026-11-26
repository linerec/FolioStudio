//! IDML properties: an attribute (`PointSize="9"`) or a `<Properties>` child
//! (`<Leading type="unit">14</Leading>`, `<TabList type="list">…`). Both read the same way.

use crate::xml::Element;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub enum Val<'a> {
    /// Attribute value or the text of a simple `<Properties>` child.
    Str(&'a str),
    /// A `<Properties>` child with structure (`type="list"`, `type="record"`).
    Elem(&'a Element<'a>),
}

impl<'a> Val<'a> {
    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            Val::Str(s) => Some(s),
            Val::Elem(_) => None,
        }
    }
    pub fn f64(&self) -> Option<f64> {
        match self {
            Val::Str(s) => num(s),
            // A list with one unit, e.g. InsetSpacing given as a single value.
            Val::Elem(e) => {
                let items = list_f64(e)?;
                if items.len() == 1 { Some(items[0]) } else { None }
            }
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match self.as_str()? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }
    /// Numbers of a list (`<ListItem type="unit">…`) or a space-separated value.
    pub fn f64s(&self) -> Option<Vec<f64>> {
        match self {
            Val::Str(s) => nums(s),
            Val::Elem(e) => list_f64(e),
        }
    }
}

pub fn num(s: &str) -> Option<f64> {
    let v: f64 = s.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

pub fn nums(s: &str) -> Option<Vec<f64>> {
    s.split_whitespace().map(num).collect()
}

fn list_f64(e: &Element<'_>) -> Option<Vec<f64>> {
    let items: Vec<&Element> = e.children_named("ListItem").collect();
    if items.is_empty() {
        return nums(&e.text());
    }
    items.iter().map(|i| num(&i.text())).collect()
}

/// Something properties can be read from.
pub trait Props<'a> {
    fn get(&self, key: &str) -> Option<Val<'a>>;

    fn str(&self, key: &str) -> Option<&'a str> {
        self.get(key)?.as_str()
    }
    fn f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.f64()
    }
    fn bool(&self, key: &str) -> Option<bool> {
        self.get(key)?.bool()
    }
}

fn prop_val<'a>(c: &'a Element<'a>) -> Val<'a> {
    let structured = matches!(c.attr("type"), Some("list") | Some("record")) || c.name == "BulletChar" || c.elements().next().is_some();
    if structured {
        Val::Elem(c)
    } else {
        // Simple values are a single text node (or none for an empty string).
        match c.children.first() {
            Some(crate::xml::Node::Text(t)) if c.children.len() == 1 => Val::Str(t.as_ref()),
            _ => Val::Str(""),
        }
    }
}

/// Reading straight from an element: attributes, then its `<Properties>` children.
impl<'a> Props<'a> for &'a Element<'a> {
    fn get(&self, key: &str) -> Option<Val<'a>> {
        let e: &'a Element<'a> = self;
        if let Some(v) = e.attr(key) {
            return Some(Val::Str(v));
        }
        let props = e.child("Properties")?;
        props.child(key).map(prop_val)
    }
}

/// An indexed copy for elements read many times (styles with hundreds of attributes).
#[derive(Default)]
pub struct PropMap<'a> {
    map: HashMap<&'a str, Val<'a>>,
}

impl<'a> PropMap<'a> {
    pub fn of(e: &'a Element<'a>) -> Self {
        let mut map = HashMap::with_capacity(e.attrs.len() + 8);
        for (k, v) in &e.attrs {
            map.insert(*k, Val::Str(v.as_ref()));
        }
        if let Some(props) = e.child("Properties") {
            for c in props.elements() {
                map.entry(c.name).or_insert_with(|| prop_val(c));
            }
        }
        Self { map }
    }
}

impl<'a> Props<'a> for &PropMap<'a> {
    fn get(&self, key: &str) -> Option<Val<'a>> {
        self.map.get(key).copied()
    }
}

/// `"n"` / `"Nothing"` / empty mean "no reference" in IDML.
pub fn is_none_ref(s: &str) -> bool {
    s.is_empty() || s == "n" || s == "Nothing" || s == "$ID/Nothing"
}

/// Strips the `$ID/` marker InDesign puts on built-in names.
pub fn clean_name(s: &str) -> &str {
    s.strip_prefix("$ID/").unwrap_or(s)
}

/// An IDML reference in a list of references (`InkList`, `AppliedConditions`) with its
/// `%XX` escapes decoded (`Condition/A%20B` → `Condition/A B`).
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (b.get(i + 1).and_then(|&c| hex(c)), b.get(i + 2).and_then(|&c| hex(c)))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn bullet_character_is_a_structured_property() {
        let e = parse("<S><Properties><BulletChar BulletCharacterType=\"UnicodeOnly\" BulletCharacterValue=\"43\"/></Properties></S>").unwrap();
        for value in [(&e).get("BulletChar"), (&PropMap::of(&e)).get("BulletChar")] {
            let Some(Val::Elem(bullet)) = value else { panic!("structured bullet lost") };
            assert_eq!(bullet.attr("BulletCharacterValue"), Some("43"));
        }
    }

    #[test]
    fn reads_attributes_and_properties() {
        let e = parse(
            "<S PointSize=\"9\" Hyphenation=\"true\"><Properties><Leading type=\"unit\">14</Leading>\
             <AppliedFont type=\"string\">Apple SD Gothic Neo</AppliedFont><Leader type=\"string\"> </Leader>\
             <InsetSpacing type=\"list\"><ListItem type=\"unit\">1</ListItem><ListItem type=\"unit\">2</ListItem></InsetSpacing>\
             </Properties></S>",
        )
        .unwrap();
        let p = &e;
        assert_eq!(p.f64("PointSize"), Some(9.0));
        assert_eq!(p.bool("Hyphenation"), Some(true));
        assert_eq!(p.f64("Leading"), Some(14.0));
        assert_eq!(p.str("AppliedFont"), Some("Apple SD Gothic Neo"));
        assert_eq!(p.str("Leader"), Some(" "));
        assert_eq!(p.get("InsetSpacing").unwrap().f64s(), Some(vec![1.0, 2.0]));
        let m = PropMap::of(&e);
        assert_eq!((&m).f64("Leading"), Some(14.0));
        assert_eq!((&m).str("AppliedFont"), Some("Apple SD Gothic Neo"));
    }
}
