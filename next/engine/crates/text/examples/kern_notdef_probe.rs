//! outlines-c3: kerning of a letter followed by U+FFFC / glyph 0 (.notdef) in an installed font.
//! usage: kern_notdef_probe <font.ttf> <letters>   -> per letter: glyph of U+FFFC, kern adjustment (font units).
use rustybuzz::{Face, Feature, UnicodeBuffer};
use ttf_parser::Tag;

fn advances(face: &Face, text: &str, kern: bool) -> Vec<(u32, i32, i32)> {
    let mut buf = UnicodeBuffer::new();
    buf.push_str(text);
    let feats = [Feature::new(Tag::from_bytes(b"kern"), u32::from(kern), ..), Feature::new(Tag::from_bytes(b"liga"), 0, ..), Feature::new(Tag::from_bytes(b"clig"), 0, ..)];
    let out = rustybuzz::shape(face, &feats, buf);
    out.glyph_infos().iter().zip(out.glyph_positions()).map(|(i, p)| (i.glyph_id, p.x_advance, p.x_offset)).collect()
}

fn main() {
    let path = std::env::args().nth(1).expect("font path");
    let letters = std::env::args().nth(2).expect("letters");
    let data = std::fs::read(&path).expect("font");
    let index: u32 = std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let second = std::env::args().nth(4).and_then(|s| s.chars().next()).unwrap_or('\u{FFFC}');
    let face = Face::from_slice(&data, index).expect("face");
    println!("upem {} fffc glyph {:?}", face.units_per_em(), face.glyph_index('\u{FFFC}'));
    for c in letters.chars() {
        let s: String = [c, second].iter().collect();
        let on = advances(&face, &s, true);
        let off = advances(&face, &s, false);
        println!("{c:?}: kern-on {on:?} kern-off {off:?} d={}", on[0].1 - off[0].1);
    }
}
