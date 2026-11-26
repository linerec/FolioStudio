fn main() {
    use folio_color::{icc::Intent, profiles::ProfileIndex};
    let index = ProfileIndex::scan_installed();
    let rgb = std::fs::read(index.find("sRGB IEC61966-2.1").expect("sRGB profile")).unwrap();
    let cmyk = std::fs::read(index.find("U.S. Web Coated (SWOP) v2").expect("output profile")).unwrap();
    for intent in [Intent::Perceptual, Intent::Relative, Intent::Saturation, Intent::Absolute] {
        let link = folio_color::rgb_link_with_intent(&rgb, &cmyk, intent).unwrap();
        println!("{intent:?}: {:?}", [[30,110,200], [220,40,40], [240,180,0], [255,255,255]].map(|rgb| link.eval8(rgb)));
    }
}
