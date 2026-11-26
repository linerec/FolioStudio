//! Placed PDF / AI pages copied into the output as Form XObjects (lopdf reads the source).
//!
//! The form shows exactly the area InDesign shows (`Graphic::bounds`, whatever the crop
//! setting), right side up in the graphic's coordinates: InDesign records the shown box
//! measured from the CropBox's bottom-left corner, y up (x = pdf x − crop.x0, y = pdf y −
//! crop.y0), and draws the box with its top-left corner at the bounds' top-left
//! (artifacts/pdf-crop-20261021: every crop option, boxes not symmetric about the CropBox).
//! Only for the CropBox itself, or a box symmetric about it, is that the CropBox top-left at
//! the origin, y down. Objects reachable from the page's resources are copied once per
//! source file into a chunk with its own numbering, renumbered when the document is
//! written. Keys PDF/X forbids (transfer functions, OPI, actions) are dropped, and DeviceRGB
//! inside resolves to the document RGB profile through /DefaultRGB — or, in CMYK-only output
//! (PDF/X-1a), DeviceRGB paint and 8-bit DeviceRGB images are converted to the destination
//! CMYK with the document conversion, as vector paint is.

mod copy;

use lopdf::ObjectId;
use pdf_writer::{Chunk, Ref};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};


/// One placement: a form of a source file's chunk.
pub struct Imported {
    /// Index into `ImportCache::files`.
    pub file: usize,
    /// The form XObject (the file chunk's numbering).
    pub form: Ref,
    pub transparency: bool,
    pub warnings: Vec<String>,
}

/// Objects copied from one source PDF, shared by all its placements, numbered from 1.
pub struct FileChunk {
    pub chunk: Chunk,
    /// Reference standing for the document RGB profile colour space (DefaultRGB).
    pub default_rgb: Ref,
    next: i32,
    map: HashMap<ObjectId, Ref>,
    reported: Vec<String>,
    /// Something copied so far needs transparency (groups, soft masks, blending).
    transparency: bool,
    /// Colour that is not CMYK, gray or spot: DeviceRGB, CalRGB, CalGray, Lab, ICC-based
    /// (PDF/X-1a cannot keep it).
    pub non_cmyk: bool,
    /// Features newer than PDF 1.3 (JPX, JBIG2, 16-bit images, optional content).
    pub newer_than_13: bool,
    /// DeviceRGB paint or images were converted to the destination CMYK.
    pub rgb_converted: bool,
}


type SourceKey = (PathBuf, String);

struct LoadedDocument {
    doc: std::sync::Arc<lopdf::Document>,
    bytes: u64,
    last_used: u64,
}

const SOURCE_BUDGET: u64 = 192 << 20;

#[derive(Default)]
pub struct ImportCache {
    source_files: HashMap<SourceKey, Option<(usize, u64)>>,
    docs: HashMap<usize, LoadedDocument>,
    source_bytes: u64,
    source_use: u64,
    pub files: Vec<FileChunk>,
    /// CMYK-only output: DeviceRGB is converted with these colours and no DefaultRGB is added
    /// (a PDF with other non-CMYK colour is not copied as it is).
    pub cmyk_output: Option<folio_color::Colors>,
    /// PDF 1.3 output: image soft masks PDF 1.3 can hold (all opaque, cut-outs) are written
    /// without transparency.
    pub pdf13: bool,
}

impl ImportCache {
    pub fn release(&mut self) {
        self.source_files.clear();
        self.docs.clear();
        self.source_bytes = 0;
    }

    fn make_source_room(&mut self, bytes: u64) {
        while self.source_bytes.saturating_add(bytes) > SOURCE_BUDGET {
            let Some((&oldest, _)) = self.docs.iter().min_by_key(|(_, entry)| entry.last_used) else { break };
            self.source_bytes -= self.docs.remove(&oldest).expect("cached PDF").bytes;
        }
    }

    fn load(&mut self, path: &Path, layers: Option<&folio_model::graphic_layers::Options>) -> Result<(std::sync::Arc<lopdf::Document>, usize), String> {
        let key = (path.to_path_buf(),serde_json::to_string(&layers).map_err(|e| e.to_string())?);
        self.source_use += 1;
        let prior = match self.source_files.get(&key) {
            Some(None) => return Err("unreadable PDF".into()),
            Some(Some((file, hash))) => {
                if let Some(hit) = self.docs.get_mut(file) {
                    hit.last_used = self.source_use;
                    return Ok((hit.doc.clone(), *file));
                }
                Some((*file, *hash))
            }
            None => None,
        };
        // During parsing the input buffer and parsed streams coexist. Account for
        // both before opening another file, including one larger than the budget.
        let bytes = std::fs::metadata(path).map_or(0, |m| m.len().saturating_mul(2));
        if self.pdf13 {
            self.make_source_room(bytes);
        }
        let mut hash = 0;
        let doc = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| {
                if self.pdf13 {
                    let mut hasher = DefaultHasher::new();
                    hasher.write(&b);
                    hash = hasher.finish();
                    if prior.is_some_and(|(_, original)| original != hash) {
                        return Err("placed PDF changed during export".into());
                    }
                }
                lopdf::Document::load_mem(&b).map_err(|e| e.to_string())
            })
            .and_then(|mut doc| { folio_links::pdfplace::layers::apply(&mut doc,layers)?; Ok(doc) })
            .map(std::sync::Arc::new);
        let doc = match doc {
            Ok(doc) => doc,
            Err(err) => {
                self.source_files.insert(key, None);
                return Err(err);
            }
        };
        let file = prior.map(|(file, _)| file).unwrap_or_else(|| {
            self.files.push(FileChunk {
                chunk: Chunk::new(),
                default_rgb: Ref::new(1),
                next: 2,
                map: HashMap::new(),
                reported: Vec::new(),
                transparency: false,
                non_cmyk: false,
                newer_than_13: false,
                rgb_converted: false,
            });
            self.files.len() - 1
        });
        self.source_files.insert(key, Some((file, hash)));
        self.docs.insert(file, LoadedDocument { doc: doc.clone(), bytes, last_used: self.source_use });
        self.source_bytes = self.source_bytes.saturating_add(bytes);
        Ok((doc, file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::copy::{inherited, rect_of};
    use crate::spotconv::InkRules;
    use folio_model::Rect;
    use lopdf::{Object, Stream, dictionary};

    #[test]
    fn damaged_pdf_files_return_errors_and_cache_failure() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../artifacts/robust-b-c1-20261023/pdf-input-tests")
            .join(std::process::id().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let mut cache = ImportCache::default();
        assert!(cache.load(&dir.join("missing.pdf"), None).is_err());
        for (name, bytes) in [("empty.pdf", &b""[..]), ("truncated.pdf", &b"%PDF-1.4\n1 0 obj\n<< /Type /Page"[..])] {
            let path = dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            assert!(cache.load(&path, None).is_err());
            assert_eq!(cache.load(&path, None).err().as_deref(), Some("unreadable PDF"));
            std::fs::remove_file(path).unwrap();
        }
        assert!(cache.files.is_empty());
    }

    #[test]
    fn invalid_pdf_rectangles_and_missing_references_return_none() {
        let doc = lopdf::Document::new();
        for value in [
            Object::Array(vec![]),
            Object::Array(vec![0.into(), 0.into(), 1.into()]),
            Object::Array(vec![0.into(), Object::Name(b"bad-number".to_vec()), 1.into(), 1.into()]),
            Object::Reference((999, 0)),
        ] {
            assert!(rect_of(&doc, &value).is_none());
        }
        assert!(inherited(&doc, (999, 0), b"MediaBox").is_none());
        let mut cyclic = lopdf::Document::new();
        cyclic.objects.insert((1, 0), Object::Dictionary(dictionary! { "Parent" => Object::Reference((1, 0)) }));
        assert!(inherited(&cyclic, (1, 0), b"MediaBox").is_none());
    }

    /// A one-page PDF with `content` and an image `Im0` (with a soft mask of these samples).
    fn placed_pdf(name: &str, content: &[u8], mut image: Stream, smask: Option<(i64, i64, Vec<u8>)>) -> PathBuf {
        let mut doc = lopdf::Document::with_version("1.3");
        let pages = doc.new_object_id();
        if let Some((w, h, a)) = smask {
            let d = dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 };
            let m = doc.add_object(Stream::new(d, a));
            image.dict.set("SMask", m);
        }
        let img = doc.add_object(image);
        let contents = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Contents" => contents, "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => img } },
        });
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let dir = std::env::temp_dir().join(format!("folio-pdfimport-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        doc.save(&path).unwrap();
        path
    }

    /// The numbers after `key` (`/BBox [..]`, `/Matrix [..]`) in a chunk.
    fn numbers_after(bytes: &[u8], key: &[u8]) -> Vec<f64> {
        let at = bytes.windows(key.len()).position(|w| w == key).expect("key");
        let rest = &bytes[at + key.len()..];
        let open = rest.iter().position(|&c| c == b'[').unwrap();
        let close = rest.iter().position(|&c| c == b']').unwrap();
        std::str::from_utf8(&rest[open + 1..close]).unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect()
    }

    /// InDesign 2025 (artifacts/pdf-crop-20261021, test PDF D): a placed page's bounds are the
    /// shown box measured from the CropBox's bottom-left, y up, and the box shows right side
    /// up. CropBox 10 100 590 790, TrimBox 40 200 560 760: the trim crop's bounds are
    /// L30 T100 R550 B660, so the form shows the trim box with its top (y 760) at the
    /// bounds' top.
    #[test]
    fn a_crop_box_not_symmetric_about_the_crop_box_shows_where_indesign_puts_it() {
        let mut doc = lopdf::Document::with_version("1.4");
        let pages = doc.new_object_id();
        let contents = doc.add_object(Stream::new(dictionary! {}, b"1 0 0 rg 100 600 200 180 re f".to_vec()));
        let num = |v: &[i64]| v.iter().map(|&x| x.into()).collect::<Vec<Object>>();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => num(&[0, 0, 600, 800]), "CropBox" => num(&[10, 100, 590, 790]),
            "TrimBox" => num(&[40, 200, 560, 760]), "Contents" => contents, "Resources" => dictionary! {},
        });
        doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let dir = std::env::temp_dir().join(format!("folio-pdfimport-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crop-trim.pdf");
        doc.save(&path).unwrap();
        let (fonts, rules) = (folio_text::FontStore::empty(), InkRules::default());
        let bounds = Rect { left: 30.0, top: 100.0, right: 550.0, bottom: 660.0 };
        let mut c = ImportCache::default();
        c.import(&path, 1, &bounds, &fonts, &rules).unwrap();
        let bytes = c.files[0].chunk.as_bytes();
        assert_eq!(numbers_after(bytes, b"/BBox"), vec![40.0, 200.0, 560.0, 760.0]);
        // pdf (x, y) -> graphic (x - 10, 860 - y): the trim top 760 at 100, its bottom 200 at 660
        assert_eq!(numbers_after(bytes, b"/Matrix"), vec![1.0, 0.0, 0.0, -1.0, -10.0, 860.0]);
    }

    #[test]
    fn device_rgb_is_converted_in_cmyk_only_output() {
        let image = || {
            let d = dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 };
            Stream::new(d, vec![255, 0, 0, 0, 0, 255])
        };
        let path = placed_pdf("rgb.pdf", b"q 1 0 0 rg 0 0 1 RG 0 0 10 10 re B 100 0 0 100 0 0 cm /Im0 Do Q", image(), None);
        let (fonts, rules) = (folio_text::FontStore::empty(), InkRules::default());
        let bounds = Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 100.0 };
        // PDF/X-4: copied as it is (DeviceRGB resolves through DefaultRGB)
        let mut kept = ImportCache::default();
        kept.import(&path, 1, &bounds, &fonts, &rules).unwrap();
        assert!(kept.files[0].non_cmyk && !kept.files[0].rgb_converted);
        // PDF/X-1a: paint and image in DeviceCMYK, nothing left to flatten
        let mut cmyk = ImportCache { cmyk_output: Some(folio_color::Colors::unmanaged()), ..Default::default() };
        cmyk.import(&path, 1, &bounds, &fonts, &rules).unwrap();
        let f = &cmyk.files[0];
        assert!(!f.non_cmyk && f.rgb_converted);
        let bytes = f.chunk.as_bytes();
        let has = |n: &[u8]| bytes.windows(n.len()).any(|w| w == n);
        assert!(has(b"/DeviceCMYK") && !has(b"/DeviceRGB") && !has(b"/DefaultRGB"));
        // RGB this does not convert (an inline RGB image) leaves the file non-CMYK
        let inline = placed_pdf("inline.pdf", b"BI /W 1 /H 1 /CS /RGB /BPC 8 ID \xff\x00\x00 EI", image(), None);
        let mut cmyk = ImportCache { cmyk_output: Some(folio_color::Colors::unmanaged()), ..Default::default() };
        cmyk.import(&inline, 1, &bounds, &fonts, &rules).unwrap();
        assert!(cmyk.files[0].non_cmyk);
    }

    #[test]
    fn pdf13_output_holds_cut_out_soft_masks_as_explicit_masks() {
        let (fonts, rules) = (folio_text::FontStore::empty(), InkRules::default());
        let bounds = Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 100.0 };
        let gray = |w: i64, h: i64| {
            let d = dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w, "Height" => h, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 };
            Stream::new(d, vec![128; (w * h) as usize])
        };
        let import = |path: &Path, pdf13: bool| {
            let mut c = ImportCache { pdf13, ..Default::default() };
            let imp = c.import(path, 1, &bounds, &fonts, &rules).unwrap();
            (imp.transparency, c.files.remove(0).chunk.as_bytes().to_vec())
        };
        let has = |b: &[u8], n: &[u8]| b.windows(n.len()).any(|w| w == n);
        // a cut-out: opaque left half, clear right half
        let cut: Vec<u8> = (0..16 * 16).map(|i| if i % 16 < 8 { 255 } else { 0 }).collect();
        let path = placed_pdf("cut.pdf", b"100 0 0 100 0 0 cm /Im0 Do", gray(16, 16), Some((16, 16, cut)));
        let (transparency, bytes) = import(&path, false);
        assert!(transparency && has(&bytes, b"/SMask"), "PDF/X-4 keeps the soft mask");
        let (transparency, bytes) = import(&path, true);
        assert!(!transparency && !has(&bytes, b"/SMask") && has(&bytes, b"/Mask") && has(&bytes, b"/ImageMask true"));
        // a fade stays transparency (the flattener draws the placement)
        let fade: Vec<u8> = (0..16 * 16).map(|i| (i % 16 * 16) as u8).collect();
        let path = placed_pdf("fade.pdf", b"100 0 0 100 0 0 cm /Im0 Do", gray(16, 16), Some((16, 16, fade)));
        assert!(import(&path, true).0);
    }

    #[test]
    fn pdf13_output_leaves_out_groups_of_opaque_content() {
        let (fonts, rules) = (folio_text::FontStore::empty(), InkRules::default());
        let bounds = Rect { left: 0.0, top: 0.0, right: 100.0, bottom: 100.0 };
        let d = dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceCMYK" } };
        let path = placed_pdf("group.pdf", b"/Im0 Do", Stream::new(d, b"0 0 0 1 k 0 0 10 10 re f".to_vec()), None);
        let import = |pdf13: bool| {
            let mut c = ImportCache { pdf13, ..Default::default() };
            let imp = c.import(&path, 1, &bounds, &fonts, &rules).unwrap();
            (imp.transparency, c.files.remove(0).chunk.as_bytes().windows(6).any(|w| w == b"/Group"))
        };
        assert_eq!(import(false), (true, true), "PDF/X-4 keeps the group");
        assert_eq!(import(true), (false, false));
    }
}
