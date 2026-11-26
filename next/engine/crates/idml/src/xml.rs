//! A small XML tree built with quick-xml, borrowing names, attribute values and text from
//! the source buffer (only values with entity references are allocated).
//!
//! IDML needs element order, attributes, text (with whitespace inside `Content`) and the
//! processing instructions that stand for InDesign special characters (`<?ACE 18?>`).
//! Names are stored without namespace prefix (`idPkg:Story` → `Story`). Subtrees the reader
//! never looks at (export options, grid data, embedded image data, …) are skipped while parsing.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use std::borrow::Cow;

/// Deepest element nesting accepted; deeper input is rejected rather than risking the stack.
const MAX_DEPTH: usize = 512;

/// Elements whose content no part of the reader uses.
const SKIPPED: &[&str] = &[
    "ObjectExportOption",
    "InCopyExportOption",
    "MetadataPacketPreference",
    "ImageIOPreference",
    "GridDataInformation",
    "FlattenerPreference",
    "TextFrameFootnoteOptionsObject",
    "BaselineFrameGridOption",
    "Contents",
];

/// Whether an element is skipped. `Contents` is kept for a placed graphic's
/// `Properties` (its embedded file data, which print output needs) and for a custom
/// text variable's (the text Define shows).
fn skipped(name: &str, stack: &[Element<'_>]) -> bool {
    if !SKIPPED.contains(&name) {
        return false;
    }
    if name == "Contents" && stack.len() >= 2 {
        let (parent, owner) = (stack[stack.len() - 1].name, stack[stack.len() - 2].name);
        let graphic = matches!(owner, "Image" | "PDF" | "EPS" | "ImportedPage" | "PICT" | "WMF" | "SVG" | "CustomTextVariablePreference");
        return !(parent == "Properties" && graphic);
    }
    true
}

#[derive(Debug, PartialEq)]
pub enum Node<'a> {
    Element(Element<'a>),
    Text(Cow<'a, str>),
    /// Processing instruction: target and content, e.g. ("ACE", "18").
    Pi(&'a str, &'a str),
}

#[derive(Debug, Default, PartialEq)]
pub struct Element<'a> {
    pub name: &'a str,
    pub attrs: Vec<(&'a str, Cow<'a, str>)>,
    pub children: Vec<Node<'a>>,
}

impl<'a> Element<'a> {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_ref())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element<'a>> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element<'a>> {
        self.elements().find(|e| e.name == name)
    }

    pub fn children_named<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Element<'a>> + 's {
        self.elements().filter(move |e| e.name == name)
    }

    /// Concatenated text of this element and its descendants.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        for n in &self.children {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => e.collect_text(out),
                Node::Pi(..) => {}
            }
        }
    }

    /// All descendants named `name` (depth-first, document order).
    pub fn descendants_named<'s>(&'s self, name: &str, out: &mut Vec<&'s Element<'a>>) {
        for e in self.elements() {
            if e.name == name {
                out.push(e);
            }
            e.descendants_named(name, out);
        }
    }
}

#[derive(Debug)]
pub struct XmlError(pub String);

fn local(qname: &str) -> &str {
    match qname.rfind(':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

/// `s` as a slice of `src` when it points into it (events from a borrowed reader do).
fn rebase<'a>(src: &'a str, s: &str) -> Option<&'a str> {
    let start = (s.as_ptr() as usize).checked_sub(src.as_ptr() as usize)?;
    let end = start.checked_add(s.len())?;
    src.get(start..end)
}

fn borrow<'a>(src: &'a str, c: Cow<'_, str>) -> Cow<'a, str> {
    match c {
        Cow::Borrowed(s) => match rebase(src, s) {
            Some(b) => Cow::Borrowed(b),
            None => Cow::Owned(s.to_owned()),
        },
        Cow::Owned(s) => Cow::Owned(s),
    }
}

fn start<'a>(src: &'a str, e: &BytesStart<'_>) -> Result<Element<'a>, XmlError> {
    let qname = e.name();
    let full: &str = qname.as_ref();
    let name = rebase(src, local(full)).ok_or_else(|| XmlError(format!("element name {full} not in source")))?;
    let mut attrs = Vec::with_capacity(8);
    // Duplicate-attribute checks are quadratic per element; InDesign never writes duplicates
    // and `Element::attr` takes the first one.
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|err| XmlError(format!("attribute of <{name}>: {err}")))?;
        let raw_key: &str = a.key.as_ref();
        // Namespace declarations carry no document data.
        if raw_key == "xmlns" || raw_key.starts_with("xmlns:") {
            continue;
        }
        let key = rebase(src, local(raw_key)).ok_or_else(|| XmlError(format!("attribute {raw_key} not in source")))?;
        let value = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|err| XmlError(format!("attribute value of <{name}>: {err}")))?;
        attrs.push((key, borrow(src, value)));
    }
    Ok(Element { name, attrs, children: Vec::new() })
}

fn push_text<'a>(parent: &mut Element<'a>, text: Cow<'a, str>) {
    if text.is_empty() {
        return;
    }
    if let Some(Node::Text(t)) = parent.children.last_mut() {
        t.to_mut().push_str(&text);
    } else {
        parent.children.push(Node::Text(text));
    }
}

/// Removes indentation-only text (contains a line break, nothing else) outside `Content`.
fn finish(e: &mut Element<'_>) {
    if e.name != "Content" && e.children.len() > 1 {
        e.children.retain(|n| match n {
            Node::Text(t) => !(t.contains('\n') && t.trim().is_empty()),
            _ => true,
        });
    }
}

fn reader(src: &str) -> Reader<&[u8]> {
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;
    reader
}

fn err_at<E: std::fmt::Display>(reader: &Reader<&[u8]>, e: E) -> XmlError {
    XmlError(format!("at byte {}: {e}", reader.buffer_position()))
}

/// Parses a whole document; returns its root element.
pub fn parse(src: &str) -> Result<Element<'_>, XmlError> {
    parse_at(src, 0)
}

/// Parses `src` whose root element sits `depth` levels down in the whole document.
fn parse_at(src: &str, depth: usize) -> Result<Element<'_>, XmlError> {
    let mut reader = reader(src);
    // A synthetic document node holds the root (and stray top-level PIs we drop).
    let mut stack: Vec<Element<'_>> = vec![Element::default()];
    loop {
        let ev = reader.read_event().map_err(|e| err_at(&reader, e))?;
        match ev {
            Event::Start(e) => {
                if stack.len() + depth > MAX_DEPTH {
                    return Err(XmlError(format!("nesting deeper than {MAX_DEPTH}")));
                }
                if skipped(local(e.name().as_ref()), &stack) {
                    let end = e.to_end().into_owned();
                    reader.read_to_end(end.name()).map_err(|err| err_at(&reader, err))?;
                    continue;
                }
                stack.push(start(src, &e)?);
            }
            Event::Empty(e) => {
                if SKIPPED.contains(&local(e.name().as_ref())) {
                    continue;
                }
                let el = start(src, &e)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Element(el));
                }
            }
            Event::End(_) => {
                let mut el = stack.pop().ok_or_else(|| XmlError("unbalanced end tag".into()))?;
                finish(&mut el);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(el)),
                    None => return Err(XmlError("unbalanced end tag".into())),
                }
            }
            Event::Text(t) => {
                if stack.len() > 1 {
                    let content = borrow(src, t.xml10_content());
                    if let Some(parent) = stack.last_mut() {
                        push_text(parent, content);
                    }
                }
            }
            Event::CData(t) => {
                if stack.len() > 1 {
                    let content = borrow(src, t.xml10_content());
                    if let Some(parent) = stack.last_mut() {
                        push_text(parent, content);
                    }
                }
            }
            Event::GeneralRef(r) => {
                let resolved: Cow<'_, str> = if let Some(c) =
                    r.resolve_char_ref().map_err(|e| XmlError(format!("character reference: {e}")))?
                {
                    Cow::Owned(c.to_string())
                } else {
                    let name = r.xml10_content();
                    match quick_xml::escape::resolve_predefined_entity(&name) {
                        Some(s) => Cow::Borrowed(s),
                        None => return Err(XmlError(format!("unknown entity &{name};"))),
                    }
                };
                if stack.len() > 1
                    && let Some(parent) = stack.last_mut()
                {
                    push_text(parent, resolved);
                }
            }
            Event::PI(pi) => {
                if stack.len() > 1
                    && let (Some(target), Some(content)) = (rebase(src, pi.target()), rebase(src, pi.content()))
                    && let Some(parent) = stack.last_mut()
                {
                    parent.children.push(Node::Pi(target, content.trim()));
                }
            }
            Event::DocType(_) => return Err(XmlError("DOCTYPE declarations are not accepted".into())),
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
        }
    }
    if stack.len() != 1 {
        return Err(XmlError("unexpected end of document".into()));
    }
    let doc = stack.pop().unwrap_or_default();
    doc.children
        .into_iter()
        .find_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
        .ok_or_else(|| XmlError("no root element".into()))
}

/// Same tree as [`parse`], built in parallel for big parts: one tokenizer pass finds the
/// children of the root's children (a spread's pages and page items), which are then parsed as
/// separate fragments of the same buffer on up to `threads` threads.
pub fn parse_split(src: &str, threads: usize) -> Result<Element<'_>, XmlError> {
    let mut reader = reader(src);
    let mut stack: Vec<Element<'_>> = vec![Element::default()];
    // Byte ranges of the fragments, in document order; each leaves a placeholder (an element
    // with an empty name, which XML cannot produce) in its parent.
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    loop {
        let before = reader.buffer_position() as usize;
        let ev = reader.read_event().map_err(|e| err_at(&reader, e))?;
        match ev {
            Event::Start(e) => {
                if skipped(local(e.name().as_ref()), &stack) {
                    let end = e.to_end().into_owned();
                    reader.read_to_end(end.name()).map_err(|err| err_at(&reader, err))?;
                } else if stack.len() == 3 {
                    // Inside the root's child: this element and its content are a fragment.
                    let end = e.to_end().into_owned();
                    reader.read_to_end(end.name()).map_err(|err| err_at(&reader, err))?;
                    ranges.push(before..reader.buffer_position() as usize);
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(Node::Element(Element::default()));
                    }
                } else {
                    stack.push(start(src, &e)?);
                }
            }
            Event::Empty(e) => {
                if !SKIPPED.contains(&local(e.name().as_ref())) {
                    let el = start(src, &e)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(Node::Element(el));
                    }
                }
            }
            Event::End(_) => {
                let mut el = stack.pop().ok_or_else(|| XmlError("unbalanced end tag".into()))?;
                finish(&mut el);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(el)),
                    None => return Err(XmlError("unbalanced end tag".into())),
                }
            }
            Event::Text(t) => {
                if stack.len() > 1 {
                    let content = borrow(src, t.xml10_content());
                    if let Some(parent) = stack.last_mut() {
                        push_text(parent, content);
                    }
                }
            }
            Event::CData(t) => {
                if stack.len() > 1 {
                    let content = borrow(src, t.xml10_content());
                    if let Some(parent) = stack.last_mut() {
                        push_text(parent, content);
                    }
                }
            }
            // Rare outside fragments; the sequential parser handles them exactly.
            Event::GeneralRef(_) | Event::PI(_) if stack.len() > 1 => return parse(src),
            Event::GeneralRef(_) | Event::PI(_) => {}
            Event::DocType(_) => return Err(XmlError("DOCTYPE declarations are not accepted".into())),
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
        }
    }
    if stack.len() != 1 {
        return Err(XmlError("unexpected end of document".into()));
    }

    // Contiguous batches of similar byte size, one per thread.
    let total: usize = ranges.iter().map(|r| r.len()).sum();
    let threads = threads.clamp(1, 64).min(ranges.len().max(1));
    let mut batches: Vec<&[std::ops::Range<usize>]> = Vec::with_capacity(threads);
    let (mut from, mut acc) = (0, 0);
    for (i, r) in ranges.iter().enumerate() {
        acc += r.len();
        if acc * threads >= total * (batches.len() + 1) || i + 1 == ranges.len() {
            batches.push(&ranges[from..=i]);
            from = i + 1;
        }
    }
    let parsed: Vec<Result<Vec<Element<'_>>, XmlError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = batches
            .iter()
            .map(|batch| scope.spawn(move || batch.iter().map(|r| parse_at(&src[r.clone()], 2)).collect()))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err(XmlError("parser thread panicked".into()))))
            .collect()
    });
    let mut fragments = Vec::with_capacity(ranges.len());
    for batch in parsed {
        fragments.extend(batch?);
    }
    let mut doc = stack.pop().unwrap_or_default();
    let mut next = fragments.into_iter();
    for n in &mut doc.children {
        let Node::Element(root) = n else { continue };
        for c in root.children.iter_mut() {
            let Node::Element(child) = c else { continue };
            for slot in child.children.iter_mut() {
                if let Node::Element(e) = slot
                    && e.name.is_empty()
                {
                    *e = next.next().ok_or_else(|| XmlError("fragment count mismatch".into()))?;
                }
            }
        }
    }
    if next.next().is_some() {
        return Err(XmlError("fragment count mismatch".into()));
    }
    doc.children
        .into_iter()
        .find_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
        .ok_or_else(|| XmlError("no root element".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_input_is_rejected_in_serial_and_split_parsers() {
        for src in ["", "<a", "<a><b/>", "<a>&unknown;</a>", "<a>&#x110000;</a>", "<a><b></a>"] {
            assert!(parse(src).is_err(), "serial: {src:?}");
            assert!(parse_split(src, 4).is_err(), "split: {src:?}");
        }
        for value in ["", "bad", "NaN", "inf", "1e9999"] {
            assert_eq!(crate::props::num(value), None, "{value:?}");
        }
        assert_eq!(crate::props::nums("1 bad 3"), None);
    }

    #[test]
    fn parses_content_pis_and_refs() {
        let root = parse(
            "<?xml version=\"1.0\"?><idPkg:Story xmlns:idPkg=\"x\"><Story Self=\"u1\">\n\t<Content>a&amp;b&#x9;<?ACE 18?> c</Content>\n\
             <ObjectExportOption A=\"1\"><Properties /></ObjectExportOption></Story></idPkg:Story>",
        )
        .unwrap();
        assert_eq!(root.name, "Story");
        let story = root.child("Story").unwrap();
        assert_eq!(story.attr("Self"), Some("u1"));
        // Indentation dropped, skipped subtree absent.
        assert_eq!(story.children.len(), 1);
        let content = story.child("Content").unwrap();
        assert!(matches!(&content.children[0], Node::Text(t) if t == "a&b\t"));
        assert!(matches!(&content.children[1], Node::Pi(t, c) if *t == "ACE" && *c == "18"));
        assert!(matches!(&content.children[2], Node::Text(Cow::Borrowed(t)) if *t == " c"));
    }

    #[test]
    fn split_parse_builds_the_same_tree() {
        let src = "<?xml version=\"1.0\"?><idPkg:Spread xmlns:idPkg=\"x\">\n<Spread Self=\"s\">\n\t<Page Self=\"p\"/>\n\
             <Rectangle Self=\"r1\"><Properties><Leading type=\"unit\">1</Leading></Properties>\
             <Rectangle Self=\"r2\"><Content>a&amp;b<?ACE 18?></Content></Rectangle></Rectangle>\n\
             <ObjectExportOption><x/></ObjectExportOption><Guide Self=\"g\"/>\
             <Group Self=\"g1\"><Oval Self=\"o\"/></Group><TextFrame Self=\"t\"/>text<![CDATA[c]]>\
             </Spread><Spread Self=\"s2\"><Rectangle Self=\"r3\"/></Spread></idPkg:Spread>";
        let a = parse(src).unwrap();
        for threads in [1, 2, 3, 8] {
            assert_eq!(parse_split(src, threads).unwrap(), a);
        }
        // A PI or reference outside fragments falls back to the sequential parser.
        let pi = "<a><b><?ACE 18?><c/></b></a>";
        assert_eq!(parse_split(pi, 4).unwrap(), parse(pi).unwrap());
        assert!(parse_split("<a><b><c><d></c></b></a>", 4).is_err());
        assert!(parse_split("<a><b><c>&bogus;</c></b></a>", 4).is_err());
        assert!(parse_split("<a><b><c>", 4).is_err());
    }

    #[test]
    fn rejects_doctype_and_bad_nesting() {
        assert!(parse("<!DOCTYPE x [<!ENTITY a \"b\">]><x>&a;</x>").is_err());
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a><ObjectExportOption><b></ObjectExportOption></a>").is_err());
        let deep = "<a>".repeat(MAX_DEPTH + 2) + &"</a>".repeat(MAX_DEPTH + 2);
        assert!(parse(&deep).is_err());
    }
}
