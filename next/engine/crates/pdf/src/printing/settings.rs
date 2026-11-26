//! The settings a print preset holds, with their defaults and when each one applies.
//!
//! Measured in InDesign 2026 (21.6; 2025 not checked): the property list, defaults and enumerator codes come from the DOM object
//! `PrinterPreset` (97 properties); which properties apply in which state comes from reading every property of a temporary preset
//! after each change (`artifacts/print-c1-20261023/out/p4_deps.txt`); the control names and the summary text come from the
//! `Print Presets` / `Edit Print Preset` dialogs read through UI Automation. Rules: `next/oracle/rules/printing.ko.md`.
//! Keys are InDesign's own property names, so a saved preset reads like the DOM object. Lengths are points.
//! Decisions (not measured): a named printer is never saved (the dialog offers only "Default Printer" and "PostScript File", so a preset
//! moves between computers); the colour list of a non-PostScript printer has no separations; `allPrinterMarks` mirrors the five marks.
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tab { General, Setup, MarksAndBleed, Output, Graphics, ColorManagement, Advanced }

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::General, Tab::Setup, Tab::MarksAndBleed, Tab::Output, Tab::Graphics, Tab::ColorManagement, Tab::Advanced];
    pub fn id(self) -> &'static str {
        match self { Tab::General => "general", Tab::Setup => "setup", Tab::MarksAndBleed => "marksAndBleed", Tab::Output => "output", Tab::Graphics => "graphics", Tab::ColorManagement => "colorManagement", Tab::Advanced => "advanced" }
    }
    /// The tab's name in InDesign's dialog (English).
    pub fn en(self) -> &'static str {
        match self { Tab::General => "General", Tab::Setup => "Setup", Tab::MarksAndBleed => "Marks and Bleed", Tab::Output => "Output", Tab::Graphics => "Graphics", Tab::ColorManagement => "Color Management", Tab::Advanced => "Advanced" }
    }
}

/// One enumerator: Folio's id, InDesign's DOM code, and the dialog's English label.
#[derive(Clone, Copy, Debug)]
pub struct Opt { pub id: &'static str, pub dom: i64, pub en: &'static str }
const fn o(id: &'static str, dom: i64, en: &'static str) -> Opt { Opt { id, dom, en } }

#[derive(Clone, Copy, Debug)]
pub enum Kind { Bool, Int, Num, Length, Text, Enum(&'static [Opt]) }
#[derive(Clone, Copy, Debug)]
pub enum Def { B(bool), I(i64), N(f64), S(&'static str) }

#[derive(Clone, Copy, Debug)]
pub struct Field { pub key: &'static str, pub tab: Tab, pub kind: Kind, pub def: Def }

const PRINTER: &[Opt] = &[o("default", 0, "Default Printer"), o("postscriptFile", 1886611052, "PostScript® File")];
const SEQUENCE: &[Opt] = &[o("all", 1634495520, "All Pages"), o("even", 1702258030, "Even Pages Only"), o("odd", 1868850208, "Odd Pages Only")];
const LAYERS: &[Opt] = &[o("allLayers", 1634495564, "All Layers"), o("visibleLayers", 1986622284, "Visible Layers"), o("visiblePrintableLayers", 1987080780, "Visible & Printable Layers")];
const ORIENTATION: &[Opt] = &[o("portrait", 1751738216, "Portrait"), o("landscape", 2003395685, "Landscape"), o("reversePortrait", 1869771376, "Reverse Portrait"), o("reverseLandscape", 1869771372, "Reverse Landscape")];
const POSITION: &[Opt] = &[o("upperLeft", 1668183118, "Upper Left"), o("centerHorizontally", 1668183112, "Center Horizontally"), o("centerVertically", 1668183126, "Center Vertically"), o("centered", 1668183106, "Centered")];
const SCALE: &[Opt] = &[o("scaleWidthHeight", 1935898745, "Scale Width and Height"), o("scaleToFit", 1935897702, "Scale to Fit")];
const PER_PAGE: &[Opt] = &[o("k1x2", 1949399090, "1x2"), o("k2x2", 1949464626, "2x2"), o("k3x3", 1949530163, "3x3"), o("k4x4", 1949595700, "4x4"), o("k5x5", 1949661237, "5x5"), o("k6x6", 1949726774, "6x6"), o("k7x7", 1949792311, "7x7")];
const TILING: &[Opt] = &[o("auto", 1635019116, "Auto"), o("autoJustified", 1634366324, "Auto Justified"), o("manual", 1835955308, "Manual")];
const MARK_TYPE: &[Opt] = &[o("default", 1147563124, "Default"), o("jMarkWithCircle", 1785558883, "J Mark with Circle"), o("jMarkWithoutCircle", 1785556579, "J Mark without Circle")];
const MARK_WEIGHT: &[Opt] = &[o("p125pt", 825374064, "0.125 pt"), o("p25pt", 842346608, "0.25 pt"), o("p50pt", 892350576, "0.5 pt"), o("p05mm", 808807789, "0.05 mm"), o("p07mm", 808938861, "0.07 mm"),
    o("p10mm", 825257325, "0.10 mm"), o("p15mm", 825585005, "0.15 mm"), o("p20mm", 842034541, "0.20 mm"), o("p30mm", 858811757, "0.30 mm")];
const COLOR: &[Opt] = &[o("compositeLeaveUnchanged", 1668107349, "Composite Leave Unchanged"), o("compositeGray", 1668116583, "Composite Gray"), o("compositeRgb", 1668108866, "Composite RGB"),
    o("compositeCmyk", 1668105035, "Composite CMYK"), o("separations", 1936027745, "Separations"), o("inRipSeparations", 1919512691, "In-RIP Separations")];
const TRAPPING: &[Opt] = &[o("off", 1330005536, "Off"), o("applicationBuiltin", 1114199152, "Application Built-In"), o("adobeInRip", 1919512660, "Adobe In-RIP")];
const FLIP: &[Opt] = &[o("none", 1852796517, "None"), o("horizontal", 1752134266, "Horizontal"), o("vertical", 1986359924, "Vertical"), o("horizontalAndVertical", 1215977068, "Horizontal & Vertical")];
const IMAGE_DATA: &[Opt] = &[o("allImageData", 1853058416, "All"), o("optimizedSubsampling", 1869640563, "Optimized Subsampling"), o("proxyImageData", 1819243130, "Proxy"), o("none", 1852796517, "None")];
const FONTS: &[Opt] = &[o("none", 1852796517, "None"), o("complete", 2003332197, "Complete"), o("subset", 1768842098, "Subset"), o("subsetLarge", 1818325607, "Subset Large")];
const PS_LEVEL: &[Opt] = &[o("level2", 1347636274, "Level 2"), o("level3", 1347636275, "Level 3")];
const DATA_FORMAT: &[Opt] = &[o("ascii", 1095975753, "ASCII"), o("binary", 1114534521, "Binary")];
const SOURCE: &[Opt] = &[o("useDocument", 1967419235, "Document"), o("proofSpace", 1886548848, "Proof")];
const INTENT: &[Opt] = &[o("useColorSettings", 1380541299, "Use Color Settings"), o("perceptual", 1380544611, "Perceptual"), o("saturation", 1380545377, "Saturation"),
    o("relativeColorimetric", 1380545123, "Relative Colorimetric"), o("absoluteColorimetric", 1380540771, "Absolute Colorimetric")];

use Def::*;
use Kind::*;
use Tab::*;
const fn f(key: &'static str, tab: Tab, kind: Kind, def: Def) -> Field { Field { key, tab, kind, def } }

/// Every setting, in the order of InDesign's dialog. `name` is the preset's name and is kept beside these.
pub const FIELDS: &[Field] = &[
    f("printer", General, Enum(PRINTER), S("default")), f("ppd", General, Text, S("")),
    f("copies", General, Int, I(1)), f("collating", General, Bool, B(false)), f("reverseOrder", General, Bool, B(false)),
    f("sequence", General, Enum(SEQUENCE), S("all")), f("printSpreads", General, Bool, B(false)), f("printMasterPages", General, Bool, B(false)),
    f("printLayers", General, Enum(LAYERS), S("visiblePrintableLayers")), f("printNonprinting", General, Bool, B(false)),
    f("printBlankPages", General, Bool, B(false)), f("printGuidesGrids", General, Bool, B(false)),
    f("paperSize", Setup, Text, S("Letter")), f("paperWidth", Setup, Length, N(612.0)), f("paperHeight", Setup, Length, N(792.0)),
    f("printPageOrientation", Setup, Enum(ORIENTATION), S("portrait")), f("paperOffset", Setup, Length, N(0.0)), f("paperGap", Setup, Length, N(0.0)),
    f("paperTransverse", Setup, Bool, B(false)), f("scaleMode", Setup, Enum(SCALE), S("scaleWidthHeight")), f("scaleWidth", Setup, Num, N(100.0)),
    f("scaleHeight", Setup, Num, N(100.0)), f("scaleProportional", Setup, Bool, B(true)), f("pagePosition", Setup, Enum(POSITION), S("upperLeft")),
    f("thumbnails", Setup, Bool, B(false)), f("thumbnailsPerPage", Setup, Enum(PER_PAGE), S("k1x2")), f("tile", Setup, Bool, B(false)),
    f("tilingType", Setup, Enum(TILING), S("auto")), f("tilingOverlap", Setup, Length, N(108.0)),
    f("cropMarks", MarksAndBleed, Bool, B(false)), f("bleedMarks", MarksAndBleed, Bool, B(false)), f("registrationMarks", MarksAndBleed, Bool, B(false)),
    f("colorBars", MarksAndBleed, Bool, B(false)), f("pageInformationMarks", MarksAndBleed, Bool, B(false)), f("allPrinterMarks", MarksAndBleed, Bool, B(false)),
    f("markType", MarksAndBleed, Enum(MARK_TYPE), S("default")), f("markLineWeight", MarksAndBleed, Enum(MARK_WEIGHT), S("p25pt")),
    f("markOffset", MarksAndBleed, Length, N(6.0)), f("useDocumentBleedToPrint", MarksAndBleed, Bool, B(true)), f("bleedTop", MarksAndBleed, Length, N(0.0)),
    f("bleedBottom", MarksAndBleed, Length, N(0.0)), f("bleedInside", MarksAndBleed, Length, N(0.0)), f("bleedOutside", MarksAndBleed, Length, N(0.0)),
    f("bleedChain", MarksAndBleed, Bool, B(false)), f("includeSlugToPrint", MarksAndBleed, Bool, B(false)),
    f("colorOutput", Output, Enum(COLOR), S("compositeRgb")), f("textAsBlack", Output, Bool, B(false)), f("trapping", Output, Enum(TRAPPING), S("off")),
    f("flip", Output, Enum(FLIP), S("none")), f("negative", Output, Bool, B(false)), f("screening", Output, Text, S("Default")),
    f("simulateOverprint", Output, Bool, B(false)), f("printCyan", Output, Bool, B(true)), f("printMagenta", Output, Bool, B(true)),
    f("printYellow", Output, Bool, B(true)), f("printBlack", Output, Bool, B(true)),
    f("cyanAngle", Output, Num, N(71.5651)), f("cyanFrequency", Output, Num, N(63.2456)), f("magentaAngle", Output, Num, N(18.4349)),
    f("magentaFrequency", Output, Num, N(63.2456)), f("yellowAngle", Output, Num, N(0.0)), f("yellowFrequency", Output, Num, N(66.6667)),
    f("blackAngle", Output, Num, N(45.0)), f("blackFrequency", Output, Num, N(70.7107)),
    f("sendImageData", Graphics, Enum(IMAGE_DATA), S("optimizedSubsampling")), f("fontDownloading", Graphics, Enum(FONTS), S("complete")),
    f("downloadPPDFonts", Graphics, Bool, B(true)), f("postscriptLevel", Graphics, Enum(PS_LEVEL), S("level3")), f("dataFormat", Graphics, Enum(DATA_FORMAT), S("binary")),
    f("sourceSpace", ColorManagement, Enum(SOURCE), S("useDocument")), f("profile", ColorManagement, Text, S("")), f("intent", ColorManagement, Enum(INTENT), S("useColorSettings")),
    f("bitmapPrinting", Advanced, Bool, B(false)), f("bitmapResolution", Advanced, Int, I(300)), f("opiImageReplacement", Advanced, Bool, B(false)),
    f("omitEPS", Advanced, Bool, B(false)), f("omitPDF", Advanced, Bool, B(false)), f("omitBitmaps", Advanced, Bool, B(false)),
    f("flattenerPresetName", Advanced, Text, S("[Medium Resolution]")), f("ignoreSpreadOverrides", Advanced, Bool, B(false)),
];

/// The line weight of a mark in points (`markLineWeight`: the point sizes as they are, the millimetre sizes converted).
pub fn mark_weight_points(id: &str) -> Option<f64> {
    let mm = |v: f64| v * 72.0 / 25.4;
    Some(match id {
        "p125pt" => 0.125, "p25pt" => 0.25, "p50pt" => 0.5,
        "p05mm" => mm(0.05), "p07mm" => mm(0.07), "p10mm" => mm(0.10), "p15mm" => mm(0.15), "p20mm" => mm(0.20), "p30mm" => mm(0.30),
        _ => return None,
    })
}

pub fn field(key: &str) -> Option<&'static Field> { FIELDS.iter().find(|f| f.key == key) }

pub fn default_value(f: &Field) -> Value {
    match f.def { B(b) => json!(b), I(i) => json!(i), N(n) => json!(n), S(s) => json!(s) }
}

/// Every key with its default (what `[Default]` holds).
pub fn defaults() -> Map<String, Value> {
    FIELDS.iter().map(|f| (f.key.to_string(), default_value(f))).collect()
}

/// A value checked against its field; the number is clamped to the field's range, nothing else is changed.
pub fn check(f: &Field, v: &Value) -> Result<Value, String> {
    let bad = || format!("{}: invalid value {v}", f.key);
    Ok(match f.kind {
        Bool => json!(v.as_bool().ok_or_else(bad)?),
        Int => json!(v.as_i64().filter(|n| (1..=9999).contains(n) || f.key == "bitmapResolution" && *n >= 1).ok_or_else(bad)?),
        Num | Length => json!(v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(bad)?),
        Text => json!(v.as_str().ok_or_else(bad)?),
        Enum(options) => json!(v.as_str().filter(|s| options.iter().any(|o| o.id == *s)).ok_or_else(bad)?),
    })
}

const MARKS: [&str; 5] = ["cropMarks", "bleedMarks", "registrationMarks", "colorBars", "pageInformationMarks"];

/// `values` made complete and consistent: legacy keys renamed, missing keys defaulted, unknown keys refused, `allPrinterMarks` recomputed from the five marks.
pub fn normalize(values: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let mut out = defaults();
    for (k, v) in values {
        let key = if k == "spreads" { "printSpreads" } else { k.as_str() }; // saved before the full settings existed
        let f = field(key).ok_or_else(|| format!("unknown print setting {k}"))?;
        out.insert(key.to_string(), check(f, v)?);
    }
    let all = MARKS.iter().all(|m| out[*m] == json!(true));
    out.insert("allPrinterMarks".into(), json!(all));
    Ok(out)
}

fn text<'a>(v: &'a Map<String, Value>, k: &str) -> &'a str { v.get(k).and_then(Value::as_str).unwrap_or("") }
fn flag(v: &Map<String, Value>, k: &str) -> bool { v.get(k).and_then(Value::as_bool).unwrap_or(false) }

// What does not apply, measured on a temporary preset in InDesign 2026 (p4_deps.txt): the default printer, then a PostScript file
// with a PPD, then the changes each setting makes on top of that.
const NA_DEFAULT_PRINTER: &[&str] = &["bitmapResolution", "blackAngle", "blackFrequency", "collating", "compositeAngle", "compositeFrequency", "crd", "cyanAngle", "cyanFrequency",
    "dataFormat", "downloadPPDFonts", "flip", "fontDownloading", "intent", "magentaAngle", "magentaFrequency", "negative", "omitBitmaps", "omitEPS", "omitPDF", "opiImageReplacement",
    "paperGap", "paperHeight", "paperOffset", "paperTransverse", "paperWidth", "postscriptLevel", "ppd", "printBlack", "printCyan", "printMagenta", "printYellow", "screening",
    "thumbnailsPerPage", "tilingOverlap", "tilingType", "trapping", "yellowAngle", "yellowFrequency"];
const NA_PS_WITH_PPD: &[&str] = &["bitmapPrinting", "bitmapResolution", "blackAngle", "blackFrequency", "collating", "compositeAngle", "compositeFrequency", "crd", "cyanAngle", "cyanFrequency",
    "intent", "magentaAngle", "magentaFrequency", "negative", "paperGap", "paperHeight", "paperOffset", "paperTransverse", "paperWidth", "printBlack", "printCyan", "printGuidesGrids",
    "printMagenta", "printYellow", "textAsBlack", "thumbnailsPerPage", "tilingOverlap", "tilingType", "trapping", "yellowAngle", "yellowFrequency"];
/// Added when the PostScript file has no PPD ("Device Independent").
const NA_PS_NO_PPD_EXTRA: &[&str] = &["downloadPPDFonts", "flip", "pagePosition", "paperSize", "printPageOrientation", "scaleHeight", "scaleMode", "scaleProportional", "scaleWidth",
    "screening", "thumbnails", "tile"];
const NA_SEPARATED: &[&str] = &["printBlankPages", "simulateOverprint", "thumbnails"];
const SEPARATED_APPLIES: &[&str] = &["blackAngle", "blackFrequency", "cyanAngle", "cyanFrequency", "magentaAngle", "magentaFrequency", "negative", "printBlack", "printCyan", "printMagenta",
    "printYellow", "trapping", "yellowAngle", "yellowFrequency"];
const NA_THUMBNAILS: &[&str] = &["allPrinterMarks", "bleedBottom", "bleedInside", "bleedMarks", "bleedOutside", "bleedTop", "colorBars", "cropMarks", "includeSlugToPrint", "markLineWeight",
    "markOffset", "markType", "pageInformationMarks", "pagePosition", "registrationMarks", "scaleHeight", "scaleMode", "scaleProportional", "scaleWidth", "tile", "useDocumentBleedToPrint"];

/// The keys that do not apply to `values` (an InDesign property that is "not applicable in the current state").
pub fn not_applicable(values: &Map<String, Value>) -> BTreeSet<&'static str> {
    let ps = text(values, "printer") == "postscriptFile";
    let mut na: BTreeSet<&'static str> = if ps { NA_PS_WITH_PPD } else { NA_DEFAULT_PRINTER }.iter().copied().collect();
    if ps && text(values, "ppd").is_empty() {
        na.extend(NA_PS_NO_PPD_EXTRA);
    }
    match text(values, "colorOutput") {
        "compositeLeaveUnchanged" => { na.insert("simulateOverprint"); }
        "compositeGray" => { na.insert("profile"); na.remove("negative"); }
        "separations" | "inRipSeparations" => {
            na.extend(NA_SEPARATED);
            for k in SEPARATED_APPLIES { na.remove(k); }
        }
        _ => {}
    }
    if flag(values, "tile") {
        na.insert("pagePosition");
        na.insert("thumbnails");
        na.remove("tilingOverlap");
        na.remove("tilingType");
    }
    if flag(values, "thumbnails") {
        na.extend(NA_THUMBNAILS);
        na.remove("thumbnailsPerPage");
    }
    if text(values, "scaleMode") == "scaleToFit" {
        na.extend(["scaleHeight", "scaleProportional", "scaleWidth"]);
    }
    if values.get("copies").and_then(Value::as_i64).unwrap_or(1) > 1 {
        na.remove("collating");
    }
    if !ps && flag(values, "bitmapPrinting") {
        na.remove("bitmapResolution");
    }
    na
}

/// What one colour choice offers: a printer that is not a PostScript file cannot separate (decision, not measured).
pub fn allowed(values: &Map<String, Value>, key: &str, id: &str) -> bool {
    !(key == "colorOutput" && text(values, "printer") != "postscriptFile" && matches!(id, "separations" | "inRipSeparations"))
}

/// The value as InDesign's DOM reports it: a bool/number/text as is, an enumerator as its code (used to compare with the measurements).
pub fn dom_value(f: &Field, v: &Value) -> String {
    match (f.kind, v) {
        (Enum(options), Value::String(s)) => options.iter().find(|o| o.id == s).map_or(s.clone(), |o| o.dom.to_string()),
        (_, Value::Bool(b)) => b.to_string(),
        (_, Value::Number(n)) => n.as_f64().map_or(n.to_string(), |x| format!("{x}")),
        (Text, Value::String(s)) if f.key == "profile" && s.is_empty() => "1967419235".into(),
        (_, Value::String(s)) => s.clone(),
        _ => v.to_string(),
    }
}

fn opt_en(options: &[Opt], id: &str) -> String { options.iter().find(|o| o.id == id).map_or(id.to_string(), |o| o.en.to_string()) }
fn pica(pt: f64) -> String {
    let whole = (pt / 12.0).floor();
    let rest = pt - whole * 12.0;
    let rest = (rest * 1000.0).round() / 1000.0;
    format!("{whole}p{rest}")
}
fn paper_points(name: &str) -> Option<[f64; 2]> {
    Some(match name { "Letter" => [612.0, 792.0], "Legal" => [612.0, 1008.0], "Tabloid" => [792.0, 1224.0], "A3" => [841.89, 1190.551], "A4" => [595.276, 841.89], "A5" => [419.528, 595.276], _ => return None })
}

/// The "Preset Settings" text of InDesign's `Print Presets` dialog (English), for `values`. `document_profile` is the open document's RGB
/// profile name; the dialog reads it from the document. Pure text of the measured format; the screen builds its own from the same rows.
pub fn summary_en(name: &str, values: &Map<String, Value>, document_profile: &str, default_printer: &str) -> String {
    let na = not_applicable(values);
    let g = |k: &str| values.get(k).cloned().unwrap_or(Value::Null);
    let fld = |k: &str| field(k).expect("field");
    let onoff = |k: &str| if flag(values, k) { "On" } else { "Off" }.to_string();
    let val = |k: &str| -> String {
        if na.contains(k) { return "N/A".into(); }
        match (fld(k).kind, g(k)) {
            (Enum(options), Value::String(s)) => opt_en(options, &s),
            (Bool, _) => onoff(k),
            (Length, Value::Number(n)) => pica(n.as_f64().unwrap_or(0.0)),
            (_, Value::Number(n)) => n.to_string(),
            (_, Value::String(s)) => s,
            _ => String::new(),
        }
    };
    let ps = text(values, "printer") == "postscriptFile";
    let paper = paper_points(text(values, "paperSize"));
    let mut rows: Vec<(Tab, Vec<(String, String)>)> = Vec::new();
    let r = |l: &str, v: String| (l.to_string(), v);
    rows.push((General, vec![
        r("Copies", val("copies")), r("Collate", val("collating")), r("Reverse Order", val("reverseOrder")), r("Pages", "N/A".into()), r("Sequence", val("sequence")),
        r("Spreads", val("printSpreads")), r("Print Parent Pages", val("printMasterPages")), r("Print Layers", val("printLayers")),
        r("Print Non-printing Objects", val("printNonprinting")), r("Print Blank Pages", val("printBlankPages")), r("Print Visible Guides and Baseline Grids", val("printGuidesGrids"))]));
    let scaling = if na.contains("scaleMode") { "N/A".to_string() } else if text(values, "scaleMode") == "scaleToFit" { "Scale to Fit".into() } else { format!("{}%", values["scaleWidth"].as_f64().unwrap_or(100.0)) };
    rows.push((Setup, vec![
        r("Paper Size", val("paperSize")),
        r("Paper Width", paper.map_or("N/A".into(), |p| pica(p[0]))), r("Paper Height", paper.map_or("N/A".into(), |p| pica(p[1]))),
        r("Page Orientation", val("printPageOrientation")), r("Paper Offset", val("paperOffset")), r("Paper Gap", val("paperGap")), r("Transverse", val("paperTransverse")),
        r("Scaling", scaling), r("Constrain Proportions", val("scaleProportional")), r("Page Position", val("pagePosition")), r("Thumbnails", val("thumbnails")),
        r("Tiling", if flag(values, "tile") { "On".into() } else { "Off".into() })]));
    rows.push((MarksAndBleed, vec![
        r("Crop Marks", val("cropMarks")), r("Bleed Marks", val("bleedMarks")), r("Registration Marks", val("registrationMarks")), r("Color Bars", val("colorBars")),
        r("Page Information", val("pageInformationMarks")), r("Printer Mark Type", val("markType")), r("Crop Mark Weight", val("markLineWeight")),
        r("Mark Offset from Page", val("markOffset")), r("Use Document Bleed Settings", val("useDocumentBleedToPrint")), r("Bleed Top", val("bleedTop")),
        r("Bleed Bottom", val("bleedBottom")), r("Bleed Left", val("bleedInside")), r("Bleed Right", val("bleedOutside")), r("Include Slug Area", val("includeSlugToPrint"))]));
    rows.push((Output, vec![
        r("Color", val("colorOutput")), r("Text As Black", val("textAsBlack")), r("Trapping", val("trapping")), r("Flip", val("flip")), r("Negative", val("negative")),
        r("Screening", val("screening")), r("Simulate Overprint", val("simulateOverprint"))]));
    rows.push((Graphics, vec![
        r("Send Data", val("sendImageData")), r("Download", val("fontDownloading")), r("Download PPD Fonts", val("downloadPPDFonts")), r("PostScript®", val("postscriptLevel")),
        r("Data Format", val("dataFormat"))]));
    rows.push((ColorManagement, vec![
        r("Document Profile", document_profile.to_string()), r("Color Handling", "Let InDesign Determine Colors".into()),
        r("Printer Profile", if text(values, "profile").is_empty() { "Document RGB".into() } else { text(values, "profile").to_string() }), r("Preserve RGB Numbers", "Off".into()),
        r("Proof Profile", "N/A".into()), r("Simulate Paper Color", "N/A".into())]));
    rows.push((Advanced, vec![
        r("Print as Bitmap", val("bitmapPrinting")), r("Bitmap Resolution", val("bitmapResolution")), r("OPI Image Replacement", val("opiImageReplacement")), r("EPS", val("omitEPS")),
        r("PDF", val("omitPDF")), r("Bitmap Images", val("omitBitmaps")), r("Transparency Flattener Preset", "N/A".into()), r("Ignore Spread Overrides", "N/A".into())]));
    let mut out = format!("Print Preset: {name}\r\nPrinter: {default_printer}\r\nPPD: N/A\r\nPPD File: N/A\r\n\r\n");
    if ps { out = format!("Print Preset: {name}\r\nPrinter: PostScript® File\r\nPPD: {}\r\nPPD File: N/A\r\n\r\n", if text(values, "ppd").is_empty() { "Device Independent" } else { text(values, "ppd") }); }
    for (tab, list) in rows {
        out.push_str(tab.en());
        out.push_str("\r\n");
        for (l, v) in list { out.push_str(&format!("    {l}: {v}\r\n")); }
        out.push_str("\r\n");
    }
    out
}

/// What a screen needs to draw the dialog: every field (tab, kind, options, default) and what does not apply to `values`.
pub fn schema(values: &Map<String, Value>) -> Value {
    let fields: Vec<Value> = FIELDS.iter().map(|f| {
        let (kind, options) = match f.kind {
            Bool => ("bool", None), Int => ("int", None), Num => ("num", None), Length => ("length", None), Text => ("text", None),
            Enum(os) => ("enum", Some(os.iter().filter(|o| allowed(values, f.key, o.id)).map(|o| json!({"id": o.id, "en": o.en})).collect::<Vec<_>>())),
        };
        json!({"key": f.key, "tab": f.tab.id(), "kind": kind, "options": options, "default": default_value(f)})
    }).collect();
    json!({"tabs": Tab::ALL.iter().map(|t| t.id()).collect::<Vec<_>>(), "fields": fields, "notApplicable": not_applicable(values).into_iter().collect::<Vec<_>>(), "values": values})
}

/// `values` with one setting changed: the aggregate `allPrinterMarks` sets the five marks (InDesign's "All Printer's Marks" check box).
pub fn set(values: &Map<String, Value>, key: &str, value: &Value) -> Result<Map<String, Value>, String> {
    let mut next = normalize(values)?;
    let f = field(key).ok_or_else(|| format!("unknown print setting {key}"))?;
    let v = check(f, value)?;
    if key == "allPrinterMarks" {
        for m in MARKS { next.insert(m.to_string(), v.clone()); }
    }
    next.insert(key.to_string(), v);
    normalize(&next)
}
