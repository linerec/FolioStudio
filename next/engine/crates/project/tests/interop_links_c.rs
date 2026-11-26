#[path = "support/interop.rs"]
mod support;
#[path = "support/interop_roundtrip.rs"]
mod typed;
use serde_json::json;
use support::{attr, by_id, fixture, roundtrip};

const PART: &str = "Spreads/Spread_u10f.xml";

#[test]
fn stored_link_metadata_survives_export_without_claiming_filesystem_state() {
    for (attribute, field, values) in [
        ("Self", "id", [("link-a", json!("link-a")), ("link-b", json!("link-b"))]),
        ("LinkResourceURI", "uri", [("file:Links/a.png", json!("file:Links/a.png")), ("file:Links/b.png", json!("file:Links/b.png"))]),
        ("LinkResourceFormat", "format", [("$ID/PNG", json!("PNG")), ("$ID/JPEG", json!("JPEG"))]),
        ("StoredState", "stored_state", [("Normal", json!("Normal")), ("Embedded", json!("Embedded"))]),
        ("LinkResourceSize", "size", [("0~2a", json!(42)), ("1~0", json!(4294967296_u64))]),
        ("LinkImportStamp", "modified_unix", [("file 116444736000000000 42", json!(0.0)), ("file 116444736100000000 42", json!(10.0))]),
        ("LinkImportModificationTime", "modification_time", [("2025-01-01T00:00:00", json!("2025-01-01T00:00:00")), ("", json!(null))]),
        ("LinkResourceModified", "resource_modified", [("true", json!(true)), ("false", json!(false))]),
        ("LinkObjectModified", "object_modified", [("true", json!(true)), ("false", json!(false))]),
    ] {
        for (value, expected) in values {
            let mut parts = fixture("objopts-clip");
            attr(parts.get_mut(PART).unwrap(), "Link", 0, attribute, value);
            let (before, after, written) = roundtrip("objopts-clip", &parts);
            let b = by_id(&before, "u130");
            let a = by_id(&after, "u130");
            assert_eq!(b["link"][field], expected, "{attribute}");
            assert_eq!(a["link"][field], expected, "{attribute}");
            assert_eq!(b["link"], a["link"]);
            if field == "uri" {
                assert_eq!(b["link_uri"], expected);
                assert_eq!(a["link_uri"], expected);
            }
            assert_eq!(written[PART], parts[PART]);
        }
    }
}

#[test]
fn placed_pdf_options_survive_raw_idml_export() {
    for (page, crop, transparent) in [(2, "CropMedia", false), (7, "CropTrim", true)] {
        let mut parts = fixture("objopts-clip");
        let xml = parts.get_mut(PART).unwrap();
        let at = xml.find("</Image>").unwrap();
        xml.insert_str(at, &format!("<PDFAttribute PageNumber=\"{page}\" PDFCrop=\"{crop}\" TransparentBackground=\"{transparent}\" />"));
        *xml = xml.replacen("<Image ", "<PDF ", 1).replacen("</Image>", "</PDF>", 1);
        let (before, after, written) = roundtrip("objopts-clip", &parts);
        let expected = json!({"page":page,"crop":crop,"transparent_background":transparent});
        assert_eq!(by_id(&before, "u130")["pdf"], expected);
        assert_eq!(by_id(&after, "u130")["pdf"], expected);
        assert_eq!(written[PART], parts[PART]);
    }
}

#[test]
fn embedded_bytes_survive_raw_idml_export() {
    // The reader accepts recognized image/PDF signatures; other original bytes remain XML only.
    for (encoded, expected) in [("JVBERg==", json!([37, 80, 68, 70])), ("iVBORw==", json!([137, 80, 78, 71])), ("AQID", json!(null))] {
        typed::check("embedded contents", &typed::oracle().join("fixtures/objopts-clip.idml"), PART,
            |xml| {
                let mut xml = xml.to_owned();
                let at = xml.find("<Image Self=\"u130\"").unwrap();
                let at = at + xml[at..].find("<Properties>").unwrap() + "<Properties>".len();
                xml.insert_str(at, &format!("<Contents>{encoded}</Contents>"));
                xml
            },
            |doc| {
                let item = doc.spreads.iter().flat_map(|s| &s.items).find_map(|item| match &item.kind {
                    folio_model::ItemKind::Rectangle { content: Some(g) } if g.id == "u130" => Some(g),
                    _ => None,
                }).unwrap();
                json!(item.embedded_data.as_ref().map(|d| d.as_slice()))
            }, expected);
    }
}
