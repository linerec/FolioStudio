//! The expected texts are what InDesign 2025 made of the same files (artifacts/placetext-c1-20261023/measurement).
use super::*;

fn text(bytes: &[u8], options: &Options) -> String {
    read_bytes(bytes, options).unwrap().story_text()
}

fn utf8() -> Options {
    Options { character_set: CharacterSet::Utf8, ..Options::default() }
}

#[test]
fn byte_order_mark_decides_before_the_character_set() {
    let want = "Hello \u{c548}\u{b155} caf\u{e9}\r\u{201c}x\u{201d}";
    let body = "Hello \u{c548}\u{b155} caf\u{e9}\r\n\"x\"";
    let mut u8bom = vec![0xEF, 0xBB, 0xBF];
    u8bom.extend(body.as_bytes());
    let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(body.encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain(body.encode_utf16().flat_map(u16::to_be_bytes)).collect();
    for set in [CharacterSet::Ansi, CharacterSet::Utf8, CharacterSet::Ksc5601] {
        let o = Options { character_set: set, ..Options::default() };
        assert_eq!(text(&u8bom, &o), want, "{set:?}");
        assert_eq!(text(&le, &o), want, "{set:?}");
        assert_eq!(text(&be, &o), want, "{set:?}");
    }
}

#[test]
fn ansi_is_windows_1252_and_utf8_without_mark_is_not_detected() {
    // utf8 bytes read as ANSI: mojibake (placetext-txt-ansi), the byte 0x9D (undefined) is a space
    assert_eq!(text("caf\u{e9} \u{201d}".as_bytes(), &Options::default()), "caf\u{c3}\u{a9} \u{e2}\u{20ac} ");
    assert_eq!(text(&[b'x', 0x81, 0x8D, 0x8F, 0x90, 0x9D, 0x80, 0xE9, b'y'], &Options::default()), "x     \u{20ac}\u{e9}y");
    assert_eq!(text("caf\u{e9} \u{201d}".as_bytes(), &utf8()), "caf\u{e9} \u{201d}");
}

#[test]
fn utf16_without_a_mark_is_little_endian() {
    let bytes: Vec<u8> = "ab\r\ncd".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(text(&bytes, &Options { character_set: CharacterSet::Utf16, ..Options::default() }), "ab\rcd");
}

#[test]
fn korean_and_japanese_code_pages_are_decoded_and_placed() {
    // the text keeps the frame's font (InDesign also names a font of the script: Adobe Myungjo Std for KSC5601)
    let (kr, _, _) = encoding_rs::EUC_KR.encode("Hello \u{c548}\u{b155}");
    let ksc = Options { character_set: CharacterSet::Ksc5601, ..Options::default() };
    assert_eq!(text(&kr, &ksc), "Hello \u{c548}\u{b155}");
    assert_eq!(crate::txt::decode(&kr, CharacterSet::Ksc5601).as_deref(), Some("Hello \u{c548}\u{b155}"));
    let (jp, _, _) = encoding_rs::SHIFT_JIS.encode("\u{65e5}\u{672c}\u{8a9e}");
    assert_eq!(crate::txt::decode(&jp, CharacterSet::ShiftJis90Ms).as_deref(), Some("\u{65e5}\u{672c}\u{8a9e}"));
    // a mark decides, the set is not used: no refusal
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend("\u{c548}".as_bytes());
    assert_eq!(text(&bom, &ksc), "\u{c548}");
}

#[test]
fn auto_finds_the_character_set_of_common_files() {
    use CharacterSet as C;
    let enc = |e: &'static encoding_rs::Encoding, s: &str| e.encode(s).0.into_owned();
    let ko = "\u{c624}\u{b298}\u{c740} \u{b0a0}\u{c528}\u{ac00} \u{b9d1}\u{ace0} \u{c0ac}\u{b78c}\u{b4e4}\u{c774} \u{acf5}\u{c6d0}\u{c5d0} \u{ac14}\u{b2e4}.";
    let ja = "\u{4eca}\u{65e5}\u{306f}\u{3044}\u{3044}\u{5929}\u{6c17}\u{3067}\u{3059}\u{306d}\u{3002}\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6587}\u{7ae0}\u{3067}\u{3059}\u{3002}";
    let zh_s = "\u{6211}\u{4eec}\u{662f}\u{4e2d}\u{56fd}\u{4eba}\u{ff0c}\u{8fd9}\u{662f}\u{6211}\u{7684}\u{5bb6}\u{3002}";
    let zh_t = "\u{6211}\u{5011}\u{662f}\u{4e2d}\u{570b}\u{4eba}\u{ff0c}\u{9019}\u{662f}\u{6211}\u{7684}\u{5bb6}\u{3002}";
    let ru = "\u{041f}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}, \u{044d}\u{0442}\u{043e} \u{0442}\u{0435}\u{043a}\u{0441}\u{0442} \u{043d}\u{0430} \u{0440}\u{0443}\u{0441}\u{0441}\u{043a}\u{043e}\u{043c} \u{044f}\u{0437}\u{044b}\u{043a}\u{0435}.";
    let el = "\u{039a}\u{03b1}\u{03bb}\u{03b7}\u{03bc}\u{03ad}\u{03c1}\u{03b1}, \u{03b1}\u{03c5}\u{03c4}\u{03cc} \u{03b5}\u{03af}\u{03bd}\u{03b1}\u{03b9} \u{03ad}\u{03bd}\u{03b1} \u{03ba}\u{03b5}\u{03af}\u{03bc}\u{03b5}\u{03bd}\u{03bf}.";
    assert_eq!(detect_character_set(&enc(encoding_rs::EUC_KR, ko)), C::Ksc5601);
    assert_eq!(detect_character_set(&enc(encoding_rs::SHIFT_JIS, ja)), C::ShiftJis90Ms);
    assert_eq!(detect_character_set(&enc(encoding_rs::GBK, zh_s)), C::Gb18030);
    assert_eq!(detect_character_set(&enc(encoding_rs::BIG5, zh_t)), C::ChineseBig5);
    assert_eq!(detect_character_set(&enc(encoding_rs::WINDOWS_1251, ru)), C::WindowsCyrillic);
    assert_eq!(detect_character_set(&enc(encoding_rs::KOI8_R, ru)), C::CyrillicKoi8r);
    assert_eq!(detect_character_set(&enc(encoding_rs::WINDOWS_1253, el)), C::WindowsGreek);
    assert_eq!(detect_character_set(ko.as_bytes()), C::Utf8);
    assert_eq!(detect_character_set(b"caf\xe9 au lait"), C::Ansi);
    assert_eq!(detect_character_set(b"plain"), C::Ansi);
    let le: Vec<u8> = "Hello".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(detect_character_set(&le), C::Utf16);
    // AUTO reads the file with the set it finds; the fixtures of placetext-c1 (CP949, Shift JIS)
    let auto = Options { character_set: C::Auto, ..Options::default() };
    assert_eq!(text(&enc(encoding_rs::EUC_KR, ko), &auto), ko);
    let links = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links");
    let cp949 = std::fs::read(links.join("placetext-cp949.txt")).unwrap();
    assert_eq!(detect_character_set(&cp949), C::Ksc5601);
    assert!(text(&cp949, &auto).contains('\u{c548}'), "{}", text(&cp949, &auto));
    assert_eq!(detect_character_set(&std::fs::read(links.join("placetext-sjis.txt")).unwrap()), C::ShiftJis90Ms);
    assert_eq!(Options::from_dom(&serde_json::json!({"characterSet": "AUTO"})).unwrap().character_set, C::Auto);
}

#[test]
fn every_kind_of_line_end_is_one_paragraph() {
    assert_eq!(text(b"a\r\nb\nc\rd\r\n\r\ne", &Options::default()), "a\rb\rc\rd\r\re");
    let s = read_bytes(b"a\r\nb\r\n", &Options::default()).unwrap();
    assert_eq!((s.paragraphs.len(), s.ends_with_return, s.paragraph_count()), (2, true, 3));
    let s = read_bytes(b"a\r\n\r\nb", &Options::default()).unwrap();
    assert_eq!((s.paragraphs.len(), s.ends_with_return), (3, false));
}

#[test]
fn nul_and_fs_are_the_invisible_marker_and_other_controls_refuse() {
    assert_eq!(text(b"ab\x00cd\x1cef\x7fgh\tij", &Options::default()), "ab\u{feff}cd\u{feff}ef\x7fgh\tij");
    for b in [1u8, 8, 0x0B, 0x0C, 0x0E, 0x1B, 0x1D, 0x1F] {
        assert!(read_bytes(&[b'a', b, b'b'], &Options::default()).is_err(), "{b:#x}");
    }
    assert!(read_bytes(b"", &Options::default()).is_err());
}

#[test]
fn removing_returns() {
    let wrapped = b"This is a hard\r\nwrapped line that\r\ncontinues here.\r\n\r\nNew paragraph\r\nwrapped too.\r\n";
    let lines = Options { strip_returns_between_lines: true, ..Options::default() };
    let paras = Options { strip_returns_between_paragraphs: true, ..Options::default() };
    let both = Options { strip_returns_between_lines: true, strip_returns_between_paragraphs: true, ..Options::default() };
    assert_eq!(text(wrapped, &lines), "This is a hard wrapped line that continues here.\r\rNew paragraph wrapped too. ");
    assert_eq!(text(wrapped, &paras), "This is a hard\rwrapped line that\rcontinues here.\rNew paragraph\rwrapped too.\r");
    assert_eq!(text(wrapped, &both), "This is a hard wrapped line that continues here.\rNew paragraph wrapped too. ");
}

#[test]
fn spaces_become_tabs_from_the_count() {
    let spaces = b"a  b   c    d\r\n    indented\r\n\tTabbed\r\nx     y\r\n";
    let three = Options { convert_spaces_into_tabs: true, spaces_into_tabs_count: 3, ..Options::default() };
    let two = Options { convert_spaces_into_tabs: true, spaces_into_tabs_count: 2, ..Options::default() };
    assert_eq!(text(spaces, &three), "a  b\tc\td\r\tindented\r\tTabbed\rx\ty\r");
    assert_eq!(text(spaces, &two), "a\tb\tc\td\r\tindented\r\tTabbed\rx\ty\r");
}

#[test]
fn straight_quotes_become_curly() {
    let q = "Tab \"straight\" and 'single' quotes. He said \"hi\" don't. 5'10\" and 12\"\r\n\"Start\" 'x' ('a\") .\"";
    assert_eq!(
        text(q.as_bytes(), &Options::default()),
        "Tab \u{201c}straight\u{201d} and \u{2018}single\u{2019} quotes. He said \u{201c}hi\u{201d} don\u{2019}t. 5\u{2019}10\u{201d} and 12\u{201d}\r\u{201c}Start\u{201d} \u{2018}x\u{2019} (\u{2018}a\u{201d}) .\u{201d}"
    );
    let off = Options { use_typographers_quotes: false, ..Options::default() };
    assert_eq!(text(b"say \"hi\"", &off), "say \"hi\"");
}

#[test]
fn options_come_in_as_a_script_names_them() {
    let o = Options::from_dom(&serde_json::json!({"characterSet": "TextImportCharacterSet.UTF8", "stripReturnsBetweenLines": true, "platform": "ImportPlatform.MACINTOSH", "spacesIntoTabsCount": 4})).unwrap();
    assert_eq!((o.character_set, o.strip_returns_between_lines, o.spaces_into_tabs_count), (CharacterSet::Utf8, true, 4));
    assert_eq!(Options::from_dom(&serde_json::Value::Null).unwrap(), Options::default());
    assert!(Options::from_dom(&serde_json::json!({"characterSet": "NOPE"})).is_err());
    assert!(Options::from_dom(&serde_json::json!({"nope": 1})).is_err());
    assert!(Options::from_dom(&serde_json::json!({"dictionary": "Korean"})).is_err());
    assert!(Options::from_dom(&serde_json::json!({"dictionary": "English: USA"})).is_ok());
}

#[test]
fn control_characters_by_the_length_of_the_file() {
    let long = |b: u8| [vec![b'x'; 60], vec![b], vec![b'y'; 60]].concat();
    // refused whatever the length
    for b in [0x01u8, 0x02, 0x04, 0x05, 0x08, 0x10, 0x12, 0x16, 0x18, 0x19, 0x1E] {
        assert!(read_bytes(&long(b), &Options::default()).is_err(), "{b:#x}");
    }
    // dropped from a text that is long enough, refused from a short one (7 bytes refused, 11 bytes not)
    for b in [0x03u8, 0x06, 0x07, 0x0B, 0x0C, 0x0E, 0x0F, 0x11, 0x13, 0x14, 0x15, 0x17, 0x1A, 0x1B, 0x1D, 0x1F] {
        assert_eq!(text(&long(b), &Options::default()), format!("{}{}", "x".repeat(60), "y".repeat(60)), "{b:#x}");
        assert!(read_bytes(&[b'x', b'x', b'x', b, b'y', b'y', b'y'], &Options::default()).is_err(), "{b:#x}");
        assert_eq!(text(&[b'x'; 5].iter().copied().chain([b]).chain([b'y'; 5]).collect::<Vec<_>>(), &Options::default()), "xxxxxyyyyy", "{b:#x}");
    }
    // text characters: NUL and FS are the marker, DEL stays, 0x85 is an ellipsis in ANSI
    assert_eq!(text(&long(0x7F), &Options::default()).chars().nth(60), Some('\x7f'));
    assert_eq!(text(&long(0x85), &Options::default()).chars().nth(60), Some('\u{2026}'));
}

#[test]
fn bytes_that_are_no_utf8_place_an_empty_text() {
    let mut bad = vec![0xEF, 0xBB, 0xBF];
    bad.extend(b"ab\xff\xfecd");
    for (bytes, options) in [(bad, Options::default()), (b"ab\xc3(cd".to_vec(), utf8())] {
        let s = read_bytes(&bytes, &options).unwrap();
        assert_eq!(s.story_text(), "");
        assert_eq!(s.warnings.len(), 1);
    }
    // a UTF-32 mark is read as UTF-16: every other character is a NUL, an invisible marker
    assert_eq!(text(&[0xFF, 0xFE, 0, 0, b'a', 0, 0, 0, b'b', 0, 0, 0], &Options::default()), "\u{feff}a\u{feff}b\u{feff}");
    // a file of only a mark holds nothing
    assert_eq!(text(&[0xEF, 0xBB, 0xBF], &Options::default()), "");
}

#[test]
fn runs_of_returns() {
    let runs = b"a\r\nb\r\n\r\nc\r\n\r\n\r\nd\r\n\r\n\r\n\r\ne\r\n\r\n";
    let lines = Options { strip_returns_between_lines: true, ..Options::default() };
    let paras = Options { strip_returns_between_paragraphs: true, ..Options::default() };
    let both = Options { strip_returns_between_lines: true, strip_returns_between_paragraphs: true, ..Options::default() };
    assert_eq!(text(runs, &Options::default()), "a\rb\r\rc\r\r\rd\r\r\r\re\r\r");
    assert_eq!(text(runs, &lines), "a b\r\rc\r\r\rd\r\r\r\re\r\r");
    assert_eq!(text(runs, &paras), "a\rb\rc\rd\re\r");
    assert_eq!(text(runs, &both), "a b\rc\rd\re\r");
    assert_eq!(text(b"\r\n\r\nx\r\ny", &both), "\rx y");
    assert_eq!(text(b"a\rb\nc\r\nd", &lines), "a b c d");
    assert_eq!(text(b"ab\n\rcd", &Options::default()), "ab\r\rcd");
}

#[test]
fn quote_contexts() {
    let q = |s: &str| text(s.as_bytes(), &utf8());
    for opener in [" ", "\t", "(", "[", "{", "\u{a0}", "\u{ab}", "\u{2018}", "\u{201c}"] {
        assert_eq!(q(&format!("a{opener}\"b'")), format!("a{opener}\u{201c}b\u{2019}"), "{opener:?}");
        assert_eq!(q(&format!("a{opener}'b")), format!("a{opener}\u{2018}b"), "{opener:?}");
    }
    for closer in ["<", "-", "/", ".", ",", "0", "a", "\u{2013}", "\u{2014}", "\u{2026}", "\u{bb}", "\u{d55c}", "\u{2019}", "\u{201d}"] {
        assert_eq!(q(&format!("a{closer}\"b")), format!("a{closer}\u{201d}b"), "{closer:?}");
        assert_eq!(q(&format!("a{closer}'b")), format!("a{closer}\u{2019}b"), "{closer:?}");
    }
    // pairs: a quote follows the character before it as written (a straight quote closes)
    assert_eq!(
        q("\"\" '' \"'\" '\"' \"a\"'b'\"c\""),
        "\u{201c}\u{201d} \u{2018}\u{2019} \u{201c}\u{2019}\u{201d} \u{2018}\u{201d}\u{2019} \u{201c}a\u{201d}\u{2019}b\u{2019}\u{201d}c\u{201d}"
    );
    assert_eq!(q("'tis '90s 'n' rock'n'roll"), "\u{2018}tis \u{2018}90s \u{2018}n\u{2019} rock\u{2019}n\u{2019}roll");
}
