use std::collections::BTreeSet;

fn valid_name(name: &str) -> bool {
    name.split('.').all(|part| {
        part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && part.bytes().all(|c| c.is_ascii_alphanumeric())
    })
}

fn branch_names(source: &str) -> Vec<&str> {
    source.lines().filter_map(|line| line.trim().split_once("=>"))
        .filter(|(head, _)| head.split('|').all(|part| {
            let part = part.trim();
            part.starts_with('"') && part.ends_with('"') && part.matches('"').count() == 2
        }))
        .flat_map(|(head, _)| head.split('|').map(|s| s.trim().trim_matches('"')))
        .filter(|s| !s.is_empty()).collect()
}

#[test]
fn catalog_covers_all_dispatchers_without_exceptions() {
    let data = super::describe_commands().unwrap();
    let tools = data["tools"].as_array().unwrap();
    let mut seen = BTreeSet::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(valid_name(name), "invalid name: {name}");
        assert!(seen.insert(name), "duplicate name: {name}");
        for lang in ["ko", "en"] {
            assert!(!tool["_meta"]["description"][lang].as_str().unwrap().trim().is_empty());
        }
        let args = tool["_meta"]["arguments"].as_array().unwrap();
        let mut fields = BTreeSet::new();
        for arg in args {
            assert!(fields.insert(arg["name"].as_str().unwrap()));
            assert!(arg["schema"].is_object());
            assert!(arg["required"].is_boolean());
            assert!(arg["unit"].is_string());
            assert!(arg["default"]["kind"].is_string());
        }
        assert!(tool["outputSchema"].is_object() || name == "quit");
    }
    let mut registered = BTreeSet::new();
    for (name, _) in super::QUERIES {
        assert!(registered.insert(*name), "duplicate query: {name}");
    }
    let main = include_str!("main.rs").split_once("fn main()").unwrap().1;
    for name in branch_names(main) {
        assert!(registered.insert(name), "duplicate dispatcher: {name}");
    }
    let source = include_str!("source_queries.rs").split_once("const QUERIES:").unwrap().1
        .split_once("];").unwrap().0;
    for quoted in source.split('"').skip(1).step_by(2) {
        assert!(registered.insert(quoted));
    }
    let presets = include_str!("pdf_presets.rs").split_once("req[\"cmd\"] != \"").unwrap().1;
    assert!(registered.insert(presets.split('"').next().unwrap()));
    let documented: BTreeSet<_> = tools.iter().filter(|t| t["_meta"]["wire"]["op"].is_null())
        .map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(registered, documented, "regenerate command documentation");
    let metadata: Vec<_> = include_str!("registered_queries.catalog.tsv").lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty()).map(|s| s.split('|').collect::<Vec<_>>()).collect();
    assert_eq!(metadata.len(), registered.len(), "duplicate metadata rows");
    for row in &metadata {
        assert_eq!(row.len(), 5);
        let tool = tools.iter().find(|t| t["name"] == row[0]).unwrap();
        assert_eq!(tool["_meta"]["description"]["ko"], row[1]);
        assert_eq!(tool["_meta"]["description"]["en"], row[2]);
        let args: Vec<_> = row[3].split(';').filter(|s| !s.is_empty())
            .map(|s| s.split(':').next().unwrap()).collect();
        let documented: Vec<_> = tool["_meta"]["arguments"].as_array().unwrap().iter()
            .map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(args, documented, "regenerate arguments for {}", row[0]);
    }
    let rows: BTreeSet<_> = metadata.iter().map(|row| row[0]).collect();
    assert_eq!(registered, rows);
}

#[test]
fn describe_commands_needs_no_document_and_preserves_the_catalog() {
    let req = serde_json::json!({"cmd": "describeCommands"});
    let profiles = crate::Profiles {
        startup: std::sync::Mutex::new(None),
        index: std::sync::OnceLock::new(),
    };
    let result = super::dispatch(&req, None, &profiles).unwrap();
    let expected = super::describe_commands().unwrap();
    assert_eq!(result["tools"], expected["tools"]);
    assert_eq!(result["ok"], true);
    assert_eq!(super::dispatch(&serde_json::json!({"cmd": "notRegistered"}), None, &profiles), None);
}
