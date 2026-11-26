//! Byte positions of the elements of an IDML part, so an edit can change one attribute or move
//! one element and leave every other byte as it was.
//!
//! IDML parts are plain XML from InDesign: double-quoted attribute values, no DTD, comments,
//! processing instructions (`<?ACE 18?>`) and CDATA (embedded files) inside elements. The
//! scanner records each element's start tag, end and parent; text is not looked at.

use std::ops::Range;
pub(crate) mod fragment;

#[derive(Clone, Debug)]
pub struct Node {
    /// Name in the start tag, with any namespace prefix (`idPkg:Story`).
    pub name: Range<usize>,
    /// `<` of the start tag.
    pub start: usize,
    /// Just after the start tag's `>`.
    pub open_end: usize,
    /// `<` of the end tag (`open_end` for an empty element written `<a/>`).
    pub close_start: usize,
    /// Just after the element's last `>`.
    pub end: usize,
    pub parent: Option<usize>,
    pub empty: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Attr {
    pub name: (usize, usize),
    /// The value between the quotes, still escaped.
    pub value: (usize, usize),
}

/// The elements of one part in document order.
#[derive(Clone, Debug, Default)]
pub struct Index {
    pub nodes: Vec<Node>,
    children: Vec<Vec<usize>>,
}

fn find(text: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || from > text.len() {
        return None;
    }
    text[from..].windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

impl Index {
    pub fn scan(text: &str) -> Result<Index, String> {
        let b = text.as_bytes();
        let mut nodes: Vec<Node> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut i = 0;
        while let Some(lt) = b[i..].iter().position(|&c| c == b'<').map(|p| p + i) {
            let rest = &b[lt..];
            if rest.starts_with(b"<?") {
                i = find(b, lt + 2, b"?>").ok_or("unterminated processing instruction")? + 2;
            } else if rest.starts_with(b"<!--") {
                i = find(b, lt + 4, b"-->").ok_or("unterminated comment")? + 3;
            } else if rest.starts_with(b"<![CDATA[") {
                i = find(b, lt + 9, b"]]>").ok_or("unterminated CDATA")? + 3;
            } else if rest.starts_with(b"<!") {
                i = find(b, lt + 2, b">").ok_or("unterminated declaration")? + 1;
            } else if rest.starts_with(b"</") {
                let gt = find(b, lt + 2, b">").ok_or("unterminated end tag")?;
                let open = stack.pop().ok_or_else(|| format!("end tag without start at {lt}"))?;
                let name = &b[nodes[open].name.clone()];
                if b[lt + 2..gt].trim_ascii_end() != name {
                    return Err(format!("end tag at {lt} does not match <{}>", String::from_utf8_lossy(name)));
                }
                nodes[open].close_start = lt;
                nodes[open].end = gt + 1;
                i = gt + 1;
            } else {
                let name_start = lt + 1;
                let mut j = name_start;
                while j < b.len() && !matches!(b[j], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
                    j += 1;
                }
                let name = name_start..j;
                // the closing `>` outside quotes
                let mut quote = None;
                while j < b.len() {
                    match (quote, b[j]) {
                        (None, b'"') | (None, b'\'') => quote = Some(b[j]),
                        (Some(q), c) if c == q => quote = None,
                        (None, b'>') => break,
                        _ => {}
                    }
                    j += 1;
                }
                if j >= b.len() {
                    return Err(format!("unterminated start tag at {lt}"));
                }
                let empty = b[j - 1] == b'/';
                let id = nodes.len();
                nodes.push(Node { name, start: lt, open_end: j + 1, close_start: j + 1, end: j + 1, parent: stack.last().copied(), empty });
                if !empty {
                    stack.push(id);
                }
                i = j + 1;
            }
        }
        if let Some(&open) = stack.last() {
            return Err(format!("element at {} is not closed", nodes[open].start));
        }
        let mut children = vec![Vec::new(); nodes.len()];
        for (i, n) in nodes.iter().enumerate() {
            if let Some(p) = n.parent {
                children[p].push(i);
            }
        }
        Ok(Index { nodes, children })
    }

    pub fn children(&self, i: usize) -> &[usize] {
        &self.children[i]
    }

    pub fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        self.nodes.iter().enumerate().filter(|(_, n)| n.parent.is_none()).map(|(i, _)| i)
    }

    /// Name without namespace prefix.
    pub fn local<'t>(&self, text: &'t str, i: usize) -> &'t str {
        let n = &text[self.nodes[i].name.clone()];
        n.rsplit(':').next().unwrap_or(n)
    }

    pub fn attrs(&self, text: &str, i: usize) -> Vec<Attr> {
        let n = &self.nodes[i];
        let b = text.as_bytes();
        let end = if n.empty { n.open_end - 2 } else { n.open_end - 1 };
        let mut out = Vec::new();
        let mut j = n.name.end;
        loop {
            while j < end && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= end {
                break;
            }
            let ns = j;
            while j < end && b[j] != b'=' && !b[j].is_ascii_whitespace() {
                j += 1;
            }
            let ne = j;
            while j < end && b[j] != b'"' && b[j] != b'\'' {
                j += 1;
            }
            if j >= end {
                break;
            }
            let q = b[j];
            let vs = j + 1;
            let ve = match b[vs..end].iter().position(|&c| c == q) {
                Some(p) => vs + p,
                None => break,
            };
            out.push(Attr { name: (ns, ne), value: (vs, ve) });
            j = ve + 1;
        }
        out
    }

    pub fn attr<'t>(&self, text: &'t str, i: usize, name: &str) -> Option<&'t str> {
        self.attrs(text, i).into_iter().find(|a| &text[a.name.0..a.name.1] == name).map(|a| &text[a.value.0..a.value.1])
    }

    /// The attribute unescaped.
    pub fn value(&self, text: &str, i: usize, name: &str) -> Option<String> {
        self.attr(text, i, name).map(unescape)
    }

    /// The element whose `Self` is `id`.
    pub fn by_self(&self, text: &str, id: &str) -> Option<usize> {
        // an id with `&`, `<` or `>` (an index topic named `Q&A`) is stored escaped
        let id = escape(id);
        let pat = format!(" Self=\"{id}\"");
        let at = text.find(&pat)?;
        let i = self.nodes.partition_point(|n| n.start <= at).checked_sub(1)?;
        (self.nodes[i].open_end > at && self.attr(text, i, "Self") == Some(id.as_str())).then_some(i)
    }

    /// Children (in order) with this local name.
    pub fn named(&self, text: &str, i: usize, name: &str) -> Vec<usize> {
        self.children[i].iter().copied().filter(|&c| self.local(text, c) == name).collect()
    }

    /// The first descendant along `path` of local names (`Properties/PathGeometry`).
    pub fn path(&self, text: &str, i: usize, path: &[&str]) -> Option<usize> {
        let mut at = i;
        for p in path {
            at = *self.named(text, at, p).first()?;
        }
        Some(at)
    }

    /// Every descendant with this local name, not looking inside elements that have a `Self`
    /// (they are objects of their own).
    pub fn own_descendants(&self, text: &str, i: usize, name: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let mut todo: Vec<usize> = self.children[i].iter().rev().copied().collect();
        while let Some(c) = todo.pop() {
            if self.attr(text, c, "Self").is_some() {
                continue;
            }
            if self.local(text, c) == name {
                out.push(c);
            }
            todo.extend(self.children[c].iter().rev().copied());
        }
        out
    }

    pub fn ancestors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.nodes[i].parent, move |&p| self.nodes[p].parent)
    }
}

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\t' => o.push_str("&#x9;"),
            '\n' => o.push_str("&#xa;"),
            '\r' => o.push_str("&#xd;"),
            c => o.push(c),
        }
    }
    o
}

pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut o = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        o.push_str(&rest[..p]);
        let tail = &rest[p..];
        let Some(semi) = tail.find(';') else {
            o.push_str(tail);
            return o;
        };
        let ent = &tail[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => o.push(c),
            None => o.push_str(&tail[..=semi]),
        }
        rest = &tail[semi + 1..];
    }
    o.push_str(rest);
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = r#"<?xml version="1.0"?><a Self="x"><!-- c --><b Self="y" V="1 2"/><?ACE 18?><c><![CDATA[<z>]]></c></a>"#;

    #[test]
    fn scans_elements_and_attributes() {
        let ix = Index::scan(T).unwrap();
        assert_eq!(ix.nodes.len(), 3);
        let b = ix.by_self(T, "y").unwrap();
        assert_eq!(ix.local(T, b), "b");
        assert_eq!(ix.attr(T, b, "V"), Some("1 2"));
        assert!(ix.nodes[b].empty);
        let a = ix.by_self(T, "x").unwrap();
        assert_eq!(&T[ix.nodes[a].start..ix.nodes[a].end], &T[T.find("<a").unwrap()..]);
        assert_eq!(ix.children(a).len(), 2);
        assert!(ix.by_self(T, "z").is_none());
    }

    #[test]
    fn rejects_broken_nesting() {
        assert!(Index::scan("<a><b></a>").is_err());
        assert!(Index::scan("<a>").is_err());
    }

    #[test]
    fn escapes_round_trip() {
        let s = "a&b<\"c\">\t";
        assert_eq!(unescape(&escape(s)), s);
        assert_eq!(unescape("&#x20;&#65;"), " A");
    }
}
