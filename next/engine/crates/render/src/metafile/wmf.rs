//! Reads WMF drawing records and maps them to the shared SVG output.

use super::{Converted, Obj, Out, State, brush_of, colorref, content, draw_poly, i16le, pen_of, select, svg_text, u16le, u32le};

pub(super) fn wmf(b: &[u8]) -> Result<Converted, String> {
    let placeable = b.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]);
    let (head, bbox, inch) = if placeable {
        let g = |at| i16le(b, at).map(f64::from).ok_or("WMF: short header");
        (22usize, Some([g(6)?, g(8)?, g(10)?, g(12)?]), f64::from(u16le(b, 14).unwrap_or(1440).max(1)))
    } else {
        (0, None, 1440.0)
    };
    let header_words = u16le(b, head + 2).unwrap_or(9) as usize;
    let mut at = head + header_words * 2;
    let mut out = Out::new(1.0);
    // the shapes stay in logical units: the window (or the placeable box) is the SVG's viewBox
    let mut s = State::new();
    let mut stack: Vec<State> = Vec::new();
    // the object table: each create takes the lowest free slot
    let mut objects: Vec<Option<Obj>> = Vec::new();
    let put = |objects: &mut Vec<Option<Obj>>, o: Obj| {
        if let Some(slot) = objects.iter_mut().find(|x| x.is_none()) {
            *slot = Some(o);
        } else {
            objects.push(Some(o));
        }
    };
    let mut window: Option<[f64; 4]> = None;
    let mut count = 0usize;
    while at + 6 <= b.len() && count < 2_000_000 {
        count += 1;
        let words = u32le(b, at).unwrap_or(0) as usize;
        let func = u16le(b, at + 4).unwrap_or(0);
        if words < 3 || at + words * 2 > b.len() {
            break;
        }
        let p = &b[at + 6..at + words * 2];
        let w = |k: usize| f64::from(i16le(p, k * 2).unwrap_or(0));
        let u = |k: usize| u16le(p, k * 2).unwrap_or(0);
        let pts = |from: usize, n: usize| -> Vec<(f64, f64)> { (0..n).map(|k| (w(from + k * 2), w(from + k * 2 + 1))).collect() };
        match func {
            0x0000 => break,
            0x020B => window = Some([w(1), w(0), window.map_or(0.0, |v| v[2]), window.map_or(0.0, |v| v[3])]),
            0x020C => window = Some([window.map_or(0.0, |v| v[0]), window.map_or(0.0, |v| v[1]), w(1), w(0)]),
            0x0106 => s.evenodd = u(0) != 2,
            0x001E => stack.push(s),
            0x0127 => {
                if let Some(t) = stack.pop() {
                    s = t;
                }
            }
            0x02FA => put(&mut objects, Obj::Pen(pen_of(u32::from(u(0)), w(1), u32::from(u(3)) | u32::from(u(4)) << 16))),
            0x02FC => {
                let (brush, hatched) = brush_of(u32::from(u(0)), u32::from(u(1)) | u32::from(u(2)) << 16);
                if hatched {
                    out.skip("hatch/pattern brush");
                }
                put(&mut objects, Obj::Brush(brush));
            }
            // text: CREATEFONTINDIRECT, SETTEXTCOLOR, SETTEXTALIGN, SETBKMODE, SETBKCOLOR, TEXTOUT, EXTTEXTOUT
            0x02FB => {
                out.fonts.push(content::Font::wmf(p));
                put(&mut objects, Obj::Font(out.fonts.len() - 1));
            }
            0x0209 => s.text_color = colorref(u32::from(u(0)) | u32::from(u(1)) << 16),
            0x012E => s.text_align = u32::from(u(0)),
            0x0102 => s.bk_mode = u32::from(u(0)),
            0x0201 => s.bk_color = colorref(u32::from(u(0)) | u32::from(u(1)) << 16),
            0x0521 => {
                // count, string (padded to a word), y, x
                let n = u(0) as usize;
                let k = 1 + n.div_ceil(2);
                let charset = s.font.and_then(|i| out.fonts.get(i)).map_or(0, |f| f.charset);
                let chars = content::decode_ansi(p.get(2..2 + n).unwrap_or(&[]), charset);
                out.text(&mut s, (w(k + 1), w(k)), &chars, &[], None);
            }
            0x0A32 => {
                // y, x, count, options, [rectangle when opaque or clipped], string, [advances]
                let (n, options) = (u(2) as usize, u(3));
                let rect = options & 6 != 0;
                let from = if rect { 16 } else { 8 };
                let charset = s.font.and_then(|i| out.fonts.get(i)).map_or(0, |f| f.charset);
                let chars = content::decode_ansi(p.get(from..from + n).unwrap_or(&[]), charset);
                let dx_at = from + n.div_ceil(2) * 2;
                let dx: Vec<f64> = (0..n).filter_map(|k| i16le(p, dx_at + k * 2).map(f64::from)).collect();
                let dx = if p.len() >= dx_at + n * 2 { dx } else { Vec::new() };
                let opaque = (rect && options & 2 != 0).then(|| [w(4), w(5), w(6), w(7)]);
                out.text(&mut s, (w(1), w(0)), &chars, &dx, opaque);
            }
            // bitmaps: STRETCHDIB, DIBSTRETCHBLT, DIBBITBLT (a packed DIB after the parameters)
            0x0F43 | 0x0B41 | 0x0940 => {
                let rop = u32::from(u(0)) | u32::from(u(1)) << 16;
                // parameter words after the raster op (STRETCHDIB: + usage): source h, w, y, x (stretch) …, dest h, w, y, x
                let (dib_at, dest, src) = match func {
                    0x0F43 => (22, (w(10), w(9), w(8), w(7)), [w(6), w(5), w(4), w(3)]),
                    0x0B41 => (20, (w(9), w(8), w(7), w(6)), [w(5), w(4), w(3), w(2)]),
                    _ => (16, (w(7), w(6), w(5), w(4)), [w(3), w(2), w(5), w(4)]),
                };
                let picture = p.get(dib_at..).and_then(content::split_dib);
                if rop != 0x00CC_0020 {
                    out.skip("bitmap raster operation");
                } else if let Some((bmi, bits)) = picture {
                    let (x, y, dw, dh) = dest;
                    out.bitmap(&s, content::dib(bmi, bits, Some(src.map(|v| v as i64)), false), (x, y, dw, dh), 1.0);
                } else {
                    out.skip("bitmap (no picture)");
                }
            }
            0x00F7 | 0x01F9 | 0x0142 | 0x06FF => {
                if func == 0x01F9 || func == 0x0142 {
                    out.skip("hatch/pattern brush");
                }
                put(&mut objects, Obj::Other);
            }
            0x012D => {
                if let Some(Some(o)) = objects.get(u(0) as usize) {
                    select(&mut s, *o);
                }
            }
            0x01F0 => {
                if let Some(slot) = objects.get_mut(u(0) as usize) {
                    *slot = None;
                }
            }
            0x0214 => s.at = (w(1), w(0)),
            0x0213 => {
                let to = (w(1), w(0));
                let d = out.poly(&s, &[s.at, to], false);
                out.draw(&s, &d, false, true);
                s.at = to;
            }
            0x041B | 0x0418 | 0x061C => {
                let o = if func == 0x061C { 2 } else { 0 };
                let (bt, r, t, l) = (w(o), w(o + 1), w(o + 2), w(o + 3));
                let d = match func {
                    0x0418 => out.ellipse(&s, l, t, r, bt),
                    0x041B => out.poly(&s, &[(l, t), (r, t), (r, bt), (l, bt)], true),
                    _ => out.round_rect(&s, l, t, r, bt, w(1), w(0)),
                };
                out.figure(&s, d, true);
            }
            0x0324 | 0x0325 => {
                let n = u(0) as usize;
                draw_poly(&mut out, &mut s, if func == 0x0324 { 86 } else { 87 }, &pts(1, n));
            }
            0x0538 => {
                let polys = u(0) as usize;
                let counts: Vec<usize> = (0..polys).map(|k| u(1 + k) as usize).collect();
                let mut k = 1 + polys;
                let mut d = String::new();
                for c in counts {
                    d.push_str(&out.poly(&s, &pts(k, c), true));
                    k += c * 2;
                }
                out.figure(&s, d, true);
            }
            0x0922 | 0x0A43 | 0x0D33 => out.skip("bitmap"),
            0x0416 | 0x0415 | 0x0220 | 0x012C => out.skip("clipping"),
            _ => {}
        }
        at += words * 2;
    }
    // the placeable box (logical units, `inch` per inch), else the window
    let view = match (bbox, window) {
        (Some([l, t, r, bt]), _) if r > l && bt > t => [l, t, r - l, bt - t],
        (_, Some([x, y, cw, ch])) if cw != 0.0 && ch != 0.0 => [x.min(x + cw), y.min(y + ch), cw.abs(), ch.abs()],
        _ => return Err("WMF: no bounding box or window".into()),
    };
    // WMF coordinates are drawn as logical units (window = viewBox); the device mapping is the viewBox's
    let size = [view[2] / inch * 72.0, view[3] / inch * 72.0];
    let skipped = out.skipped.into_iter().collect();
    Ok(Converted { svg: svg_text(&out.shapes, view, size), size, skipped })
}
