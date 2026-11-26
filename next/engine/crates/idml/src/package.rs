//! Reading the IDML ZIP in two parallel phases: decompress every part the designmap
//! references into its own buffer, then parse each buffer into a tree that borrows from it.
//! Only parts named by the designmap are opened; link URIs are never followed.

use crate::Error;
use crate::xml::{self, Element};
use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Seek};
use crate::par::{parallel, workers};
use zip::ZipArchive;

/// Largest single XML part accepted (uncompressed).
const MAX_PART_BYTES: u64 = 512 * 1024 * 1024;
/// Largest total of parsed parts (uncompressed).
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MIMETYPE: &str = "application/vnd.adobe.indesign-idml-package";

/// Decompressed XML text of the designmap and the parts it references.
pub struct Texts {
    designmap: String,
    /// (path, text), largest first.
    parts: Vec<(String, String)>,
    /// `META-INF/metadata.xml` (the dates the text variables read), empty without one.
    metadata: String,
}

impl Texts {
    /// Give the editor the text already inflated for the model, along with every other
    /// package part it preserves on export.
    pub fn into_editor_parts(self, bytes: &[u8]) -> Result<(BTreeMap<String, String>, bool), Error> {
        let references_private_part = self.parts.iter().any(|(path, _)| path.starts_with("folio/"));
        let has_metadata = !self.metadata.is_empty();
        let mut ready: BTreeMap<_, _> = self.parts.into_iter().collect();
        ready.insert("designmap.xml".into(), self.designmap);
        if has_metadata {
            ready.insert("META-INF/metadata.xml".into(), self.metadata);
        }
        let (parts, same_names) = editor_parts(Cursor::new(bytes), ready)?;
        // The editor excludes private parts and still reads metadata above the layout reader's limit.
        let same_parts = same_names && !references_private_part
            && (has_metadata || parts.get("META-INF/metadata.xml").is_none_or(String::is_empty));
        Ok((parts, same_parts))
    }
}

pub fn read_editor_parts(file: impl Read + Seek) -> Result<BTreeMap<String, String>, Error> {
    editor_parts(file, BTreeMap::new()).map(|(parts, _)| parts)
}

fn editor_parts(file: impl Read + Seek, mut ready: BTreeMap<String, String>) -> Result<(BTreeMap<String, String>, bool), Error> {
    let mut archive = ZipArchive::new(file).map_err(|e| Error::Package(e.to_string()))?;
    let mut parts = BTreeMap::new();
    let mut same_names = true;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| Error::Package(e.to_string()))?;
        let name = entry.name().map_err(|e| Error::Package(e.to_string()))?.into_owned();
        if entry.is_dir() || name.starts_with("folio/") {
            continue;
        }
        // ZIP name lookup uses raw bytes, while editor parts are keyed by decoded names.
        let cached = if entry.name_raw() == name.as_bytes() {
            ready.remove(&name)
        } else {
            same_names = false;
            None
        };
        let text = if let Some(text) = cached {
            text
        } else {
            let mut text = String::new();
            entry.read_to_string(&mut text).map_err(|e| Error::Xml(format!("{name}: {e}")))?;
            text
        };
        if parts.insert(name, text).is_some() {
            same_names = false;
        }
    }
    Ok((parts, same_names && ready.is_empty()))
}

/// Parsed parts, borrowing from `Texts`.
pub struct Package<'a> {
    pub designmap: Element<'a>,
    parts: HashMap<&'a str, Element<'a>>,
    metadata: &'a str,
}

impl<'a> Package<'a> {
    /// `META-INF/metadata.xml`, empty when the package has none.
    pub fn metadata(&self) -> &'a str {
        self.metadata
    }

    pub fn part(&self, path: &str) -> Option<&Element<'a>> {
        self.parts.get(path)
    }
}

type Archive<'a> = ZipArchive<Cursor<&'a [u8]>>;

fn read_part(archive: &mut Archive<'_>, path: &str) -> Result<String, Error> {
    let mut file = archive.by_name(path).map_err(|e| Error::Package(format!("{path}: {e}")))?;
    let size = file.size();
    if size > MAX_PART_BYTES {
        return Err(Error::Limit(format!("{path} expands to {size} bytes")));
    }
    // Decompress straight into the buffer the parser borrows from (no further copies).
    let mut bytes = Vec::with_capacity(size as usize);
    file.by_ref()
        .take(MAX_PART_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Package(format!("{path}: {e}")))?;
    if bytes.len() as u64 > MAX_PART_BYTES {
        return Err(Error::Limit(format!("{path} expands beyond {MAX_PART_BYTES} bytes")));
    }
    String::from_utf8(bytes).map_err(|_| Error::Xml(format!("{path}: not UTF-8")))
}

/// Parts above this size parse their top-level items on several threads.
const SPLIT_BYTES: usize = 1 << 20;

pub(crate) fn parse_text<'a>(path: &str, text: &'a str) -> Result<Element<'a>, Error> {
    let tree = if text.len() > SPLIT_BYTES { xml::parse_split(text, workers(16)) } else { xml::parse(text) };
    tree.map_err(|e| Error::Xml(format!("{path}: {}", e.0)))
}

pub fn decompress(bytes: &[u8]) -> Result<Texts, Error> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Package(format!("not a ZIP package: {e}")))?;
    if archive.index_for_name("mimetype").is_some() {
        let mt = read_part(&mut archive, "mimetype")?;
        if mt.trim() != MIMETYPE {
            return Err(Error::Package(format!("mimetype is {:?}, not an IDML package", mt.trim())));
        }
    }
    if archive.index_for_name("designmap.xml").is_none() {
        return Err(Error::Package("designmap.xml is missing".into()));
    }
    let designmap = read_part(&mut archive, "designmap.xml")?;
    // Part paths the designmap references (`idPkg:*` elements with `src`). XML/ parts
    // (tags, backing story) carry no layout data.
    let mut paths: Vec<(u64, String)> = Vec::new();
    {
        let dm = parse_text("designmap.xml", &designmap)?;
        if dm.name != "Document" {
            return Err(Error::Package(format!("designmap root is <{}>, expected <Document>", dm.name)));
        }
        for e in dm.elements() {
            let Some(src) = e.attr("src") else { continue };
            if src.starts_with("XML/") || paths.iter().any(|(_, p)| p == src) {
                continue;
            }
            if let Some(i) = archive.index_for_name(src) {
                let size = archive.by_index(i).map(|f| f.size()).unwrap_or(0);
                paths.push((size, src.to_owned()));
            }
        }
    }
    let total = paths.iter().fold(0u64, |t, (s, _)| t.saturating_add(*s));
    if total > MAX_TOTAL_BYTES {
        return Err(Error::Limit(format!("package parts expand to {total} bytes")));
    }
    // Largest first so one big spread does not start last.
    paths.sort_by_key(|(size, _)| std::cmp::Reverse(*size));
    let texts = parallel(paths.len(), || archive.clone(), |a, i| read_part(a, &paths[i].1));
    if texts.len() != paths.len() {
        return Err(Error::Package("a decompression thread failed".into()));
    }
    let mut parts = Vec::with_capacity(paths.len());
    for ((_, path), text) in paths.into_iter().zip(texts) {
        parts.push((path, text?));
    }
    // the XMP packet is small and optional: a package without one still reads
    let metadata = read_part(&mut archive, "META-INF/metadata.xml").unwrap_or_default();
    Ok(Texts { designmap, parts, metadata })
}

/// Parts already in memory (an editor's), by path: the designmap and the parts it
/// references, as `decompress` would take them from a ZIP.
pub fn from_parts(parts: &std::collections::BTreeMap<String, String>) -> Result<Texts, Error> {
    let designmap = parts.get("designmap.xml").ok_or_else(|| Error::Package("designmap.xml is missing".into()))?.clone();
    let mut paths: Vec<String> = Vec::new();
    {
        let dm = parse_text("designmap.xml", &designmap)?;
        if dm.name != "Document" {
            return Err(Error::Package(format!("designmap root is <{}>, expected <Document>", dm.name)));
        }
        for e in dm.elements() {
            let Some(src) = e.attr("src") else { continue };
            if src.starts_with("XML/") || paths.iter().any(|p| p == src) || !parts.contains_key(src) {
                continue;
            }
            paths.push(src.to_owned());
        }
    }
    paths.sort_by_key(|p| std::cmp::Reverse(parts[p].len()));
    Ok(Texts { designmap, parts: paths.into_iter().map(|p| { let t = parts[&p].clone(); (p, t) }).collect(), metadata: parts.get("META-INF/metadata.xml").cloned().unwrap_or_default() })
}

pub fn parse(texts: &Texts) -> Result<Package<'_>, Error> {
    let designmap = parse_text("designmap.xml", &texts.designmap)?;
    let parsed = parallel(texts.parts.len(), || (), |_, i| {
        let (path, text) = &texts.parts[i];
        parse_text(path, text)
    });
    if parsed.len() != texts.parts.len() {
        return Err(Error::Package("a parser thread failed".into()));
    }
    let mut parts = HashMap::with_capacity(parsed.len());
    for ((path, _), e) in texts.parts.iter().zip(parsed) {
        parts.insert(path.as_str(), e?);
    }
    Ok(Package { designmap, parts, metadata: &texts.metadata })
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;

    #[test]
    fn reusing_layout_text_preserves_editor_parts_and_detects_private_references() {
        for src in ["Stories/main.xml", "folio/private.xml"] {
            let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let designmap = format!("<Document><Story src=\"{src}\"/></Document>");
            for (name, text) in [
                ("designmap.xml", designmap.as_str()),
                ("Stories/main.xml", "<Story/>"),
                ("folio/private.xml", "<Story/>"),
                ("XML/Custom.xml", "<Other unknown='preserve'/>"),
            ] {
                archive.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                archive.write_all(text.as_bytes()).unwrap();
            }
            let bytes = archive.finish().unwrap().into_inner();
            let (parts, shared) = decompress(&bytes).unwrap().into_editor_parts(&bytes).unwrap();
            assert_eq!(parts, read_editor_parts(Cursor::new(&bytes)).unwrap());
            assert_eq!(parts["XML/Custom.xml"], "<Other unknown='preserve'/>");
            assert!(!parts.contains_key("folio/private.xml"));
            assert_eq!(shared, src == "Stories/main.xml");
        }
    }

    #[test]
    fn invalid_archives_and_missing_designmap_keep_context() {
        for bytes in [b"".as_slice(), b"PK\x03\x04", b"not a package"] {
            assert!(matches!(decompress(bytes), Err(Error::Package(m)) if m.starts_with("not a ZIP package:")));
        }
        assert!(matches!(from_parts(&BTreeMap::new()), Err(Error::Package(m)) if m == "designmap.xml is missing"));
        let wrong = BTreeMap::from([("designmap.xml".into(), "<Story/>".into())]);
        assert!(matches!(from_parts(&wrong), Err(Error::Package(m)) if m == "designmap root is <Story>, expected <Document>"));
    }

    #[test]
    fn truncated_parts_report_their_path_and_absent_references_stay_optional() {
        let mut parts = BTreeMap::from([
            ("designmap.xml".into(), "<Document><Story src=\"Stories/broken.xml\"/></Document>".into()),
            ("Stories/broken.xml".into(), "<Story><Content>cut off".into()),
        ]);
        let texts = from_parts(&parts).unwrap();
        assert!(matches!(parse(&texts), Err(Error::Xml(m)) if m.starts_with("Stories/broken.xml:")));
        parts.remove("Stories/broken.xml");
        let texts = from_parts(&parts).unwrap();
        assert!(parse(&texts).unwrap().part("Stories/broken.xml").is_none());
        parts.insert("designmap.xml".into(), "<Document>".into());
        assert!(matches!(from_parts(&parts), Err(Error::Xml(m)) if m.starts_with("designmap.xml:")));
    }
}
