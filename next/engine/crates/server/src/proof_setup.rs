//! View ▸ Proof Colors and View ▸ Proof Setup: the engine's proof state.
//!
//! The state belongs to the engine (not to the screen) so that a person and an LLM set it the same way:
//! `{"cmd":"proofSetup"}` reads it, `{"cmd":"proofSetup","set":{…}}` changes it, and the `render` request then
//! draws pages through it (see [`colors_for`]). Whether the proof is on (`enabled`) belongs to the running
//! window and starts off; which device it simulates (`source`, `customProfile`, `intent`,
//! `blackPointCompensation`) is a per-computer user setting kept in `proof-setup.json` of `FOLIO_USER_DATA`
//! (never in a document or project). Working CMYK (`workingCmyk`) is the Color Settings' working space;
//! the screen passes it on whenever it changes.
//!
//! `{"cmd":"proofColors","cmyk":[[c,m,y,k]…],"rgb":[[r,g,b]…]}` answers with the display colour of each value
//! with the proof off (`plain`) and as the current setup proofs it (`proofed`).
use folio_color::{Colors, ProfileIndex, ProofSpec, read_profile};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// InDesign's working CMYK until Color Settings say otherwise.
const DEFAULT_WORKING_CMYK: &str = "U.S. Web Coated (SWOP) v2";
const SOURCES: &[&str] = &["document", "working", "custom"];
const INTENTS: &[&str] = &["Perceptual", "Saturation", "RelativeColorimetric", "AbsoluteColorimetric"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProofSetup {
    /// View ▸ Proof Colors (not kept between runs).
    #[serde(skip_serializing_if = "is_false")]
    pub enabled: bool,
    /// `document` (the document's CMYK profile), `working` (Color Settings' working CMYK) or `custom` (`customProfile`).
    pub source: String,
    pub working_cmyk: String,
    pub custom_profile: String,
    /// Custom…: Preserve CMYK Numbers (the document's ink numbers are the device's; off, they are converted to it).
    pub preserve_numbers: bool,
    /// Custom…: Display Options ▸ Simulate Black Ink (the device's black is not lifted to display black).
    pub simulate_black_ink: bool,
    /// Custom…: Display Options ▸ Simulate Paper Color (the paper's own white; it implies Simulate Black Ink).
    pub simulate_paper_color: bool,
    /// Custom…: the rendering intent of the conversion to the device when the numbers are not preserved
    /// (`Perceptual`, `Saturation`, `RelativeColorimetric` or `AbsoluteColorimetric`).
    pub intent: String,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Default for ProofSetup {
    fn default() -> Self {
        ProofSetup {
            enabled: false,
            source: "document".into(),
            working_cmyk: DEFAULT_WORKING_CMYK.into(),
            custom_profile: String::new(),
            preserve_numbers: true,
            simulate_black_ink: false,
            simulate_paper_color: false,
            intent: "RelativeColorimetric".into(),
        }
    }
}

fn user_file() -> Option<PathBuf> {
    std::env::var_os("FOLIO_USER_DATA").filter(|p| !p.is_empty()).map(|p| PathBuf::from(p).join("proof-setup.json"))
}

fn state() -> &'static Mutex<ProofSetup> {
    static STATE: OnceLock<Mutex<ProofSetup>> = OnceLock::new();
    STATE.get_or_init(|| {
        let mut setup = user_file()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<ProofSetup>(&b).ok())
            .unwrap_or_default();
        setup.enabled = false;
        Mutex::new(setup)
    })
}

/// The current setup.
pub fn current() -> ProofSetup {
    state().lock().expect("shared state must remain unpoisoned after each operation").clone()
}

fn validate(s: &ProofSetup) -> Result<(), String> {
    if !SOURCES.contains(&s.source.as_str()) {
        return Err(format!("unknown proof source {:?} (document, working or custom)", s.source));
    }
    if !INTENTS.contains(&s.intent.as_str()) {
        return Err(format!("unknown rendering intent {:?}", s.intent));
    }
    if s.source == "custom" && s.custom_profile.trim().is_empty() {
        return Err("a custom proof setup needs customProfile".into());
    }
    Ok(())
}

/// A profile of the setup that cannot be used (why), or the file and name that are.
struct Resolved {
    name: String,
    path: Option<PathBuf>,
    bytes: Option<Vec<u8>>,
    error: Option<String>,
}

fn resolve(setup: &ProofSetup, s: Option<&crate::Session>, index: &ProfileIndex) -> Resolved {
    let name = match setup.source.as_str() {
        "document" => s.and_then(|s| s.doc.print.cmyk_profile.clone()).unwrap_or_default(),
        "working" => setup.working_cmyk.clone(),
        _ => setup.custom_profile.clone(),
    };
    if name.trim().is_empty() {
        let why = if s.is_none() { "no document open" } else { "the document names no CMYK profile" };
        return Resolved { name, path: None, bytes: None, error: Some(why.into()) };
    }
    let dir = s.and_then(|s| s.source_dir.as_deref());
    let Some(path) = index.find_for_document(&name, dir).or_else(|| ProfileIndex::scan(index.roots()).find(&name)) else {
        return Resolved { error: Some(format!("profile \"{name}\" is not installed")), name, path: None, bytes: None };
    };
    match read_profile(&path) {
        Ok(bytes) => Resolved { name, path: Some(path), bytes: Some(bytes), error: None },
        Err(e) => Resolved { error: Some(format!("profile \"{name}\" cannot be read: {e}")), name, path: Some(path), bytes: None },
    }
}

/// What the setup converts with: Document CMYK and Working CMYK are Custom… with Simulate Black Ink on (measured in InDesign 2026 and 2025).
/// A Custom… device may be a CMYK, RGB or gray profile (the colour crate's `Colors::proofed` tells what each does).
fn spec(setup: &ProofSetup) -> ProofSpec {
    if setup.source != "custom" {
        return ProofSpec::default();
    }
    ProofSpec {
        preserve_numbers: setup.preserve_numbers,
        simulate_black: setup.simulate_black_ink || setup.simulate_paper_color,
        simulate_paper: setup.simulate_paper_color,
        intent: folio_color::icc::Intent::from_name(&setup.intent),
    }
}

/// The conversions a page is drawn with while the proof is on: `None` when it is off, or when the profile cannot be
/// used (then the reason is the second value, and the page is drawn as without a proof).
pub fn colors_for(s: &crate::Session, index: &ProfileIndex) -> (Option<Arc<Colors>>, Option<String>) {
    let setup = current();
    if !setup.enabled {
        return (None, None);
    }
    match build(&setup, Some(s), index) {
        Ok(colors) => (Some(colors), None),
        Err(e) => (None, Some(e)),
    }
}

/// The conversions the Overprint Preview and Separations Preview draw with while the proof is on (any device: RGB content is separated into
/// the document's CMYK from the device's numbers or the document RGB's, see `Colors::proofed`). The ink limit view does not follow the proof
/// (InDesign 2025 shows its red marker and gray through the device in a way not explained), so the caller leaves it out.
pub fn colors_for_ink_views(s: &crate::Session, index: &ProfileIndex) -> Option<Arc<Colors>> {
    let setup = current();
    if !setup.enabled {
        return None;
    }
    build(&setup, Some(s), index).ok()
}

fn build(setup: &ProofSetup, s: Option<&crate::Session>, index: &ProfileIndex) -> Result<Arc<Colors>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Colors>>>> = OnceLock::new();
    let r = resolve(setup, s, index);
    if let Some(e) = r.error {
        return Err(e);
    }
    let rgb_name = s.and_then(|s| s.doc.print.rgb_profile.clone()).filter(|n| n.trim() != folio_color::SRGB_NAME);
    let dir = s.and_then(|s| s.source_dir.as_deref());
    let rgb = rgb_name.as_deref().and_then(|n| index.find_for_document(n, dir));
    let spec = spec(setup);
    // the document's CMYK profile converts the numbers when they are not preserved, and every CMYK colour shown on an RGB or gray device
    let doc_cmyk = s.and_then(|s| s.doc.print.cmyk_profile.as_deref()).and_then(|n| index.find_for_document(n, dir));
    let key = format!("{:?}|{:?}|{:?}|{:?}", r.path, spec, rgb, doc_cmyk);
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().expect("shared state must remain unpoisoned after each operation").get(&key) {
        return Ok(hit.clone());
    }
    let rgb_bytes = rgb.as_deref().and_then(|p| read_profile(p).ok());
    let doc_bytes = doc_cmyk.as_deref().and_then(|p| read_profile(p).ok());
    let colors = Arc::new(
        Colors::proofed(r.bytes.as_deref().unwrap_or_default(), doc_bytes.as_deref(), rgb_bytes.as_deref(), spec).map_err(|e| format!("proof profile \"{}\": {e}", r.name))?,
    );
    let mut map = cache.lock().expect("shared state must remain unpoisoned after each operation");
    if map.len() >= 6 {
        map.clear();
    }
    map.insert(key, colors.clone());
    Ok(colors)
}

fn report(setup: &ProofSetup, s: Option<&crate::Session>, index: &ProfileIndex) -> Value {
    let r = resolve(setup, s, index);
    let status = match (&r.error, &r.bytes) {
        (Some(_), _) if r.path.is_none() && r.name.is_empty() => "notNamed",
        (Some(_), _) if r.path.is_none() => "missing",
        (Some(_), _) => "unusable",
        _ => "used",
    };
    // the colour space of the device (`CMYK`, `RGB` or `GRAY`) tells the dialog which "Preserve … Numbers" it offers
    let space = r.path.as_ref().and_then(|p| index.entries().iter().find(|e| &e.path == p)).map(|e| e.space.trim().to_string());
    json!({"setup": setup, "profile": {"name": r.name, "file": r.path.as_ref().map(|p| p.display().to_string()), "space": space, "status": status, "error": r.error}})
}

/// `{"cmd":"proofSetup"}` / `{"cmd":"proofSetup","set":{…}}` (any of the setup's fields; `enabled` too).
pub fn query(req: &Value, s: Option<&crate::Session>, profiles: &crate::Profiles) -> Result<Value, String> {
    if let Some(set) = req.get("set").filter(|v| !v.is_null()) {
        let obj = set.as_object().ok_or("set must be an object")?;
        let mut merged = serde_json::to_value(current()).map_err(|e| e.to_string())?;
        merged["enabled"] = json!(current().enabled);
        for (k, v) in obj {
            merged[k] = v.clone();
        }
        let next: ProofSetup = serde_json::from_value(merged).map_err(|e| format!("bad proof setup: {e}"))?;
        validate(&next)?;
        let keep = ProofSetup { enabled: false, ..next.clone() };
        let changed_setting = keep != ProofSetup { enabled: false, ..current() };
        *state().lock().expect("shared state must remain unpoisoned after each operation") = next;
        if changed_setting
            && let Some(file) = user_file() {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, serde_json::to_vec_pretty(&keep).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
    }
    let setup = current();
    Ok(report(&setup, s, &profiles.get()))
}

/// `{"cmd":"proofColors","cmyk":[[c,m,y,k]…](percent),"rgb":[[r,g,b]…]}`.
pub fn colors(req: &Value, s: Option<&crate::Session>, profiles: &crate::Profiles) -> Result<Value, String> {
    let index = profiles.get();
    let setup = current();
    let proofed = build(&setup, s, &index)?;
    let plain = s.map(|s| s.colors.clone()).unwrap_or_else(Colors::unmanaged);
    let cmyk: Vec<[f64; 4]> = req["cmyk"].as_array().map(|a| a.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect()).unwrap_or_default();
    let rgb: Vec<[u8; 3]> = req["rgb"].as_array().map(|a| a.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect()).unwrap_or_default();
    let of = |c: &Colors| json!({
        "cmyk": cmyk.iter().map(|v| c.cmyk_to_srgb(v[0], v[1], v[2], v[3])).collect::<Vec<_>>(),
        "rgb": rgb.iter().map(|v| c.rgb_to_srgb(*v)).collect::<Vec<_>>(),
    });
    Ok(json!({"plain": of(&plain), "proofed": of(&proofed), "setup": setup}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_keeps_only_what_belongs_to_the_user() {
        // `enabled` belongs to the running window: it is not written, and an old file cannot turn the proof on
        let text = serde_json::to_string(&ProofSetup::default()).unwrap();
        assert!(!text.contains("enabled"), "{text}");
        let read: ProofSetup = serde_json::from_str(r#"{"source":"custom","customProfile":"X","simulatePaperColor":true}"#).unwrap();
        assert_eq!((read.source.as_str(), read.preserve_numbers, read.simulate_black_ink, read.enabled), ("custom", true, false, false));
    }

    #[test]
    fn document_and_working_cmyk_are_custom_with_black_ink_simulated() {
        let doc = ProofSetup { source: "document".into(), simulate_black_ink: false, ..ProofSetup::default() };
        assert!(spec(&doc).simulate_black && spec(&doc).preserve_numbers && !spec(&doc).simulate_paper);
        assert_eq!(spec(&ProofSetup { source: "working".into(), ..ProofSetup::default() }), ProofSpec::default());
        // Custom… follows its own options; the paper simulation implies the black ink one
        let custom = ProofSetup { source: "custom".into(), custom_profile: "X".into(), simulate_paper_color: true, ..ProofSetup::default() };
        assert!(spec(&custom).simulate_paper && spec(&custom).simulate_black);
        let plain = ProofSetup { source: "custom".into(), custom_profile: "X".into(), ..ProofSetup::default() };
        assert!(!spec(&plain).simulate_black && !spec(&plain).simulate_paper);
    }

    #[test]
    fn a_setup_is_checked_before_it_is_kept() {
        assert!(validate(&ProofSetup::default()).is_ok());
        assert!(validate(&ProofSetup { source: "paper".into(), ..ProofSetup::default() }).is_err());
        assert!(validate(&ProofSetup { intent: "Fancy".into(), ..ProofSetup::default() }).is_err());
        assert!(validate(&ProofSetup { source: "custom".into(), ..ProofSetup::default() }).is_err(), "Custom… needs a device");
    }
}
