//! Writes the runtime conversion of a CMYK profile at the 8-bit grid points
//! round(i·255/16) (CMYK, 17⁴) and round(i·255/32) (sRGB round trip, 33³), so it can be
//! compared with another engine. Writes nothing derived from the profile anywhere else.
//!
//! Usage: cargo run -p folio-color --release --example dump_grids -- <cmyk.icc> <out dir> [rgb.icc]
//!
//! Output (u8 RGB triples, C-order, last index fastest):
//!   cmyk_exact.bin / rt_exact.bin  the conversion evaluated at each point
//!   cmyk_table.bin / rt_table.bin  what the renderer gives for a colour there
//!   rt_exact_q8.bin                the round trip with the CMYK rounded to 8 bits between
//!   rt_ink.bin                     that 8-bit CMYK (4 bytes per point)
use folio_color::icc::{self, Bpc, Profile, Srgb};
use folio_color::{BlendSpace, Colors};
use std::time::Instant;

fn grid(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i as f64 * 255.0 / (n - 1) as f64).round() as u8).collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(&a[0]).expect("profile");
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();
    let rgb_bytes = a.get(2).map(|p| std::fs::read(p).expect("rgb profile"));

    let t = Instant::now();
    let colors = Colors::from_profiles(Some(&bytes), rgb_bytes.as_deref()).expect("usable profile");
    println!("tables built in {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);

    let p = Profile::parse(&bytes).unwrap();
    let srgb = Srgb::new();
    let src_bp = icc::source_black_point(&p);
    let dst_bp = icc::destination_black_point(&p);
    println!("black point: source L* {:.3}, destination L* {:.3}", icc::xyz_to_lab(src_bp)[0], icc::xyz_to_lab(dst_bp)[0]);
    let to_display = Bpc::new(src_bp, [0.0; 3]);
    let to_cmyk = Bpc::new([0.0; 3], dst_bp);

    let g4 = grid(17);
    let (mut exact, mut table) = (Vec::new(), Vec::new());
    for &c in &g4 {
        for &m in &g4 {
            for &y in &g4 {
                for &k in &g4 {
                    let v = [c, m, y, k].map(|x| x as f64 / 255.0);
                    exact.extend(srgb.from_xyz8(to_display.apply(p.to_xyz(&v))));
                    table.extend(colors.cmyk8_to_srgb(c, m, y, k));
                }
            }
        }
    }
    std::fs::write(out.join("cmyk_exact.bin"), &exact).unwrap();
    std::fs::write(out.join("cmyk_table.bin"), &table).unwrap();

    let g3 = grid(33);
    let (mut exact, mut table, mut q8, mut inks) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut px = Vec::new();
    for &r in &g3 {
        for &g in &g3 {
            for &b in &g3 {
                let v = [r, g, b].map(|x| x as f64 / 255.0);
                let ink = p.from_xyz(to_cmyk.apply(srgb.to_xyz(v))).unwrap();
                exact.extend(srgb.from_xyz8(to_display.apply(p.to_xyz(&ink))));
                // as an 8-bit CMYK image would carry it between two conversions
                let ink8 = ink.map(|v| (v * 255.0).round() / 255.0);
                inks.extend(ink8.map(|v| (v * 255.0) as u8));
                q8.extend(srgb.from_xyz8(to_display.apply(p.to_xyz(&ink8))));
                table.extend(colors.srgb_through_cmyk([r, g, b]));
                px.extend([r, g, b, 255]);
            }
        }
    }
    // The pixel path (sampled at every fifth level) against the colour path (exact).
    colors.rgba_to_display(&mut px, BlendSpace::Cmyk);
    if rgb_bytes.is_none() {
        let d: Vec<i32> = px.chunks_exact(4).zip(table.chunks_exact(3)).flat_map(|(p, t)| (0..3).map(move |k| (p[k] as i32 - t[k] as i32).abs())).collect();
        println!("pixel vs colour path: mean {:.4}, max {}", d.iter().sum::<i32>() as f64 / d.len() as f64, d.iter().max().unwrap());
    }
    std::fs::write(out.join("rt_exact.bin"), &exact).unwrap();
    std::fs::write(out.join("rt_table.bin"), &table).unwrap();
    std::fs::write(out.join("rt_exact_q8.bin"), &q8).unwrap();
    std::fs::write(out.join("rt_ink.bin"), &inks).unwrap();
    println!("written to {}", out.display());
}
