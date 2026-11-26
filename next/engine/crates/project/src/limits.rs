//! What Folio does differently from InDesign, as data the reports can use (principle 2, item 3: "do not hide it").
//!
//! The catalogue is `next/oracle/rules/limits.toml`, turned into `limits.generated.json` by `next/tools/limits-report.py`
//! (the same script writes the document `artifacts/finish-20261113/LIMITS.ko.md`). Each difference may carry `detect`
//! strings: text that only appears in an IDML part when the document really uses the feature. A [`Scanner`] counts them
//! while the import plan and the IDML export already read every part, so the reports can say "this document uses N of
//! them" without another pass. The differences a document cannot be checked for stay in the fixed part of the report
//! ([`report`]'s `fixed`), together with the place of the full document.
use std::path::Path;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DATA: &str = include_str!("../../../../oracle/rules/limits.generated.json");

#[derive(Debug, Deserialize)]
pub struct Area {
    pub id: String,
    pub ko: String,
    pub en: String,
}

#[derive(Debug, Deserialize)]
pub struct Difference {
    pub id: String,
    /// `approx`, `unsupported`, `unverified` or `excluded`.
    pub kind: String,
    pub area: String,
    /// Rank in the one-page summary's "most important differences" (1 is first).
    pub top: Option<u32>,
    pub ko: String,
    pub en: String,
    #[serde(default)]
    pub detect: Vec<String>,
    #[serde(default)]
    pub subtract: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub doc: String,
    pub areas: Vec<Area>,
    pub differences: Vec<Difference>,
    pub table: Value,
}

/// The catalogue compiled into the engine (a parse error is a build problem, caught by the tests).
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(DATA).expect("oracle/rules/limits.generated.json"))
}

/// A difference a document uses, with how many places in it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub id: String,
    pub kind: String,
    pub area: String,
    pub count: usize,
}

/// Counts the `detect` strings of every difference over the XML parts it is fed.
pub struct Scanner {
    /// per difference: (`detect` matches, `subtract` matches)
    counts: Vec<(usize, usize)>,
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Scanner {
    pub fn new() -> Self {
        Scanner { counts: vec![(0, 0); catalog().differences.len()] }
    }

    /// One IDML part (`Stories/Story_u1.xml`, `Spreads/…`, `Resources/…`, `designmap.xml`). The `folio/` parts and anything
    /// that is not XML are not documents' content and are skipped.
    pub fn feed(&mut self, part: &str, text: &str) {
        if part.starts_with("folio/") || !part.ends_with(".xml") {
            return;
        }
        for (slot, d) in self.counts.iter_mut().zip(&catalog().differences) {
            for m in &d.detect {
                slot.0 += text.matches(m.as_str()).count();
            }
            if slot.0 > 0 {
                for m in &d.subtract {
                    slot.1 += text.matches(m.as_str()).count();
                }
            }
        }
    }

    /// The differences the fed parts use, in catalogue order.
    pub fn hits(&self) -> Vec<Hit> {
        catalog()
            .differences
            .iter()
            .zip(&self.counts)
            .filter(|(d, (found, taken))| !d.detect.is_empty() && found > taken)
            .map(|(d, (found, taken))| Hit { id: d.id.clone(), kind: d.kind.clone(), area: d.area.clone(), count: found - taken })
            .collect()
    }
}

/// The differences used by the XML parts (name → text), for a document that is already open.
pub fn scan_parts<'a>(parts: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<Hit> {
    let mut scanner = Scanner::new();
    for (name, text) in parts {
        scanner.feed(name, text);
    }
    scanner.hits()
}

/// The differences used by an IDML package or `.folio` file on disk.
pub fn scan_package(package: &Path) -> Result<Vec<Hit>, String> {
    let mut scanner = Scanner::new();
    crate::import::for_each_part(package, &mut |name, text| scanner.feed(name, text))?;
    Ok(scanner.hits())
}

fn difference_json(d: &Difference, count: Option<usize>) -> Value {
    let mut v = json!({"id": d.id, "kind": d.kind, "area": d.area, "ko": d.ko, "en": d.en});
    if let Some(n) = count {
        v["count"] = json!(n);
    }
    v
}

/// The report block for one document, written into the import report and the IDML export report (`limits`) and answered by
/// the `knownLimits` query: `used` are the differences this document uses (with counts), `fixed` the most important
/// differences that cannot be checked in a document (and are not already in `used`), `doc` where the full list is.
pub fn report(hits: &[Hit]) -> Value {
    let cat = catalog();
    let used: Vec<Value> = hits
        .iter()
        .filter_map(|h| cat.differences.iter().find(|d| d.id == h.id).map(|d| difference_json(d, Some(h.count))))
        .collect();
    let mut top: Vec<&Difference> = cat.differences.iter().filter(|d| d.top.is_some() && !hits.iter().any(|h| h.id == d.id)).collect();
    top.sort_by_key(|d| d.top);
    json!({
        "format": "folio-limits", "version": cat.version,
        "used": used,
        "fixed": top.into_iter().map(|d| difference_json(d, None)).collect::<Vec<_>>(),
        "doc": cat.doc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_reads_and_every_detect_string_is_specific() {
        let cat = catalog();
        assert!(cat.differences.len() >= 40, "{}", cat.differences.len());
        for d in &cat.differences {
            assert!(cat.areas.iter().any(|a| a.id == d.area), "{}: area {}", d.id, d.area);
            assert!(!d.ko.is_empty() && !d.en.is_empty(), "{}", d.id);
            assert!(["approx", "unsupported", "unverified", "excluded"].contains(&d.kind.as_str()), "{}", d.kind);
            // a string every InDesign document holds would flag every document
            for m in &d.detect {
                assert!(m.len() >= 6, "{}: {m:?}", d.id);
            }
        }
        let tops: Vec<u32> = cat.differences.iter().filter_map(|d| d.top).collect();
        assert_eq!(tops.len(), 10, "{tops:?}");
    }

    #[test]
    fn a_document_reports_only_what_it_uses() {
        let plain = r#"<Document DOMVersion="20.5"><TOCStyle Self="TOCStyle/$ID/[Default]"/><TrapPreset Self="TrapPreset/$ID/kDefaultTrapStyleName"/><Page AppliedTrapPreset="TrapPreset/$ID/kDefaultTrapStyleName"/></Document>"#;
        assert!(scan_parts([("designmap.xml", plain)]).is_empty());
        let story = r#"<Story Self="u1"><StoryPreference StoryOrientation="Vertical"/><CharacterStyleRange KerningMethod="$ID/Optical"/><CharacterStyleRange KerningMethod="$ID/Optical"/></Story>"#;
        let spread = r#"<Spread><Rectangle><MathObject/></Rectangle><Page AppliedTrapPreset="TrapPreset/$ID/kDefaultTrapStyleName"/><Page AppliedTrapPreset="TrapPreset/MyTraps"/></Spread>"#;
        let hits = scan_parts([("Stories/Story_u1.xml", story), ("Spreads/Spread_u2.xml", spread), ("folio/manifest.json", "<MathObject/>")]);
        let got: Vec<(&str, usize)> = hits.iter().map(|h| (h.id.as_str(), h.count)).collect();
        assert_eq!(got, vec![("optical-kerning", 2), ("vertical-text", 1), ("math-shape", 1), ("traps-settings", 1)]);
    }

    #[test]
    fn the_report_keeps_the_top_differences_it_cannot_check_in_a_document() {
        let hits = scan_parts([("Stories/Story_u1.xml", r#"<StoryPreference StoryOrientation="Vertical"/>"#)]);
        let r = report(&hits);
        assert_eq!(r["used"][0]["id"], "vertical-text");
        assert_eq!(r["used"][0]["count"], 1);
        let fixed: Vec<&str> = r["fixed"].as_array().unwrap().iter().filter_map(|d| d["id"].as_str()).collect();
        assert!(fixed.contains(&"saved-lines") && fixed.contains(&"printer-direct"), "{fixed:?}");
        assert!(!fixed.contains(&"vertical-text"), "{fixed:?}");
        assert_eq!(r["doc"], "artifacts/finish-20261113/LIMITS.ko.md");
    }
}
