#[path = "support/interop_roundtrip.rs"]
mod support;
use serde_json::{Value, json};

const STYLE: &str = "<ParagraphStyle Self=\"ParagraphStyle/Shared paragraphs\"";
const PART: &str = "Resources/Styles.xml";

fn list(doc: &folio_model::Document) -> Value {
    serde_json::to_value(doc.stories["u113"].paragraphs[0].attrs.list.as_ref().unwrap()).unwrap()
}

fn check(field: &str, attr: &str, value: &str, expected: Value) {
    support::check(&format!("ListStyle.{field}"), &support::oracle().join("fixtures/bullets2-parent.idml"), PART,
        |xml| support::attribute(xml, STYLE, attr, value), |doc| list(doc)[field].clone(), expected);
}

#[test]
fn list_numbering_and_text_fields_survive_idml_export() {
    for (value, expected) in [("BulletList", "Bullet"), ("NumberedList", "Numbered")] {
        check("kind", "BulletsAndNumberingListType", value, json!(expected));
    }
    for (field, attr) in [("start_at", "NumberingStartAt"), ("level", "NumberingLevel")] {
        for value in [2, 7] { check(field, attr, &value.to_string(), json!(value)); }
    }
    for value in [false, true] {
        check("restart_after_higher_level", "NumberingApplyRestartPolicy", if value { "true" } else { "false" }, json!(value));
    }
    for (value, expected) in [("^t", "\t"), ("^m^>^s", "\u{2003}\u{2002}\u{a0}")] {
        check("bullet_text_after", "BulletsTextAfter", value, json!(expected));
    }
    for (value, expected) in [("^#.^t", "^#.\t"), ("^1-^#^m", "^1-^#\u{2003}")] {
        check("expression", "NumberingExpression", value, json!(expected));
    }
    for value in ["1, 2, 3, 4...", "I, II, III, IV..."] { check("format", "NumberingFormat", value, json!(value)); }
    for (value, expected) in [("NumberingList/$ID/[Default]", ""), ("NumberingList/List A", "List A"), ("NumberingList/List B", "List B")] {
        check("list", "AppliedNumberingList", value, json!(expected));
    }
    for start in [3, 8] {
        for continues in [false, true] {
            support::check("ListStyle.restart_at", &support::oracle().join("fixtures/bullets2-parent.idml"), PART,
                |xml| {
                    let xml = support::attribute(xml, STYLE, "NumberingStartAt", &start.to_string());
                    support::attribute(&xml, STYLE, "NumberingContinue", if continues { "true" } else { "false" })
                }, |doc| list(doc)["restart_at"].clone(), if continues { Value::Null } else { json!(start) });
        }
    }
}

#[test]
fn unicode_bullets_survive_but_glyph_id_bullets_fall_back() {
    for (kind, value, expected) in [("UnicodeOnly", "9733", "★"), ("UnicodeOnly", "9632", "■"),
        ("GlyphWithFont", "45", "•"), ("GlyphWithFont", "80", "•")] {
        support::check("ListStyle.bullet", &support::oracle().join("fixtures/bullets2-parent.idml"), PART,
            |xml| {
                let xml = support::attribute(xml, STYLE, "BulletsAndNumberingListType", "BulletList");
                let xml = support::attribute(&xml, "<BulletChar ", "BulletCharacterType", kind);
                support::attribute(&xml, "<BulletChar ", "BulletCharacterValue", value)
            }, |doc| list(doc)["bullet"].clone(), json!(expected));
    }
}
