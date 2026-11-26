//! Preflight: the checks of InDesign's Preflight panel plus what printing houses check
//! before plating.
//!
//! ```ignore
//! let report = folio_preflight::preflight(&doc, &layout, &links, &Options::default());
//! for f in &report.findings { println!("{:?} {} p{:?} {}", f.severity, f.rule, f.page, f.message); }
//! ```
//!
//! Inputs are the parsed document, its text layout (overset text and missing fonts come
//! from there), and the [`LinkReport`] (found, missing and modified links, file header
//! facts). Nothing is read from disk here except, optionally, image samples for ink
//! coverage ([`Options::image_ink`]), which are cached per file identity.
//!
//! Every finding carries a stable rule id ([`rules`]), a severity, the document page index
//! (`Page::index`, `None` for masters and document-wide findings), the item id, a short
//! Korean message for the UI and machine-readable data.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

mod checks;
mod colors;
mod geom;
mod messages;
pub mod image_ink;
pub mod rules;
pub mod profiles;
pub mod scope;

pub use colors::Inks;
pub use folio_links::LinkReport;

use folio_model::Document;
use folio_model::layout::DocumentLayout;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable rule id, e.g. "LINK_MISSING" (see [`rules`]).
    pub rule: String,
    pub severity: Severity,
    /// Document page index (`Page::index`); `None` for masters, the pasteboard and
    /// document-wide findings.
    pub page: Option<usize>,
    /// Page name as InDesign shows it (e.g. "12"), when on a page.
    pub page_name: Option<String>,
    /// The page item (or graphic / story) the finding is about.
    pub item: Option<String>,
    /// Short Korean message for the UI.
    pub message: String,
    /// Stable presentation ID and arguments; legacy readers may omit these fields.
    #[serde(default, rename = "messageId")]
    pub message_id: String,
    #[serde(default, rename = "messageArgs")]
    pub message_args: serde_json::Value,
    /// Machine data (values, thresholds, names).
    pub data: serde_json::Value,
}

/// Font facts the layout cannot tell: which missing pairs have their family installed (the
/// style was substituted) and which face stood in. Built by the caller from its font store
/// (`folio-preflight` CLI: `font_facts`); `None` treats every pair as missing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FontFacts {
    /// (family, style, substitute face "Family Style") whose family is installed.
    pub substituted: Vec<(String, String, String)>,
    /// (family, style, fallback face) whose family is not installed.
    pub missing: Vec<(String, String, String)>,
}

/// [`FontFacts`] from the font store the layout used: a missing pair whose family is
/// installed had its style substituted; the face that stood in is named.
#[cfg(feature = "cli")]
pub fn font_facts(store: &folio_text::FontStore, doc: &Document, layout: &DocumentLayout) -> FontFacts {
    fn norm(s: &str) -> String {
        s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
    }
    let mut f = FontFacts::default();
    for (family, style) in &layout.missing_fonts {
        let r = store.resolve(family, style, &doc.font_table);
        let face = store.face_info(r.font).map(|i| format!("{} {}", i.family, i.style)).unwrap_or_else(|| "(대체 글꼴 없음)".into());
        let fam = norm(family);
        if store.faces().iter().any(|i| norm(&i.family) == fam) {
            f.substituted.push((family.clone(), style.clone(), face));
        } else {
            f.missing.push((family.clone(), style.clone(), face));
        }
    }
    f
}

/// Thresholds and output conditions. Defaults follow common printing-house limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub disabled_rules: Vec<String>,
    pub scope: folio_model::preflight::Settings,
    /// Output separates to CMYK (RGB, Lab and Indexed content is flagged).
    pub cmyk_output: bool,
    /// Effective resolution of colour and grayscale images: warn below, error below.
    pub image_ppi_warn: f64,
    pub image_ppi_error: f64,
    /// 1-bit images.
    pub bitmap_ppi_warn: f64,
    pub bitmap_ppi_error: f64,
    /// Total area coverage (percent).
    pub max_ink: f64,
    /// Text smaller than this (points) should use one ink.
    pub small_text_pt: f64,
    /// Strokes thinner than this (points) are hairlines.
    pub hairline_pt: f64,
    /// Required bleed (points); `None` = the document's bleed setting, or 3 mm when it has
    /// none.
    pub bleed_pt: Option<f64>,
    /// Distance from the trim inside which content is flagged (points).
    pub safety_margin_pt: f64,
    /// Check items on the pasteboard and master pages too (InDesign: off / on).
    pub include_pasteboard: bool,
    /// Sample linked CMYK images for ink coverage (reads image data; cached).
    pub image_ink: bool,
    /// Fonts: substituted vs missing, when known.
    pub fonts: Option<FontFacts>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            disabled_rules: Vec::new(),
            scope: Default::default(),
            cmyk_output: true,
            image_ppi_warn: 300.0,
            image_ppi_error: 200.0,
            bitmap_ppi_warn: 1200.0,
            bitmap_ppi_error: 800.0,
            max_ink: 300.0,
            small_text_pt: 12.0,
            hairline_pt: 0.25,
            bleed_pt: None,
            safety_margin_pt: 3.0 * 72.0 / 25.4,
            include_pasteboard: false,
            image_ink: false,
            fonts: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    /// Findings per rule id.
    pub by_rule: BTreeMap<String, usize>,
    /// Pages with at least one error.
    pub pages_with_errors: Vec<usize>,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub summary: Summary,
    /// What these checks could not establish (approximations, unread data).
    pub limitations: Vec<String>,
    /// Effective resolution of every placed raster graphic whose pixel size is known,
    /// keyed by graphic id (the owner of its link).
    #[serde(default)]
    pub images: BTreeMap<String, ImageFact>,
}

/// Effective resolution of one placed raster graphic; the resolution rule compares the
/// lower of the two.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageFact {
    /// Frame holding the graphic (the item findings name).
    pub frame: String,
    /// Document page index; `None` on the pasteboard or a master.
    pub page: Option<usize>,
    /// Effective ppi along x and y.
    pub ppi: [f64; 2],
}

/// Where an item sits on its document page, for showing it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// Document page index (`Page::index`).
    pub page: usize,
    /// Visual bounds (left, top, right, bottom) in points from the page's top-left corner.
    pub bounds: [f64; 4],
}

/// Page and page-relative bounds of every item on a document page, by item id, with the
/// same page assignment the checks use.
pub fn locate(doc: &Document, layout: &DocumentLayout) -> std::collections::HashMap<String, Location> {
    let g = geom::Doc::new(doc, layout);
    g.items
        .iter()
        .filter_map(|p| {
            let page = p.page?;
            let r = p.visual.or(p.bounds)?;
            let &(s, i) = g.page_at.get(&page)?;
            let pr = g.spreads[s].pages[i].rect;
            Some((p.item.id.clone(), Location { page, bounds: [r.left - pr.left, r.top - pr.top, r.right - pr.left, r.bottom - pr.top] }))
        })
        .collect()
}

/// Where each page item is, as the checks see it: (id, page name or master page name,
/// prints, visual bounds in spread coordinates). For comparisons with InDesign's
/// `parentPage`.
#[doc(hidden)]
// Keep the oracle-facing tuple API; its fields are documented immediately above.
#[allow(clippy::type_complexity)]
pub fn item_pages(doc: &Document, layout: &DocumentLayout) -> Vec<(String, Option<String>, bool, Option<[f64; 4]>)> {
    let g = geom::Doc::new(doc, layout);
    g.items
        .iter()
        .map(|p| {
            let name = p.page.and_then(|pg| g.spreads[p.spread].pages.iter().find(|b| b.index == pg)).map(|b| b.name.clone());
            (p.item.id.clone(), name, p.prints(), p.visual.or(p.bounds).map(|r| [r.left, r.top, r.right, r.bottom]))
        })
        .collect()
}

/// Runs every check.
pub fn preflight(doc: &Document, layout: &DocumentLayout, links: &LinkReport, options: &Options) -> Report {
    if options.scope.off {return Report::default();}
    let started = std::time::Instant::now();
    let g = geom::Doc::new(doc, layout);
    let mut ctx = checks::Ctx::new(&g, doc, layout, links, options);
    ctx.run();
    let checks::Ctx { findings, limitations, images, .. } = ctx;
    let mut findings = findings;
    findings.retain(|f|!options.disabled_rules.contains(&f.rule) && scope::finding(f,&g,doc,options));
    findings.sort_by(|a, b| {
        (a.page.unwrap_or(usize::MAX), a.severity, &a.rule, &a.item).cmp(&(b.page.unwrap_or(usize::MAX), b.severity, &b.rule, &b.item))
    });
    let mut s = Summary::default();
    let mut err_pages = std::collections::BTreeSet::new();
    for f in &findings {
        match f.severity {
            Severity::Error => {
                s.errors += 1;
                if let Some(p) = f.page {
                    err_pages.insert(p);
                }
            }
            Severity::Warning => s.warnings += 1,
            Severity::Info => s.infos += 1,
        }
        *s.by_rule.entry(f.rule.clone()).or_default() += 1;
    }
    s.pages_with_errors = err_pages.into_iter().collect();
    s.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    Report { findings, summary: s, limitations, images }
}
