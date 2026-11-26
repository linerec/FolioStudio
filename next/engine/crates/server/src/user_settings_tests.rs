use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Profile(PathBuf);
impl Profile {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../artifacts/settings-tests").join(format!("folio-settings-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))))
    }
    fn ask(&self, request: Value) -> Value { access(&self.0, &request).unwrap() }
}
impl Drop for Profile {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn defaults_and_round_trip_keep_unrelated_settings() {
    let p = Profile::new();
    assert_eq!(p.ask(json!({})), json!({}));
    p.ask(json!({"patch":{"language":"ko","shortcuts":{"x":"Ctrl+X"},"preferences":{"existing":true}}}));
    p.ask(json!({"values":{"folio.units":"mm","folio.view.guides":"false"}}));
    let saved = p.ask(json!({}));
    assert_eq!(saved["language"], "ko");
    assert_eq!(saved["shortcuts"]["x"], "Ctrl+X");
    assert_eq!(saved["preferences"], json!({"existing":true,"folio.units":"mm","folio.view.guides":"false"}));
    assert_eq!(serde_json::from_slice::<Value>(&fs::read(p.0.join("settings.json")).unwrap()).unwrap(), saved);
}

#[test]
fn migration_is_repeatable_and_new_values_win() {
    let p = Profile::new();
    p.ask(json!({"values":{"folio.theme":"light"}}));
    let migrate = json!({"migrate":{"folio.theme":"dark","folio.stepRepeat":"{\"count\":3}"}});
    let first = p.ask(migrate.clone());
    assert_eq!(first["preferences"]["folio.theme"], "light");
    assert_eq!(first["preferences"]["folio.stepRepeat"], "{\"count\":3}");
    assert_eq!(p.ask(migrate), first);
}

#[test]
fn damaged_file_recovers_without_losing_original_bytes() {
    let p = Profile::new();
    fs::create_dir_all(&p.0).unwrap();
    fs::write(p.0.join("settings.json"), b"{broken").unwrap();
    assert_eq!(p.ask(json!({})), json!({}));
    p.ask(json!({"values":{"folio.units":"pt"}}));
    assert_eq!(fs::read(p.0.join("settings.corrupt.json")).unwrap(), b"{broken");
    assert_eq!(p.ask(json!({}))["preferences"]["folio.units"], "pt");
}

#[test]
fn invalid_requests_do_not_change_file() {
    let p = Profile::new();
    let before = p.ask(json!({"values":{"folio.units":"mm"}}));
    for bad in [json!({"values":false}), json!({"values":{"x":{}}}), json!({"patch":{},"migrate":{}})] {
        assert!(access(&p.0, &bad).is_err());
    }
    assert_eq!(p.ask(json!({})), before);
}

#[test]
fn concurrent_windows_preserve_each_others_keys() {
    let p = Profile::new();
    std::thread::scope(|scope| {
        for n in 0..8 {
            let root = &p.0;
            scope.spawn(move || { access(root, &json!({"values":{format!("key{n}"):n}})).unwrap(); });
        }
    });
    assert_eq!(p.ask(json!({}))["preferences"].as_object().unwrap().len(), 8);
}
