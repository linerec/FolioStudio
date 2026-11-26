//! The `.folio` document: an IDML package plus Folio parts.
//!
//! ```text
//! <name>.folio  (ZIP; `mimetype` first and stored, as IDML requires)
//!   mimetype                       application/vnd.adobe.indesign-idml-package
//!   designmap.xml, Spreads/, MasterSpreads/, Stories/, Resources/, XML/, META-INF/
//!                                  the IDML parts the importer produced, with these edits:
//!                                  every absolute `file:` URI → `folio:ref/<id>` (Link
//!                                  attributes; `folio:url-ref/<id>` inside XMP; native
//!                                  paths such as `Movie/@FilePath` → `folio:path-ref/<id>`), and
//!                                  `folio.*` labels on graphics and on the Document
//!   folio/manifest.json            this format's manifest (below), versioned
//!   folio/saved-lines.json         InDesign's saved composition (reader sidecar, unchanged)
//!   folio/import-report.json       what the import did and what InDesign features differ
//! ```
//!
//! The manifest is the authority for every reference: a Folio stable id → root-relative path,
//! SHA-256, size and kind (`link`, `embedded`, `preview`, `font`, `file`). Absolute paths
//! appear nowhere, except `hint` — the original URI of a file that was not in the project
//! at import (a missing link, or the source of an embedded link) — which is only ever shown
//! and written back to IDML exports.

use crate::paths::RelPath;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{Read, Seek, Write};
use std::path::Path;

pub const EXTENSION: &str = "folio";
pub const MIMETYPE: &str = "application/vnd.adobe.indesign-idml-package";
pub const MANIFEST_PART: &str = "folio/manifest.json";
pub const SAVED_LINES_PART: &str = "folio/saved-lines.json";
pub const REPORT_PART: &str = "folio/import-report.json";
pub const FORMAT: &str = "folio-document";
pub const VERSION: u32 = 1;
/// Placeholder scheme of references inside IDML parts.
pub const REF_SCHEME: &str = "folio:ref/";
/// Placeholder for `file:///…` URLs (XMP) — written back in that form.
pub const URL_REF_SCHEME: &str = "folio:url-ref/";
/// Placeholder for native paths in attributes (`Movie/@FilePath`) — written back as a path.
pub const PATH_REF_SCHEME: &str = "folio:path-ref/";
/// Label keys.
pub const LABEL_REF: &str = "folio.ref";
pub const LABEL_DOCUMENT: &str = "folio.document";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    /// A linked file (placed graphic, movie, sound).
    Link,
    /// A file the document holds itself (embedded link, pasted graphic), extracted.
    Embedded,
    /// The reader's preview of a graphic (missing links, vector content, the screen).
    Preview,
    Font,
    /// Any other file the IDML names (XMP history, data merge source, hyperlink to a file).
    File,
}

/// Facts of a font reference.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFacts {
    pub family: String,
    pub style: String,
    #[serde(default)]
    pub postscript: String,
    /// The `name` table version string.
    #[serde(default)]
    pub version: String,
    /// Copy decision: `copied`, or the code why not (`nonAdobeCjk`, `fsTypeRestricted`, …),
    /// `missing` / `substituted` when no exact face was found.
    #[serde(default)]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One reference of the document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// Stable id (`r1`, `r2`, …), used in the IDML parts (`folio:ref/r1`) and labels.
    pub id: String,
    pub kind: RefKind,
    /// Where the file is, root-relative; `None` for a file not in the project.
    #[serde(default)]
    pub path: Option<RelPath>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub bytes: Option<u64>,
    /// Original URI of a file that was not in the project at import (only shown, and
    /// written back to IDML exports).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The name the file had before a name that is not portable was fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    /// `found`, `missing`, `ambiguous` or `embedded` at import (links).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontFacts>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The references of one placed graphic (or movie / sound).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicRefs {
    /// IDML `Self` of the graphic in this package.
    #[serde(rename = "self")]
    pub self_id: String,
    /// Folio's stable id (also in the `folio.ref` label, which survives InDesign).
    pub folio_id: String,
    /// IDML `Self` of the frame carrying the `folio.ref` label (InDesign keeps frame labels
    /// through IDML, not those of placed graphics).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    /// IDML `Self` of its `Link`, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_self: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedded: Option<String>,
}

/// A shared resource as it was when the document was last saved ("shipped issue" pinning).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    pub reference: String,
    pub path: RelPath,
    pub sha256: String,
    pub bytes: u64,
    pub kind: RefKind,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    /// File name of the imported INDD / IDML (no folder).
    pub name: String,
    /// `indd` or `idml`.
    pub kind: String,
    pub sha256: String,
    pub bytes: u64,
    /// The reader that produced the IDML parts (INDD).
    #[serde(default)]
    pub reader: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartRef {
    pub part: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocManifest {
    pub format: String,
    pub version: u32,
    /// The document's Folio UUID (also the `folio.document` label; IDML `Self` ids change).
    pub id: String,
    /// The project it was created in.
    pub project: String,
    pub created: String,
    pub saved: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub source: Option<SourceInfo>,
    /// How links are written in the IDML parts: `folio-ref` (`folio:ref/<id>`).
    #[serde(default = "default_uris")]
    pub link_uris: String,
    pub references: Vec<Reference>,
    pub graphics: Vec<GraphicRefs>,
    #[serde(default)]
    pub pins: Vec<Pin>,
    #[serde(default)]
    pub saved_lines: Option<PartRef>,
    #[serde(default)]
    pub import_report: Option<PartRef>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_uris() -> String {
    "folio-ref".into()
}

impl DocManifest {
    pub fn reference(&self, id: &str) -> Option<&Reference> {
        self.references.iter().find(|r| r.id == id)
    }

    /// References that point into the project.
    pub fn files(&self) -> impl Iterator<Item = &Reference> {
        self.references.iter().filter(|r| r.path.is_some())
    }
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Zip(String),
    Invalid(String),
    /// Written by a newer Folio (major version above ours).
    Newer(u32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Zip(e) => write!(f, ".folio: {e}"),
            Error::Invalid(e) => write!(f, ".folio: {e}"),
            Error::Newer(v) => write!(f, ".folio 형식 {v}은(는) 이 Folio보다 새 버전입니다"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Error {
        Error::Zip(e.to_string())
    }
}

/// Reads the manifest of a `.folio` (and the names of its parts).
pub fn read_manifest(path: &Path) -> Result<(DocManifest, Vec<String>), Error> {
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f))?;
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).collect();
    let mut text = String::new();
    z.by_name(MANIFEST_PART).map_err(|_| Error::Invalid("folio/manifest.json가 없습니다 (IDML 파일일 수 있음)".into()))?.read_to_string(&mut text)?;
    let m: DocManifest = serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("manifest: {e}")))?;
    if m.format != FORMAT {
        return Err(Error::Invalid(format!("format {:?}", m.format)));
    }
    if m.version > VERSION {
        return Err(Error::Newer(m.version));
    }
    Ok((m, names))
}

/// The bytes of one part.
pub fn read_part(path: &Path, part: &str) -> Result<Option<Vec<u8>>, Error> {
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f))?;
    let Ok(mut e) = z.by_name(part) else { return Ok(None) };
    let mut v = Vec::with_capacity(e.size() as usize);
    e.read_to_end(&mut v)?;
    Ok(Some(v))
}

/// What becomes of each IDML part of the source package when writing.
pub enum PartAction {
    /// Copied as stored (compressed bytes are not touched).
    Keep,
    /// Replaced by this text (deflated).
    Replace(String),
    /// Left out.
    Drop,
}

/// A package part visitor; the reader is lazy so unchanged compressed bytes can be copied.
pub type PartVisitor<'a> = dyn FnMut(&str, &mut dyn FnMut() -> std::io::Result<String>) -> std::io::Result<PartAction> + 'a;

fn options(stored: bool) -> zip::write::SimpleFileOptions {
    let method = if stored { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
    zip::write::SimpleFileOptions::default()
        .compression_method(method)
        .last_modified_time(zip::DateTime::default())
        .large_file(false)
}

/// Writes a package: the IDML parts of `source` (an IDML or `.folio` ZIP) as `action` says,
/// then `extra` parts (`folio/…`). `mimetype` is always first and stored. Parts of `source`
/// under `folio/` are not copied (the caller supplies the new ones).
pub fn write_package<W: Write + Seek, R: Read + Seek>(
    out: W,
    source: &mut zip::ZipArchive<R>,
    action: &mut PartVisitor<'_>,
    extra: &[(&str, &[u8])],
) -> Result<W, Error> {
    write_package_with_parts(out, source, action, extra, &[])
}

/// Also visits newly created IDML parts through `action`. Existing parts still come from
/// `source`; candidates already present there are not appended a second time after save.
pub fn write_package_with_parts<W: Write + Seek, R: Read + Seek>(
    out: W,
    source: &mut zip::ZipArchive<R>,
    action: &mut PartVisitor<'_>,
    extra: &[(&str, &[u8])],
    added: &[(&str, &str)],
) -> Result<W, Error> {
    let mut seen = std::collections::HashSet::new();
    let mut z = zip::ZipWriter::new(out);
    z.start_file("mimetype", options(true))?;
    z.write_all(MIMETYPE.as_bytes())?;
    for i in 0..source.len() {
        let name = source.by_index_raw(i)?.name()?.into_owned();
        seen.insert(name.clone());
        if name == "mimetype" || name.starts_with("folio/") || name.ends_with('/') {
            continue;
        }
        let mut read_text = || -> std::io::Result<String> {
            let mut s = String::new();
            source.by_index(i).map_err(std::io::Error::other)?.read_to_string(&mut s)?;
            Ok(s)
        };
        match action(&name, &mut read_text)? {
            PartAction::Keep => {
                let raw = source.by_index_raw(i)?;
                z.raw_copy_file(raw)?;
            }
            PartAction::Replace(text) => {
                z.start_file(&name, options(false))?;
                z.write_all(text.as_bytes())?;
            }
            PartAction::Drop => {}
        }
    }
    // Text frame duplication creates Story parts that are absent from the source ZIP.
    // Iterating source entries alone silently saved the frame but lost its copied text.
    for &(name, text) in added {
        if name == "mimetype" || name.starts_with("folio/") || name.ends_with('/') || !seen.insert(name.to_owned()) {
            continue;
        }
        let value = match action(name, &mut || Ok(text.to_owned()))? {
            PartAction::Keep => text.to_owned(),
            PartAction::Replace(value) => value,
            PartAction::Drop => continue,
        };
        z.start_file(name, options(false))?;
        z.write_all(value.as_bytes())?;
    }
    for (name, bytes) in extra {
        z.start_file(*name, options(false))?;
        z.write_all(bytes)?;
    }
    Ok(z.finish()?)
}

/// Writes a `.folio` atomically: IDML parts of `source` (`action` decides each), the manifest
/// and the other Folio parts.
pub fn write_folio(
    target: &Path,
    source: &Path,
    action: &mut PartVisitor<'_>,
    manifest: &DocManifest,
    saved_lines: Option<&[u8]>,
    report: Option<&[u8]>,
) -> Result<u64, Error> {
    write_folio_with_parts(target, source, action, manifest, saved_lines, report, &[])
}

/// [`write_folio`] with newly created IDML parts (for example a copied Story).
pub fn write_folio_with_parts(
    target: &Path,
    source: &Path,
    action: &mut PartVisitor<'_>,
    manifest: &DocManifest,
    saved_lines: Option<&[u8]>,
    report: Option<&[u8]>,
    added: &[(&str, &str)],
) -> Result<u64, Error> {
    let manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut extra: Vec<(&str, &[u8])> = vec![(MANIFEST_PART, &manifest_bytes)];
    if let Some(s) = saved_lines {
        extra.push((SAVED_LINES_PART, s));
    }
    if let Some(r) = report {
        extra.push((REPORT_PART, r));
    }
    let src = std::fs::File::open(source)?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(src))?;
    let mut failure: Option<Error> = None;
    crate::atomic::write_file_with(target, &mut |file| {
        let w = std::io::BufWriter::with_capacity(1 << 20, file);
        match write_package_with_parts(w, &mut archive, action, &extra, added) {
            Ok(mut w) => w.flush(),
            Err(e) => {
                let msg = e.to_string();
                failure = Some(e);
                Err(std::io::Error::other(msg))
            }
        }
    })
    .map_err(|e| failure.take().unwrap_or(Error::Io(e)))?;
    Ok(std::fs::metadata(target)?.len())
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub(crate) fn tiny_idml(path: &Path) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        z.start_file("mimetype", options(true)).unwrap();
        z.write_all(MIMETYPE.as_bytes()).unwrap();
        z.start_file("designmap.xml", options(false)).unwrap();
        z.write_all(br#"<?xml version="1.0"?><Document Self="d" DOMVersion="20.5"><idPkg:Spread src="Spreads/Spread_u1.xml"/></Document>"#).unwrap();
        z.start_file("Spreads/Spread_u1.xml", options(false)).unwrap();
        z.write_all(br#"<idPkg:Spread><Spread Self="u1"><Rectangle Self="r"><Image Self="g1"><Link Self="l1" LinkResourceURI="file:C:/x/a.jpg"/></Image></Rectangle></Spread></idPkg:Spread>"#).unwrap();
        z.finish().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::tiny_idml;

    fn manifest() -> DocManifest {
        DocManifest {
            format: FORMAT.into(),
            version: VERSION,
            id: "doc".into(),
            project: "p".into(),
            created: "2026-10-19T00:00:00Z".into(),
            saved: "2026-10-19T00:00:00Z".into(),
            engine: "0".into(),
            source: Some(SourceInfo { name: "a.indd".into(), kind: "indd".into(), sha256: "00".into(), bytes: 1, reader: "r".into() }),
            link_uris: default_uris(),
            references: vec![
                Reference {
                    id: "r1".into(),
                    kind: RefKind::Link,
                    path: Some(RelPath::parse("assets/a/a.jpg").unwrap()),
                    sha256: Some("ab".into()),
                    bytes: Some(3),
                    hint: None,
                    original_name: Some("a?.jpg".into()),
                    status: Some("found".into()),
                    font: None,
                    extra: Map::new(),
                },
                Reference { id: "r2".into(), kind: RefKind::Link, path: None, sha256: None, bytes: None, hint: Some("file:/Volumes/x/b.pdf".into()), original_name: None, status: Some("missing".into()), font: None, extra: Map::new() },
                Reference {
                    id: "r3".into(),
                    kind: RefKind::Font,
                    path: None,
                    sha256: Some("cd".into()),
                    bytes: Some(10),
                    hint: None,
                    original_name: None,
                    status: None,
                    font: Some(FontFacts { family: "Noto".into(), style: "Bold".into(), postscript: "Noto-Bold".into(), version: "Version 2.0".into(), status: "nonAdobeCjk".into(), reason: None }),
                    extra: Map::new(),
                },
            ],
            graphics: vec![GraphicRefs { self_id: "g1".into(), folio_id: "f1".into(), frame: None, link_self: Some("l1".into()), link: Some("r1".into()), preview: None, embedded: None }],
            pins: vec![Pin { reference: "r1".into(), path: RelPath::parse("assets/a/a.jpg").unwrap(), sha256: "ab".into(), bytes: 3, kind: RefKind::Link }],
            saved_lines: Some(PartRef { part: SAVED_LINES_PART.into(), format: "folio-saved-lines".into(), version: 1 }),
            import_report: None,
            extra: Map::new(),
        }
    }

    #[test]
    fn manifest_schema_round_trip() {
        let m = manifest();
        let text = serde_json::to_string_pretty(&m).unwrap();
        let back: DocManifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, m);
        // unknown fields of newer writers survive
        let mut v: Value = serde_json::from_str(&text).unwrap();
        v["futureTable"] = serde_json::json!([1, 2]);
        v["references"][0]["futureField"] = serde_json::json!("x");
        let back: DocManifest = serde_json::from_value(v.clone()).unwrap();
        let again = serde_json::to_value(&back).unwrap();
        assert_eq!(again["futureTable"], serde_json::json!([1, 2]));
        assert_eq!(again["references"][0]["futureField"], "x");
        // stored paths are validated when read
        let mut bad = v.clone();
        bad["references"][0]["path"] = serde_json::json!("C:/x/a.jpg");
        assert!(serde_json::from_value::<DocManifest>(bad).is_err());
        let mut bad = v;
        bad["references"][0]["path"] = serde_json::json!("../a.jpg");
        assert!(serde_json::from_value::<DocManifest>(bad).is_err());
    }

    #[test]
    fn malformed_manifest_numbers_and_paths_return_errors() {
        for (field, value) in [
            ("version", serde_json::json!("not a number")),
            ("version", serde_json::json!(-1)),
            ("version", serde_json::json!(u64::MAX)),
        ] {
            let mut bad = serde_json::to_value(manifest()).unwrap();
            bad[field] = value;
            assert!(serde_json::from_value::<DocManifest>(bad).is_err());
        }
        for path in ["", "../missing", "assets//missing", "assets/CON", "assets/a\0b"] {
            let mut bad = serde_json::to_value(manifest()).unwrap();
            bad["references"][0]["path"] = serde_json::json!(path);
            assert!(serde_json::from_value::<DocManifest>(bad).is_err(), "{path:?}");
        }
        assert!(manifest().reference("absent-reference").is_none());
    }

    #[test]
    fn malformed_packages_report_existing_errors() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/robust-a-c1-20261023/project-inputs");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in [("empty", &b""[..]), ("truncated", &b"PK\x03\x04broken"[..])] {
            let path = dir.join(format!("{name}.folio"));
            std::fs::write(&path, bytes).unwrap();
            assert!(matches!(read_manifest(&path), Err(Error::Zip(_))));
        }
        let missing = dir.join("missing-manifest.folio");
        tiny_idml(&missing);
        assert_eq!(read_manifest(&missing).unwrap_err().to_string(), ".folio: folio/manifest.json가 없습니다 (IDML 파일일 수 있음)");
        assert_eq!(read_part(&missing, "missing.xml").unwrap(), None);
        for (name, text) in [("empty-json", ""), ("truncated-json", "{\"format\":"), ("wrong-number", "{\"version\":\"bad\"}")] {
            let path = dir.join(format!("{name}.folio"));
            let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            archive.start_file(MANIFEST_PART, options(false)).unwrap();
            archive.write_all(text.as_bytes()).unwrap();
            archive.finish().unwrap();
            assert!(read_manifest(&path).unwrap_err().to_string().starts_with(".folio: manifest: "));
        }
    }

    #[test]
    fn package_write_and_read() {
        let dir = std::env::temp_dir().join(format!("folio-format-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let idml = dir.join("a.idml");
        tiny_idml(&idml);
        let target = dir.join("a.folio");
        let m = manifest();
        let n = write_folio(
            &target,
            &idml,
            &mut |name, text| {
                if name.starts_with("Spreads/") {
                    Ok(PartAction::Replace(text()?.replace("file:C:/x/a.jpg", "folio:ref/r1")))
                } else {
                    Ok(PartAction::Keep)
                }
            },
            &m,
            Some(b"{}"),
            None,
        )
        .unwrap();
        assert!(n > 0);
        let (back, names) = read_manifest(&target).unwrap();
        assert_eq!(back, m);
        assert_eq!(names[0], "mimetype");
        assert!(names.contains(&MANIFEST_PART.to_string()) && names.contains(&SAVED_LINES_PART.to_string()));
        let spread = String::from_utf8(read_part(&target, "Spreads/Spread_u1.xml").unwrap().unwrap()).unwrap();
        assert!(spread.contains("folio:ref/r1"));
        // the mimetype entry is stored, uncompressed, first (IDML readers check it)
        let raw = std::fs::read(&target).unwrap();
        // The ZIP writer records DOS or Unix metadata in the central directory.
        let expected_hash = if cfg!(windows) {
            "75a9d7c9dc12ef87705ec4a037df06d5a181509ec887d5e15f9e9c0a72cc32ae"
        } else {
            "a8f2bed10a8427c9857966977ada8532f70285868558c3580b9835ec44665853"
        };
        assert_eq!(crate::hash::sha256_bytes(&raw), expected_hash);
        assert_eq!(&raw[30..38], b"mimetype");
        assert_eq!(&raw[38..38 + MIMETYPE.len()], MIMETYPE.as_bytes());
        // the IDML reader opens a .folio as it is
        let doc = folio_idml::read_package(&target, None).unwrap();
        assert_eq!(doc.spreads.len(), 1);
        // re-saving a .folio from itself keeps the parts and replaces the Folio ones
        let copy = dir.join("b.folio");
        let mut m2 = m.clone();
        m2.saved = "2026-10-20T00:00:00Z".into();
        write_folio(&copy, &target, &mut |_, _| Ok(PartAction::Keep), &m2, None, None).unwrap();
        let (back2, names2) = read_manifest(&copy).unwrap();
        assert_eq!(back2.saved, "2026-10-20T00:00:00Z");
        assert_eq!(names2.iter().filter(|n| n.starts_with("folio/")).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn created_story_parts_are_written_rewritten_and_not_duplicated_on_resave() {
        use std::io::Cursor;
        let mut source = zip::ZipWriter::new(Cursor::new(Vec::new()));
        source.start_file("mimetype", options(true)).unwrap();
        source.write_all(MIMETYPE.as_bytes()).unwrap();
        source.start_file("designmap.xml", options(false)).unwrap();
        source.write_all(b"<Document/>").unwrap();
        let raw = source.finish().unwrap().into_inner();
        let added = [
            ("Stories/Story_new.xml", "<Story><Content>copied text</Content></Story>"),
            ("Stories/Story_removed.xml", "<Story/>"),
            ("designmap.xml", "must not append a duplicate"),
        ];
        let rewrite = |name: &str, read: &mut dyn FnMut() -> std::io::Result<String>| {
            Ok(match name {
                "Stories/Story_new.xml" => PartAction::Replace(read()?.replace("copied text", "rewritten text")),
                "Stories/Story_removed.xml" => PartAction::Drop,
                _ => PartAction::Keep,
            })
        };
        let mut archive = zip::ZipArchive::new(Cursor::new(raw)).unwrap();
        let first = write_package_with_parts(Cursor::new(Vec::new()), &mut archive, &mut { rewrite }, &[], &added).unwrap().into_inner();
        let mut archive = zip::ZipArchive::new(Cursor::new(first)).unwrap();
        let second = write_package_with_parts(Cursor::new(Vec::new()), &mut archive, &mut { rewrite }, &[], &added).unwrap().into_inner();
        let mut archive = zip::ZipArchive::new(Cursor::new(second)).unwrap();
        assert_eq!(archive.len(), 3);
        assert_eq!(archive.by_index(0).unwrap().name().unwrap(), "mimetype");
        let mut text = String::new();
        archive.by_name("Stories/Story_new.xml").unwrap().read_to_string(&mut text).unwrap();
        assert!(text.contains("rewritten text"));
        assert!(archive.by_name("Stories/Story_removed.xml").is_err());
        // Undo after save can remove the part now stored in the source ZIP.
        let undone = write_package_with_parts(Cursor::new(Vec::new()), &mut archive, &mut |name, _| {
            Ok(if name.starts_with("Stories/") { PartAction::Drop } else { PartAction::Keep })
        }, &[], &[]).unwrap().into_inner();
        assert_eq!(zip::ZipArchive::new(Cursor::new(undone)).unwrap().len(), 2);
    }
}
