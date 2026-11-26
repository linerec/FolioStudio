use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A warning's stable identity and values, alongside its unchanged legacy text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub message: String,
    pub message_id: String,
    pub message_args: Value,
}

pub(crate) fn msg(id: &str, message: impl Into<String>, args: Value) -> Message {
    Message { message: message.into(), message_id: id.into(), message_args: args }
}

#[cfg(test)]
mod tests {
    use crate::{Options, TextSource, read_bytes};

    fn check(source: &TextSource) {
        assert_eq!(source.warnings.len(), source.warning_messages.len());
        let wire = serde_json::to_value(source).unwrap();
        for (i, warning) in source.warnings.iter().enumerate() {
            let message = &wire["warningMessages"][i];
            assert_eq!(message["message"], *warning);
            assert!(message["messageId"].as_str().unwrap().starts_with("textimport."));
            assert!(message["messageArgs"].is_object());
        }
        let mut legacy = wire;
        legacy.as_object_mut().unwrap().remove("warningMessages");
        let old: TextSource = serde_json::from_value(legacy).unwrap();
        assert_eq!(old.warnings, source.warnings);
        assert!(old.warning_messages.is_empty());
    }

    #[test]
    fn every_fixture_warning_has_metadata_without_changing_legacy_text() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links");
        let mut checked = 0;
        for path in std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()) {
            if !matches!(path.extension().and_then(|e| e.to_str()), Some("docx" | "rtf" | "txt")) { continue; }
            if let Ok(source) = read_bytes(&std::fs::read(path).unwrap(), &Options::default()) {
                checked += source.warnings.len();
                check(&source);
            }
        }
        assert!(checked > 20, "must exercise real import warnings, checked {checked}");
    }

    #[test]
    fn dynamic_values_are_separate_from_the_message_template() {
        let source = read_bytes(br"{\rtf1\ansi\unknowncontrol42 hello}", &Options::default()).unwrap();
        check(&source);
        let message = source.warning_messages.iter().find(|m| m.message_id == "textimport.rtfDirective").unwrap();
        assert_eq!(message.message_args["other"], "unknowncontrol");
        assert_eq!(message.message, "RTF 지시어 \\unknowncontrol 은(는) 아직 가져오지 않습니다");
    }
}
