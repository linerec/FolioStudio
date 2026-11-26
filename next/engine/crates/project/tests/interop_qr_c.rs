#[path = "support/interop.rs"]
mod support;
use folio_model::{
    Rect,
    qr::{self, QrContent, VCard},
};
use serde_json::json;
use support::{attr, by_id, fixture, roundtrip};

const PART: &str = "Spreads/Spread_ud1.xml";
const FIXTURE: &str = "datamerge-qr/qr1-merged";

fn replace_qr(xml: &mut String, tag: &str, key: &str, value: &str, keep_eps: bool) {
    let start = xml.find("<PlainTextQRCode ").unwrap();
    let end = start + xml[start..].find("</PlainTextQRCode>").unwrap() + "</PlainTextQRCode>".len();
    xml.replace_range(
        start..end,
        &format!("<{tag} Self=\"u19b\" {key}=\"{value}\"/>"),
    );
    if !keep_eps {
        let start = xml.find("<Contents>").unwrap();
        let end = start + xml[start..].find("</Contents>").unwrap() + "</Contents>".len();
        xml.replace_range(start..end, "");
        for (key, value) in [
            ("Left", "0"),
            ("Top", "0"),
            ("Right", "100"),
            ("Bottom", "100"),
        ] {
            attr(xml, "GraphicBounds", 0, key, value);
        }
    }
}

fn content(tag: &str, key: &str, value: &str) -> QrContent {
    let field = |name| {
        if name == key {
            value.to_owned()
        } else {
            String::new()
        }
    };
    match tag {
        "EmailQRCode" => QrContent::Email {
            address: field("EmailAddress"),
            subject: field("Subject"),
            body: field("Body"),
        },
        "TextMessageQRCode" => QrContent::TextMessage {
            number: field("CellNumber"),
            message: field("TextMessage"),
        },
        "VCardQRCode" => QrContent::VCard(VCard {
            first_name: field("FirstName"),
            last_name: field("LastName"),
            job_title: field("JobTitle"),
            cell_phone: field("CellPhone"),
            phone: field("Phone"),
            email: field("Email"),
            organisation: field("Organisation"),
            street_address: field("StreetAddress"),
            city: field("City"),
            state: field("AdrState"),
            postal_code: field("PostalCode"),
            country: field("Country"),
            website: field("Website"),
        }),
        _ => panic!("unexpected QR kind"),
    }
}

#[test]
fn qr_content_attributes_survive_raw_export_and_drive_fallback_modules() {
    let bounds = Rect {
        left: 0.0,
        top: 0.0,
        right: 100.0,
        bottom: 100.0,
    };
    for (tag, keys) in [
        ("EmailQRCode", "EmailAddress Subject Body"),
        ("TextMessageQRCode", "CellNumber TextMessage"),
        (
            "VCardQRCode",
            "FirstName LastName JobTitle CellPhone Phone Email Organisation StreetAddress City AdrState PostalCode Country Website",
        ),
    ] {
        for key in keys.split_whitespace() {
            for value in ["alpha-17", "beta-29"] {
                let mut parts = fixture(FIXTURE);
                replace_qr(parts.get_mut(PART).unwrap(), tag, key, value, false);
                let (before, after, written) = roundtrip(FIXTURE, &parts);
                let modules = qr::encode(&content(tag, key, value).payload()).unwrap();
                let expected = json!(qr::module_paths(&modules, &bounds));
                assert_eq!(
                    by_id(&before, "u19b")["paths"],
                    expected,
                    "{tag}.{key}={value}"
                );
                assert_eq!(
                    by_id(&after, "u19b")["paths"],
                    expected,
                    "{tag}.{key}={value}"
                );
                assert_eq!(written[PART], parts[PART]);
            }
        }
    }
}

#[test]
fn embedded_eps_takes_priority_while_changed_content_attributes_are_preserved() {
    let original = fixture(FIXTURE);
    let (baseline, _, _) = roundtrip(FIXTURE, &original);
    let paths = &by_id(&baseline, "u19b")["paths"];
    assert!(!paths.as_array().unwrap().is_empty());
    for (tag, key) in [
        ("EmailQRCode", "Body"),
        ("TextMessageQRCode", "TextMessage"),
        ("VCardQRCode", "FirstName"),
    ] {
        let mut parts = original.clone();
        replace_qr(
            parts.get_mut(PART).unwrap(),
            tag,
            key,
            "changed-metadata",
            true,
        );
        let (before, after, written) = roundtrip(FIXTURE, &parts);
        assert_eq!(&by_id(&before, "u19b")["paths"], paths);
        assert_eq!(&by_id(&after, "u19b")["paths"], paths);
        assert_eq!(written[PART], parts[PART]);
    }
}
