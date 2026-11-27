use folio_ai::session::{Secrets, Store};
use serde_json::json;

#[test]
fn history_is_append_only_and_secrets_never_reach_session_or_index() {
    let root = std::env::var_os("FOLIO_USER_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(4)
                .unwrap()
                .join("artifacts/ai-tests/userdata")
        })
        .join("tests")
        .join(folio_project::util::uuid_v4());
    let project = "sample";
    let store = Store::open(&root, project).unwrap();
    assert!(Store::open(&root, "../escape").is_err());
    let mut secrets = Secrets::default();
    secrets.remember("remember-this-fake-value");
    let header = json!({"kind":"session","name":"sample","language":"ko","mode":"ask",
        "system":folio_ai::system_prompt("ko"),"tools":[{"name":"render"}]});
    let id = store.create(&header, &secrets).unwrap();
    let dir = root.join("ai").join(project);
    let path = dir.join(format!("{id}.jsonl"));
    let original = std::fs::read(&path).unwrap();
    store
        .append(
            &id,
            &json!({"kind":"user","text":"test-key-not-real remember-this-fake-value",
        "args":{"authorization":"hidden-fake-token","sk-example-fake":"x"}}),
            &secrets,
        )
        .unwrap();
    let appended = std::fs::read(&path).unwrap();
    assert!(appended.starts_with(&original));
    store.rename(&id, "new sk-fake-value", &secrets).unwrap();
    let again = Store::open(&root, project).unwrap();
    assert!(std::fs::read(&path).unwrap().starts_with(&appended));
    assert_eq!(again.records(&id).unwrap()[0], header);
    assert_eq!(again.list().unwrap()[0]["name"], "new [redacted]");
    for text in [
        std::fs::read_to_string(&path).unwrap(),
        std::fs::read_to_string(dir.join("index.json")).unwrap(),
        again.markdown(&id).unwrap(),
    ] {
        for secret in [
            "test-key-",
            "remember-this-fake-value",
            "hidden-fake-token",
            "sk-",
        ] {
            assert!(!text.contains(secret));
        }
    }
    assert!(again.records("../sample").is_err());
    again.delete(&id).unwrap();
    assert!(again.list().unwrap().is_empty());
    assert!(!path.exists());
}
