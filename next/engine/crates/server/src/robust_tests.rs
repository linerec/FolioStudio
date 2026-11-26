//! Malformed input must stay on the existing error response path.
use crate::{Fonts, Profiles, edit, load_session};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::{Arc, Mutex}};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

#[test]
fn bugfind_cache_override_is_used_by_reader_and_color_cache() {
    let cache = root().join("artifacts/bugfind-c1-20261023/userdata/cache-override");
    if std::env::var_os("FOLIO_CACHE_TEST_CHILD").is_some() {
        assert_eq!(crate::cache_root(), cache.join("reader-cache"));
        assert_eq!(crate::color_cache_root(), cache.join("color-cache"));
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "robust_tests::bugfind_cache_override_is_used_by_reader_and_color_cache", "--nocapture"])
        .env("FOLIO_CACHE_TEST_CHILD", "1").env("FOLIO_CACHE_DIR", &cache)
        .env_remove("FOLIO_COLOR_CACHE").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn malformed_commands_leave_the_open_document_unchanged() {
    let fonts = Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(Profiles::start());
    let path = root().join("next/oracle/fixtures/basic.idml");
    let session = load_session(path.to_str().unwrap(), true, &fonts, &profiles, &|_, _| {}).unwrap();
    let before = edit::digest(&session);
    for commands in [Value::Null, json!(false), json!(0), json!("invalid"), json!({})] {
        let result = edit::run(&json!({"cmd": "edit", "commands": commands}), &session);
        let error = match result { Err(error) => error, Ok(_) => panic!("malformed commands accepted") };
        assert_eq!(error, json!({"ok": false, "code": "place", "error": "편집 명령이 없습니다"}));
        assert_eq!(edit::digest(&session), before);
    }
    for commands in [json!([null]), json!([{"op": "move", "ids": ["missing"], "by": ["bad", 0]}]), json!([{"op": "move", "ids": ["missing"], "by": [1, 0]}])] {
        let result = edit::run(&json!({"cmd": "edit", "commands": commands}), &session);
        assert!(result.is_err());
        assert_eq!(edit::digest(&session), before);
    }
}

#[test]
fn empty_and_truncated_packages_return_errors() {
    let dir = root().join("artifacts/robust-d-c1-20261023/test-inputs");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in [("empty", &b""[..]), ("truncated", &b"PK\x03\x04"[..])] {
        let path = dir.join(format!("{name}-{}.idml", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let result = edit::package_parts(&path);
        std::fs::remove_file(&path).unwrap();
        assert!(result.is_err(), "{name}");
    }
    assert!(edit::package_parts(&dir.join("missing.idml")).is_err());
}
