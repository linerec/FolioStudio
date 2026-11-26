use super::*;
use std::io::{Cursor, Write};

#[test]
fn bugfind_oversized_word_grid_span_returns_error() {
    for value in ["1e100", "18446744073709551615", "1000000000"] {
        let xml = format!(r#"<document><body><tbl><tr><tc><tcPr><gridSpan val="{value}"/></tcPr><p/></tc></tr></tbl></body></document>"#);
        let bytes = word_file(&[("word/document.xml", &xml)]);
        let error = read_bytes(&bytes, &Options::default()).unwrap_err();
        assert!(error.contains("gridSpan"), "{error}");
        assert!(error.contains("word/document.xml"), "{error}");
    }
}

fn word_file(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn malformed_xml_returns_existing_errors() {
    for (input, error) in [
        ("", "XML 에 루트 요소가 없습니다"),
        ("<root>", "XML 이 끝나기 전에 문서가 끝났습니다"),
        ("<root>&missing;</root>", "알 수 없는 엔티티 &missing;"),
        ("<!DOCTYPE root><root/>", "DOCTYPE 선언은 받지 않습니다"),
    ] {
        assert_eq!(xmltree::parse(input).unwrap_err(), error, "{input:?}");
    }
    for input in ["</root>", "<root></other>", "<root value=\"", "<root>&#x110000;</root>"] {
        assert!(xmltree::parse(input).is_err(), "{input:?}");
    }
    let deep = format!("{}{}", "<r>".repeat(257), "</r>".repeat(257));
    assert_eq!(xmltree::parse(&deep).unwrap_err(), "XML 중첩이 너무 깊습니다");
}

#[test]
fn missing_word_parts_and_truncated_xml_return_errors() {
    let options = Options::default();
    let bytes = word_file(&[("word/styles.xml", "<styles/>")]);
    assert_eq!(read_bytes(&bytes, &options).unwrap_err(), "Word 파일이 아닙니다: word/document.xml 이 없습니다");
    let bytes = word_file(&[("word/document.xml", "<document><body>")]);
    assert_eq!(read_bytes(&bytes, &options).unwrap_err(), "XML 이 끝나기 전에 문서가 끝났습니다");
    for bytes in [b"PK\x03\x04".as_slice(), b"PK\x03\x04truncated"] {
        assert!(read_bytes(bytes, &options).unwrap_err().starts_with("Word 파일을 열 수 없습니다:"));
    }
}

#[test]
fn missing_styles_and_bad_numbers_keep_the_existing_fallback() {
    let bytes = word_file(&[("word/document.xml", r#"<document><body><p><pPr><pStyle val="missing"/><spacing before="bad"/></pPr><r><rPr><rStyle val="missing"/><sz val="bad"/></rPr><t>kept</t></r></p></body></document>"#)]);
    let text = read_bytes(&bytes, &Options::default()).unwrap();
    assert_eq!(text.story_text(), "kept");
    assert!(text.paragraph_styles.is_empty());
    assert!(text.character_styles.is_empty());
}

#[test]
fn empty_and_invalid_option_values_return_errors() {
    assert_eq!(read_bytes(b"", &Options::default()).unwrap_err(), "빈 파일은 가져올 수 없습니다");
    for value in [serde_json::json!({"spacesIntoTabsCount": -1}), serde_json::json!({"spacesIntoTabsCount": "bad"})] {
        assert!(Options::from_dom(&value).unwrap_err().starts_with("가져오기 옵션:"));
    }
}
