//! The XMP packet InDesign's own toolkit makes from the tags of a placed raster file and keeps with the
//! graphic (`MetadataPacketPreference`; measured in basic/place-tiff*, place-jpeg-exif*, place-psd*, placeimg-c1).
//! A TIFF's first IFD, or the first IFD of a JPEG's EXIF block, gives the `tiff:` properties in the toolkit's fixed
//! order, each as the file states it. Files without such tags (PNG, a JPEG without EXIF) get none. Not covered:
//! the descriptive tags (`Make`, `Model`, `DateTime`, …) and the EXIF sub-IFD. A PSD that has no packet and no profile of its
//! own gets InDesign's three-date packet (the tool named is Folio's own). A file that carries an XMP packet of its own
//! (JPEG APP1, TIFF tag 700, PSD resource 1060) gets the packet InDesign writes after parsing it again in its own layout,
//! with no `tiff:` merge (place-xmp-jpg, place-xmp-tif: measured); that rewrite is not made here, so such files get none.

use super::tiff::{FileSource, Ifd, Tiff};
use std::path::Path;

/// The packet text for the raster file at `path`, or none when the toolkit would not make one.
pub fn raster_xmp(path: &Path) -> Option<String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    raster_xmp_at(path, now)
}

/// [`raster_xmp`] with the time of the placement (Unix seconds; written in UTC) a PSD's packet carries.
pub fn raster_xmp_at(path: &Path, now: i64) -> Option<String> {
    use std::io::{Read, Seek};
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 4];
    file.read_exact(&mut head).ok()?;
    if &head == b"8BPS" {
        file.rewind().ok()?;
        return psd_packet(&mut file, now);
    }
    let lines = if &head == b"II*\0" || &head == b"MM\0*" {
        let mut t = Tiff::open(FileSource(&mut file))?;
        let first = t.first;
        let ifd = t.ifd(first)?;
        if ifd.get(700).is_some() {
            return None;
        }
        lines(&ifd)
    } else if head[..3] == [0xFF, 0xD8, 0xFF] {
        drop(file);
        let bytes = std::fs::read(path).ok()?;
        if carries_xmp(&bytes) {
            return None;
        }
        let exif = jpeg_exif(&bytes)?;
        let mut t = Tiff::open(exif)?;
        let first = t.first;
        lines(&t.ifd(first)?)
    } else {
        return None;
    };
    if lines.is_empty() {
        return None;
    }
    Some(wrap("            xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\"", &lines.concat()))
}

fn wrap(namespaces: &str, body: &str) -> String {
    let mut out = String::from("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    out.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Adobe XMP Core 9.1-c003 116.7932243, 2025/09/24-19:55:18        \">\n");
    out.push_str("   <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    out.push_str(&format!("      <rdf:Description rdf:about=\"\"\n{namespaces}>\n"));
    out.push_str(body);
    out.push_str("      </rdf:Description>\n   </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"r\"?>");
    out
}

/// A PSD (grayscale, RGB or CMYK) with no XMP packet and no ICC profile of its own; others are not covered.
fn psd_packet(file: &mut std::fs::File, now: i64) -> Option<String> {
    use std::io::Read;
    let mut h = [0u8; 30];
    file.read_exact(&mut h).ok()?;
    let mode = u16::from_be_bytes([h[24], h[25]]);
    if !matches!(mode, 1 | 3 | 4) {
        return None;
    }
    let mut skip = vec![0u8; u32::from_be_bytes([h[26], h[27], h[28], h[29]]) as usize];
    file.read_exact(&mut skip).ok()?;
    let mut len = [0u8; 4];
    file.read_exact(&mut len).ok()?;
    let len = u32::from_be_bytes(len) as usize;
    if len > 64 << 20 {
        return None;
    }
    let mut b = vec![0u8; len];
    file.read_exact(&mut b).ok()?;
    let mut b = &b[..];
    while b.len() >= 12 && &b[..4] == b"8BIM" {
        let id = u16::from_be_bytes([b[4], b[5]]);
        if id == 1060 || id == 1039 {
            return None;
        }
        let mut o = 7 + b[6] as usize;
        o += o % 2;
        let size = u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?) as usize;
        let next = o + 4 + size + size % 2;
        b = b.get(next..).unwrap_or(&[]);
    }
    let date = utc_stamp(now);
    let body = format!(
        "         <xmp:CreatorTool>Folio Studio</xmp:CreatorTool>\n         <xmp:CreateDate>{date}</xmp:CreateDate>\n         <xmp:ModifyDate>{date}</xmp:ModifyDate>\n         <xmp:MetadataDate>{date}</xmp:MetadataDate>\n         <dc:format>application/vnd.adobe.photoshop</dc:format>\n         <photoshop:ColorMode>{mode}</photoshop:ColorMode>\n"
    );
    let namespaces = "            xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n            xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n            xmlns:photoshop=\"http://ns.adobe.com/photoshop/1.0/\"";
    Some(wrap(namespaces, &body))
}

/// `YYYY-MM-DDTHH:MM:SS+00:00` for Unix seconds.
fn utc_stamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Whether a JPEG has an XMP APP1 segment before the first scan.
fn carries_xmp(b: &[u8]) -> bool {
    let mut o = 2;
    while o + 4 <= b.len() && b[o] == 0xFF {
        let m = b[o + 1];
        if m == 0xDA || m == 0xD9 {
            return false;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            o += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[o + 2], b[o + 3]]) as usize;
        let Some(seg) = b.get(o + 4..o + 2 + len) else { return false };
        if m == 0xE1 && seg.starts_with(b"http://ns.adobe.com/xap/1.0/\0") {
            return true;
        }
        o += 2 + len;
    }
    false
}

/// The EXIF block (after `Exif\0\0`) of the first APP1 segment that has one, before the first scan.
fn jpeg_exif(b: &[u8]) -> Option<&[u8]> {
    let mut o = 2;
    while o + 4 <= b.len() && b[o] == 0xFF {
        let m = b[o + 1];
        if m == 0xDA || m == 0xD9 {
            return None;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            o += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[o + 2], b[o + 3]]) as usize;
        let seg = b.get(o + 4..o + 2 + len)?;
        if m == 0xE1 && seg.starts_with(b"Exif\0\0") {
            return Some(&seg[6..]);
        }
        o += 2 + len;
    }
    None
}

/// The properties, in the toolkit's order (tag, XMP name, kind).
const ORDER: [(u16, &str, Kind); 18] = [
    (256, "ImageWidth", Kind::One),
    (257, "ImageLength", Kind::One),
    (258, "BitsPerSample", Kind::Seq),
    (259, "Compression", Kind::One),
    (262, "PhotometricInterpretation", Kind::One),
    (274, "Orientation", Kind::One),
    (277, "SamplesPerPixel", Kind::One),
    (284, "PlanarConfiguration", Kind::One),
    (530, "YCbCrSubSampling", Kind::Seq),
    (531, "YCbCrPositioning", Kind::One),
    (282, "XResolution", Kind::Ratio),
    (283, "YResolution", Kind::Ratio),
    (296, "ResolutionUnit", Kind::One),
    (301, "TransferFunction", Kind::Seq),
    (318, "WhitePoint", Kind::RatioSeq),
    (319, "PrimaryChromaticities", Kind::RatioSeq),
    (529, "YCbCrCoefficients", Kind::RatioSeq),
    (532, "ReferenceBlackWhite", Kind::RatioSeq),
];

#[derive(Clone, Copy)]
enum Kind {
    One,
    Seq,
    Ratio,
    RatioSeq,
}

fn lines(ifd: &Ifd) -> Vec<String> {
    let mut out = Vec::new();
    let seq = |name: &str, items: Vec<String>| {
        let mut s = format!("         <tiff:{name}>\n            <rdf:Seq>\n");
        for i in items {
            s.push_str(&format!("               <rdf:li>{i}</rdf:li>\n"));
        }
        s.push_str(&format!("            </rdf:Seq>\n         </tiff:{name}>\n"));
        s
    };
    for (tag, name, kind) in ORDER {
        if ifd.get(tag).is_none() {
            continue;
        }
        match kind {
            Kind::One => {
                if let Some(v) = ifd.uint(tag) {
                    out.push(format!("         <tiff:{name}>{v}</tiff:{name}>\n"));
                }
            }
            Kind::Seq => {
                let v = ifd.uints(tag);
                if !v.is_empty() {
                    out.push(seq(name, v.iter().map(u64::to_string).collect()));
                }
            }
            Kind::Ratio => {
                if let Some((n, d)) = ifd.rationals(tag).first() {
                    out.push(format!("         <tiff:{name}>{n}/{d}</tiff:{name}>\n"));
                }
            }
            Kind::RatioSeq => {
                let v = ifd.rationals(tag);
                if !v.is_empty() {
                    out.push(seq(name, v.iter().map(|(n, d)| format!("{n}/{d}")).collect()));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_stamp_is_iso() {
        assert_eq!(super::utc_stamp(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(super::utc_stamp(1_792_000_000), "2026-10-14T17:46:40+00:00");
    }
}
