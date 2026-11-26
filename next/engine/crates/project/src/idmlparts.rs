//! Targeted edits of IDML XML parts, leaving every other byte as it was.
//!
//! A `.folio` keeps the IDML parts the importer produced and changes only what principle 1
//! and the Folio labels need:
//!
//! * `Link/@LinkResourceURI` and every other `file:` URI (attribute values; text and CDATA of
//!   XMP packets) are replaced through callbacks (import: absolute → `folio:ref/<id>`;
//!   export: back to absolute `file:` URIs of this machine);
//! * `folio.*` script labels are added to elements by `Self` — the frames that hold placed
//!   graphics, movies and sounds, and the designmap's `Document` — as a `Label` of
//!   `KeyValuePair`s at the end of the element's `Properties`, as InDesign writes labels
//!   (measured with InDesign 2025, see the project README). InDesign reads frame and
//!   document labels back from IDML but not those of placed graphics (`Image`, `PDF`, …),
//!   so graphics are labelled through their frame. Keys we write replace pairs of the same
//!   key; other pairs are kept;
//! * export can put an embedded graphic's file back as `Properties/Contents` (base64 in
//!   CDATA, 76-column lines, before `ClippingPathGeometry` / `GraphicBounds` as InDesign
//!   writes it).
//!
//! Story text (`Content`) is never touched.

use std::borrow::Cow;
use std::collections::HashMap;

pub const GRAPHIC_ELEMENTS: &[&str] = &["Image", "EPS", "PDF", "ImportedPage", "WMF", "PICT", "SVG"];

/// Resolve a graphic's optional embedded payload.
pub type GraphicContents<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;
/// Resolve an optional replacement for a URI.
pub type UriReplacement<'a> = dyn FnMut(&str) -> Option<String> + 'a;

/// What to change in a part.
#[derive(Default)]
pub struct Edits<'a> {
    /// Inspect every graphic link, including an unsupported URI scheme.
    pub inspect_link: Option<&'a mut dyn FnMut(&str)>,
    /// `Link` Self → new `LinkResourceURI` (unescaped text).
    pub link_uris: HashMap<String, String>,
    /// `Link` Self → other attributes to set (unescaped values).
    pub link_attrs: HashMap<String, Vec<(String, String)>>,
    /// Graphic Self → label pairs.
    pub labels: HashMap<String, Vec<(String, String)>>,
    /// Labels of the designmap's `Document`.
    pub document_labels: Vec<(String, String)>,
    /// Graphic Self → file bytes for `Properties/Contents` (export of extracted payloads).
    pub contents: Option<&'a GraphicContents<'a>>,
    /// Graphics whose `Contents` is wanted (keys of `contents`).
    pub contents_for: std::collections::HashSet<String>,
    /// Any other URI with one of `prefixes` → replacement (`None` keeps it).
    pub other_uri: Option<&'a mut UriReplacement<'a>>,
    /// URI prefixes `other_uri` looks for; empty means `file:` (import). Export asks for
    /// the Folio placeholders.
    pub prefixes: Vec<&'static str>,
    /// Also pass attribute values that are whole native absolute paths (`C:\…`,
    /// `\\server\…`, `/Users/…`; e.g. `Movie/@FilePath`) to `other_uri` (import).
    pub native_paths: bool,
}

/// A content link to another document, `idexternalSharedArticle:<path>?<n>` or `idexternalcontent:<path>?<n>`, as (the part up to
/// and with the colon, the path, `?<n>` or nothing).
pub fn split_external_content_uri(v: &str) -> Option<(&str, &str, &str)> {
    let head = ["idexternalSharedArticle:", "idexternalcontent:"].into_iter().find(|h| v.starts_with(h))?;
    let rest = &v[head.len()..];
    let (path, tail) = match rest.rfind('?') {
        Some(i) if rest[i + 1..].bytes().all(|b| b.is_ascii_digit()) && i + 1 < rest.len() => rest.split_at(i),
        _ => (rest, ""),
    };
    (!path.is_empty()).then_some((head, path, tail))
}

/// Whether a whole attribute value is an absolute path of some OS (not a URI).
pub fn is_native_absolute(v: &str) -> bool {
    let b = v.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || (v.starts_with("\\\\") && v.len() > 3)
        || ["/Users/", "/Volumes/", "/home/", "/mnt/", "/media/", "/private/", "/tmp/", "/var/", "/opt/"].iter().any(|r| v.starts_with(r))
}

/// What a rewrite did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub link_uris: usize,
    pub link_attrs: usize,
    pub labels: usize,
    pub contents: usize,
    pub other_uris: usize,
}

impl Counts {
    pub fn add(&mut self, o: &Counts) {
        self.link_uris += o.link_uris;
        self.link_attrs += o.link_attrs;
        self.labels += o.labels;
        self.contents += o.contents;
        self.other_uris += o.other_uris;
    }
    pub fn any(&self) -> bool {
        *self != Counts::default()
    }
}

/// Whether a part may need edits (cheap test before the full scan).
pub fn worth_scanning(name: &str, text: &str) -> bool {
    name == "designmap.xml"
        || name.ends_with("metadata.xml")
        || text.contains("file:")
        || text.contains("folio:")
        || text.contains("<Link ")
        || GRAPHIC_ELEMENTS.iter().any(|g| text.contains(&format!("<{g} ")))
}

pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            '\t' => out.push_str("&#x9;"),
            c => out.push(c),
        }
    }
    out
}

pub fn unescape(s: &str) -> String {
    unescaped(s).into_owned()
}

fn unescaped(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return Cow::Owned(out);
        };
        let ent = &tail[1..end];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The bytes of base64 text (whitespace between the characters is left out); none for text that is not base64.
pub fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Base64 in 76-column lines, as InDesign writes `Contents`.
pub fn base64_lines(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4 * 77 / 76 + 4);
    let mut col = 0;
    for g in data.chunks(3) {
        if col == 76 {
            out.push('\n');
            col = 0;
        }
        let b = [g[0], g.get(1).copied().unwrap_or(0), g.get(2).copied().unwrap_or(0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= g.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
        col += 4;
    }
    out
}

fn labels_xml(pairs: &[(String, String)]) -> String {
    pairs.iter().map(|(k, v)| format!("<KeyValuePair Key=\"{}\" Value=\"{}\" />", escape_attr(k), escape_attr(v))).collect()
}

/// One attribute of a start tag: name and the byte range of its value (without quotes).
struct Attr<'t> {
    name: &'t str,
    value: std::ops::Range<usize>,
}

/// Parses a start tag `<name a="b" …>` (`tag` from `<` to `>` inclusive): name, attributes,
/// and whether it closes itself.
fn parse_tag<'t>(tag: &'t str, attrs: &mut Vec<Attr<'t>>) -> (&'t str, bool) {
    let b = tag.as_bytes();
    let mut i = 1;
    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let name = &tag[1..i];
    attrs.clear();
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] == b'>' || b[i] == b'/' {
            break;
        }
        let start = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() && b[i] != b'>' {
            i += 1;
        }
        let aname = &tag[start..i];
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'=') {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            break;
        }
        let q = b[i];
        let vstart = i + 1;
        let mut j = vstart;
        while j < b.len() && b[j] != q {
            j += 1;
        }
        attrs.push(Attr { name: aname, value: vstart..j });
        i = j + 1;
    }
    let self_closing = tag.trim_end_matches('>').trim_end().ends_with('/');
    (name, self_closing)
}

fn uri_prefixes<'a>(prefixes: &'a [&'static str]) -> &'a [&'static str] {
    if prefixes.is_empty() { &["file:"] } else { prefixes }
}

#[derive(Debug)]
enum Frame {
    Plain,
    /// A graphic (or the designmap Document) that gets labels / contents; whether its first
    /// child has been seen and whether the Properties block is done.
    Target { key: String, document: bool, first_child_seen: bool, props_done: bool },
    /// The target's `Properties`.
    Props { key: String, document: bool, label_done: bool, contents_done: bool },
    /// The target's `Label`.
    Label { key: String, document: bool },
    /// Text of this element is an XMP packet or XMP (scrub `file:` URIs in it).
    Xmp,
}

struct Rewriter<'e, 'a> {
    edits: &'e mut Edits<'a>,
    counts: Counts,
    xmp_part: bool,
}

impl Rewriter<'_, '_> {
    fn label_pairs(&self, key: &str, document: bool) -> &[(String, String)] {
        if document { &self.edits.document_labels } else { self.edits.labels.get(key).map(Vec::as_slice).unwrap_or(&[]) }
    }

    fn label_block(&mut self, key: &str, document: bool) -> String {
        let pairs = self.label_pairs(key, document);
        if pairs.is_empty() {
            return String::new();
        }
        let s = format!("<Label>{}</Label>", labels_xml(pairs));
        self.counts.labels += 1;
        s
    }

    fn contents_block(&mut self, key: &str, document: bool) -> String {
        if document || !self.edits.contents_for.contains(key) {
            return String::new();
        }
        let Some(get) = self.edits.contents else { return String::new() };
        match get(key) {
            Some(data) => {
                self.counts.contents += 1;
                format!("<Contents><![CDATA[{}]]></Contents>", base64_lines(&data))
            }
            None => String::new(),
        }
    }

    /// XMP text: the references in it are replaced in place; the replacement is written escaped, the rest stays as it is.
    fn scrub_text(&mut self, text: &str) -> Option<String> {
        self.scrub(text, true)
    }

    /// `text` is plain (unescaped) text; the references in it are replaced (by `escape_new` escaped: the text stays XML text, otherwise plain).
    fn scrub(&mut self, text: &str, escape_new: bool) -> Option<String> {
        let prefixes = uri_prefixes(&self.edits.prefixes);
        let f = self.edits.other_uri.as_mut()?;
        let next = |r: &str| prefixes.iter().filter_map(|p| r.find(p)).min();
        let mut at = Some(next(text)?);
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        let mut changed = false;
        while let Some(i) = at {
            out.push_str(&rest[..i]);
            let tail = &rest[i..];
            let end = tail.find(|c: char| c == '<' || c == '"' || c == '\'' || c.is_whitespace()).unwrap_or(tail.len());
            let uri = &tail[..end];
            match f(&unescaped(uri)) {
                Some(new) => {
                    if escape_new { out.push_str(&escape_attr(&new)) } else { out.push_str(&new) }
                    self.counts.other_uris += 1;
                    changed = true;
                }
                None => out.push_str(uri),
            }
            rest = &tail[end..];
            at = next(rest);
        }
        out.push_str(rest);
        changed.then_some(out)
    }

    /// Rewrites attribute values of a start tag; returns the new tag text when changed.
    fn rewrite_attrs(&mut self, tag: &str, name: &str, attrs: &[Attr<'_>], self_id: Option<&str>) -> Option<String> {
        let mut changes: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        let mut extra: Vec<(String, String)> = Vec::new();
        let mut handled_uri = false;
        if name == "Link"
            && let Some(a) = attrs.iter().find(|a| a.name == "LinkResourceURI")
            && let Some(inspect) = self.edits.inspect_link.as_mut()
        {
            inspect(&unescaped(&tag[a.value.clone()]));
        }
        if name == "Link"
            && let Some(id) = self_id
        {
            if let Some(a) = attrs.iter().find(|a| a.name == "LinkResourceURI")
                && let Some(new) = self.edits.link_uris.get(id)
            {
                changes.push((a.value.clone(), escape_attr(new)));
                self.counts.link_uris += 1;
                handled_uri = true;
            }
            if let Some(set) = self.edits.link_attrs.get(id) {
                for (k, v) in set {
                    match attrs.iter().find(|a| a.name == k) {
                        Some(a) => changes.push((a.value.clone(), escape_attr(v))),
                        None => extra.push((k.clone(), v.clone())),
                    }
                }
                self.counts.link_attrs += 1;
            }
        }
        // a content link to an object of another document: `idexternalSharedArticle:<path>?<n>` (a story: `idexternalcontent:`). The
        // path is the other document on the computer that made the link: a reference like any path (import), a path again (export)
        if name == "Link"
            && !handled_uri
            && let Some(a) = attrs.iter().find(|a| a.name == "LinkResourceURI")
        {
            let value = unescaped(&tag[a.value.clone()]);
            if let Some((head, path, tail)) = split_external_content_uri(&value)
                && let Some(f) = self.edits.other_uri.as_mut()
                && (if self.edits.native_paths { is_native_absolute(path) } else { path.starts_with("folio:") })
                && let Some(new) = f(path)
            {
                let new = if self.edits.native_paths { new } else { new.replace('\\', "/") };
                changes.push((a.value.clone(), escape_attr(&format!("{head}{new}{tail}"))));
                self.counts.other_uris += 1;
                handled_uri = true;
            }
        }
        for a in attrs {
            if handled_uri && a.name == "LinkResourceURI" {
                continue;
            }
            let raw = &tag[a.value.clone()];
            if uri_prefixes(&self.edits.prefixes).iter().any(|p| raw.contains(p))
                && let Some(new) = self.scrub(&unescaped(raw), false)
            {
                // the value was unescaped to be read (a JSON label has quotes, `&`, `<`): it is written escaped again as a whole
                changes.push((a.value.clone(), escape_attr(&new)));
            } else if self.edits.native_paths
                && a.name != "Self"
                && let Some(f) = self.edits.other_uri.as_mut()
            {
                let value = unescaped(raw);
                // the file of an imported PDF comment is written `$ID/<native path>`: the path is the reference, the prefix stays
                let (head, path) = match value.strip_prefix("$ID/") {
                    Some(rest) if a.name == "CommentFilePath" => ("$ID/", rest),
                    _ => ("", value.as_ref()),
                };
                if is_native_absolute(path)
                    && let Some(new) = f(path)
                {
                    changes.push((a.value.clone(), escape_attr(&format!("{head}{new}"))));
                    self.counts.other_uris += 1;
                }
            }
        }
        if changes.is_empty() && extra.is_empty() {
            return None;
        }
        changes.sort_by_key(|c| c.0.start);
        let mut out = String::with_capacity(tag.len() + 64);
        let mut at = 0;
        for (r, v) in changes {
            out.push_str(&tag[at..r.start]);
            out.push_str(&v);
            at = r.end;
        }
        out.push_str(&tag[at..]);
        if !extra.is_empty() {
            let close = if out.ends_with("/>") { out.len() - 2 } else { out.len() - 1 };
            let trimmed = out[..close].trim_end().len();
            let add: String = extra.iter().map(|(k, v)| format!(" {k}=\"{}\"", escape_attr(v))).collect();
            out.insert_str(trimmed, &add);
        }
        Some(out)
    }
}

/// Applies `edits` to one part. `None` when nothing changed (copy the part as it is).
pub fn rewrite(part: &str, text: &str, edits: &mut Edits<'_>) -> (Option<String>, Counts) {
    let mut rw = Rewriter { edits, counts: Counts::default(), xmp_part: part.ends_with("metadata.xml") };
    let mut out = String::with_capacity(text.len() + 1024);
    let mut stack: Vec<(&str, Frame)> = Vec::new();
    let mut attrs = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    let mut changed = false;
    while i < b.len() {
        if b[i] != b'<' {
            let end = text[i..].find('<').map_or(b.len(), |k| i + k);
            let chunk = &text[i..end];
            let in_xmp = rw.xmp_part || matches!(stack.last(), Some((_, Frame::Xmp)));
            match in_xmp.then(|| rw.scrub_text(chunk)).flatten() {
                Some(new) => {
                    out.push_str(&new);
                    changed = true;
                }
                None => out.push_str(chunk),
            }
            i = end;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").map_or(b.len(), |k| i + k + 3);
            let body = &text[i + 9..end.saturating_sub(3).max(i + 9)];
            let in_xmp = rw.xmp_part || matches!(stack.last(), Some((_, Frame::Xmp)));
            match in_xmp.then(|| rw.scrub_text(body)).flatten() {
                Some(new) => {
                    out.push_str("<![CDATA[");
                    out.push_str(&new);
                    out.push_str("]]>");
                    changed = true;
                }
                None => out.push_str(&text[i..end]),
            }
            i = end;
            continue;
        }
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(b.len(), |k| i + k + 3);
            out.push_str(&text[i..end]);
            i = end;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest.find('>').map_or(b.len(), |k| i + k + 1);
            out.push_str(&text[i..end]);
            i = end;
            continue;
        }
        // find the end of the tag, skipping quoted attribute values
        let mut j = i + 1;
        let mut quote = 0u8;
        while j < b.len() {
            let c = b[j];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                break;
            }
            j += 1;
        }
        let end = (j + 1).min(b.len());
        let tag = &text[i..end];
        i = end;
        if tag.starts_with("</") {
            match stack.pop().map(|f| f.1) {
                Some(Frame::Props { key, document, label_done, contents_done }) => {
                    if !contents_done {
                        let c = rw.contents_block(&key, document);
                        changed |= !c.is_empty();
                        out.push_str(&c);
                    }
                    if !label_done {
                        let l = rw.label_block(&key, document);
                        changed |= !l.is_empty();
                        out.push_str(&l);
                    }
                    if let Some((_, Frame::Target { props_done, .. })) = stack.last_mut() {
                        *props_done = true;
                    }
                }
                Some(Frame::Label { key, document }) => {
                    let pairs = labels_xml(rw.label_pairs(&key, document));
                    if !pairs.is_empty() {
                        rw.counts.labels += 1;
                        changed = true;
                    }
                    out.push_str(&pairs);
                }
                Some(Frame::Target { key, document, props_done: false, .. }) => {
                    let block = format!("{}{}", rw.contents_block(&key, document), rw.label_block(&key, document));
                    if !block.is_empty() {
                        out.push_str(&format!("<Properties>{block}</Properties>"));
                        changed = true;
                    }
                }
                _ => {}
            }
            out.push_str(tag);
            continue;
        }
        let (name, self_closing) = parse_tag(tag, &mut attrs);
        // children of a target: the Properties block comes first
        let mut before = String::new();
        let mut frame = Frame::Plain;
        let mut skip = false;
        match stack.last_mut().map(|f| &mut f.1) {
            Some(Frame::Target { key, document, first_child_seen, props_done }) if !*first_child_seen => {
                *first_child_seen = true;
                if name == "Properties" {
                    if self_closing {
                        let (k, d) = (key.clone(), *document);
                        *props_done = true;
                        let block = format!("{}{}", rw.contents_block(&k, d), rw.label_block(&k, d));
                        out.push_str(&format!("<Properties>{block}</Properties>"));
                        changed |= !block.is_empty();
                        continue;
                    }
                    frame = Frame::Props { key: key.clone(), document: *document, label_done: false, contents_done: false };
                } else {
                    let (k, d) = (key.clone(), *document);
                    *props_done = true;
                    let block = format!("{}{}", rw.contents_block(&k, d), rw.label_block(&k, d));
                    if !block.is_empty() {
                        before = format!("<Properties>{block}</Properties>");
                    }
                }
            }
            Some(Frame::Props { key, document, label_done, contents_done }) => {
                let (k, d) = (key.clone(), *document);
                if name == "Contents" {
                    *contents_done = true;
                } else if !*contents_done && matches!(name, "ClippingPathGeometry" | "GraphicBounds" | "Label") {
                    *contents_done = true;
                    before.push_str(&rw.contents_block(&k, d));
                }
                if name == "Label" {
                    *label_done = true;
                    if self_closing {
                        let pairs = labels_xml(rw.label_pairs(&k, d));
                        rw.counts.labels += usize::from(!pairs.is_empty());
                        out.push_str(&before);
                        out.push_str(&format!("<Label>{pairs}</Label>"));
                        changed = true;
                        continue;
                    }
                    frame = Frame::Label { key: k, document: d };
                }
            }
            Some(Frame::Label { key, document }) if name == "KeyValuePair" && self_closing => {
                // pairs we write replace those of the same key
                let k = attrs.iter().find(|a| a.name == "Key").map(|a| unescaped(&tag[a.value.clone()]));
                if k.is_some_and(|k| rw.label_pairs(key, *document).iter().any(|(ours, _)| ours == k.as_ref())) {
                    skip = true;
                }
            }
            _ => {}
        }
        if !before.is_empty() {
            changed = true;
            out.push_str(&before);
        }
        if skip {
            changed = true;
            continue;
        }
        let self_id = attrs.iter().find(|a| a.name == "Self").map(|a| unescaped(&tag[a.value.clone()]));
        if matches!(frame, Frame::Plain) {
            // labels go on any element by Self (page items: InDesign reads frame labels back
            // from IDML, not those of placed graphics); Contents only on graphics
            let is_target_graphic = self_id.as_deref().is_some_and(|id| {
                rw.edits.labels.contains_key(id) || (GRAPHIC_ELEMENTS.contains(&name) && rw.edits.contents_for.contains(id))
            });
            let is_document = name == "Document" && stack.is_empty() && part == "designmap.xml" && !rw.edits.document_labels.is_empty();
            if is_target_graphic || is_document {
                frame = Frame::Target { key: self_id.as_deref().unwrap_or_default().to_owned(), document: is_document, first_child_seen: false, props_done: false };
            } else if name == "Contents" && stack.len() >= 2 && stack[stack.len() - 1].0 == "Properties" && stack[stack.len() - 2].0 == "MetadataPacketPreference" {
                frame = Frame::Xmp;
            }
        }
        let new_tag = rw.rewrite_attrs(tag, name, &attrs, self_id.as_deref());
        changed |= new_tag.is_some();
        let tag_text = new_tag.as_deref().unwrap_or(tag);
        if self_closing {
            if let Frame::Target { key, document, .. } = &frame {
                let (k, d) = (key.clone(), *document);
                let block = format!("{}{}", rw.contents_block(&k, d), rw.label_block(&k, d));
                if !block.is_empty() {
                    let open = tag_text.trim_end_matches('>').trim_end().trim_end_matches('/').trim_end();
                    out.push_str(&format!("{open}><Properties>{block}</Properties></{name}>"));
                    changed = true;
                    continue;
                }
            }
            out.push_str(tag_text);
        } else {
            out.push_str(tag_text);
            stack.push((name, frame));
        }
    }
    let counts = rw.counts;
    (changed.then_some(out), counts)
}

/// Absolute paths left in a text (for the "no absolute paths in a .folio" check): `file:`
/// URIs, drive paths (`C:\`, `C:/`), UNC paths and the usual POSIX home / volume roots.
pub fn absolute_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut push = |start: usize| {
        let end = text[start..].find(|c: char| c == '"' || c == '<' || c == '\'' || c.is_whitespace()).map_or(text.len(), |k| start + k);
        out.push(text[start..end].chars().take(160).collect::<String>());
    };
    let mut k = 0;
    while let Some(p) = text[k..].find("file:") {
        push(k + p);
        k += p + 5;
    }
    for i in 0..b.len().saturating_sub(2) {
        let prev_ok = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b':' || b[i - 1] == b'/');
        if prev_ok && b[i].is_ascii_alphabetic() && b[i + 1] == b':' && (b[i + 2] == b'\\' || (b[i + 2] == b'/' && b.get(i + 3) != Some(&b'/'))) {
            push(i);
        }
        if prev_ok && b[i] == b'\\' && b[i + 1] == b'\\' && b[i + 2].is_ascii_alphanumeric() {
            push(i);
        }
    }
    for root in ["/Users/", "/Volumes/", "/home/"] {
        let mut k = 0;
        while let Some(p) = text[k..].find(root) {
            let at = k + p;
            let prev = if at == 0 { b' ' } else { b[at - 1] };
            if matches!(prev, b'"' | b'>' | b' ' | b'\n' | b'\t' | b'\'' | b'(') {
                push(at);
            }
            k = at + root.len();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPREAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<idPkg:Spread xmlns:idPkg="http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging" DOMVersion="20.5">
	<Spread Self="u1">
		<Rectangle Self="r1">
			<Image Self="g1" ItemTransform="1 0 0 1 0 0">
				<Properties>
					<Profile type="string">$ID/None</Profile>
					<GraphicBounds Left="0" Top="0" Right="10" Bottom="10" />
				</Properties>
				<Link Self="l1" LinkResourceURI="file:C:/Users/kim/a%20b.jpg" StoredState="Normal" LinkImportStamp="file 1 2" />
			</Image>
		</Rectangle>
		<Rectangle Self="r2">
			<Image Self="g2"><Properties><Contents><![CDATA[AAAA]]></Contents><GraphicBounds Left="0" Top="0" Right="1" Bottom="1" /><Label><KeyValuePair Key="mine" Value="1" /><KeyValuePair Key="folio.ref" Value="old" /></Label></Properties></Image>
		</Rectangle>
		<Rectangle Self="r3"><PDF Self="g3" /></Rectangle>
		<Rectangle Self="r4"><EPS Self="g4"><TextWrapPreference /></EPS></Rectangle>
		<TextFrame Self="t1" Note="see file:/Volumes/x/y.pdf" />
	</Spread>
</idPkg:Spread>"#;

    #[test]
    fn a_content_link_to_another_document_keeps_no_absolute_path() {
        let link = |uri: &str| format!(r#"<Spread><TextFrame Self="t"><Link Self="l1" LinkResourceURI="{uri}" LinkResourceFormat="$ID/External Linked Object" /></TextFrame></Spread>"#);
        let part = link("idexternalSharedArticle:C:/Users/kim/My Docs/a.indd?18927");
        // import: the path of the other document becomes a reference, the resource number stays
        let mut seen = Vec::new();
        let mut take = |u: &str| {
            seen.push(u.to_string());
            Some("folio:path-ref/r4".to_string())
        };
        let mut import = Edits { other_uri: Some(&mut take), native_paths: true, ..Default::default() };
        let (out, counts) = rewrite("Spreads/Spread_u1.xml", &part, &mut import);
        let out = out.unwrap();
        assert_eq!(seen, vec!["C:/Users/kim/My Docs/a.indd".to_string()]);
        assert_eq!(counts.other_uris, 1);
        assert!(out.contains(r#"LinkResourceURI="idexternalSharedArticle:folio:path-ref/r4?18927""#), "{out}");
        assert!(absolute_paths(&out).is_empty(), "{:?}", absolute_paths(&out));
        // export: back to the path of the document (InDesign's own spelling: forward slashes)
        let mut give = |u: &str| (u == "folio:path-ref/r4").then(|| r"D:\Work\a.indd".to_string());
        let mut export = Edits { other_uri: Some(&mut give), prefixes: vec!["folio:ref/", "folio:url-ref/", "folio:path-ref/"], ..Default::default() };
        let (back, _) = rewrite("Spreads/Spread_u1.xml", &out, &mut export);
        assert!(back.unwrap().contains(r#"LinkResourceURI="idexternalSharedArticle:D:/Work/a.indd?18927""#));
        // a story link and a path without a number
        assert_eq!(split_external_content_uri("idexternalcontent:C:/a.indd?4276"), Some(("idexternalcontent:", "C:/a.indd", "?4276")));
        assert_eq!(split_external_content_uri("idexternalcontent:C:/a b?c.indd"), Some(("idexternalcontent:", "C:/a b?c.indd", "")));
        assert_eq!(split_external_content_uri("idinternalcontent:4276"), None);
    }

    /// An attribute value that holds a reference and also characters XML must escape (a JSON label: quotes, `&`, `<`) goes out
    /// as well-formed XML, with the reference replaced and the rest as it was (dbmerge-a found the quotes came out raw).
    #[test]
    fn an_attribute_with_a_reference_and_quotes_stays_well_formed() {
        let json = r#"{"snapshot":"folio:path-ref/r7","note":"a & b < c","name":"잡지DB"}"#;
        let part = format!(r#"<Document Self="d"><Label><KeyValuePair Key="folio.datamerge" Value="{}" /></Label></Document>"#, escape_attr(json));
        let mut give = |u: &str| (u == "folio:path-ref/r7").then(|| r"D:\Work\assets\a&b.csv".to_string());
        let mut export = Edits { other_uri: Some(&mut give), prefixes: vec!["folio:ref/", "folio:url-ref/", "folio:path-ref/"], ..Default::default() };
        let (out, counts) = rewrite("designmap.xml", &part, &mut export);
        let out = out.unwrap();
        assert_eq!(counts.other_uris, 1);
        // the text reads back as the same JSON with the path in it: the value is well-formed and unescapes to what was written
        let start = out.find("Value=\"").unwrap() + 7;
        let end = out[start..].find('"').map(|i| start + i).expect("the attribute value ends at a quote of its own");
        let value = unescape(&out[start..end]);
        assert_eq!(value, json.replace("folio:path-ref/r7", r"D:\Work\assets\a&b.csv"), "{out}");
        assert!(out[start..end].contains("&quot;") && out[start..end].contains("&amp;") && out[start..end].contains("&lt;"), "{out}");
        assert!(out.ends_with(r#"" /></Label></Document>"#), "{out}");
    }

    #[test]
    fn the_file_of_a_pdf_comment_keeps_no_absolute_path() {
        let part = r#"<Document Self="d"><PDFComment Self="u1" CommentFilePath="$ID/C:\Review\a b.pdf" /><PDFComment Self="u2" CommentFilePath="$ID//Users/kim/a.pdf" /></Document>"#;
        let mut seen = Vec::new();
        let mut take = |u: &str| {
            seen.push(u.to_string());
            Some(format!("folio:path-ref/r{}", seen.len()))
        };
        let mut import = Edits { other_uri: Some(&mut take), native_paths: true, ..Default::default() };
        let (out, counts) = rewrite("designmap.xml", part, &mut import);
        let out = out.unwrap();
        assert_eq!(seen, vec![r"C:\Review\a b.pdf".to_string(), "/Users/kim/a.pdf".to_string()]);
        assert_eq!(counts.other_uris, 2);
        assert!(out.contains(r#"CommentFilePath="$ID/folio:path-ref/r1""#) && out.contains(r#"CommentFilePath="$ID/folio:path-ref/r2""#), "{out}");
        assert!(absolute_paths(&out).is_empty(), "{:?}", absolute_paths(&out));
        // export: InDesign's own spelling again
        let mut give = |u: &str| (u == "folio:path-ref/r1").then(|| r"C:\Review\a b.pdf".to_string());
        let mut export = Edits { other_uri: Some(&mut give), prefixes: vec!["folio:ref/", "folio:url-ref/", "folio:path-ref/"], ..Default::default() };
        let (back, _) = rewrite("designmap.xml", &out, &mut export);
        assert!(back.unwrap().contains(r#"CommentFilePath="$ID/C:\Review\a b.pdf""#));
        // another attribute that starts with `$ID/` is not a path
        let mut none = |_: &str| -> Option<String> { panic!("not a path") };
        let mut other = Edits { other_uri: Some(&mut none), native_paths: true, ..Default::default() };
        assert!(rewrite("designmap.xml", r#"<Document Self="d"><Language Name="$ID/C:\x"/></Document>"#, &mut other).0.is_none());
    }

    #[test]
    fn links_labels_and_contents() {
        let mut edits = Edits::default();
        edits.link_uris.insert("l1".into(), "folio:ref/r1".into());
        for g in ["g1", "g2", "g3", "g4"] {
            edits.labels.insert(g.into(), vec![("folio.ref".into(), format!("{{\"id\":\"{g}\",\"path\":\"assets/a b.jpg\"}}"))]);
        }
        let get = |k: &str| (k == "g1" || k == "g2").then(|| vec![1u8, 2, 3]);
        edits.contents = Some(&get);
        edits.contents_for = ["g1".to_string(), "g2".to_string()].into_iter().collect();
        let mut seen = Vec::new();
        let mut other = |u: &str| {
            seen.push(u.to_string());
            Some("folio:ref/r9".to_string())
        };
        edits.other_uri = Some(&mut other);
        let (out, counts) = rewrite("Spreads/Spread_u1.xml", SPREAD, &mut edits);
        let out = out.unwrap();
        assert_eq!(counts.link_uris, 1);
        assert_eq!(counts.labels, 4);
        assert_eq!(counts.contents, 1, "g2 already has Contents");
        assert_eq!(counts.other_uris, 1);
        assert_eq!(seen, vec!["file:/Volumes/x/y.pdf".to_string()]);
        assert!(out.contains(r#"LinkResourceURI="folio:ref/r1""#));
        // g1: Contents before GraphicBounds, Label at the end of Properties
        let g1 = &out[out.find(r#"<Image Self="g1""#).unwrap()..out.find("</Image>").unwrap()];
        let (c, gb, l) = (g1.find("<Contents>").unwrap(), g1.find("<GraphicBounds").unwrap(), g1.find("<Label>").unwrap());
        assert!(c < gb && gb < l, "{g1}");
        assert!(g1.contains("AQID"));
        // g2: our key replaced, the other kept, existing contents untouched
        let g2 = &out[out.find(r#"<Image Self="g2""#).unwrap()..];
        let g2 = &g2[..g2.find("</Image>").unwrap()];
        assert!(g2.contains(r#"Key="mine""#) && !g2.contains(r#"Value="old""#) && g2.contains("AAAA") && !g2.contains("AQID"), "{g2}");
        assert_eq!(g2.matches("folio.ref").count(), 1);
        // g3: self-closing graphic gets a Properties block; g4: Properties before its first child
        assert!(out.contains(r#"<PDF Self="g3"><Properties><Label><KeyValuePair Key="folio.ref""#), "{out}");
        assert!(out.contains(r#"<EPS Self="g4"><Properties><Label>"#));
        assert!(out.contains("folio:ref/r9"));
        // well-formed: the idml crate's own reader parses it
        assert!(out.matches("<Properties>").count() == out.matches("</Properties>").count());
        // unchanged parts come back as None
        let mut none = Edits::default();
        assert!(rewrite("Spreads/x.xml", SPREAD, &mut none).0.is_none());
    }

    #[test]
    fn document_labels_and_xmp() {
        let dm = r#"<?xml version="1.0"?><Document Self="d" DOMVersion="20.5"><Properties><Label><KeyValuePair Key="kAdobeDPS_Version" Value="2" /></Label></Properties><Language Self="x" /></Document>"#;
        let mut e = Edits { document_labels: vec![("folio.document".into(), "{\"id\":\"u-1\"}".into())], ..Default::default() };
        let (out, c) = rewrite("designmap.xml", dm, &mut e);
        let out = out.unwrap();
        assert_eq!(c.labels, 1);
        assert!(out.contains(r#"<KeyValuePair Key="kAdobeDPS_Version" Value="2" /><KeyValuePair Key="folio.document" Value="{&quot;id&quot;:&quot;u-1&quot;}" /></Label>"#), "{out}");
        let dm2 = r#"<Document Self="d"><Language Self="x" /></Document>"#;
        let mut e2 = Edits { document_labels: vec![("folio.document".into(), "v".into())], ..Default::default() };
        assert!(rewrite("designmap.xml", dm2, &mut e2).0.unwrap().starts_with(r#"<Document Self="d"><Properties><Label>"#));

        let xmp = "<x:xmpmeta><stRef:filePath>file:///C:/Users/editor/Links/merge.csv</stRef:filePath><stRef:lastURL>file:///C:/Users/editor/Links/a.pdf</stRef:lastURL></x:xmpmeta>";
        let mut n = 0;
        let mut f = |u: &str| {
            n += 1;
            Some(format!("folio:url-ref/r{}", u.len()))
        };
        let mut e3 = Edits { other_uri: Some(&mut f), ..Default::default() };
        let (out, c) = rewrite("META-INF/metadata.xml", xmp, &mut e3);
        assert_eq!(c.other_uris, 2);
        assert!(absolute_paths(&out.unwrap()).is_empty());
        assert_eq!(n, 2);
    }

    #[test]
    fn finds_absolute_paths() {
        let t = r#"<a x="file:C:/a.jpg" y="D:\x\y" z="\\nas\s\a" w="/Volumes/m/a.pdf" v="http://example.com" u="ratio 3:2" />"#;
        let found = absolute_paths(t);
        assert_eq!(found.len(), 4, "{found:?}");
        assert!(absolute_paths(r#"<a href="https://x.y/z" t="folio:ref/r1" />"#).is_empty());
    }

    #[test]
    fn base64_matches_indesign_layout() {
        assert_eq!(base64_lines(b""), "");
        assert_eq!(base64_lines(b"f"), "Zg==");
        assert_eq!(base64_lines(b"foobar"), "Zm9vYmFy");
        let long = base64_lines(&[0u8; 120]);
        assert_eq!(long.split('\n').next().unwrap().len(), 76);
        assert!(!long.ends_with('\n'));
        assert_eq!(unescape("a&amp;b&quot;&#x41;&#66;&lt;"), "a&b\"AB<");
    }
}
