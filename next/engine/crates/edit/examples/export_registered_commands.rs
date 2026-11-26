fn main() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/renderer/src/generated/edit-commands.ts");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, folio_edit::commands::typescript()).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/renderer/src/generated/doc-preferences.ts");
    std::fs::write(path, folio_idml::docprefs::typescript()).unwrap();
}
