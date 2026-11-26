use std::process::Command;

#[test]
fn bugfind_bad_input_and_output_are_reported_without_panic() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cleanroom/status/userdata/compose-repro");
    std::fs::create_dir_all(&root).unwrap();
    let bad = root.join("bad.json");
    let good = root.join("empty.json");
    std::fs::write(&bad, "{").unwrap();
    std::fs::write(&good, r#"{"stories":[]}"#).unwrap();
    let absent = root.join("absent.json");
    for (args, kind, path) in [
        (vec![absent.clone()], "read", absent.clone()),
        (vec![bad.clone()], "json", bad.clone()),
        (vec![root.clone()], "read", root.clone()),
        (vec![good.clone(), "--report".into(), root.clone()], "write", root.clone()),
        (vec![good.clone(), "--exclude-texts-from".into(), absent.clone()], "read", absent),
        (vec![good.clone(), "--recomposed".into(), bad.clone()], "json", bad),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_compose-conformance")).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["kind"], kind);
        assert_eq!(error["error"]["path"], path.to_str().unwrap());
    }
    assert!(Command::new(env!("CARGO_BIN_EXE_compose-conformance")).arg(good).output().unwrap().status.success());
}
