use folio_idml::datamerge::parse;

fn utf16(text: &str, le: bool) -> Vec<u8> {
    let mut bytes = if le { vec![255, 254] } else { vec![254, 255] };
    for word in text.encode_utf16() { bytes.extend(if le { word.to_le_bytes() } else { word.to_be_bytes() }); }
    bytes
}

#[test]
fn measured_encodings_and_delimiter_detection() {
    for le in [false, true] {
        let source = parse(&utf16("Name\tCity\r\n가나\t서울\r\n", le)).unwrap();
        assert_eq!(source.records, vec![vec!["가나", "서울"]]);
        assert_eq!(source.delimiter, "\t");
    }
    assert_eq!(parse(b"Name,City\r\nAlice,Seoul\r\n").unwrap().records[0], ["Alice", "Seoul"]);
    assert!(parse(b"\xef\xbb\xbfName,City\r\nAlice,Seoul").is_err());
    assert!(parse("Name\n가나".as_bytes()).is_err());
    assert_eq!(parse(b"Name,City\tOther\nA,B\tC").unwrap().records[0], ["A", "B\tC"]);
}

#[test]
fn measured_names_and_types() {
    let source = parse(b" Name \tName\tName1\tName\t@Name\t@@Photo\t#Code\t@ Photo \nA\tB\tC\tD\t\t\t\t").unwrap();
    assert_eq!(source.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["Name", "Name1", "Name11", "Name2", "Name", "@Photo", "Code", " Photo"]);
    assert_eq!(source.fields[4].kind, "image"); assert_eq!(source.fields[6].kind, "qrcode");
    assert!(parse(b"Name\t\tCity\nA\tB\tC").is_err());
    assert!(parse(b"Name\t@\nA\tB").is_err());
    assert!(parse(b"Name\n").is_err());
    assert_eq!(parse(b"\nName\nAlice").unwrap().fields[0].name, "record");
}

#[test]
fn measured_physical_lines_quotes_trimming_and_missing_columns() {
    let source = parse(b"Name,Note\r\nA,\"He said \"\"yes\"\"\"\r\nB,\"line1\r\nline2\"\r\nC,ab\"cd\r\nD,\"x\"tail\r\n").unwrap();
    assert_eq!(source.records, vec![vec!["A", "He said \"yes\""], vec!["B", "line1"],
        vec!["line2\"", ""], vec!["C", "ab\"cd"], vec!["D", "xtail"]]);
    let source = parse(b"Name,City\n  Alice , Seoul \n\n\" Bob \" ,\" Busan \"\nC,X,extra\n,\n").unwrap();
    assert_eq!(source.records, vec![vec!["Alice", "Seoul"], vec!["Bob", "Busan"], vec!["C", "X"], vec!["", ""]]);
}
#[test]
fn indesign_source_corpus() {
    let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/datamerge-source-golden.json");
    let cases:serde_json::Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for c in cases.as_array().unwrap() {
        let bytes:Vec<u8>=c["bytes"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
        let result=folio_idml::datamerge::parse(&bytes);
        if c["reject"]==true {assert!(result.is_err(),"{}",c["name"]);continue;}
        let source=result.unwrap_or_else(|e|panic!("{}: {e}",c["name"]));
        assert_eq!(serde_json::to_value(&source.fields).unwrap(),c["fields"],"{}",c["name"]);
        let text:Vec<Vec<&str>>=source.records.iter().map(|row|row.iter().zip(&source.fields).filter(|(_,f)|f.kind=="text").map(|(v,_)|v.as_str()).collect()).collect();
        if c.get("textRecords").is_some() { assert_eq!(serde_json::to_value(text).unwrap(),c["textRecords"],"{}",c["name"]); }
    }
}


/// The Data Source Import Options dialog against InDesign 2025 (`oracle/fixtures/datamerge-import`, built by
/// `oracle/datamerge-import-golden.py` from the probes `datamerge-c4-e..l`): for every case the text of every record the merged document held.
#[test]
fn import_options_match_indesign() {
    use folio_idml::datamerge::{choice, detect_encoding, parse_with};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/datamerge-import");
    let golden: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let mut checked = 0;
    for c in golden["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let bytes = std::fs::read(dir.join(c["file"].as_str().unwrap())).unwrap();
        let chosen = choice(c["delimiter"].as_str().unwrap(), c["encoding"].as_str().unwrap(), c["platform"].as_str().unwrap(), c["preserveSpaces"].as_bool().unwrap()).unwrap();
        let result = parse_with(&bytes, &chosen);
        if c["reject"] == true {
            assert!(result.is_err(), "{name}: InDesign produced no fields, Folio read {:?}", result.map(|s| s.records));
            continue;
        }
        let source = result.unwrap_or_else(|e| panic!("{name}: {e}"));
        let fields: Vec<&str> = source.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(serde_json::json!(fields), c["fields"], "{name}: fields");
        let records: Vec<String> = source.records.iter().map(|r| r.join("|")).collect();
        assert_eq!(serde_json::json!(records), c["records"], "{name}: records");
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} cases checked");
    // what the dialog shows for a file before anything is chosen
    for (file, expected) in golden["detected"].as_object().unwrap() {
        let bytes = std::fs::read(dir.join(file)).unwrap();
        let source = parse_with(&bytes, &choice("", "", "", false).unwrap()).unwrap();
        let want: Vec<&str> = expected.as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(source.options.delimiter.name(), want[0], "{file}: delimiter shown");
        assert_eq!(detect_encoding(&bytes).name(), want[1], "{file}: encoding shown");
        assert_eq!(source.options.encoding.name(), want[1], "{file}: encoding applied");
    }
}

#[test]
fn file_type_is_written_for_ascii_files_only() {
    use folio_idml::datamerge::{choice, parse_with};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/datamerge-import");
    let kind = |file: &str| parse_with(&std::fs::read(dir.join(file)).unwrap(), &choice("", "", "", false).unwrap()).unwrap().file_type();
    assert_eq!(kind("ascii.csv"), Some("CommaSeparated"));
    assert_eq!(kind("tab.txt"), Some("TabDelimited"));
    assert_eq!(kind("semicolon.csv"), Some("SemicolonSeparated"));
    assert_eq!(kind("u16le_tab.txt"), None);
    assert_eq!(kind("sjis.csv"), None);
}
