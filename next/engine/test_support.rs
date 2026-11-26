//! 통합 시험에서 쓰는 선택 입력 확인과 합성 픽스처.
#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn skip(reason: &str) -> bool {
    assert!(std::env::var("FOLIO_REQUIRE_INPUTS").as_deref() != Ok("1"), "required test input: {reason}");
    eprintln!("SKIP {reason}");
    true
}

pub fn portable_only(test: &str) -> bool {
    std::env::var("FOLIO_PORTABLE_TESTS").as_deref() == Ok("1")
        && skip(&format!("{test}: 설치 글꼴·실제 문서 시험은 portable 모드에서 제외"))
}

pub fn missing(path: &Path) -> bool {
    !path.exists() && skip(&format!("input {} not found", path.display()))
}

/// Synthetic CMYK output profile for PDF structure tests. It contains no color tables.
pub fn write_test_cmyk_profile(path: &Path) -> std::io::Result<()> {
    let mut profile = [0u8; 132];
    profile[0..4].copy_from_slice(&132u32.to_be_bytes());
    profile[8] = 2;
    profile[12..16].copy_from_slice(b"prtr");
    profile[16..20].copy_from_slice(b"CMYK");
    profile[20..24].copy_from_slice(b"Lab ");
    profile[36..40].copy_from_slice(b"acsp");
    std::fs::write(path, profile)
}

/// Each call writes its own profile so parallel tests can read it safely.
pub fn test_cmyk_profile() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("folio-pdf-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("fake-cmyk-{}.icc", NEXT.fetch_add(1, Ordering::Relaxed)));
    write_test_cmyk_profile(&path).unwrap();
    path
}
