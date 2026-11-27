//! Header probes on hand-made files: JPEG (JFIF density, Adobe CMYK, ICC chunks), PNG
//! (pHYs, compressed iCCP), PSD (resolution resource), EPS (DSC colours and fonts), SVG
//! units, PDF (inherited boxes, spot colours, repair of a broken cross-reference table).

use folio_links::{ColorSpace, ImageFormat, probe};
use std::io::Write;

fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("folio-links-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn seg(marker: u8, data: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, marker];
    v.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(data);
    v
}

#[test]
fn jpeg_cmyk_with_icc_and_density() {
    let mut b = vec![0xFF, 0xD8];
    b.extend(seg(0xE0, &[b'J', b'F', b'I', b'F', 0, 1, 2, 1, 0, 200, 0, 150, 0, 0]));
    // ICC profile in two chunks, stored out of order
    let mut c1 = b"ICC_PROFILE\0".to_vec();
    c1.extend_from_slice(&[1, 2]);
    c1.extend_from_slice(b"abc");
    let mut c2 = b"ICC_PROFILE\0".to_vec();
    c2.extend_from_slice(&[2, 2]);
    c2.extend_from_slice(b"def");
    b.extend(seg(0xE2, &c2));
    b.extend(seg(0xE2, &c1));
    b.extend(seg(0xEE, &[b'A', b'd', b'o', b'b', b'e', 0, 100, 0, 0, 0, 0, 2]));
    b.extend(seg(0xC0, &[8, 0, 40, 0, 60, 4, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0, 4, 0x11, 0]));
    b.extend(seg(0xDA, &[0; 8]));
    let i = probe(&tmp("a.jpg", &b)).unwrap();
    assert_eq!((i.format, i.width, i.height, i.color_space), (ImageFormat::Jpeg, 60, 40, ColorSpace::Cmyk));
    assert_eq!(i.ppi, Some((200.0, 150.0)));
    assert_eq!(i.icc.as_deref(), Some(&b"abcdef"[..]));
}

fn chunk(t: &[u8], data: &[u8]) -> Vec<u8> {
    let mut v = (data.len() as u32).to_be_bytes().to_vec();
    v.extend_from_slice(t);
    v.extend_from_slice(data);
    v.extend_from_slice(&[0, 0, 0, 0]); // CRC is not checked
    v
}

#[test]
fn png_phys_and_iccp() {
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = 30u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&20u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    b.extend(chunk(b"IHDR", &ihdr));
    let mut phys = 11811u32.to_be_bytes().to_vec(); // 300 ppi in pixels per metre
    phys.extend_from_slice(&11811u32.to_be_bytes());
    phys.push(1);
    b.extend(chunk(b"pHYs", &phys));
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(b"PROFILE-BYTES").unwrap();
    let mut iccp = b"sRGB\0\0".to_vec();
    iccp.extend(z.finish().unwrap());
    b.extend(chunk(b"iCCP", &iccp));
    b.extend(chunk(b"IDAT", &[0]));
    let i = probe(&tmp("a.png", &b)).unwrap();
    assert_eq!((i.width, i.height, i.color_space, i.alpha), (30, 20, ColorSpace::Rgb, true));
    assert_eq!(i.ppi.map(|(x, _)| x.round()), Some(300.0));
    assert_eq!(i.icc.as_deref(), Some(&b"PROFILE-BYTES"[..]));
}

#[test]
fn psd_cmyk_resolution() {
    let mut b = b"8BPS".to_vec();
    b.extend_from_slice(&1u16.to_be_bytes());
    b.extend_from_slice(&[0; 6]);
    b.extend_from_slice(&5u16.to_be_bytes()); // channels: CMYK + alpha
    b.extend_from_slice(&100u32.to_be_bytes()); // height
    b.extend_from_slice(&200u32.to_be_bytes()); // width
    b.extend_from_slice(&8u16.to_be_bytes());
    b.extend_from_slice(&4u16.to_be_bytes()); // CMYK
    b.extend_from_slice(&0u32.to_be_bytes()); // colour mode data
    let mut res = b"8BIM".to_vec();
    res.extend_from_slice(&0x03EDu16.to_be_bytes());
    res.extend_from_slice(&[0, 0]); // empty pascal name, padded
    res.extend_from_slice(&16u32.to_be_bytes());
    res.extend_from_slice(&(350u32 << 16).to_be_bytes());
    res.extend_from_slice(&[0, 1, 0, 1]);
    res.extend_from_slice(&(350u32 << 16).to_be_bytes());
    res.extend_from_slice(&[0, 1, 0, 1]);
    b.extend_from_slice(&(res.len() as u32).to_be_bytes());
    b.extend(res);
    let i = probe(&tmp("a.psd", &b)).unwrap();
    assert_eq!((i.format, i.width, i.height, i.color_space, i.alpha), (ImageFormat::Psd, 200, 100, ColorSpace::Cmyk, true));
    assert_eq!(i.ppi, Some((350.0, 350.0)));
}

#[test]
fn eps_dsc_colours_and_fonts() {
    let eps = [
        "%!PS-Adobe-3.0 EPSF-3.0",
        "%%BoundingBox: 0 0 144 72",
        "%%DocumentProcessColors: Cyan Magenta Black",
        "%%DocumentCustomColors: (PANTONE 280 C)",
        "%%+ (Gold)",
        "%%DocumentNeededResources: font Helvetica",
        "%%+ font Myriad-Roman",
        "%%DocumentSuppliedResources: font Myriad-Roman",
        "%%EndComments",
        "",
    ]
    .join("\n");
    let i = probe(&tmp("a.eps", eps.as_bytes())).unwrap();
    assert_eq!((i.format, i.width, i.height, i.vector), (ImageFormat::Eps, 144, 72, true));
    let c = i.vector_colors.unwrap();
    assert_eq!(c.spaces, [ColorSpace::Cmyk]);
    assert_eq!(c.spots, ["Gold", "PANTONE 280 C"]);
    assert!(i.fonts.iter().any(|f| f.name == "Helvetica" && !f.embedded), "{:?}", i.fonts);
    assert!(i.fonts.iter().any(|f| f.name == "Myriad-Roman" && f.embedded));
}

#[test]
fn svg_units() {
    let i = probe(&tmp("a.svg", br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="2in" height="50.8mm"></svg>"#)).unwrap();
    assert_eq!((i.width, i.height), (144, 144));
    let i = probe(&tmp("b.svg", br#"<svg viewBox="0 0 400 200"/>"#)).unwrap();
    assert_eq!((i.width, i.height), (300, 150));
}

/// Two pages with an inherited MediaBox, a CropBox, a Separation colour space and RGB
/// operators; written with a correct cross-reference table, or with wrong offsets that
/// force the repair scan.
fn pdf(broken_xref: bool) -> Vec<u8> {
    let content = "q 1 0 0 rg 0 0 10 10 re f /CS0 cs 1 scn Q";
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 600 800] >>".into(),
        "<< /Type /Page /Parent 2 0 R /CropBox [10 10 590 790] /Contents 5 0 R /Resources << /ColorSpace << /CS0 [/Separation /PANTONE#20280#20C /DeviceCMYK 6 0 R] >> >> >>".into(),
        "<< /Type /Page /Parent 2 0 R /Rotate 90 /TrimBox [20 20 580 780] >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [1 0.7 0 0.2] /N 1 >>".into(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        let o = if broken_xref { o + 7 } else { o };
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    let start = if broken_xref { xref + 3 } else { xref };
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

#[test]
fn pdf_boxes_spots_and_repair() {
    for broken in [false, true] {
        let i = probe(&tmp(&format!("a{broken}.pdf"), &pdf(broken))).unwrap_or_else(|e| panic!("broken={broken}: {e}"));
        assert_eq!(i.pages, Some(2), "broken={broken}");
        assert_eq!(i.page_boxes[0].media, [0.0, 0.0, 600.0, 800.0]);
        assert_eq!(i.page_boxes[0].crop, Some([10.0, 10.0, 590.0, 790.0]));
        assert_eq!((i.page_boxes[1].rotate, i.page_boxes[1].trim), (90, Some([20.0, 20.0, 580.0, 780.0])));
        assert_eq!((i.width, i.height), (580, 780));
        let c = i.vector_colors.unwrap();
        assert_eq!(c.spots, ["PANTONE 280 C"]);
        assert!(c.spaces.contains(&ColorSpace::Rgb) && c.content_scanned, "{c:?}");
    }
}

#[test]
fn pdf_object_streams_keep_fonts_and_output_intent() {
    use lopdf::{Document, Object, dictionary};

    let mut document = Document::load_mem(&pdf(false)).unwrap();
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let page = document.get_object_mut((3, 0)).unwrap().as_dict_mut().unwrap();
    page.get_mut(b"Resources").unwrap().as_dict_mut().unwrap()
        .set("Font", dictionary! { "F1" => font });
    let intent = document.add_object(dictionary! {
        "Type" => "OutputIntent", "S" => "GTS_PDFX",
        "OutputConditionIdentifier" => Object::string_literal("Proof"),
    });
    document.catalog_mut().unwrap().set("OutputIntents", vec![Object::Reference(intent)]);
    let mut bytes = Vec::new();
    document.save_modern(&mut bytes).unwrap();
    assert!(bytes.windows(7).any(|w| w == b"/ObjStm"));

    let info = probe(&tmp("object-stream.pdf", &bytes)).unwrap();
    assert_eq!(info.pages, Some(2));
    assert_eq!(info.fonts.len(), 1);
    let font = &info.fonts[0];
    assert_eq!((font.name.as_str(), font.kind.as_str(), font.embedded), ("Helvetica", "Type1", false));
    let colors = info.vector_colors.unwrap();
    assert_eq!(colors.output_intent.as_deref(), Some("Proof"));
    assert_eq!(colors.spots, ["PANTONE 280 C"]);
    assert!(colors.content_scanned && colors.spaces.contains(&ColorSpace::Rgb));
}
