//! TIFF (and EXIF, which is a TIFF structure): first IFD tags and the IFD count.

use super::{ColorSpace, ImageFormat, ImageInfo, ProbeError, Result, malformed, read_at};

/// Random access to TIFF bytes: a file or an in-memory EXIF block.
pub(crate) trait Source {
    fn read(&mut self, offset: u64, len: usize) -> Option<Vec<u8>>;
}

impl Source for &[u8] {
    fn read(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let o = usize::try_from(offset).ok()?;
        self.get(o..o.checked_add(len)?).map(<[u8]>::to_vec)
    }
}

pub(crate) struct FileSource<'a>(pub &'a mut std::fs::File);

impl Source for FileSource<'_> {
    fn read(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let v = read_at(self.0, offset, len).ok()?;
        (v.len() == len).then_some(v)
    }
}

#[derive(Clone, Copy)]
struct Order {
    little: bool,
    big: bool,
}

impl Order {
    fn u16(&self, b: &[u8], o: usize) -> u16 {
        let a: [u8; 2] = b[o..o + 2].try_into().unwrap_or([0; 2]);
        if self.little { u16::from_le_bytes(a) } else { u16::from_be_bytes(a) }
    }
    fn u32(&self, b: &[u8], o: usize) -> u32 {
        let a: [u8; 4] = b[o..o + 4].try_into().unwrap_or([0; 4]);
        if self.little { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) }
    }
    fn u64(&self, b: &[u8], o: usize) -> u64 {
        let a: [u8; 8] = b[o..o + 8].try_into().unwrap_or([0; 8]);
        if self.little { u64::from_le_bytes(a) } else { u64::from_be_bytes(a) }
    }
}

/// One IFD entry: tag, type, count and the value bytes (inline or fetched).
pub(crate) struct Entry {
    pub tag: u16,
    pub typ: u16,
    pub count: u64,
    pub data: Vec<u8>,
}

pub(crate) struct Ifd {
    pub entries: Vec<Entry>,
    pub next: u64,
    order: Order,
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => 1,
    }
}

/// Largest tag payload fetched (ICC profiles are the largest ones read).
const MAX_VALUE: usize = 16 << 20;

impl Ifd {
    pub fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|e| e.tag == tag)
    }
    /// Unsigned integer values (BYTE, SHORT, LONG, LONG8).
    pub fn uints(&self, tag: u16) -> Vec<u64> {
        let Some(e) = self.get(tag) else { return Vec::new() };
        let n = e.count.min(64) as usize;
        let sz = type_size(e.typ);
        (0..n)
            .filter_map(|i| {
                let o = i * sz;
                if o + sz > e.data.len() {
                    return None;
                }
                Some(match e.typ {
                    1 | 7 => e.data[o] as u64,
                    3 => self.order.u16(&e.data, o) as u64,
                    4 | 13 => self.order.u32(&e.data, o) as u64,
                    16 | 18 => self.order.u64(&e.data, o),
                    _ => return None,
                })
            })
            .collect()
    }
    pub fn uint(&self, tag: u16) -> Option<u64> {
        self.uints(tag).first().copied()
    }
    /// RATIONAL values as stored (numerator, denominator).
    pub fn rationals(&self, tag: u16) -> Vec<(u32, u32)> {
        let Some(e) = self.get(tag) else { return Vec::new() };
        if e.typ != 5 {
            return Vec::new();
        }
        (0..(e.count.min(64) as usize)).filter(|i| i * 8 + 8 <= e.data.len()).map(|i| (self.order.u32(&e.data, i * 8), self.order.u32(&e.data, i * 8 + 4))).collect()
    }
    pub fn rational(&self, tag: u16) -> Option<f64> {
        let e = self.get(tag)?;
        match e.typ {
            5 | 10 if e.data.len() >= 8 => {
                let n = self.order.u32(&e.data, 0) as f64;
                let d = self.order.u32(&e.data, 4) as f64;
                (d != 0.0).then(|| n / d)
            }
            _ => self.uint(tag).map(|v| v as f64),
        }
    }
}

pub(crate) struct Tiff<S: Source> {
    src: S,
    order: Order,
    pub first: u64,
}

impl<S: Source> Tiff<S> {
    pub fn open(mut src: S) -> Option<Self> {
        let h = src.read(0, 16).or_else(|| src.read(0, 8))?;
        let little = match &h[..2] {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let o = Order { little, big: false };
        match o.u16(&h, 2) {
            42 => Some(Tiff { first: o.u32(&h, 4) as u64, order: o, src }),
            43 if h.len() >= 16 => Some(Tiff { first: o.u64(&h, 8), order: Order { little, big: true }, src }),
            _ => None,
        }
    }

    pub fn ifd(&mut self, offset: u64) -> Option<Ifd> {
        let o = self.order;
        let (count_len, entry_len, off_len) = if o.big { (8, 20, 8) } else { (2, 12, 4) };
        let c = self.src.read(offset, count_len)?;
        let count = if o.big { o.u64(&c, 0) } else { o.u16(&c, 0) as u64 };
        if count == 0 || count > 4096 {
            return None;
        }
        let body = self.src.read(offset + count_len as u64, count as usize * entry_len + off_len)?;
        let mut entries = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let e = &body[i * entry_len..(i + 1) * entry_len];
            let tag = o.u16(e, 0);
            let typ = o.u16(e, 2);
            let n = if o.big { o.u64(e, 4) } else { o.u32(e, 4) as u64 };
            let size = (n as usize).saturating_mul(type_size(typ));
            let inline = if o.big { 8 } else { 4 };
            let vo = if o.big { 12 } else { 8 };
            let data = if size <= inline {
                e[vo..vo + size].to_vec()
            } else if size <= MAX_VALUE {
                let at = if o.big { o.u64(e, vo) } else { o.u32(e, vo) as u64 };
                self.src.read(at, size).unwrap_or_default()
            } else {
                Vec::new()
            };
            entries.push(Entry { tag, typ, count: n, data });
        }
        let t = &body[count as usize * entry_len..];
        let next = if o.big { o.u64(t, 0) } else { o.u32(t, 0) as u64 };
        Some(Ifd { entries, next, order: o })
    }
}

/// Resolution in pixels per inch from XResolution / YResolution / ResolutionUnit.
pub(crate) fn resolution(ifd: &Ifd) -> Option<(f64, f64)> {
    let x = ifd.rational(282)?;
    let y = ifd.rational(283).unwrap_or(x);
    let f = match ifd.uint(296).unwrap_or(2) {
        3 => 2.54,
        1 => return None,
        _ => 1.0,
    };
    (x > 0.0 && y > 0.0).then_some((x * f, y * f))
}

/// EXIF block (after `Exif\0\0`): resolution and orientation of IFD0.
pub(crate) fn exif(b: &[u8]) -> (Option<(f64, f64)>, Option<u16>) {
    let Some(mut t) = Tiff::open(b) else { return (None, None) };
    let first = t.first;
    let Some(ifd) = t.ifd(first) else { return (None, None) };
    (resolution(&ifd), ifd.uint(274).map(|v| v as u16))
}

pub(crate) fn probe_file(file: &mut std::fs::File, _len: u64) -> Result<ImageInfo> {
    let mut t = Tiff::open(FileSource(file)).ok_or_else(|| ProbeError::Malformed("TIFF header".into()))?;
    let first = t.first;
    let Some(ifd) = t.ifd(first) else { return malformed("TIFF first IFD") };
    let width = ifd.uint(256).unwrap_or(0) as u32;
    let height = ifd.uint(257).unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return malformed("TIFF without size");
    }
    let samples = ifd.uint(277).unwrap_or(1) as u16;
    let bits = ifd.uint(258).unwrap_or(1) as u8;
    let photometric = ifd.uint(262).unwrap_or(1);
    let extra = ifd.uints(338).len() as u16;
    let space = match photometric {
        0 | 1 => ColorSpace::Gray,
        2 | 6 => ColorSpace::Rgb,
        3 => ColorSpace::Indexed,
        // Separated: CMYK when InkSet is 1 (default) and 4 inks; otherwise other inks.
        5 => {
            if ifd.uint(332).unwrap_or(1) == 1 && samples.saturating_sub(extra) == 4 {
                ColorSpace::Cmyk
            } else {
                ColorSpace::Other
            }
        }
        8..=10 => ColorSpace::Lab,
        _ => ColorSpace::Other,
    };
    let mut info = ImageInfo::raster(ImageFormat::Tiff, width, height, space, bits, samples);
    info.alpha = extra > 0;
    info.ppi = resolution(&ifd);
    info.orientation = ifd.uint(274).map(|v| v as u16);
    info.icc = ifd.get(34675).map(|e| e.data.clone()).filter(|d| !d.is_empty());
    // Count pages (IFD chain), bounded.
    let mut pages = 1u32;
    let mut next = ifd.next;
    let mut seen = vec![first];
    while next != 0 && pages < 10_000 && !seen.contains(&next) {
        seen.push(next);
        match t.ifd(next) {
            Some(i) => {
                pages += 1;
                next = i.next;
            }
            None => break,
        }
    }
    info.pages = Some(pages);
    Ok(info)
}
