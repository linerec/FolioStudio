use folio_project::paths::*;
use unicode_normalization::UnicodeNormalization;

#[test]
fn every_device_number_and_control_character_is_rejected() {
    for prefix in ["COM", "LPT"] {
        for suffix in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"] {
            for name in [format!("{prefix}{suffix}"), format!("{}{suffix}.photo.jpg", prefix.to_lowercase())] {
                assert!(matches!(check_name(&name), Err(NameIssue::Reserved { .. })), "{name}");
                assert!(check_name(&fix_name(&name)).is_ok());
            }
        }
        for suffix in ["0", "10", "12", "x"] {
            assert!(check_name(&format!("{prefix}{suffix}.txt")).is_ok());
        }
    }
    for c in (0..32).chain([127]) {
        let name = format!("a{}b", char::from_u32(c).unwrap());
        assert_eq!(check_name(&name), Err(NameIssue::Control { code: c }));
        assert!(check_name(&fix_name(&name)).is_ok());
    }
}

#[test]
fn byte_and_utf16_boundaries_are_distinct() {
    assert!(check_name(&"a".repeat(255)).is_ok());
    assert!(check_name(&"a".repeat(256)).is_err());
    assert!(check_name(&"가".repeat(85)).is_ok());
    assert!(check_name(&"가".repeat(86)).is_err());
    let at_limit = format!("{}/{}", "a".repeat(99), "b".repeat(100));
    assert!(!RelPath::parse(&at_limit).unwrap().is_long());
    assert!(RelPath::parse(&(at_limit + "x")).unwrap().is_long());
    let emoji = format!("{}/{}/{}", "😀".repeat(40), "😀".repeat(40), "a".repeat(38));
    assert!(!RelPath::parse(&emoji).unwrap().is_long());
    assert!(RelPath::parse(&(emoji + "x")).unwrap().is_long());
    for input in [format!("x.{}", "a".repeat(300)), format!("{}.{}", "가".repeat(100), "나".repeat(100)), "😀".repeat(100)] {
        let fixed = fix_name(&input);
        assert!(check_name(&fixed).is_ok(), "{} bytes: {fixed}", fixed.len());
        assert_eq!(fix_name(&fixed), fixed);
    }
}

#[test]
fn normalization_and_cross_volume_boundaries() {
    for (root, file) in [
        (r"\\?\C:\책", r"\\?\c:\책\그림\표지.pdf"),
        (r"\\?\UNC\server\share\책", r"\\server\SHARE\책\그림\표지.pdf"),
        ("/Volumes/Book/책", "/Volumes/book/책/그림/표지.pdf"),
        ("/home/editor/책", "/home/editor/책/그림/표지.pdf"),
    ] {
        let nfd: String = file.nfd().collect();
        assert_eq!(relative_to(root, &nfd).unwrap().as_str(), "그림/표지.pdf");
    }
    for (root, file) in [("C:/Book", "D:/Book/a"), ("//nas/a", "//nas/b/a"),
                         ("/Volumes/A", "/Volumes/B/a"), ("/home/a", "/home/ab/x"),
                         ("/home/a", "/home/a/../x"), ("C:/a", "C:a/x")] {
        assert!(relative_to(root, file).is_err(), "{root} {file}");
    }
    assert_eq!(collision_key("ÉCOLE.PDF"), collision_key("e\u{301}cole.pdf"));
    for bad in ["a/./b", "a//b", "./a", "a/NUL.txt", "a/b ", "a/b."] {
        assert!(serde_json::from_str::<RelPath>(&serde_json::to_string(bad).unwrap()).is_err());
    }
}
