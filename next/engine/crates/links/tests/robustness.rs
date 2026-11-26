use folio_links::{ImageFormat, probe};
use std::path::PathBuf;

fn fixture(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../artifacts/robust-a-c1-20261023/links-inputs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn empty_and_truncated_headers_return_errors() {
    for (name, bytes) in [
        ("empty.bin", &b""[..]),
        ("truncated.jpg", &b"\xff\xd8\xff"[..]),
        ("truncated.png", &b"\x89PNG\r\n\x1a\n"[..]),
        ("truncated.psd", &b"8BPS"[..]),
        ("truncated.tif", &b"II*\0"[..]),
        ("truncated.pdf", &b"%PDF-1.7\n"[..]),
    ] {
        assert!(probe(&fixture(name, bytes)).is_err(), "{name}");
    }
}

#[test]
fn missing_file_and_pdf_root_are_errors() {
    let missing = fixture("present.bin", b"").with_file_name("absent.bin");
    assert!(probe(&missing).is_err());
    let pdf = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 999 0 R >>\nendobj\ntrailer\n<< /Root 999 0 R >>\n%%EOF";
    assert!(probe(&fixture("missing-root.pdf", pdf)).is_err());
}

#[test]
fn truncated_gif_keeps_existing_zero_dimensions() {
    let info = probe(&fixture("truncated.gif", b"GIF89a")).unwrap();
    assert_eq!((info.format, info.width, info.height), (ImageFormat::Gif, 0, 0));
}
