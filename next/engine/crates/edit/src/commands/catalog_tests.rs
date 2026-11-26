use serde_json::Value;
use std::collections::BTreeSet;

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn every_registered_command_has_current_documentation() {
    let data: Value = serde_json::from_str(include_str!("../../../../commands.generated.json")).unwrap();
    let tools: Vec<_> = data["tools"].as_array().unwrap().iter()
        .filter(|t| t["_meta"]["wire"]["op"].is_string()).collect();
    let fields = super::documented_fields();
    assert_eq!(tools.len(), fields.len(), "regenerate command documentation");
    let descriptions: Vec<_> = include_str!("descriptions.tsv").lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty()).collect();
    assert_eq!(descriptions.len(), fields.len());
    let mut seen = BTreeSet::new();
    for (op, args) in fields {
        assert!(seen.insert(op), "duplicate registered name: {op}");
        let tool = tools.iter().find(|t| t["_meta"]["wire"]["op"] == op).expect(op);
        assert_eq!(tool["name"], format!("edit.{op}"));
        let description = descriptions.iter().find(|line| line.split('|').next() == Some(op)).expect(op);
        let columns: Vec<_> = description.split('|').collect();
        assert_eq!(columns.len(), 3);
        for (i, language) in [(1, "ko"), (2, "en")] {
            assert!(!columns[i].trim().is_empty(), "missing {language}: {op}");
            assert_eq!(tool["_meta"]["description"][language], columns[i]);
        }
        let documented = tool["_meta"]["arguments"].as_array().unwrap();
        assert_eq!(documented.len(), args.len(), "{op}");
        for ((name, ty), actual) in args.iter().zip(documented) {
            assert_eq!(actual["name"], *name, "{op}");
            assert_eq!(compact(actual["rustType"].as_str().unwrap()), compact(ty), "{op}.{name}");
            assert_eq!(actual["required"], !compact(ty).starts_with("Option<"));
            assert!(actual["unit"].is_string());
            assert!(actual["default"]["kind"].is_string());
            assert_eq!(tool["inputSchema"]["properties"][name], actual["schema"]);
        }
        assert!(tool["outputSchema"].is_object());
    }
}
