//! Which font files go into "Document fonts", following InDesign's Package rules:
//!
//! - the faces our font matching (`folio_text::FontStore::resolve`) chose exactly for the
//!   family + style pairs the text uses (substitutes are reported, never copied);
//! - a font collection (TTC / OTC) is copied once, as one file;
//! - not copied: fonts whose `OS/2.fsType` forbids installable and editable embedding
//!   (restricted, preview & print, bitmap only); fonts activated from Adobe Fonts; non-Adobe
//!   CJK fonts (InDesign: "Copy Fonts (Except activated fonts from Adobe Fonts and non-Adobe
//!   CJK fonts)") unless the caller asks for them; anything in InDesign's own
//!   `Resources\Required\Fonts` folder (licensed to InDesign only).
//!
//! Only the sfnt table directory and the `OS/2` table are read from each file.

use folio_text::{FontSource, FontStore};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// `OS/2` facts that decide whether a face may be copied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FaceFlags {
    /// `fsType` (embedding licensing rights).
    pub fs_type: u16,
    /// `achVendID`, trimmed.
    pub vendor: String,
    /// `ulCodePageRange1` (OS/2 version ≥ 1).
    pub code_pages: Option<u32>,
}

/// What `fsType` allows (OpenType spec: bits 0–3 are exclusive; when several are set the
/// least restrictive applies).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Embedding {
    Installable,
    Editable,
    PreviewPrint,
    Restricted,
    /// Bit 9: only bitmaps may be embedded, no outlines.
    BitmapOnly,
}

pub fn embedding(fs_type: u16) -> Embedding {
    if fs_type & 0x0200 != 0 {
        return Embedding::BitmapOnly;
    }
    let usage = fs_type & 0x000F;
    if usage & 0x0008 != 0 {
        Embedding::Editable
    } else if usage & 0x0004 != 0 {
        Embedding::PreviewPrint
    } else if usage & 0x0002 != 0 {
        Embedding::Restricted
    } else {
        // 0 = installable; bit 0 is reserved
        Embedding::Installable
    }
}

/// Installable and editable embedding allow handing the file on with the document.
pub fn allows_packaging(e: Embedding) -> bool {
    matches!(e, Embedding::Installable | Embedding::Editable)
}

/// Code page bits of Japanese (17), Simplified Chinese (18), Korean Wansung (19),
/// Traditional Chinese (20) and Korean Johab (21).
const CJK_CODE_PAGES: u32 = 0b11111 << 17;

pub fn is_cjk(flags: &FaceFlags) -> bool {
    flags.code_pages.is_some_and(|c| c & CJK_CODE_PAGES != 0)
}

/// Mac OS script codes InDesign records as a font's `WritingScript`: Japanese, Traditional
/// Chinese, Korean, Simplified Chinese.
pub fn is_cjk_script(script: u32) -> bool {
    matches!(script, 1 | 2 | 3 | 25)
}

/// Adobe's vendor ids (`ADBE`; `ADBO` on Source Han and other open-source Adobe fonts).
pub fn is_adobe_vendor(vendor: &str) -> bool {
    matches!(vendor.trim(), "ADBE" | "ADBO" | "adbe")
}

/// A file inside an InDesign installation's `Resources\Required\Fonts` (or `Fonts`) folder:
/// licensed for InDesign's own use only.
pub fn indesign_only(path: &Path) -> bool {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    parts.iter().enumerate().any(|(i, p)| {
        p.starts_with("adobe indesign") && parts.get(i + 1).is_some_and(|n| n == "resources" || n == "fonts" || n == "required")
    })
}

fn be16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn read_at(f: &mut std::fs::File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(len);
    f.take(len as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

fn bad(m: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string())
}

/// `OS/2` facts of face `index` of a TrueType / OpenType file or collection; `Ok(None)` when
/// the face has no `OS/2` table (treated as installable, as InDesign does with old fonts).
pub fn read_face_flags(path: &Path, index: u32) -> std::io::Result<Option<FaceFlags>> {
    let mut f = std::fs::File::open(path)?;
    let head = read_at(&mut f, 0, 12)?;
    let face_offset = if head.get(..4) == Some(b"ttcf") {
        let n = be32(&head, 8).ok_or_else(|| bad("short collection header"))?;
        if index >= n {
            return Err(bad("face index outside the collection"));
        }
        let entry = read_at(&mut f, 12 + 4 * index as u64, 4)?;
        be32(&entry, 0).ok_or_else(|| bad("short collection offsets"))? as u64
    } else {
        0
    };
    let offset_table = read_at(&mut f, face_offset, 12)?;
    let tag = offset_table.get(..4).ok_or_else(|| bad("short font header"))?;
    if !matches!(tag, [0, 1, 0, 0] | b"OTTO" | b"true" | b"typ1") {
        return Err(bad("not a TrueType / OpenType font"));
    }
    let tables = be16(&offset_table, 4).ok_or_else(|| bad("short font header"))? as usize;
    let dir = read_at(&mut f, face_offset + 12, tables * 16)?;
    for i in 0..tables {
        let r = i * 16;
        if dir.get(r..r + 4) == Some(b"OS/2") {
            let off = be32(&dir, r + 8).ok_or_else(|| bad("short table record"))? as u64;
            let len = be32(&dir, r + 12).ok_or_else(|| bad("short table record"))? as usize;
            let os2 = read_at(&mut f, off, len.min(96))?;
            let version = be16(&os2, 0).unwrap_or(0);
            let fs_type = be16(&os2, 8).ok_or_else(|| bad("short OS/2 table"))?;
            let vendor = os2.get(58..62).map(|v| String::from_utf8_lossy(v).trim().to_string()).unwrap_or_default();
            let code_pages = if version >= 1 { be32(&os2, 78) } else { None };
            return Ok(Some(FaceFlags { fs_type, vendor, code_pages }));
        }
    }
    Ok(None)
}

/// Why a used font file is not copied (stable codes for the JSON report).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip {
    pub code: &'static str,
    pub reason: String,
}

fn skip(code: &'static str, reason: impl Into<String>) -> Skip {
    Skip { code, reason: reason.into() }
}

/// The copy decision for one font file holding `faces` (face index, used with a CJK
/// writing script in the document).
pub fn decide(path: &Path, source: FontSource, faces: &[(u32, bool)], cjk_fonts: bool) -> Result<Vec<FaceFlags>, Skip> {
    if indesign_only(path) {
        return Err(skip("indesignOnly", "InDesign 전용 글꼴 폴더(Resources\\Required\\Fonts)의 글꼴 — InDesign 사용권이라 복사하지 않음"));
    }
    if source == FontSource::Adobe {
        return Err(skip("adobeFonts", "Adobe Fonts에서 활성화한 글꼴 — InDesign과 같이 복사하지 않음(인쇄소에서 Adobe Fonts로 활성화)"));
    }
    let mut out = Vec::new();
    for &(index, cjk_script) in faces {
        let flags = match read_face_flags(path, index) {
            Ok(f) => f.unwrap_or_default(),
            Err(e) => return Err(skip("unreadable", format!("글꼴 파일을 읽을 수 없음: {e}"))),
        };
        match embedding(flags.fs_type) {
            Embedding::Restricted => return Err(skip("fsTypeRestricted", format!("글꼴 사용권이 포함·전달을 금지함(OS/2 fsType 0x{:04X}: 제한된 사용권)", flags.fs_type))),
            Embedding::PreviewPrint => {
                return Err(skip("fsTypePreviewPrint", format!("글꼴 사용권이 미리보기·인쇄용 포함만 허용(OS/2 fsType 0x{:04X})", flags.fs_type)));
            }
            Embedding::BitmapOnly => return Err(skip("fsTypeBitmapOnly", format!("글꼴 사용권이 비트맵 포함만 허용(OS/2 fsType 0x{:04X})", flags.fs_type))),
            Embedding::Installable | Embedding::Editable => {}
        }
        if !cjk_fonts && (is_cjk(&flags) || cjk_script) && !is_adobe_vendor(&flags.vendor) {
            let vendor = if flags.vendor.is_empty() { "제작사 없음".to_string() } else { format!("제작사 {}", flags.vendor) };
            return Err(skip("nonAdobeCjk", format!("Adobe가 아닌 한중일 글꼴({vendor}) — InDesign과 같이 복사하지 않음(글꼴 사용권 확인 후 '한중일 글꼴도 복사'로 넣을 수 있음)")));
        }
        out.push(flags);
    }
    Ok(out)
}

/// Where a face came from, for the report.
pub fn origin_word(source: FontSource) -> &'static str {
    match source {
        FontSource::Extra => "documentFonts",
        FontSource::Adobe => "adobeFonts",
        FontSource::User => "user",
        FontSource::System => "system",
    }
}

/// The face the store would use for (family, style), when it is the one asked for, and
/// whether it is a CJK face by its character map ([`has_cjk_glyphs`]).
pub fn exact_face(store: &FontStore, family: &str, style: &str, table: &[folio_model::FontInfo]) -> Option<(folio_text::FaceInfo, bool)> {
    let r = store.resolve(family, style, table);
    if !r.exact || store.is_synthetic(r.font) {
        return None;
    }
    let cjk = store.with_face(r.font, |f| has_cjk_glyphs(&|c| f.glyph_index(c).is_some())).unwrap_or(false);
    store.face_info(r.font).cloned().map(|i| (i, cjk))
}

/// A face that covers a CJK writing system: all Hangul syllables (가, 힣), kana (あ, ア) or
/// common ideographs (一, 龥). Some CJK fonts (variable Noto Sans KR) do not declare a CJK
/// code page in `OS/2`.
pub fn has_cjk_glyphs(has: &dyn Fn(char) -> bool) -> bool {
    (has('가') && has('힣')) || (has('あ') && has('ア')) || (has('一') && has('龥'))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal sfnt with only an `OS/2` table (version 1).
    pub fn sfnt(fs_type: u16, vendor: &[u8; 4], code_pages: u32) -> Vec<u8> {
        let mut os2 = vec![0u8; 86];
        os2[0..2].copy_from_slice(&1u16.to_be_bytes());
        os2[8..10].copy_from_slice(&fs_type.to_be_bytes());
        os2[58..62].copy_from_slice(vendor);
        os2[78..82].copy_from_slice(&code_pages.to_be_bytes());
        let mut f = Vec::new();
        f.extend_from_slice(&[0, 1, 0, 0]);
        f.extend_from_slice(&1u16.to_be_bytes());
        f.extend_from_slice(&[0; 6]);
        let table_at = 12 + 16;
        f.extend_from_slice(b"OS/2");
        f.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(&(table_at as u32).to_be_bytes());
        f.extend_from_slice(&(os2.len() as u32).to_be_bytes());
        f.extend_from_slice(&os2);
        f
    }

    /// A collection of `faces` (each made by `sfnt`), table offsets rebased.
    pub fn ttc(faces: &[Vec<u8>]) -> Vec<u8> {
        let header = 12 + 4 * faces.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"ttcf");
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(faces.len() as u32).to_be_bytes());
        let mut at = header;
        let mut bodies = Vec::new();
        for f in faces {
            out.extend_from_slice(&(at as u32).to_be_bytes());
            let mut b = f.clone();
            // rebase the single table record's offset
            let off = u32::from_be_bytes(b[20..24].try_into().unwrap()) + at as u32;
            b[20..24].copy_from_slice(&off.to_be_bytes());
            at += b.len();
            bodies.push(b);
        }
        for b in bodies {
            out.extend_from_slice(&b);
        }
        out
    }

    #[test]
    fn fs_type_rules() {
        assert_eq!(embedding(0x0000), Embedding::Installable);
        assert_eq!(embedding(0x0001), Embedding::Installable);
        assert_eq!(embedding(0x0002), Embedding::Restricted);
        assert_eq!(embedding(0x0004), Embedding::PreviewPrint);
        assert_eq!(embedding(0x0008), Embedding::Editable);
        // several usage bits: the least restrictive applies
        assert_eq!(embedding(0x000C), Embedding::Editable);
        assert_eq!(embedding(0x0006), Embedding::PreviewPrint);
        // no-subsetting does not matter for a whole-file copy; bitmap-only does
        assert_eq!(embedding(0x0100), Embedding::Installable);
        assert_eq!(embedding(0x0208), Embedding::BitmapOnly);
        for (t, ok) in [(0x0000, true), (0x0008, true), (0x0108, true), (0x0002, false), (0x0004, false), (0x0200, false)] {
            assert_eq!(allows_packaging(embedding(t)), ok, "fsType {t:#06x}");
        }
    }

    #[test]
    fn reads_os2_of_fonts_and_collections() {
        let dir = std::env::temp_dir().join(format!("folio-package-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let one = dir.join("one.otf");
        std::fs::write(&one, sfnt(0x0004, b"ACME", 1 << 19)).unwrap();
        let f = read_face_flags(&one, 0).unwrap().unwrap();
        assert_eq!(f, FaceFlags { fs_type: 4, vendor: "ACME".into(), code_pages: Some(1 << 19) });
        assert!(is_cjk(&f));

        let coll = dir.join("set.ttc");
        std::fs::write(&coll, ttc(&[sfnt(0, b"ADBE", 1 << 17), sfnt(0x0002, b"XYZ ", 1)])).unwrap();
        let a = read_face_flags(&coll, 0).unwrap().unwrap();
        let b = read_face_flags(&coll, 1).unwrap().unwrap();
        assert_eq!((a.fs_type, a.vendor.as_str()), (0, "ADBE"));
        assert_eq!((b.fs_type, b.vendor.as_str()), (2, "XYZ"));
        assert!(read_face_flags(&coll, 2).is_err());

        // decisions: the collection is one file; a restricted used face keeps it out
        assert!(decide(&coll, FontSource::User, &[(0, false)], false).is_ok(), "Adobe CJK face is copied");
        assert_eq!(decide(&coll, FontSource::User, &[(0, false), (1, false)], false).unwrap_err().code, "fsTypeRestricted");
        assert_eq!(decide(&one, FontSource::System, &[(0, false)], true).unwrap_err().code, "fsTypePreviewPrint");
        let ko = dir.join("ko.ttf");
        std::fs::write(&ko, sfnt(0x0008, b"SDOL", 1 << 19)).unwrap();
        assert_eq!(decide(&ko, FontSource::User, &[(0, false)], false).unwrap_err().code, "nonAdobeCjk");
        assert!(decide(&ko, FontSource::User, &[(0, false)], true).is_ok(), "copied when the caller asks for CJK fonts");
        let latin = dir.join("latin.ttf");
        std::fs::write(&latin, sfnt(0, b"ACME", 1)).unwrap();
        assert!(decide(&latin, FontSource::Extra, &[(0, false)], false).is_ok());
        // the document says the face is used for Korean even though the file declares no code page
        assert_eq!(decide(&latin, FontSource::User, &[(0, true)], false).unwrap_err().code, "nonAdobeCjk");
        assert_eq!(decide(&latin, FontSource::Adobe, &[(0, false)], true).unwrap_err().code, "adobeFonts");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cjk_by_character_map() {
        let set = |s: &'static str| move |c: char| s.contains(c);
        assert!(has_cjk_glyphs(&set("abc가힣")));
        assert!(has_cjk_glyphs(&set("あア")));
        assert!(has_cjk_glyphs(&set("一龥")));
        // a Latin font with a few Hangul letters or a single kana is not a CJK font
        assert!(!has_cjk_glyphs(&set("abc가")));
        assert!(!has_cjk_glyphs(&set("あ一")));
    }

    #[test]
    fn indesign_required_fonts_are_never_copied() {
        let required = Path::new("Adobe/Adobe InDesign 2025/Resources/Required/Fonts/x.otf");
        assert!(indesign_only(required));
        assert!(indesign_only(Path::new("Adobe/Adobe InDesign 2024/Fonts/x.otf")));
        assert!(!indesign_only(Path::new("system/Fonts/arial.ttf")));
        assert!(!indesign_only(Path::new("work/Adobe InDesign notes/x.otf")));
        let e = decide(required, FontSource::Extra, &[(0, false)], true);
        assert_eq!(e.unwrap_err().code, "indesignOnly");
    }
}
