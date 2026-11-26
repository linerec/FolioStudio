#[path = "support/interop.rs"]
mod support;
use serde_json::json;
use support::{attr, check, fixture, roundtrip};

#[test]
fn toc_generation_flags_preserve_both_stored_boolean_values() {
    let mut original = fixture("toc");
    original.insert(
        "Resources/Styles.xml".into(),
        std::fs::read_to_string(
            support::root().join("oracle/goldens/toc/toc-create/after/Resources/Styles.xml"),
        )
        .unwrap(),
    );
    for (key, field) in [
        ("IncludeBookDocuments", "includeBookDocuments"),
        ("CreateBookmarks", "createBookmarks"),
        ("MakeAnchor", "makeAnchor"),
        ("RunIn", "runIn"),
    ] {
        for value in [false, true] {
            let mut parts = original.clone();
            attr(
                parts.get_mut("Resources/Styles.xml").unwrap(),
                "TOCStyle",
                1,
                key,
                &value.to_string(),
            );
            let (before, after, written) = roundtrip("toc", &parts);
            check(
                &before,
                &after,
                &format!("/toc_styles/1/{field}"),
                json!(value),
            );
            assert_eq!(
                written["Resources/Styles.xml"],
                parts["Resources/Styles.xml"]
            );
        }
    }
}
