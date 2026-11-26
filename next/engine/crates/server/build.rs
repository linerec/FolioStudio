//! The dialogs of the shared registry are one file each (`next/shared/dialogs/<id>.json`, so that dialogs added by different work do
//! not touch one file); this lists them for `dialog_state::schema` (`include_str!` of every file, sorted by name).
use std::{env, fs, path::Path};

fn main() {
    let annotation_fixture = "../../../oracle/rules/pdfcomments-annot.pdf";
    println!("cargo:rustc-check-cfg=cfg(has_comment_annot)");
    println!("cargo:rerun-if-changed={annotation_fixture}");
    if Path::new(annotation_fixture).is_file() {
        println!("cargo:rustc-cfg=has_comment_annot");
    }
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let dir = Path::new(&manifest).join("../../../shared/dialogs");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = fs::read_dir(&dir).expect("next/shared/dialogs").filter_map(Result::ok).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    let mut out = String::from("pub const DIALOG_FILES: &[(&str, &str)] = &[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_stem().and_then(|s| s.to_str()).expect("dialog file name");
        out.push_str(&format!("    ({name:?}, include_str!({:?})),\n", path.to_string_lossy()));
    }
    out.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").expect("out dir")).join("dialog_files.rs"), out).expect("dialog list");
}
