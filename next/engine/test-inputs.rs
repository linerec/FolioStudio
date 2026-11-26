use std::path::Path;

fn inputs(name: &str, paths: &[&str]) {
    println!("cargo:rustc-check-cfg=cfg({name})");
    for path in paths {
        println!("cargo:rerun-if-changed={path}");
    }
    if paths.iter().all(|path| Path::new(path).exists()) {
        println!("cargo:rustc-cfg={name}");
    }
}

fn main() {
    inputs("has_reference_vectors", &["../../../../cleanroom/vectors"]);
    if std::env::var("CARGO_PKG_NAME").as_deref() == Ok("folio-links") {
        inputs("has_comment_sample", &["../../../../artifacts/pdfcomments-20261023/measurement/sample.pdf"]);
        inputs("has_comment_annot", &["../../../oracle/rules/pdfcomments-annot.pdf"]);
        inputs("has_comment_pair", &["../../../oracle/rules/pdfcomments-annot.pdf", "../../../oracle/rules/pdfcomments-rich.pdf"]);
        inputs("has_comment_intents", &[
            "../../../oracle/rules/pdfcomments-c5-all.pdf", "../../../oracle/rules/pdfcomments-c5-intents.pdf",
            "../../../oracle/rules/pdfcomments-c5-arrows.pdf", "../../../oracle/rules/pdfcomments-c5-callouts.pdf",
            "../../../oracle/rules/pdfcomments-c5-stamps.pdf",
        ]);
    }
}
