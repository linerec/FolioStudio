//! The XMP packet a file carries (build-a): JPEG APP1, TIFF tag 700, PSD resource 1060, PNG `iTXt XML:com.adobe.xmp`, the metadata
//! stream of a PDF or an Illustrator file, an EPS or SVG: all of them hold the packet as plain text, so the file is searched for
//! `<x:xmpmeta` (older writers: `<x:xapmeta`) up to its end tag. A file of more than 64 MB is searched in its first 16 MB and last 4 MB.
//! A PDF whose metadata stream is compressed gives none.
//!
//! [`XmpFields`] are the properties captions and the Links panel use, read from either form RDF allows (an attribute of
//! `rdf:Description` or an element; arrays `rdf:Alt`, `rdf:Bag`, `rdf:Seq` give their items, `x-default` first).
use serde::{Deserialize, Serialize};
use std::path::Path;

const WHOLE: u64 = 64 << 20;
const HEAD: usize = 16 << 20;
const TAIL: usize = 4 << 20;

/// The packet text (from `<x:xmpmeta` to `</x:xmpmeta>`), or none.
pub fn read_packet(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size <= WHOLE {
        let mut bytes = Vec::with_capacity(size as usize);
        file.read_to_end(&mut bytes).ok()?;
        return find_packet(&bytes);
    }
    let mut head = vec![0u8; HEAD];
    file.read_exact(&mut head).ok()?;
    if let Some(p) = find_packet(&head) {
        return Some(p);
    }
    let mut tail = vec![0u8; TAIL];
    file.seek(SeekFrom::End(-(TAIL as i64))).ok()?;
    file.read_exact(&mut tail).ok()?;
    find_packet(&tail)
}

/// The first packet in `bytes`.
pub fn find_packet(bytes: &[u8]) -> Option<String> {
    for (open, close) in [(&b"<x:xmpmeta"[..], &b"</x:xmpmeta>"[..]), (b"<x:xapmeta", b"</x:xapmeta>")] {
        if let Some(start) = find(bytes, open, 0)
            && let Some(end) = find(bytes, close, start)
        {
            return String::from_utf8(bytes[start..end + close.len()].to_vec()).ok();
        }
    }
    None
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let first = needle[0];
    let mut i = from;
    while i + needle.len() <= hay.len() {
        let k = hay[i..hay.len() - needle.len() + 1].iter().position(|b| *b == first)?;
        i += k;
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The properties of a packet that captions and the Links panel show. Empty: the packet does not say it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmpFields {
    /// `dc:title`
    pub title: String,
    /// `dc:description` (the IPTC caption)
    pub description: String,
    /// `dc:creator`, in order
    pub creator: Vec<String>,
    /// `dc:rights` (the copyright notice)
    pub rights: String,
    /// `dc:subject` (the keywords)
    pub keywords: Vec<String>,
    /// `photoshop:Headline`
    pub headline: String,
    /// `photoshop:Credit`
    pub credit: String,
    /// `photoshop:Source`
    pub source: String,
    /// `photoshop:City`, `photoshop:State`, `photoshop:Country`
    pub city: String,
    pub state: String,
    pub country: String,
    /// `photoshop:AuthorsPosition`
    pub author_title: String,
    /// `photoshop:CaptionWriter`
    pub caption_writer: String,
    /// `photoshop:Instructions`
    pub instructions: String,
    /// `photoshop:DateCreated` (or `xmp:CreateDate`)
    pub date_created: String,
    /// `xmp:CreatorTool`
    pub creator_tool: String,
    /// `xmpRights:WebStatement` (the copyright URL)
    pub rights_url: String,
    /// `Iptc4xmpCore:Location`
    pub location: String,
    /// `xmp:Rating`
    pub rating: String,
    /// `exif:DateTimeOriginal`
    pub capture_date: String,
    /// `tiff:Make` and `tiff:Model`
    pub camera: String,
    /// `aux:Lens` (or `exifEX:LensModel`)
    pub lens: String,
}

impl XmpFields {
    pub fn parse(packet: &str) -> Self {
        let one = |q: &str| values(packet, q).into_iter().next().unwrap_or_default();
        let date = one("photoshop:DateCreated");
        Self {
            title: one("dc:title"),
            description: one("dc:description"),
            creator: values(packet, "dc:creator"),
            rights: one("dc:rights"),
            keywords: values(packet, "dc:subject"),
            headline: one("photoshop:Headline"),
            credit: one("photoshop:Credit"),
            source: one("photoshop:Source"),
            city: one("photoshop:City"),
            state: one("photoshop:State"),
            country: one("photoshop:Country"),
            author_title: one("photoshop:AuthorsPosition"),
            caption_writer: one("photoshop:CaptionWriter"),
            instructions: one("photoshop:Instructions"),
            date_created: if date.is_empty() { one("xmp:CreateDate") } else { date },
            creator_tool: one("xmp:CreatorTool"),
            rights_url: one("xmpRights:WebStatement"),
            location: one("Iptc4xmpCore:Location"),
            rating: one("xmp:Rating"),
            capture_date: one("exif:DateTimeOriginal"),
            camera: [one("tiff:Make"), one("tiff:Model")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "),
            lens: Some(one("aux:Lens")).filter(|s| !s.is_empty()).unwrap_or_else(|| one("exifEX:LensModel")),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The value of a caption metadata name as the Caption Setup window names them (`Description`, `Author`, `Copyright Notice` …);
    /// `None` for a name that is not an XMP property.
    pub fn caption_value(&self, name: &str) -> Option<String> {
        Some(match name {
            "Description" => self.description.clone(),
            "Title" => self.title.clone(),
            "Author" => self.creator.join("; "),
            // the Links panel's Creator is the program that made the file
            "Creator" | "Creator Tool" => self.creator_tool.clone(),
            "Location" => self.location.clone(),
            "Rating" => self.rating.clone(),
            "Capture Date" => self.capture_date.clone(),
            "Camera" => self.camera.clone(),
            "Lens" => self.lens.clone(),
            "Copyright" | "Copyright Notice" => self.rights.clone(),
            "Copyright Info URL" => self.rights_url.clone(),
            "Keywords" => self.keywords.join("; "),
            "Headline" => self.headline.clone(),
            "Credit" => self.credit.clone(),
            "Source" => self.source.clone(),
            "City" => self.city.clone(),
            "State" | "State/Province" => self.state.clone(),
            "Country" => self.country.clone(),
            "Author Title" | "Author's Position" => self.author_title.clone(),
            "Description Writer" | "Caption Writer" => self.caption_writer.clone(),
            "Instructions" => self.instructions.clone(),
            "Date Created" => self.date_created.clone(),
            _ => return None,
        })
    }
}

/// The values of the property `q` (a prefixed name as written): an attribute `q="…"`, or an element whose text is the value or whose
/// `rdf:li` items are (the `x-default` item first).
fn values(packet: &str, q: &str) -> Vec<String> {
    let attr = format!("{q}=");
    let mut at = 0;
    while let Some(k) = packet[at..].find(&attr) {
        let i = at + k;
        let before = packet[..i].chars().next_back();
        let rest = &packet[i + attr.len()..];
        if before.is_some_and(char::is_whitespace)
            && let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'')
            && let Some(end) = rest[1..].find(quote)
        {
            return vec![unescape(&rest[1..1 + end])];
        }
        at = i + attr.len();
    }
    let open = format!("<{q}");
    let close = format!("</{q}>");
    let mut at = 0;
    while let Some(k) = packet[at..].find(&open) {
        let i = at + k + open.len();
        let next = packet[i..].chars().next();
        if !next.is_some_and(|c| c == '>' || c == '/' || c.is_whitespace()) {
            at = i;
            continue;
        }
        let Some(gt) = packet[i..].find('>') else { break };
        if packet[..i + gt].ends_with('/') {
            return Vec::new();
        }
        let body_start = i + gt + 1;
        let Some(end) = packet[body_start..].find(&close) else { break };
        let body = &packet[body_start..body_start + end];
        if body.contains("<rdf:li") {
            let mut items: Vec<(bool, String)> = Vec::new();
            let mut rest = body;
            while let Some(s) = rest.find("<rdf:li") {
                let tag_end = rest[s..].find('>').map(|g| s + g);
                let Some(tag_end) = tag_end else { break };
                let tag = &rest[s..tag_end];
                let Some(e) = rest[tag_end..].find("</rdf:li>") else { break };
                let text = unescape(strip_tags(&rest[tag_end + 1..tag_end + e]).trim());
                if !text.is_empty() {
                    items.push((tag.contains("x-default"), text));
                }
                rest = &rest[tag_end + e..];
            }
            items.sort_by_key(|(default, _)| !*default);
            return items.into_iter().map(|(_, t)| t).collect();
        }
        let text = unescape(strip_tags(body).trim());
        return if text.is_empty() { Vec::new() } else { vec![text] };
    }
    Vec::new()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(semi) = tail.find(';').filter(|k| *k <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..semi];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKET: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
  photoshop:Headline="Spring &amp; rain" photoshop:City='Seoul'>
 <dc:title><rdf:Alt><rdf:li xml:lang="ko">봄</rdf:li><rdf:li xml:lang="x-default">Spring</rdf:li></rdf:Alt></dc:title>
 <dc:description><rdf:Alt><rdf:li xml:lang="x-default">A street in the rain</rdf:li></rdf:Alt></dc:description>
 <dc:creator><rdf:Seq><rdf:li>Kim</rdf:li><rdf:li>Lee</rdf:li></rdf:Seq></dc:creator>
 <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">&#xA9; 2026 Kim</rdf:li></rdf:Alt></dc:rights>
 <dc:subject><rdf:Bag><rdf:li>rain</rdf:li><rdf:li>city</rdf:li></rdf:Bag></dc:subject>
 <photoshop:Credit>Folio Photo</photoshop:Credit>
</rdf:Description></rdf:RDF></x:xmpmeta>
<?xpacket end="w"?>"#;

    #[test]
    fn fields_in_attribute_and_element_form() {
        let f = XmpFields::parse(PACKET);
        assert_eq!(f.title, "Spring");
        assert_eq!(f.description, "A street in the rain");
        assert_eq!(f.creator, ["Kim", "Lee"]);
        assert_eq!(f.rights, "\u{a9} 2026 Kim");
        assert_eq!(f.keywords, ["rain", "city"]);
        assert_eq!(f.headline, "Spring & rain");
        assert_eq!(f.city, "Seoul");
        assert_eq!(f.credit, "Folio Photo");
        assert_eq!(f.caption_value("Author").as_deref(), Some("Kim; Lee"));
        assert_eq!(f.caption_value("Copyright Notice").as_deref(), Some("\u{a9} 2026 Kim"));
        assert_eq!(f.caption_value("Name"), None);
        assert!(XmpFields::parse("<x:xmpmeta/>").is_empty());
    }

    #[test]
    fn the_packet_is_found_inside_any_file() {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 0];
        jpeg.extend(b"http://ns.adobe.com/xap/1.0/\0<?xpacket begin=''?>");
        jpeg.extend(PACKET.split_once("?>\n").unwrap().1.as_bytes());
        jpeg.extend([0xFF, 0xD9]);
        let p = find_packet(&jpeg).unwrap();
        assert!(p.starts_with("<x:xmpmeta") && p.ends_with("</x:xmpmeta>"));
        assert_eq!(XmpFields::parse(&p).description, "A street in the rain");
        assert_eq!(find_packet(b"%PDF-1.7 no metadata"), None);
        let dir = std::env::temp_dir().join(format!("folio-xmp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.pdf");
        std::fs::write(&file, [b"%PDF-1.4\n1 0 obj<</Type/Metadata/Subtype/XML>>stream\n".as_slice(), PACKET.as_bytes(), b"\nendstream"].concat()).unwrap();
        assert_eq!(read_packet(&file).map(|p| XmpFields::parse(&p).title), Some("Spring".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
