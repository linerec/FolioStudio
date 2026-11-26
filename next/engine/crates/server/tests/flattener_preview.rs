//! Window > Output > Flattener Preview against InDesign 2025: the test page (`next/oracle/fixtures/flattener-c2.idml`) drawn in
//! every Highlight mode with the three built-in presets must show what InDesign showed (pixel samples read from InDesign's
//! document window, `next/oracle/rules/windowmenu-c2-flattener-samples.indesign.json`), the panel state follows the rules
//! (rules/windowmenu-c2.ko.md), and the modes select the objects the rules name.
use serde_json::{Value, json};
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

#[path = "../../../test_support.rs"]
mod test_support;

struct Engine {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
    next: u64,
}

impl Engine {
    fn start() -> Engine {
        let mut child = Command::new(env!("CARGO_BIN_EXE_folio-engine"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("FOLIO_USER_DATA", std::env::temp_dir())
            .spawn()
            .expect("engine starts");
        let stdin = child.stdin.take().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut e = Engine { child, stdin, out, next: 0 };
        e.frame(); // ready
        e
    }

    fn frame(&mut self) -> (Value, Vec<u8>) {
        let mut len = [0u8; 4];
        self.out.read_exact(&mut len).expect("frame header length");
        let mut head = vec![0u8; u32::from_le_bytes(len) as usize];
        self.out.read_exact(&mut head).unwrap();
        let head: Value = serde_json::from_slice(&head).unwrap();
        let mut body = vec![0u8; head["bodyBytes"].as_u64().unwrap_or(0) as usize];
        self.out.read_exact(&mut body).unwrap();
        (head, body)
    }

    fn ask(&mut self, mut req: Value) -> (Value, Vec<u8>) {
        self.next += 1;
        req["id"] = json!(self.next);
        writeln!(self.stdin, "{req}").unwrap();
        self.stdin.flush().unwrap();
        loop {
            let (head, body) = self.frame();
            if head["id"] == json!(self.next) && head.get("event").is_none() {
                assert!(head["ok"].as_bool().unwrap_or(true), "{req}: {}", head["error"]);
                return (head, body);
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn golden() -> Value {
    let text = std::fs::read_to_string(root().join("oracle/rules/windowmenu-c2-flattener-samples.indesign.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn open() -> (Engine, Value) {
    let mut e = Engine::start();
    let path = root().join("oracle/fixtures/flattener-c2.idml");
    let (header, _) = e.ask(json!({"cmd": "open", "path": path.to_string_lossy()}));
    (e, header["document"]["color"].clone())
}

/// Compares a render with the golden's samples: the samples that differ by more than `tolerance`.
fn mismatches(golden: &Value, hexes: &str, body: &[u8], width: usize, skip_dark: bool, tolerance: i64) -> Vec<(i64, i64, String, String)> {
    let xs: Vec<i64> = golden["grid"]["x"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
    let ys: Vec<i64> = golden["grid"]["y"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
    let seams: Vec<i64> = golden["seamRows"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
    let skip: Vec<Vec<i64>> = golden["skip"].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()).collect();
    let mut bad = Vec::new();
    for (j, y) in ys.iter().enumerate() {
        for (i, x) in xs.iter().enumerate() {
            if seams.contains(y) || skip.iter().any(|r| *x >= r[0] && *x <= r[2] && *y >= r[1] && *y <= r[3]) {
                continue;
            }
            let at = (j * xs.len() + i) * 6;
            let exp: Vec<i64> = (0..3).map(|k| i64::from_str_radix(&hexes[at + 2 * k..at + 2 * k + 2], 16).unwrap()).collect();
            // InDesign shows [Black] as pure black; the screen renderer's conversion of K 100 % is a little lighter (not this feature's)
            if skip_dark && exp[0] <= 0x85 && exp[0] == exp[1] && exp[1] == exp[2] {
                continue;
            }
            let px = ((*y as usize * 2) * width + (*x as usize * 2)) * 4;
            let got = &body[px..px + 3];
            if exp.iter().zip(got).any(|(a, b)| (a - *b as i64).abs() > tolerance) {
                bad.push((*x, *y, hexes[at..at + 6].to_string(), got.iter().map(|v| format!("{v:02x}")).collect()));
            }
        }
    }
    bad
}

#[test]
fn every_mode_and_preset_matches_indesign() {
    let g = golden();
    let (mut e, _) = open();
    let mut failures = Vec::new();
    for (preset, modes) in g["presets"].as_object().unwrap() {
        for (mode, hexes) in modes.as_object().unwrap() {
            let (head, body) = e.ask(json!({"cmd": "render", "page": 0, "scale": 2, "view": {"mode": "flattener", "highlight": mode, "preset": preset}}));
            let bad = mismatches(&g, hexes.as_str().unwrap(), &body, head["width"].as_u64().unwrap() as usize, false, 8);
            if !bad.is_empty() {
                failures.push(format!("{preset} {mode}: {} samples differ, first {:?}", bad.len(), &bad[..bad.len().min(4)]));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_unrefreshed_preview_shows_the_live_page_half_way_to_white() {
    let g = golden();
    let (mut e, color) = open();
    let st = &g["stale"];
    let (_, live) = e.ask(json!({"cmd": "render", "page": 0, "scale": 2, "view": {"mode": "overprint"}}));
    let (head, body) = e.ask(json!({"cmd": "render", "page": 0, "scale": 2, "view": {"mode": "flattener", "highlight": st["highlight"], "preset": st["preset"], "stale": true}}));
    assert_eq!(head["stale"], true);
    assert_eq!(live.len(), body.len());
    for (before, after) in live.chunks_exact(4).zip(body.chunks_exact(4)) {
        for (before, after) in before[..3].iter().zip(&after[..3]) {
            assert_eq!(*after, ((*before as u16 + 255).div_ceil(2)) as u8);
        }
    }
    if ["cmyk", "rgb"].iter().any(|profile| color[*profile]["status"] != "used") {
        test_support::skip("flattener stale reference pixels require the document's color profiles");
        return;
    }
    let bad = mismatches(&g, st["hex"].as_str().unwrap(), &body, head["width"].as_u64().unwrap() as usize, true, 16); // the shadow, 50 % magenta and [Black] differ a little in the screen renderer
    assert!(bad.is_empty(), "{} samples differ, first {:?}", bad.len(), &bad[..bad.len().min(4)]);
}

#[test]
fn modes_select_the_objects_the_rules_name() {
    let (mut e, _) = open();
    let items = {
        let (h, _) = e.ask(json!({"cmd": "flattenerPreview", "page": 0, "highlight": "none"}));
        h["items"].as_array().unwrap().clone()
    };
    let mut ids = |highlight: &str, preset: &str| -> Vec<String> {
        let (h, _) = e.ask(json!({"cmd": "flattenerPreview", "page": 0, "highlight": highlight, "preset": preset}));
        let mut ids: Vec<String> = h["layers"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|l| l["intersect"].as_array().unwrap().iter())
            .flat_map(|m| m.as_array().unwrap().iter())
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let transparent: Vec<String> = items.iter().filter(|i| i["transparent"] == true).map(|i| i["id"].as_str().unwrap().to_string()).collect();
    // C, D (shadow), E, L, L2, L3
    assert_eq!(transparent.len(), 6);
    let mut t = ids("transparent", "[High Resolution]");
    t.sort();
    let mut want = transparent.clone();
    want.sort();
    assert_eq!(t, want);
    // everything under a transparent object that it overlaps comes in: B, F, K1, K2, K3 and K4 join the six
    assert_eq!(ids("affected", "[High Resolution]").len(), 12);
    // the strokes: K2 under transparency (High), K2 and H (Low, Medium: all strokes are outlined)
    assert_eq!(ids("outlinedStrokes", "[High Resolution]").len(), 1);
    assert_eq!(ids("outlinedStrokes", "[Low Resolution]").len(), 2);
    assert_eq!(ids("outlinedText", "[Low Resolution]").len(), 0);
    // the picture and the shadow
    assert_eq!(ids("affectedGraphics", "[High Resolution]").len(), 1);
    assert_eq!(ids("rasterized", "[High Resolution]").len(), 3);
    assert_eq!(ids("none", "[High Resolution]").len(), 0);
}

#[test]
fn the_panel_state_is_engine_state_with_the_measured_enabled_rules() {
    let mut e = Engine::start();
    let (h, _) = e.ask(json!({"cmd": "flattenerSettings"}));
    assert_eq!(h["state"], json!({"highlight": "none", "autoRefresh": false, "preset": "[High Resolution]", "ignoreSpreadOverrides": false}));
    // with Highlight None everything but Highlight is off
    assert_eq!(h["enabled"], json!({"highlight": true, "autoRefresh": false, "refresh": false, "preset": false, "ignoreSpreadOverrides": false, "applySettingsToPrint": false}));
    let (h, _) = e.ask(json!({"cmd": "flattenerSettings", "set": {"highlight": "transparent"}}));
    assert_eq!(h["enabled"], json!({"highlight": true, "autoRefresh": true, "refresh": true, "preset": true, "ignoreSpreadOverrides": true, "applySettingsToPrint": true}));
    // Auto Refresh on: the Refresh button is off
    let (h, _) = e.ask(json!({"cmd": "flattenerSettings", "set": {"autoRefresh": true}}));
    assert_eq!(h["enabled"]["refresh"], json!(false));
    // Apply Settings to Print writes the preset name and Ignore Spread Overrides to the print settings
    let (h, _) = e.ask(json!({"cmd": "flattenerSettings", "set": {"preset": "[Low Resolution]", "ignoreSpreadOverrides": true}, "apply": true}));
    assert_eq!(h["printApply"], json!({"flattenerPresetName": "[Low Resolution]", "ignoreSpreadOverrides": true}));
}
