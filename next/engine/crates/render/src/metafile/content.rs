//! Text and bitmaps of a metafile as SVG `<text>` and `<image>` (build-a2): fonts (LOGFONT), the text colour, alignment and
//! background, glyph advances; device-independent bitmaps (1, 4, 8, 16, 24 and 32 bits, bit fields, embedded JPEG and PNG) as
//! PNG pictures inside the SVG. resvg sets the text with the installed fonts (svg/rich.rs).

/// A font object (LOGFONT): what `<text>` needs.
#[derive(Clone, Debug, Default)]
pub(super) struct Font {
    pub face: String,
    /// lfHeight (logical units): negative = the em size, positive = the cell height.
    pub height: f64,
    pub weight: i32,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// lfEscapement, tenths of a degree counterclockwise.
    pub escapement: f64,
    pub charset: u8,
    pub pitch_family: u8,
}

impl Font {
    /// EMF LOGFONTW (from `at`).
    pub fn emf(p: &[u8], at: usize) -> Font {
        let i = |k: usize| p.get(at + k..at + k + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let u = |k: usize| p.get(at + k).copied().unwrap_or(0);
        let face: Vec<u16> = (0..32).filter_map(|k| p.get(at + 28 + k * 2..at + 30 + k * 2).map(|b| u16::from_le_bytes([b[0], b[1]]))).take_while(|c| *c != 0).collect();
        Font {
            face: String::from_utf16_lossy(&face),
            height: f64::from(i(0)),
            escapement: f64::from(i(8)),
            weight: i(16),
            italic: u(20) != 0,
            underline: u(21) != 0,
            strike: u(22) != 0,
            charset: u(23),
            pitch_family: u(27),
        }
    }

    /// WMF LOGFONT (16-bit fields, ANSI face name).
    pub fn wmf(p: &[u8]) -> Font {
        let i = |k: usize| p.get(k..k + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]));
        let u = |k: usize| p.get(k).copied().unwrap_or(0);
        let face: Vec<u8> = p.get(18..50).unwrap_or(&[]).iter().copied().take_while(|c| *c != 0).collect();
        let charset = u(13);
        Font {
            face: decode_ansi(&face, charset),
            height: f64::from(i(0)),
            escapement: f64::from(i(4)),
            weight: i32::from(i(8)),
            italic: u(10) != 0,
            underline: u(11) != 0,
            strike: u(12) != 0,
            charset,
            pitch_family: u(17),
        }
    }
}

/// 8-bit text in the code page of a font's character set.
pub(super) fn decode_ansi(b: &[u8], charset: u8) -> String {
    let enc = match charset {
        128 => encoding_rs::SHIFT_JIS,
        129 | 130 => encoding_rs::EUC_KR,
        134 => encoding_rs::GBK,
        136 => encoding_rs::BIG5,
        161 => encoding_rs::WINDOWS_1253,
        162 => encoding_rs::WINDOWS_1254,
        177 => encoding_rs::WINDOWS_1255,
        178 => encoding_rs::WINDOWS_1256,
        186 => encoding_rs::WINDOWS_1257,
        204 => encoding_rs::WINDOWS_1251,
        222 => encoding_rs::WINDOWS_874,
        238 => encoding_rs::WINDOWS_1250,
        77 => encoding_rs::MACINTOSH,
        _ => encoding_rs::WINDOWS_1252,
    };
    enc.decode_without_bom_handling(b).0.into_owned()
}

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

/// Text alignment (SETTEXTALIGN): horizontal 0 left, 2 right, 6 centre; vertical 0 top, 8 bottom, 24 baseline.
#[derive(Clone, Copy, Debug)]
pub(super) struct Text<'a> {
    pub font: &'a Font,
    /// The em size in device units.
    pub size: f64,
    pub color: [u8; 3],
    pub align: u32,
    /// Each character's x in device units (the glyph advances the file gives), else the start only.
    pub xs: &'a [f64],
    pub y: f64,
}

/// A `<text>` element: `chars` placed at `xs` on the baseline `y` (device units), rotated by the font's escapement around the start.
pub(super) fn text_element(t: Text<'_>, chars: &str) -> String {
    let f = t.font;
    let chars: String = chars.chars().filter(|c| *c != '\0').collect();
    #[allow(clippy::neg_cmp_op_on_partial_ord, reason = "reject NaN sizes along with nonpositive sizes")]
    if chars.trim().is_empty() || t.xs.is_empty() || !(t.size > 0.0) {
        return String::new();
    }
    let num = super::num;
    // the reference point is the top, bottom or baseline of the text (ascent and descent of a typical face)
    let y = match t.align & 24 {
        24 => t.y,
        8 => t.y - 0.21 * t.size,
        _ => t.y + 0.88 * t.size,
    };
    let anchor = match t.align & 6 {
        6 => " text-anchor=\"middle\"",
        2 => " text-anchor=\"end\"",
        _ => "",
    };
    let generic = match f.pitch_family & 0xF0 {
        0x10 => "serif",
        0x30 => "monospace",
        _ => "sans-serif",
    };
    let family = if f.face.is_empty() { generic.to_owned() } else { format!("'{}', {generic}", escape(&f.face).replace('\'', "")) };
    let deco = match (f.underline, f.strike) {
        (true, true) => " text-decoration=\"underline line-through\"",
        (true, false) => " text-decoration=\"underline\"",
        (false, true) => " text-decoration=\"line-through\"",
        _ => "",
    };
    let xs = if t.xs.len() > 1 && t.align & 6 == 0 { t.xs.iter().map(|v| num(*v)).collect::<Vec<_>>().join(" ") } else { num(t.xs[0]) };
    let rotate = if f.escapement != 0.0 { format!(" transform=\"rotate({} {} {})\"", num(-f.escapement / 10.0), num(t.xs[0]), num(t.y)) } else { String::new() };
    format!(
        "<text x=\"{xs}\" y=\"{}\" font-family=\"{family}\" font-size=\"{}\" font-weight=\"{}\"{}{anchor}{deco} fill=\"{}\" xml:space=\"preserve\"{rotate}>{}</text>\n",
        num(y),
        num(t.size),
        if f.weight > 0 { f.weight.clamp(100, 900) } else { 400 },
        if f.italic { " font-style=\"italic\"" } else { "" },
        super::hex(t.color),
        escape(&chars)
    )
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(b: &[u8]) -> String {
    let mut o = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                o.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                o.push('=');
            }
        }
    }
    o
}

/// An `<image>` of a picture (`mime`, its bytes) on the device rectangle from (x0, y0) to (x1, y1) (either way round: a negative
/// extent mirrors it), `opacity` 0–1.
pub(super) fn image_element(mime: &str, data: &[u8], (x0, y0): (f64, f64), (x1, y1): (f64, f64), opacity: f64) -> String {
    let num = super::num;
    let (w, h) = (x1 - x0, y1 - y0);
    if w == 0.0 || h == 0.0 {
        return String::new();
    }
    // a mirrored destination: the picture drawn into the unit box, scaled by the signed extents
    let op = if opacity < 1.0 { format!(" opacity=\"{}\"", num(opacity.max(0.0))) } else { String::new() };
    format!(
        "<image x=\"0\" y=\"0\" width=\"1\" height=\"1\" preserveAspectRatio=\"none\" transform=\"matrix({} 0 0 {} {} {})\"{op} xlink:href=\"data:{mime};base64,{}\"/>\n",
        num(w), num(h), num(x0), num(y0), base64(data)
    )
}

/// A packed DIB (WMF bitmap records): its BITMAPINFO and its pixels.
pub(super) fn split_dib(d: &[u8]) -> Option<(&[u8], &[u8])> {
    let u32_at = |k: usize| d.get(k..k + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let u16_at = |k: usize| d.get(k..k + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let head = u32_at(0)? as usize;
    let core = head == 12;
    let bits = if core { u16_at(10)? } else { u16_at(14)? } as usize;
    let compression = if core { 0 } else { u32_at(16)? };
    let used = if core { 0 } else { u32_at(32).unwrap_or(0) as usize };
    let colors = if bits <= 8 { if used > 0 { used.min(256) } else { 1 << bits } } else { 0 };
    let masks = if compression == 3 && head == 40 { 12 } else if compression == 6 && head == 40 { 16 } else { 0 };
    let at = head + colors * if core { 3 } else { 4 } + masks;
    (at <= d.len()).then(|| (&d[..at], &d[at..]))
}

/// A device-independent bitmap (BITMAPINFO `bmi`, the pixels `bits`), the part `src` (x, y, width, height; `None`: all) as
/// a picture: its MIME type and bytes (PNG; an embedded JPEG or PNG as it is). `alpha`: a 32-bit bitmap's fourth byte is
/// its (premultiplied) alpha (ALPHABLEND), else 32-bit pixels are opaque.
pub(super) fn dib(bmi: &[u8], bits: &[u8], src: Option<[i64; 4]>, alpha: bool) -> Option<(&'static str, Vec<u8>)> {
    let u32_at = |k: usize| bmi.get(k..k + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let i32_at = |k: usize| u32_at(k).map(|v| v as i32);
    let u16_at = |k: usize| bmi.get(k..k + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let head = u32_at(0)? as usize;
    // BITMAPCOREHEADER (12 bytes) has 16-bit sizes and 3-byte colours
    let core = head == 12;
    let (width, height, bitcount, compression, used) = if core {
        (i64::from(u16_at(4)?), i64::from(u16_at(6)?), u16_at(10)?, 0, 0)
    } else {
        (i64::from(i32_at(4)?), i64::from(i32_at(8)?), u16_at(14)?, u32_at(16)?, u32_at(32).unwrap_or(0) as usize)
    };
    match compression {
        4 => return bits.starts_with(&[0xFF, 0xD8]).then(|| ("image/jpeg", bits.to_vec())),
        5 => return bits.starts_with(b"\x89PNG").then(|| ("image/png", bits.to_vec())),
        0 | 3 | 6 => {}
        _ => return None,
    }
    let (w, h) = (width.unsigned_abs() as usize, height.unsigned_abs() as usize);
    if w == 0 || h == 0 || w > 20_000 || h > 20_000 || w * h > 64_000_000 {
        return None;
    }
    let bottom_up = height > 0;
    let bpp = bitcount as usize;
    if !matches!(bpp, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    let colors = if bpp <= 8 { if used > 0 { used.min(256) } else { 1 << bpp } } else { 0 };
    let entry = if core { 3 } else { 4 };
    let palette: Vec<[u8; 3]> = (0..colors).map(|k| {
        let at = head + k * entry;
        bmi.get(at..at + 3).map_or([0, 0, 0], |c| [c[2], c[1], c[0]])
    }).collect();
    // BI_BITFIELDS: masks after the 40-byte header (or inside a V4/V5 header)
    let masks = if compression == 3 || compression == 6 {
        [u32_at(40).unwrap_or(0), u32_at(44).unwrap_or(0), u32_at(48).unwrap_or(0), if compression == 6 || head >= 56 { u32_at(52).unwrap_or(0) } else { 0 }]
    } else if bpp == 16 {
        [0x7C00, 0x03E0, 0x001F, 0]
    } else {
        [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, if alpha { 0xFF00_0000 } else { 0 }]
    };
    let channel = |v: u32, m: u32| -> u8 {
        if m == 0 { return 255; }
        let shift = m.trailing_zeros();
        let max = m >> shift;
        (((v & m) >> shift) * 255 / max.max(1)) as u8
    };
    let stride = (w * bpp).div_ceil(32) * 4;
    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        let line = if bottom_up { h - 1 - row } else { row };
        let Some(src_line) = bits.get(line * stride..line * stride + stride) else { break };
        for x in 0..w {
            let px: [u8; 4] = match bpp {
                1 | 4 | 8 => {
                    let bit = x * bpp;
                    let byte = src_line[bit / 8];
                    let idx = (byte >> (8 - bpp - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    let c = palette.get(idx as usize).copied().unwrap_or([0, 0, 0]);
                    [c[0], c[1], c[2], 255]
                }
                16 | 32 => {
                    let v = if bpp == 16 { u32::from(u16::from_le_bytes([src_line[x * 2], src_line[x * 2 + 1]])) } else { u32::from_le_bytes([src_line[x * 4], src_line[x * 4 + 1], src_line[x * 4 + 2], src_line[x * 4 + 3]]) };
                    let a = if masks[3] != 0 { channel(v, masks[3]) } else { 255 };
                    let (r, g, b) = (channel(v, masks[0]), channel(v, masks[1]), channel(v, masks[2]));
                    // ALPHABLEND pixels are premultiplied
                    let un = |c: u8| if alpha && a > 0 && a < 255 { ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8 } else { c };
                    [un(r), un(g), un(b), a]
                }
                _ => [src_line[x * 3 + 2], src_line[x * 3 + 1], src_line[x * 3], 255],
            };
            rgba[(row * w + x) * 4..(row * w + x) * 4 + 4].copy_from_slice(&px);
        }
    }
    let mut img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    if let Some([sx, sy, sw, sh]) = src {
        let (sx, sy) = (sx.clamp(0, w as i64) as u32, sy.clamp(0, h as i64) as u32);
        let (sw, sh) = (sw.unsigned_abs().min(u64::from(w as u32 - sx)) as u32, sh.unsigned_abs().min(u64::from(h as u32 - sy)) as u32);
        if sw > 0 && sh > 0 && (sx, sy, sw, sh) != (0, 0, w as u32, h as u32) {
            img = image::imageops::crop_imm(&img, sx, sy, sw, sh).to_image();
        }
    }
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(("image/png", png))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn a_24_bit_bottom_up_dib_becomes_a_png() {
        let mut bmi = vec![0u8; 40];
        bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
        bmi[4..8].copy_from_slice(&2i32.to_le_bytes());
        bmi[8..12].copy_from_slice(&2i32.to_le_bytes());
        bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
        bmi[14..16].copy_from_slice(&24u16.to_le_bytes());
        // rows of 8 bytes (6 + 2 padding), bottom row first: blue, green / red, white
        let bits = [255, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0];
        let (mime, png) = dib(&bmi, &bits, None, false).unwrap();
        assert_eq!(mime, "image/png");
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255], "top left is the last row's first pixel");
        assert_eq!(img.get_pixel(0, 1).0, [0, 0, 255, 255]);
    }

    #[test]
    fn ansi_text_follows_the_font_charset() {
        assert_eq!(decode_ansi(&[0xC7, 0xD1], 129), "한");
        assert_eq!(decode_ansi(b"caf\xE9", 0), "café");
    }
}
