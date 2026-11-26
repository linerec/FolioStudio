//! Windows의 실제 드라이브 문자 경계를 검사한다. 매핑 대상은 시험 artifacts 안이다.
//! Mac/Linux의 볼륨 형식은 path_boundaries가 OS와 무관하게 검사한다.
#[cfg(windows)]
#[test]
fn reference_survives_another_drive_letter() {
    use folio_project::{hash, paths::{RelPath, relative_to_root, resolve_on_disk}};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    struct Mapping(String);
    impl Drop for Mapping {
        fn drop(&mut self) { let _ = Command::new("subst").args([&self.0, "/D"]).output(); }
    }
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/infra-drive-tests")
        .join(folio_project::util::uuid_v4());
    let source = base.join("source");
    let destination = base.join("destination");
    std::fs::create_dir_all(source.join("assets")).unwrap();
    std::fs::create_dir_all(destination.join("renamed/deep/assets")).unwrap();
    std::fs::write(source.join("assets/a.bin"), b"portable data").unwrap();
    std::fs::copy(source.join("assets/a.bin"), destination.join("renamed/deep/assets/a.bin")).unwrap();
    let target = std::path::absolute(&destination).unwrap();
    let mut mapping = None;
    for letter in ('D'..='Z').rev() {
        let drive = format!("{letter}:");
        if PathBuf::from(format!("{drive}/")).exists() { continue; }
        if Command::new("subst").arg(&drive).arg(&target).output().is_ok_and(|r| r.status.success()) {
            mapping = Some(Mapping(drive));
            break;
        }
    }
    if let Some(mapping) = mapping {
        let root = PathBuf::from(format!("{}/renamed/deep", mapping.0));
        let rel = RelPath::parse("assets/a.bin").unwrap();
        let found = resolve_on_disk(&root, &rel).unwrap();
        assert_eq!(relative_to_root(&root, &found).unwrap(), rel);
        assert_eq!(hash::sha256_bytes(&std::fs::read(found).unwrap()), hash::sha256_bytes(b"portable data"));
        drop(mapping); // 매핑을 만든 시험만 해제한다. 기존 드라이브는 건드리지 않는다.
    } else {
        assert!(std::env::var("FOLIO_REQUIRE_INPUTS").as_deref() != Ok("1"), "subst unavailable");
        eprintln!("SKIP drive_relocation: 새 subst 드라이브를 만들 수 없음");
    }
    std::fs::remove_dir_all(base).unwrap();
}
