//! 16-bit CMYK and CIELab TIFF pictures are placed (build-a): the 8-bit drawing and print paths take the high byte of each sample.
use folio_edit::GraphicSource;

/// A baseline uncompressed little-endian TIFF of one pixel row.
fn tiff(w: u32, photometric: u16, spp: u16, bits: u16, samples: &[u8]) -> Vec<u8> {
    let entries: Vec<(u16, u16, u32, u32)> = vec![
        (256, 4, 1, w), (257, 4, 1, 1), (258, 3, spp as u32, 0), (259, 3, 1, 1), (262, 3, 1, photometric as u32),
        (273, 4, 1, 0), (277, 3, 1, spp as u32), (278, 4, 1, 1), (279, 4, 1, samples.len() as u32), (284, 3, 1, 1),
    ];
    let ifd_len = 2 + entries.len() as u32 * 12 + 4;
    let bits_at = 8 + ifd_len;
    let data_at = bits_at + spp as u32 * 2;
    let mut out = b"II*\0".to_vec();
    out.extend(8u32.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        let value = match tag { 258 => bits_at, 273 => data_at, _ => value };
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(value.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes());
    for _ in 0..spp {
        out.extend(bits.to_le_bytes());
    }
    out.extend_from_slice(samples);
    out
}

#[test]
fn sixteen_bit_cmyk_and_lab_tiffs_are_placed() {
    let dir = std::env::temp_dir().join(format!("folio-place-deep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, photometric, spp, space) in [("cmyk16.tif", 5u16, 4u16, "CMYK"), ("lab16.tif", 8, 3, "LAB")] {
        let samples: Vec<u8> = (0..spp as usize * 2).flat_map(|i| ((i as u16) << 12).to_le_bytes()).collect();
        let path = dir.join(name);
        std::fs::write(&path, tiff(2, photometric, spp, 16, &samples)).unwrap();
        let source = GraphicSource::read(&path, format!("file:{name}")).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((source.format.as_str(), source.space.as_str()), ("TIFF", space), "{name}");
    }
    // 16-bit CMYK and Lab are said in the refusal of a depth that is still not placed
    let path = dir.join("rgb32.tif");
    std::fs::write(&path, tiff(1, 2, 3, 32, &[0; 12])).unwrap();
    let err = GraphicSource::read(&path, "file:rgb32.tif".into()).unwrap_err();
    assert!(err.contains("16비트 CMYK"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_eps_is_placed_as_an_eps_graphic_with_its_link() {
    let dir = std::env::temp_dir().join(format!("folio-place-eps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let eps = dir.join("ad.eps");
    std::fs::write(&eps, b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 144 96\n%%HiResBoundingBox: 0 0 144 96\n%%DocumentProcessColors: Cyan Magenta Yellow Black\n0 0 1 0 setcmykcolor 0 0 144 96 rectfill\n%%EOF\n").unwrap();
    let source = GraphicSource::read(&eps, "file:ad.eps".into()).unwrap();
    assert_eq!((source.format.as_str(), source.space.as_str(), source.size), ("EPS", "CMYK", [144.0, 96.0]));
    assert_eq!(source.extension(), "eps");
    let f = std::fs::File::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/variables.idml")).unwrap();
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut parts = std::collections::BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() { continue; }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        parts.insert(e.name().unwrap().into_owned(), s);
    }
    let mut ed = folio_edit::Editor::new(parts);
    let page = ed.parts().iter().filter(|(k, _)| k.starts_with("Spreads/")).find_map(|(_, t)| {
        let ix = folio_edit::xml::Index::scan(t).ok()?;
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Page").and_then(|i| ix.attr(t, i, "Self").map(str::to_owned))
    }).unwrap();
    ed.apply("place", &[folio_edit::Command::PlaceGraphic { source, id: None, page: Some(page), point: Some([20.0, 20.0]), rect: None, layer: None }]).unwrap();
    let all = ed.parts().values().cloned().collect::<Vec<_>>().join("\n");
    assert!(all.contains("<EPS Self=") && all.contains("LinkResourceFormat=\"$ID/EPS\"") && all.contains("Right=\"144\" Bottom=\"96\""), "an EPS element");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_placeable_wmf_is_placed_as_a_wmf_graphic() {
    let dir = std::env::temp_dir().join(format!("folio-place-wmf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // placeable header: 0 0 1440 720 at 1440 per inch (72 x 36 pt), a metafile header, a polygon, the end
    let mut b = vec![0xD7, 0xCD, 0xC6, 0x9A, 0, 0];
    for v in [0i16, 0, 1440, 720] {
        b.extend(v.to_le_bytes());
    }
    b.extend(1440u16.to_le_bytes());
    b.extend([0; 6]);
    b.extend([1, 0, 9, 0, 0, 3, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    for words in [vec![10u16, 0, 0x0324, 3, 0, 0, 1440, 0, 720, 720], vec![3, 0, 0]] {
        for w in words {
            b.extend(w.to_le_bytes());
        }
    }
    let wmf = dir.join("logo.wmf");
    std::fs::write(&wmf, &b).unwrap();
    let source = GraphicSource::read(&wmf, "file:logo.wmf".into()).unwrap();
    assert_eq!((source.format.as_str(), source.size, source.extension()), ("WMF", [72.0, 36.0], "wmf"));
    let _ = std::fs::remove_dir_all(&dir);
}
