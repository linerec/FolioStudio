//! composer-c1: the paragraph composer's hyphen weight slider (HyphenWeight 0-10) against InDesign 2025's own line starts.
//! `next/oracle/composer-c1/weight-sweep{,2}.idml` hold 72 frames each (3 texts x 6 widths x 2 sizes x left/justified, Arial) and
//! `weight-sweep{,2}.json` InDesign's DOM line start of every line of every frame at each slider value
//! (`artifacts/composer-c1-20261023/sweep.jsx`). Folio composes the same IDML with HyphenWeight set on every paragraph.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn oracle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/composer-c1")
}

fn read_zip(path: &Path) -> BTreeMap<String, String> {
    let f = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut z = zip::ZipArchive::new(f).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        out.insert(e.name().unwrap().into_owned(), s);
    }
    out
}

struct Set {
    parts: BTreeMap<String, String>,
    golden: Value,
}

impl Set {
    fn load(name: &str) -> Set {
        Set {
            parts: read_zip(&oracle().join(format!("{name}.idml"))),
            golden: serde_json::from_str(&std::fs::read_to_string(oracle().join(format!("{name}.json"))).unwrap()).unwrap(),
        }
    }
    fn count(&self) -> usize {
        self.golden["frames"].as_array().unwrap().len()
    }
    fn key(&self, k: usize) -> String {
        self.golden["frames"][k]["key"].as_str().unwrap().to_string()
    }
    /// InDesign's line starts of every frame at slider value `w`.
    fn want(&self, w: u32) -> Vec<Vec<usize>> {
        Self::lines(&self.golden["weights"][w.to_string()])
    }
    fn lines(v: &Value) -> Vec<Vec<usize>> {
        v.as_array().unwrap().iter().map(|e| e.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect()).collect()
    }
    /// Folio's line starts of every frame (spread order) with `HyphenWeight=weight` on every paragraph range.
    fn starts(&self, weight: u32) -> Vec<Vec<usize>> {
        self.starts_with("HyphenWeight", &weight.to_string())
    }
    /// Folio's line starts of every frame with the IDML attribute `attr="value"` on every paragraph range.
    fn starts_with(&self, attr: &str, value: &str) -> Vec<Vec<usize>> {
        static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
        let mut parts = self.parts.clone();
        for (name, xml) in parts.iter_mut() {
            if name.starts_with("Stories/") {
                *xml = xml.replace("<ParagraphStyleRange ", &format!("<ParagraphStyleRange {attr}=\"{value}\" "));
            }
        }
        let doc = folio_idml::read_parts(&parts, Path::new("weight-sweep.idml"), &folio_idml::ReaderData::default()).unwrap();
        let layout = folio_text::layout_document(&doc, FONTS.get_or_init(|| folio_text::FontStore::discover(&[])));
        let spread = parts.iter().find(|(n, _)| n.starts_with("Spreads/")).unwrap().1;
        let mut ids = Vec::new();
        let mut rest = spread.as_str();
        while let Some(i) = rest.find("<TextFrame Self=\"") {
            rest = &rest[i + 17..];
            ids.push(rest[..rest.find('"').unwrap()].to_string());
        }
        // the page's own (empty) master text frame comes first; the measured frames follow in creation order
        let ids = &ids[ids.len() - self.count()..];
        ids.iter().map(|id| layout.frames.get(id.as_str()).map(|f| f.lines.iter().map(|l| l.start).collect()).unwrap_or_default()).collect()
    }
}

fn sets() -> Vec<(&'static str, Set)> {
    ["weight-sweep", "weight-sweep2"].into_iter().map(|n| (n, Set::load(n))).collect()
}

/// Frames whose line starts differ from InDesign's, per slider value 0..=10, as measured with the fitted multipliers
/// (`folio_compose::cost::HYPHEN_WEIGHT_TABLE`). The differences that remain are not caused by the weight: slider 5, the
/// calibrated default, differs in 20 and 16 frames too (hyphenation dictionary points of narrow columns and "Extraordinary").
/// A change of the composer must not make any slider value worse; improvements lower these numbers.
const KNOWN_DIFFERENT: [(&str, [usize; 11]); 2] = [
    ("weight-sweep", [19, 19, 20, 19, 19, 20, 18, 18, 17, 20, 23]),
    ("weight-sweep2", [12, 12, 14, 11, 14, 16, 16, 13, 15, 11, 12]),
];

#[test]
fn weight_sweep_matches_indesign_as_measured() {
    let mut dump = serde_json::Map::new();
    for (name, set) in sets() {
        let known = KNOWN_DIFFERENT.iter().find(|(n, _)| *n == name).unwrap().1;
        let mut per_slider = serde_json::Map::new();
        for w in 0..=10u32 {
            let want = set.want(w);
            let got = set.starts(w);
            per_slider.insert(w.to_string(), serde_json::json!(got));
            let bad: Vec<String> = (0..want.len()).filter(|&k| got[k] != want[k]).map(|k| set.key(k)).collect();
            println!("{name} weight {w}: {} of {} frames differ {}", bad.len(), want.len(), bad.iter().take(4).cloned().collect::<Vec<_>>().join(" "));
            assert!(bad.len() <= known[w as usize], "{name} HyphenWeight {w}: {} frames differ from InDesign, {} are known", bad.len(), known[w as usize]);
        }
        dump.insert(name.to_string(), Value::Object(per_slider));
    }
    if let Ok(path) = std::env::var("FOLIO_HW_DUMP") {
        std::fs::write(path, Value::Object(dump).to_string()).unwrap();
    }
}

/// hyphenateAcrossColumns: 48 frames of 2 / 3 columns at six heights, true and false. The two frames that still differ
/// at each value are not caused by the option: `interdisciplin-ary` (the dictionary point InDesign takes) at false and a ragged
/// balance difference of the second text at true; every frame of the first text is the same as InDesign's.
#[test]
fn across_columns_matches_indesign_as_measured() {
    let set = Set::load("across-sweep");
    let mut dump = serde_json::Map::new();
    for value in ["true", "false"] {
        let inframe: Vec<usize> = set.golden["inframe"][value].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
        // InDesign also lists the lines of overset text; Folio lays out the lines in frames only
        let want: Vec<Vec<usize>> = Set::lines(&set.golden["lines"][value]).into_iter().zip(&inframe).map(|(l, &n)| l[..n].to_vec()).collect();
        let got = set.starts_with("HyphenateAcrossColumns", value);
        dump.insert(value.to_string(), serde_json::json!(got));
        let bad: Vec<String> = (0..want.len()).filter(|&k| got[k] != want[k]).map(|k| set.key(k)).collect();
        println!("across {value}: {} of {} frames differ {}", bad.len(), want.len(), bad.iter().take(6).cloned().collect::<Vec<_>>().join(" "));
        assert!(bad.len() <= 2, "hyphenateAcrossColumns={value}: {} frames differ from InDesign: {bad:?}", bad.len());
        assert!(bad.iter().all(|k| k.starts_with("t1-")), "a frame of the first text differs: {bad:?}");
    }
    if let Ok(path) = std::env::var("FOLIO_ACROSS_DUMP") {
        std::fs::write(path, Value::Object(dump).to_string()).unwrap();
    }
}
