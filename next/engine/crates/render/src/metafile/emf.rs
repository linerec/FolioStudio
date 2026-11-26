//! Reads EMF drawing records and maps them to the shared SVG output.

use super::{Converted, M, Obj, Out, State, brush_of, colorref, content, draw_poly, f32le, hex, i16le, i32le, num, pen_of, select, stock, svg_text, u16le, u32le};

pub(super) fn emf(b: &[u8]) -> Result<Converted, String> {
    let r = |at| i32le(b, at).map(f64::from).ok_or("EMF: short header");
    let (fl, ft, fr, fb) = (r(24)?, r(28)?, r(32)?, r(36)?);
    let (dev_w, dev_h) = (r(72)?, r(76)?);
    let (mm_w, mm_h) = (r(80)?, r(84)?);
    if dev_w <= 0.0 || dev_h <= 0.0 || mm_w <= 0.0 || mm_h <= 0.0 || fr <= fl || fb <= ft {
        return Err("EMF: empty frame or device size".into());
    }
    // device units per mm; the frame (0.01 mm) in device units is the picture
    let (px_mm_x, px_mm_y) = (dev_w / mm_w, dev_h / mm_h);
    let view = [fl / 100.0 * px_mm_x, ft / 100.0 * px_mm_y, (fr - fl) / 100.0 * px_mm_x, (fb - ft) / 100.0 * px_mm_y];
    let size = [(fr - fl) / 100.0 * 72.0 / 25.4, (fb - ft) / 100.0 * 72.0 / 25.4];
    let mut out = Out::new(px_mm_x / 100.0);
    let mut s = State::new();
    let mut stack: Vec<State> = Vec::new();
    let mut objects: std::collections::HashMap<u32, Obj> = Default::default();
    let mut at = 0usize;
    let mut count = 0usize;
    while at + 8 <= b.len() && count < 2_000_000 {
        count += 1;
        let kind = u32le(b, at).unwrap_or(0);
        let size_r = u32le(b, at + 4).unwrap_or(0) as usize;
        if size_r < 8 || at + size_r > b.len() {
            break;
        }
        let p = &b[at + 8..at + size_r];
        let pi = |k: usize| i32le(p, k * 4).map(f64::from).unwrap_or(0.0);
        let pu = |k: usize| u32le(p, k * 4).unwrap_or(0);
        // the metric map modes: device units per logical unit
        let set_metric = |mode: u32, out: &mut Out| {
            out.metric = match mode {
                2 => px_mm_x / 10.0,        // 0.1 mm
                3 => px_mm_x / 100.0,       // 0.01 mm
                4 => px_mm_x * 25.4 / 100.0, // 0.01 inch
                5 => px_mm_x * 25.4 / 1000.0, // 0.001 inch
                6 => px_mm_x * 25.4 / 1440.0, // twips
                _ => 1.0,
            };
        };
        let pts16 = |from: usize, n: usize| -> Vec<(f64, f64)> {
            (0..n).filter_map(|k| Some((f64::from(i16le(p, from + k * 4)?), f64::from(i16le(p, from + k * 4 + 2)?)))).collect()
        };
        let pts32 = |from: usize, n: usize| -> Vec<(f64, f64)> {
            (0..n).filter_map(|k| Some((f64::from(i32le(p, from + k * 8)?), f64::from(i32le(p, from + k * 8 + 4)?)))).collect()
        };
        match kind {
            14 => break,
            9 => s.win_ext = (pi(0), pi(1)),
            10 => s.win_org = (pi(0), pi(1)),
            11 => s.vp_ext = (pi(0), pi(1)),
            12 => s.vp_org = (pi(0), pi(1)),
            17 => {
                s.mode = pu(0);
                set_metric(s.mode, &mut out);
            }
            19 => s.evenodd = pu(0) != 2,
            33 => stack.push(s),
            34 => {
                let n = pi(0) as i64;
                let keep = if n < 0 { stack.len() as i64 + n } else { n - 1 };
                if keep >= 0 && (keep as usize) < stack.len() {
                    s = stack[keep as usize];
                    stack.truncate(keep as usize);
                }
            }
            35 | 36 => {
                let x = M([f32le(p, 0).unwrap_or(1.0), f32le(p, 4).unwrap_or(0.0), f32le(p, 8).unwrap_or(0.0), f32le(p, 12).unwrap_or(1.0), f32le(p, 16).unwrap_or(0.0), f32le(p, 20).unwrap_or(0.0)]);
                s.world = if kind == 35 {
                    x
                } else {
                    match pu(6) {
                        1 => M::ID,
                        2 => x.then(&s.world),
                        3 => s.world.then(&x),
                        _ => x,
                    }
                };
            }
            37 => {
                let i = pu(0);
                let o = if i & 0x8000_0000 != 0 { stock(i) } else { objects.get(&i).copied() };
                if let Some(o) = o {
                    select(&mut s, o);
                }
            }
            38 => {
                objects.insert(pu(0), Obj::Pen(pen_of(pu(1), pi(2), pu(4))));
            }
            39 => {
                let (brush, hatched) = brush_of(pu(1), pu(2));
                if hatched {
                    out.skip("hatch/pattern brush");
                }
                objects.insert(pu(0), Obj::Brush(brush));
            }
            95 => {
                // EXTCREATEPEN: ihPen, offBmi, cbBmi, offBits, cbBits, then style, width, brush style, colour
                objects.insert(pu(0), Obj::Pen(pen_of(pu(5), f64::from(pu(6)), pu(8))));
            }
            40 => {
                objects.remove(&pu(0));
            }
            27 => {
                s.at = (pi(0), pi(1));
                if let Some(path) = out.path.as_mut() {
                    let (x, y) = s.device(s.at.0, s.at.1, out.metric);
                    path.push_str(&format!("M{} {} ", num(x), num(y)));
                }
            }
            54 => {
                let to = (pi(0), pi(1));
                let d = out.poly(&s, &[s.at, to], false);
                if let Some(path) = out.path.as_mut() {
                    let (x, y) = s.device(to.0, to.1, out.metric);
                    path.push_str(&format!("L{} {} ", num(x), num(y)));
                } else {
                    out.draw(&s, &d, false, true);
                }
                s.at = to;
            }
            42..=44 => {
                let (l, t, r, bt) = (pi(0), pi(1), pi(2), pi(3));
                let d = match kind {
                    42 => out.ellipse(&s, l, t, r, bt),
                    43 => out.poly(&s, &[(l, t), (r, t), (r, bt), (l, bt)], true),
                    _ => out.round_rect(&s, l, t, r, bt, pi(4), pi(5)),
                };
                out.figure(&s, d, true);
            }
            85..=89 => {
                // bounds (16), count, points (16-bit)
                let n = pu(4) as usize;
                let pts = pts16(20, n);
                draw_poly(&mut out, &mut s, kind, &pts);
            }
            2..=6 => {
                let n = pu(4) as usize;
                let pts = pts32(20, n);
                let k = match kind { 3 => 86, 4 => 87, 2 => 85, 5 => 88, _ => 89 };
                draw_poly(&mut out, &mut s, k, &pts);
            }
            91 | 90 | 8 | 7 => {
                // bounds, number of polygons, number of points, counts, points
                let polys = pu(4) as usize;
                let total = pu(5) as usize;
                if polys > 100_000 || total > 10_000_000 {
                    out.skip("oversized poly-polygon");
                } else {
                    let counts: Vec<usize> = (0..polys).map(|k| pu(6 + k) as usize).collect();
                    let from = 24 + polys * 4;
                    let pts = if matches!(kind, 91 | 90) { pts16(from, total) } else { pts32(from, total) };
                    let close = matches!(kind, 91 | 8);
                    let mut d = String::new();
                    let mut k = 0;
                    for c in counts {
                        if k + c > pts.len() {
                            break;
                        }
                        d.push_str(&out.poly(&s, &pts[k..k + c], close));
                        k += c;
                    }
                    out.figure(&s, d, close);
                }
            }
            59 => out.path = Some(String::new()),
            60 => {}
            61 => {
                if let Some(path) = out.path.as_mut() {
                    path.push('Z');
                }
            }
            62..=64 => {
                if let Some(d) = out.path.take() {
                    out.draw(&s, &d, kind != 64, kind != 62);
                }
            }
            68 => out.path = None,
            // text: EXTCREATEFONTINDIRECTW, SETTEXTCOLOR, SETTEXTALIGN, SETBKMODE, SETBKCOLOR, EXTTEXTOUTA/W
            82 => {
                out.fonts.push(content::Font::emf(p, 4));
                objects.insert(pu(0), Obj::Font(out.fonts.len() - 1));
            }
            24 => s.text_color = colorref(pu(0)),
            22 => s.text_align = pu(0),
            18 => s.bk_mode = pu(0),
            25 => s.bk_color = colorref(pu(0)),
            83 | 84 => {
                // bounds, graphics mode, x/y scale, EMRTEXT: reference, count, string offset, options, rectangle, advances offset
                let n = (pu(9) as usize).min(65_536);
                let (off, options, off_dx) = ((pu(10) as usize).saturating_sub(8), pu(11), (pu(16) as usize).saturating_sub(8));
                let chars = if kind == 84 {
                    let units: Vec<u16> = (0..n).filter_map(|k| u16le(p, off + k * 2)).collect();
                    String::from_utf16_lossy(&units)
                } else {
                    let charset = s.font.and_then(|i| out.fonts.get(i)).map_or(0, |f| f.charset);
                    content::decode_ansi(p.get(off..off + n).unwrap_or(&[]), charset)
                };
                // ETO_PDY (0x2000): an x and a y advance per character
                let step = if options & 0x2000 != 0 { 8 } else { 4 };
                let dx: Vec<f64> = if off_dx > 0 { (0..n).filter_map(|k| i32le(p, off_dx + k * step).map(f64::from)).collect() } else { Vec::new() };
                // ETO_OPAQUE: the rectangle filled with the background colour
                let opaque = (options & 2 != 0).then(|| [pi(12), pi(13), pi(14), pi(15)]);
                out.text(&mut s, (pi(7), pi(8)), &chars, &dx, opaque);
            }
            96 | 97 | 108 => out.skip("text (poly or small text out)"),
            // bitmaps
            81 | 80 => {
                // STRETCHDIBITS / SETDIBITSTODEVICE: bounds, dest x, y; source x, y, w, h; BITMAPINFO and bits (offsets from the
                // record); STRETCHDIBITS: usage, raster op, dest w, h
                let (bmi, bits) = (pu(10) as usize, pu(12) as usize);
                let rop = if kind == 81 { pu(15) } else { 0x00CC_0020 };
                let (dw, dh) = if kind == 81 { (pi(16), pi(17)) } else { (pi(8), pi(9)) };
                let rec = &b[at..at + size_r];
                let picture = rec.get(bmi..bmi + pu(11) as usize).zip(rec.get(bits..bits + pu(13) as usize));
                if rop != 0x00CC_0020 {
                    out.skip("bitmap raster operation");
                } else if let Some((bmi, bits)) = picture {
                    let src = (kind == 81).then(|| [pi(6) as i64, pi(7) as i64, pi(8) as i64, pi(9) as i64]);
                    out.bitmap(&s, content::dib(bmi, bits, src, false), (pi(4), pi(5), dw, dh), 1.0);
                }
            }
            76 | 77 | 114 => {
                // BITBLT / STRETCHBLT / ALPHABLEND: dest x, y, w, h; raster op (ALPHABLEND: blend function); source x, y; …
                let (bmi, cb_bmi, bits, cb_bits) = (pu(19) as usize, pu(20) as usize, pu(21) as usize, pu(22) as usize);
                let rop = pu(8);
                let rec = &b[at..at + size_r];
                let (x, y, w, h) = (pi(4), pi(5), pi(6), pi(7));
                if cb_bmi == 0 || cb_bits == 0 {
                    // no source: a pattern fill of the rectangle (PATCOPY with the brush, BLACKNESS, WHITENESS)
                    let fill = match rop { 0x00F0_0021 => s.brush.color, 0x0000_0042 => Some([0, 0, 0]), 0x00FF_0062 => Some([255, 255, 255]), _ => None };
                    match fill {
                        Some(c) => {
                            let d = out.poly(&s, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], true);
                            out.shapes.push_str(&format!("<path d=\"{}\" fill=\"{}\" stroke=\"none\"/>\n", d.trim(), hex(c)));
                        }
                        None => out.skip("bitmap raster operation"),
                    }
                } else if kind != 114 && rop != 0x00CC_0020 {
                    out.skip("bitmap raster operation");
                } else if let Some((bmi, bits)) = rec.get(bmi..bmi + cb_bmi).zip(rec.get(bits..bits + cb_bits)) {
                    let (sw, sh) = if kind == 76 { (w, h) } else { (pi(23), pi(24)) };
                    // ALPHABLEND: AC_SRC_ALPHA (per-pixel alpha) and the constant alpha
                    let (per_pixel, constant) = if kind == 114 { ((rop >> 24) & 1 == 1, f64::from((rop >> 16) & 0xFF) / 255.0) } else { (false, 1.0) };
                    out.bitmap(&s, content::dib(bmi, bits, Some([pi(9) as i64, pi(10) as i64, sw as i64, sh as i64]), per_pixel), (x, y, w, h), constant);
                }
            }
            115 | 78 | 79 => out.skip("bitmap (transparent, mask or parallelogram blit)"),
            67 | 75 | 30 | 29 | 26 => out.skip("clipping"),
            _ => {}
        }
        at += size_r;
    }
    let skipped = out.skipped.into_iter().collect();
    Ok(Converted { svg: svg_text(&out.shapes, view, size), size, skipped })
}
