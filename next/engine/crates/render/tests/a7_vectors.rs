use std::path::Path;
use std::process::Command;

#[cfg_attr(not(has_reference_vectors), ignore = "optional reference vectors are missing")]
#[test]
fn a7_effects_match_required_vectors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .expect("locate the A7 checkout");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let mut checker = Command::new(python);
    let output = checker
        .current_dir(&root)
        .args(["-B", "-I", "next/engine/crates/render/tests/a7_vectors.py"])
        .arg(if cfg!(debug_assertions) { "debug" } else { "release" })
        .env("CARGO", env!("CARGO"))
        .output()
        .expect("run A7 vector checker");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}{stderr}");
    print!("{stdout}");
}
