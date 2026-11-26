//! IDML package → `folio_model::Document`.
//!
//! ```no_run
//! let doc = folio_idml::read_package(
//!     std::path::Path::new("document.idml"),
//!     Some(std::path::Path::new("result.json")),
//! )?;
//! # Ok::<(), folio_idml::Error>(())
//! ```
//!
//! The reader keeps IDML semantics (points, `ItemTransform`s, inner-coordinate paths) and
//! resolves styles fully: every paragraph and run carries final attributes. Anything not
//! modelled becomes an aggregated entry in `Document::warnings`. Input is never modified and
//! nothing outside the package and the reader's preview folder is opened.

#![cfg_attr(not(test), warn(clippy::unwrap_used))]

mod color;
mod incremental;
pub mod grep;
mod grep_engine;
pub mod nested;
pub mod docprefs;
pub mod chapter;
pub mod editorial;
pub mod footnotes;
pub mod endnotes;
pub mod fitting;
pub mod toc;
pub mod hyperlinks;
pub mod conditions;
pub mod pdfcomments;
mod indexes;
mod traps;
mod clipping;
mod graphic_layers;
mod items;
mod mathml;
mod package;
mod par;
mod props;
pub mod saved;
mod strokes;
pub mod preflight;
mod styles;
pub mod style_identity;
pub mod style_values;
pub mod opentype;
pub mod listprops;
pub mod paragraph_decorations;
pub mod character_lines;
pub mod parcomp;
pub mod tabstops;
pub mod paragraph_options;
pub mod paragraph_columns;
pub mod optical_margin;
pub mod datamerge;
pub mod datamerge_expr;
mod tables;
mod text;
mod text_grep;
pub mod textpath;
mod warn;
mod xml;

pub use incremental::ReadBase;
pub use text::story_chars;

use folio_model::{Document, FontInfo, Layer, PageItem, Paint, SourceInfo, Story, TextPreferences};
use items::ItemReader;
use props::{Props, clean_name};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::mpsc::SyncSender;
use warn::Warnings;
use xml::Element;

/// Why a package could not be read at all.
#[derive(Debug)]
pub enum Error {
    /// The file (or the reader's result.json) could not be read.
    Io(String),
    /// Not a usable IDML ZIP package (bad archive, missing designmap, wrong mimetype).
    Package(String),
    /// A package part is not well-formed XML.
    Xml(String),
    /// A part exceeds a safety limit.
    Limit(String),
    /// The reader's result.json is not valid JSON.
    ReaderResult(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(m) => write!(f, "I/O error: {m}"),
            Error::Package(m) => write!(f, "invalid IDML package: {m}"),
            Error::Xml(m) => write!(f, "invalid XML: {m}"),
            Error::Limit(m) => write!(f, "limit exceeded: {m}"),
            Error::ReaderResult(m) => write!(f, "invalid reader result: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Reads an IDML package; `reader_result` is the INDD reader's `result.json` (graphic
/// previews and reader warnings), whose preview files must sit next to it.
pub fn read_package(idml: &Path, reader_result: Option<&Path>) -> Result<Document, Error> {
    let data = match reader_result {
        Some(rp) => ReaderData::from_result(rp)?,
        None => ReaderData::default(),
    };
    read_package_with(idml, data)
}

/// A placed file's location to use instead of the package's `LinkResourceURI` (a `.folio`
/// resolves its project references on this machine). `stamp`, when set, replaces the
/// recorded size and time the link panel compares with the file: a `.folio` decides
/// sameness by content hash and states the outcome this way.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkOverride {
    pub uri: String,
    pub stamp: Option<(Option<u64>, Option<f64>)>,
}

/// What comes with a package from outside it: the INDD reader's output, or what a `.folio`'s
/// manifest resolves on this machine.
#[derive(Clone, Debug, Default)]
pub struct ReaderData {
    /// The reader that wrote the package (`result.json` `parser`).
    pub parser: Option<String>,
    /// Graphic `Self` → preview file (absolute).
    pub previews: HashMap<String, String>,
    /// The saved composition sidecar (`folio-saved-lines`) as JSON.
    pub saved_lines: Option<serde_json::Value>,
    /// `Link` `Self` → where the file is on this machine.
    pub links: HashMap<String, LinkOverride>,
    /// Graphic `Self` → its embedded file, extracted (`.folio` assets).
    pub embedded: HashMap<String, std::path::PathBuf>,
    /// Reader warnings to carry into the document.
    pub warnings: Vec<String>,
}

impl ReaderData {
    /// The INDD reader's `result.json` and the files next to it.
    pub fn from_result(rp: &Path) -> Result<ReaderData, Error> {
        let text = std::fs::read_to_string(rp).map_err(|e| Error::Io(format!("{}: {e}", rp.display())))?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| Error::ReaderResult(format!("{}: {e}", rp.display())))?;
        let dir = rp.parent().unwrap_or(Path::new("."));
        let mut w = Warnings::default();
        let previews = items::load_previews(&json, dir, &mut w);
        let saved_lines = saved::load_value(&json, dir, &mut w);
        let mut warnings = w.into_strings();
        for m in json.get("warnings").and_then(|v| v.as_array()).into_iter().flatten() {
            if let Some(s) = m.as_str() {
                warnings.push(format!("reader: {s}"));
            }
        }
        Ok(ReaderData {
            parser: json.get("parser").and_then(|p| p.as_str()).map(str::to_owned),
            previews,
            saved_lines,
            links: HashMap::new(),
            embedded: HashMap::new(),
            warnings,
        })
    }
}

/// Reads an IDML package (or a `.folio`, which holds one) with data from outside it.
pub fn read_package_with(idml: &Path, data: ReaderData) -> Result<Document, Error> {
    let bytes = std::fs::read(idml).map_err(|e| Error::Io(format!("{}: {e}", idml.display())))?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let path = idml.to_path_buf();
    // The reading thread hands the document over first and frees its parse trees (≈10⁵
    // small allocations) afterwards, off the caller's time. A panic there becomes an error.
    let spawned = std::thread::Builder::new()
        .name("folio-idml".into())
        .spawn(move || read_and_send(bytes, &path, &data, &tx));
    match spawned {
        Ok(_) => rx.recv().unwrap_or_else(|_| Err(Error::Package("reader thread stopped unexpectedly".into()))),
        Err(e) => Err(Error::Io(format!("cannot start reader thread: {e}"))),
    }
}

/// Package data for an editable session. Preserve every editable part while sharing the
/// initial model read with the incremental reader.
pub struct EditablePackage {
    pub document: Document,
    pub parts: BTreeMap<String, String>,
    pub base: Option<ReadBase>,
}

pub fn read_editable_package(idml: &Path, edit_path: &Path, data: &ReaderData, incremental: bool) -> Result<EditablePackage, Error> {
    let bytes = std::fs::read(idml).map_err(|e| Error::Io(format!("{}: {e}", idml.display())))?;
    let texts = package::decompress(&bytes)?;
    let (document, facts) = {
        let pkg = package::parse(&texts)?;
        build_with_facts(&pkg, idml, bytes.len() as u64, data)?
    };
    let (parts, same_parts) = texts.into_editor_parts(&bytes)?;
    let base = if !incremental {
        None
    } else if same_parts {
        let mut for_edit = document.clone();
        for_edit.source.path = std::path::absolute(edit_path).unwrap_or_else(|_| edit_path.to_path_buf()).to_string_lossy().into_owned();
        for_edit.source.bytes = parts.values().map(|text| text.len() as u64).sum();
        Some(ReadBase::from_document(for_edit, facts, edit_path, data))
    } else {
        Some(ReadBase::read(&parts, edit_path, data)?)
    };
    Ok(EditablePackage { document, parts, base })
}

/// All parts the editor preserves, excluding the private `.folio` data.
pub fn read_editor_parts(file: &Path) -> Result<BTreeMap<String, String>, Error> {
    let file = std::fs::File::open(file).map_err(|e| Error::Io(format!("{}: {e}", file.display())))?;
    package::read_editor_parts(std::io::BufReader::new(file))
}

/// Reads a package whose parts are in memory (an editor's), by path; `idml` names the
/// document (its folder is where relative things are looked for).
pub fn read_parts(parts: &BTreeMap<String, String>, idml: &Path, data: &ReaderData) -> Result<Document, Error> {
    let texts = package::from_parts(parts)?;
    let bytes = parts.values().map(|t| t.len() as u64).sum();
    let pkg = package::parse(&texts)?;
    build(&pkg, idml, bytes, data)
}

fn read_and_send(bytes: Vec<u8>, idml: &Path, data: &ReaderData, tx: &SyncSender<Result<Document, Error>>) {
    let texts = match package::decompress(&bytes) {
        Ok(t) => t,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };
    let byte_len = bytes.len() as u64;
    drop(bytes);
    let pkg = match package::parse(&texts) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };
    let _ = tx.send(build(&pkg, idml, byte_len, data));
    // `pkg` and `texts` are dropped here, after the caller has its document.
}

/// A date of the package's XMP packet (`xmp:CreateDate`, `xmp:ModifyDate`) as written, as an element or as an attribute; empty
/// without one (the text variables of dates then keep the result the file stores).
fn xmp_date(xmp: &str, name: &str) -> String {
    for open in [format!(":{name}>"), format!(":{name}=\"")] {
        if let Some(at) = xmp.find(&open) {
            let rest = &xmp[at + open.len()..];
            let end = rest.find(['<', '"']).unwrap_or(rest.len());
            return rest[..end].trim().to_owned();
        }
    }
    String::new()
}

/// What a full read learns beyond the document that an incremental update (`ReadBase`) needs.
struct Facts {
    /// The saved-lines sidecar as parsed (None: there is none or it was left out).
    sidecar: Option<saved::Sidecar>,
    /// Each part's `TextPath` child -> owning item pairs (what `textpath::FrameIds` holds, by part).
    text_paths: BTreeMap<String, Vec<(String, String)>>,
}

/// The document from parsed parts.
fn build(pkg: &package::Package<'_>, idml: &Path, byte_len: u64, data: &ReaderData) -> Result<Document, Error> {
    build_with_facts(pkg, idml, byte_len, data).map(|(doc, _)| doc)
}

/// The facts of the document that come from outside the spread and story parts: read the same way by the full read and by an
/// incremental update of it.
struct Globals<'a> {
    colors: color::Colors,
    sheet: styles::StyleSheet<'a>,
    stroke_styles: strokes::StrokeStyles,
    global_light: (f64, f64),
    view: folio_model::ViewPreferences,
}

fn globals<'a>(dm: &Element<'a>, graphic: &[&'a Element<'a>], styles_parts: &[&'a Element<'a>], prefs_parts: &[&'a Element<'a>], w: &mut Warnings) -> Globals<'a> {
    let colors = color::Colors::read(graphic, w);
    let sheet = styles::StyleSheet::new(styles_parts, w);
    let stroke_styles = strokes::StrokeStyles::read(graphic);
    let global_light = prefs_parts.iter().find_map(|r| r.child("TransparencyPreference"))
        .map(|t| (t.f64("GlobalLightAngle").unwrap_or(120.0), t.f64("GlobalLightAltitude").unwrap_or(30.0))).unwrap_or((120.0, 30.0));
    let mut view = view_preferences(prefs_parts);
    if let Some(z) = dm.attr("ZeroPoint").and_then(props::nums).filter(|z| z.len() == 2) {
        view.zero_point = [z[0], z[1]];
    }
    Globals { colors, sheet, stroke_styles, global_light, view }
}

/// Reads the top-level items of spreads in parallel, in chunks: `work` holds (spread number, item element); the items come back in
/// the same order. The warnings merge into `w` in order.
fn read_items<'a>(reader: &ItemReader<'a, '_>, work: &[(usize, &'a Element<'a>)], w: &mut Warnings) -> Result<Vec<(usize, PageItem)>, Error> {
    const CHUNK: usize = 16;
    let chunks = work.len().div_ceil(CHUNK);
    let read = par::parallel(chunks, || (), |_, c| {
        let mut cw = Warnings::default();
        let items: Vec<(usize, folio_model::PageItem)> =
            work[c * CHUNK..((c + 1) * CHUNK).min(work.len())].iter().map(|&(s, e)| (s, reader.item(e, &mut cw, 0))).collect();
        (items, cw)
    });
    if read.len() != chunks {
        return Err(Error::Package("an item reading thread failed".into()));
    }
    let mut out = Vec::with_capacity(work.len());
    for (items, cw) in read {
        w.merge(cw);
        out.extend(items);
    }
    Ok(out)
}

/// Each page's section marker (the marker of the section starting at it or before it) and the first and last page of each section.
fn assign_sections(spreads: &mut [folio_model::Spread], dm: &Element<'_>) {
    let section_starts: std::collections::HashMap<&str, String> = dm
        .children_named("Section")
        .filter_map(|s| Some((s.attr("PageStart")?, s.attr("Marker").unwrap_or_default().to_owned())))
        .collect();
    let mut doc_pages: Vec<&mut folio_model::Page> = spreads.iter_mut().flat_map(|s: &mut folio_model::Spread| s.pages.iter_mut()).collect();
    doc_pages.sort_by_key(|p| p.index);
    let mut marker = String::new();
    // the first and last page of each section: a section runs to the page before the next one starts
    for page in &mut doc_pages {
        if let Some(m) = section_starts.get(page.id.as_str()) {
            marker = m.clone();
        }
        page.section_marker = marker.clone();
    }
    let names: Vec<String> = doc_pages.iter().map(|p| p.name.clone()).collect();
    let mut k = 0;
    while k < doc_pages.len() {
        let mut end = k + 1;
        while end < doc_pages.len() && !section_starts.contains_key(doc_pages[end].id.as_str()) {
            end += 1;
        }
        for p in &mut doc_pages[k..end] {
            p.section_last = names[end - 1].clone();
            p.section_start = k;
        }
        k = end;
    }
}

/// The parts the designmap lists for `kind` (`Graphic`, `Spread`, …) with their paths, in the designmap's order; a listed part the
/// package lacks is a warning.
fn named_parts<'p>(pkg: &'p package::Package<'_>, kind: &'static str, w: &mut Warnings) -> Vec<(&'p str, &'p Element<'p>)> {
    pkg.designmap
        .children_named(kind)
        .filter_map(|e| e.attr("src"))
        .filter_map(|src| match pkg.part(src) {
            Some(p) => Some((src, p)),
            None => {
                w.add(format!("{kind} part {src} missing from package"));
                None
            }
        })
        .collect()
}

fn build_with_facts(pkg: &package::Package<'_>, idml: &Path, byte_len: u64, data: &ReaderData) -> Result<(Document, Facts), Error> {
    let mut w = Warnings::default();

    let reader_name = match &data.parser {
        Some(p) => format!("{p} + folio-idml"),
        None => String::from("folio-idml"),
    };
    let previews = &data.previews;
    let sidecar = data.saved_lines.as_ref().and_then(|v| match saved::parse(v) {
        Ok(s) => Some(s),
        Err(e) => {
            w.add(format!("saved lines: left out: {e}"));
            None
        }
    });
    for m in &data.warnings {
        w.add(m.clone());
    }

    let dm = &pkg.designmap;
    let named = |kind: &'static str, w: &mut Warnings| named_parts(pkg, kind, w);
    let parts = |kind: &'static str, w: &mut Warnings| -> Vec<&Element<'_>> { named_parts(pkg, kind, w).into_iter().map(|(_, p)| p).collect() };
    let graphic = parts("Graphic", &mut w);
    let styles_parts = parts("Styles", &mut w);
    let fonts_parts = parts("Fonts", &mut w);
    let prefs_parts = parts("Preferences", &mut w);
    let master_named = named("MasterSpread", &mut w);
    let spread_named = named("Spread", &mut w);
    let story_named = named("Story", &mut w);
    let master_parts: Vec<&Element<'_>> = master_named.iter().map(|(_, p)| *p).collect();
    let spread_parts: Vec<&Element<'_>> = spread_named.iter().map(|(_, p)| *p).collect();
    let story_parts: Vec<&Element<'_>> = story_named.iter().map(|(_, p)| *p).collect();
    let (text_path_frames, text_paths) = textpath::FrameIds::tracked(master_named.iter().chain(&spread_named).chain(&story_named).map(|(n, p)| (*n, *p)));
    let traps = traps::read(&styles_parts, &spread_parts);

    let Globals { colors, sheet, stroke_styles, global_light, view } = globals(dm, &graphic, &styles_parts, &prefs_parts, &mut w);
    let reader = ItemReader { text_path_frames: &text_path_frames, sheet: &sheet, colors: &colors, previews, strokes: &stroke_styles, links: &data.links, embedded: &data.embedded, global_light };

    let layer_els: Vec<&Element<'_>> = dm.children_named("Layer").collect();
    // IDML designmap lists layers bottom to top (unlike document.layers in the DOM).
    // Also checked via hidden INDD save -> folio.7 reader: DOC_LAYERS (0x301) is emitted
    // in order after dropping its internal first layer; Base, Top, Locked layer matches
    // Adobe's export. artifacts/objects-3-rest-20261020/layer-order.json.
    // Oracle align-page-bottom / align-spread-bottom overlap Base items with Top and
    // prove this in the printed page; reversing it puts the rotated Base frame on top.
    let layers = layer_els
        .iter()
        .enumerate()
        .map(|(i, l)| Layer {
            id: l.attr("Self").unwrap_or_default().to_owned(),
            name: l.attr("Name").map(|n| clean_name(n).to_owned()).unwrap_or_default(),
            visible: l.attr("Visible") != Some("false"),
            printable: l.attr("Printable") != Some("false"),
            order: i,
            locked: l.attr("Locked") == Some("true"),
            color: l.get("LayerColor").and_then(items::ui_color).unwrap_or_default(),
            show_guides: l.attr("ShowGuides") != Some("false"),
            lock_guides: l.attr("LockGuides") == Some("true"),
        })
        .collect::<Vec<_>>();

    // Spread shells (pages) in order, then every top-level item of every spread read in
    // parallel; warnings merge back in document order.
    let mut shells: Vec<(bool, folio_model::Spread)> = Vec::new();
    let mut work: Vec<(usize, &Element<'_>)> = Vec::new();
    let mut unused = folio_model::MASTER_PAGE_BASE;
    let mut next_page = 0;
    for (master, p) in master_parts.into_iter().map(|p| (true, p)).chain(spread_parts.into_iter().map(|p| (false, p))) {
        let counter = if master { &mut unused } else { &mut next_page };
        if let Some((shell, items)) = reader.spread(p, master, counter, &mut w) {
            work.extend(items.into_iter().map(|e| (shells.len(), e)));
            shells.push((master, shell));
        }
    }
    for (s, item) in read_items(&reader, &work, &mut w)? {
        shells[s].1.items.push(item);
    }
    let mut master_spreads = Vec::new();
    let mut spreads = Vec::new();
    for (master, mut s) in shells {
        items::place_guides(&mut s, &view.ruler_origin);
        if master { master_spreads.push(s) } else { spreads.push(s) }
    }
    assign_sections(&mut spreads, dm);

    // Conditions whose text InDesign hides (and does not compose or print).
    let conditions = conditions::from_element(dm);
    let hidden_conditions = conditions.iter().filter(|c| !c.visible).map(|c| c.id.clone()).collect();
    let story_reader = text::StoryReader {
        sheet: &sheet,
        colors: &colors,
        items: &reader,
        hidden_conditions: &hidden_conditions,
        saved: sidecar.as_ref(),
    };
    let read = par::parallel(story_parts.len(), || (), |_, i| {
        let mut sw = Warnings::default();
        let (story, extras) = story_reader.story(story_parts[i], &mut sw);
        (story, extras, sw)
    });
    if read.len() != story_parts.len() {
        return Err(Error::Package("a story reading thread failed".into()));
    }
    let mut stories: BTreeMap<String, Story> = BTreeMap::new();
    let (mut endnote_ranges, mut endnote_refs) = (Vec::new(), Vec::new());
    let mut endnote_story_of = std::collections::HashMap::new();
    for (story, extras, sw) in read {
        w.merge(sw);
        if extras.is_endnote_story {
            endnote_ranges.extend(extras.endnote_ranges);
        }
        if let Some(s) = &story {
            for r in &extras.endnote_refs { endnote_story_of.insert(r.clone(), s.id.clone()); }
        }
        endnote_refs.extend(extras.endnote_refs);
        if let Some(s) = story
            && let Some(old) = stories.insert(s.id.clone(), s)
        {
            w.add(format!("duplicate story {}", old.id));
        }
    }
    if !endnote_refs.is_empty() || !endnote_ranges.is_empty() {
        let mut all: Vec<&mut Story> = stories.values_mut().collect();
        let start = dm.child("EndnoteOption").and_then(|o|o.attr("StartEndnoteNumberAt")).and_then(|s|s.parse().ok()).unwrap_or(1);
        // the scope Story numbers each story's endnotes from the start number, whatever the restart option says (rest-c1, oracle
        // endnotes-story en-story-scope: the second story's endnote prints 1 with Continuous)
        let story_scope = dm.child("EndnoteOption").and_then(|o| o.child("Properties")).and_then(|p| p.child("ScopeValue")).is_some_and(|v| v.text() == "StoryScope");
        let restart = endnotes::options(dm).restart == "StoryRestart" || story_scope;
        text::number_endnotes(&mut all, &endnote_ranges, &endnote_refs, start, restart.then_some(&endnote_story_of));
    }

    let font_table = fonts(&fonts_parts);
    let text_preferences = preferences(&prefs_parts);
    let footnotes = footnotes::options(&prefs_parts, &colors, &mut w);
    let print = print_setup(dm, &prefs_parts, &graphic);
    let pasteboard = prefs_parts.iter().find_map(|r| r.child("PasteboardPreference")).map(|p| {
        let m = p.attr("PasteboardMargins").and_then(crate::props::nums).unwrap_or_default();
        folio_model::Pasteboard {
            horizontal: m.first().copied().unwrap_or(-1.0),
            vertical: p.f64("MinimumSpaceAboveAndBelow").or(m.get(1).copied()).unwrap_or(72.0),
        }
    }).unwrap_or_default();
    let dom_version = dm.attr("DOMVersion").unwrap_or_default().to_owned();
    let swatch_order = swatch_order(dm, &graphic, &colors);
    let styles = style_catalog(&styles_parts);
    let text_variables = text_variables(dm);
    let (chapter_number, chapter_number_source) = chapter::from_element(prefs_parts.iter().find_map(|r| r.child("ChapterNumberPreference")));

    let mut doc = Document {
        numbering_lists: numbering::from_element(dm),
        story_order: dm.attr("StoryList").unwrap_or_default().split_whitespace().map(str::to_owned).collect(),
        conditions,
        condition_indicators: dm.children_named("ConditionalTextPreference").next().and_then(|p| p.attr("ShowConditionIndicators")).map(folio_model::conditions::Indicators::from_idml).unwrap_or_default(),
        indexes: indexes::from_elements(dm, &story_parts),
        traps,
        hyperlinks: hyperlinks::from_elements(dm, &story_parts),
        stroke_styles: stroke_styles.definitions.clone(),
        preflight_profiles: preflight::from_element(dm),
        preflight_settings: preflight::settings(dm),
        toc_styles: styles_parts.iter().flat_map(|r| toc::from_element(r)).chain(toc::from_element(dm)).collect(),
        toc_stories: story_parts.iter().filter_map(|r| r.child("Story")).filter_map(|s| {
            let style = s.attr("AppliedTOCStyle")?;
            (style != "n").then(|| (s.attr("Self").unwrap_or_default().to_owned(), style.to_owned()))
        }).collect(),
        source: SourceInfo {
            path: std::path::absolute(idml).unwrap_or_else(|_| idml.to_path_buf()).to_string_lossy().into_owned(),
            bytes: byte_len,
            reader: reader_name,
            dom_version,
        },
        spreads,
        master_spreads,
        stories,
        swatches: colors.swatches,
        layers,
        fonts_used: Vec::new(),
        font_table,
        warnings: Vec::new(),
        gradients: colors.gradients,
        text_preferences,
        print,
        pasteboard,
        footnotes,
        endnotes: endnotes::options(dm),
        swatch_order,
        styles,
        view,
        text_variables,
        chapter_number,
        chapter_number_source,
        facts: folio_model::DocumentFacts { created: xmp_date(pkg.metadata(), "CreateDate"), modified: xmp_date(pkg.metadata(), "ModifyDate"), ..Default::default() },
    };
    doc.fonts_used = fonts_used(&doc);
    check_references(&doc, &mut w);
    doc.warnings = w.into_strings();
    Ok((doc, Facts { sidecar, text_paths }))
}

/// The designmap's `TextVariable`s with their options (`MatchParagraphStylePreference`, `PageNumberVariablePreference`, …).
fn text_variables(dm: &Element<'_>) -> BTreeMap<String, folio_model::VariableDef> {
    let mut out = BTreeMap::new();
    for v in dm.children_named("TextVariable") {
        let Some(id) = v.attr("Self") else { continue };
        let mut d = folio_model::VariableDef { id: id.to_owned(), name: v.attr("Name").unwrap_or_default().to_owned(), kind: v.attr("VariableType").unwrap_or_default().to_owned(), ..Default::default() };
        for p in v.elements().filter(|p| p.name.ends_with("VariablePreference") || p.name.ends_with("StylePreference")) {
            let a = |k: &str| p.attr(k).unwrap_or_default().to_owned();
            d.text_before = a("TextBefore");
            d.text_after = a("TextAfter");
            match p.name {
                "MatchParagraphStylePreference" => d.style = a("AppliedParagraphStyle"),
                "MatchCharacterStylePreference" => d.style = a("AppliedCharacterStyle"),
                "PageNumberVariablePreference" => d.scope = a("Scope"),
                _ => {}
            }
            // every setting as the file has it (the Define dialog shows and edits them), a custom text's contents among them
            d.settings = p.attrs.iter().map(|(k, v)| ((*k).to_owned(), v.to_string())).collect();
            if let Some(c) = p.child("Properties").and_then(|pr| pr.child("Contents")) {
                d.settings.insert("Contents".into(), c.text().trim().to_owned());
            }
            if p.name.starts_with("Match") {
                d.search = a("SearchStrategy");
                d.change_case = a("ChangeCase");
                d.delete_end_punctuation = p.attr("DeleteEndPunctuation") == Some("true");
            }
        }
        out.insert(d.id.clone(), d);
    }
    out
}

fn fonts(roots: &[&Element<'_>]) -> Vec<FontInfo> {
    let mut found = Vec::new();
    for r in roots {
        r.descendants_named("Font", &mut found);
    }
    found
        .into_iter()
        .map(|f| {
            let a = |k: &str| f.attr(k).unwrap_or_default().to_owned();
            FontInfo {
                family: a("FontFamily"),
                style: a("FontStyleName"),
                postscript_name: a("PostScriptName"),
                full_name: a("FullName"),
                font_type: a("FontType"),
                version: a("Version"),
                status: a("Status"),
                writing_script: f.attr("WritingScript").and_then(|v| v.trim().parse().ok()),
            }
        })
        .collect()
}

fn preferences(roots: &[&Element<'_>]) -> TextPreferences {
    let mut p = TextPreferences::default();
    for r in roots {
        if let Some(t) = r.child("TextPreference") {
            let set = |k: &str, f: &mut f64| {
                if let Some(v) = t.f64(k) {
                    *f = v;
                }
            };
            set("SmallCap", &mut p.small_cap);
            set("SuperscriptSize", &mut p.superscript_size);
            set("SuperscriptPosition", &mut p.superscript_position);
            set("SubscriptSize", &mut p.subscript_size);
            set("SubscriptPosition", &mut p.subscript_position);
            if let Some(v) = t.bool("UseParagraphLeading") {
                p.use_paragraph_leading = v;
            }
            if let Some(v) = t.bool("ZOrderTextWrap") {
                p.z_order_text_wrap = v;
            }
        }
        if let Some(g) = r.child("GridPreference") {
            if let Some(v) = g.f64("BaselineStart") {
                p.baseline_start = v;
            }
            if let Some(v) = g.f64("BaselineDivision") {
                p.baseline_division = v;
            }
            if let Some(v) = g.str("BaselineGridRelativeOption") {
                p.baseline_relative_to = v.to_owned();
            }
        }
    }
    p
}

/// Units, ruler origin and guide settings (`ViewPreference`, `GuidePreference`).
fn view_preferences(roots: &[&Element<'_>]) -> folio_model::ViewPreferences {
    let mut v = folio_model::ViewPreferences::default();
    for r in roots {
        if let Some(p) = r.child("ViewPreference") {
            let s = |k: &str, f: &mut String| {
                if let Some(x) = p.str(k).filter(|x| !x.is_empty()) {
                    *f = x.to_owned();
                }
            };
            s("HorizontalMeasurementUnits", &mut v.horizontal_units);
            s("VerticalMeasurementUnits", &mut v.vertical_units);
            s("RulerOrigin", &mut v.ruler_origin);
            s("StrokeMeasurementUnits", &mut v.stroke_units);
            v.cursor_key_increment = p.f64("CursorKeyIncrement").unwrap_or(v.cursor_key_increment);
            v.snap_zone = p.f64("GuideSnaptoZone").unwrap_or(v.snap_zone);
        }
        if let Some(t) = r.child("TextPreference") {
            v.leading_key_increment = t.f64("LeadingKeyIncrement").unwrap_or(v.leading_key_increment);
            v.baseline_shift_key_increment = t.f64("BaselineShiftKeyIncrement").unwrap_or(v.baseline_shift_key_increment);
            v.kerning_key_increment = t.f64("KerningKeyIncrement").unwrap_or(v.kerning_key_increment);
        }
        if let Some(p) = r.child("PasteboardPreference") {
            if let Some(m) = p.str("PasteboardMargins").and_then(crate::props::nums).filter(|m| m.len() == 2) {
                v.pasteboard_margins = [m[0], m[1]];
            }
            v.match_preview_background = p.bool("MatchPreviewBackgroundToThemeColor").unwrap_or(v.match_preview_background);
            for (key, into) in [("PreviewBackgroundColor", &mut v.preview_background_color), ("BleedGuideColor", &mut v.bleed_guide_color), ("SlugGuideColor", &mut v.slug_guide_color)] {
                if let Some(c) = p.get(key).and_then(crate::items::ui_color) {
                    *into = c;
                }
            }
        }
        if let Some(g) = r.child("GuidePreference") {
            v.guides_locked = g.bool("GuidesLocked").unwrap_or(v.guides_locked);
            v.guides_shown = g.bool("GuidesShown").unwrap_or(v.guides_shown);
            v.guides_in_back = g.bool("GuidesInBack").unwrap_or(v.guides_in_back);
            v.guides_snap = g.bool("GuidesSnapto").unwrap_or(v.guides_snap);
            v.ruler_guides_threshold = g.f64("RulerGuidesViewThreshold").unwrap_or(v.ruler_guides_threshold);
            if let Some(c) = g.get("RulerGuidesColor").and_then(crate::items::ui_color) {
                v.ruler_guides_color = c;
            }
        }
        if let Some(d) = r.child("DocumentPreference") {
            v.column_guides_locked = d.bool("ColumnGuideLocked").unwrap_or(v.column_guides_locked);
        }
        if let Some(g) = r.child("GridPreference") {
            let n = &mut v.grid;
            n.baseline_shown = g.bool("BaselineGridShown").unwrap_or(n.baseline_shown);
            n.baseline_start = g.f64("BaselineStart").unwrap_or(n.baseline_start);
            n.baseline_division = g.f64("BaselineDivision").filter(|d| *d > 0.0).unwrap_or(n.baseline_division);
            n.baseline_threshold = g.f64("BaselineViewThreshold").unwrap_or(n.baseline_threshold);
            if let Some(o) = g.str("BaselineGridRelativeOption") {
                n.baseline_relative_to = o.to_owned();
            }
            n.document_shown = g.bool("DocumentGridShown").unwrap_or(n.document_shown);
            n.document_snap = g.bool("DocumentGridSnapto").unwrap_or(n.document_snap);
            n.horizontal_division = g.f64("HorizontalGridlineDivision").filter(|d| *d > 0.0).unwrap_or(n.horizontal_division);
            n.vertical_division = g.f64("VerticalGridlineDivision").filter(|d| *d > 0.0).unwrap_or(n.vertical_division);
            n.horizontal_subdivision = g.f64("HorizontalGridSubdivision").filter(|d| *d >= 1.0).unwrap_or(n.horizontal_subdivision);
            n.vertical_subdivision = g.f64("VerticalGridSubdivision").filter(|d| *d >= 1.0).unwrap_or(n.vertical_subdivision);
            n.grids_in_back = g.bool("GridsInBack").unwrap_or(n.grids_in_back);
            for (key, into) in [("GridColor", &mut n.grid_color), ("BaselineColor", &mut n.baseline_color)] {
                if let Some(c) = g.get(key).and_then(crate::items::ui_color) {
                    *into = c;
                }
            }
        }
    }
    v
}

/// The Swatches panel order (`Document::swatch_order`): the designmap's colour groups (the
/// root group first) list the panel's swatches; without them, the visible swatches of
/// Graphic.xml in order, None, Registration, Paper and Black first. Only swatches and
/// gradients the document read are listed.
fn swatch_order(dm: &Element<'_>, graphic: &[&Element<'_>], colors: &color::Colors) -> Vec<String> {
    let known = |id: &str| colors.swatches.contains_key(id) || colors.gradients.contains_key(id);
    let mut groups: Vec<&Element<'_>> = dm.children_named("ColorGroup").collect();
    // The root group first, the others in their order.
    groups.sort_by_key(|g| g.attr("IsRootColorGroup") != Some("true"));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for g in &groups {
        for s in g.children_named("ColorGroupSwatch") {
            if let Some(id) = s.attr("SwatchItemRef").filter(|id| known(id))
                && seen.insert(id)
            {
                out.push(id.to_owned());
            }
        }
    }
    if !groups.is_empty() {
        return out;
    }
    const FIRST: [&str; 4] = ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black"];
    let mut listed: Vec<&str> = Vec::new();
    for r in graphic {
        for e in r.elements() {
            if matches!(e.name, "Color" | "Swatch" | "Tint" | "MixedInk" | "Gradient")
                && e.attr("Visible") != Some("false")
                && let Some(id) = e.attr("Self").filter(|id| known(id))
            {
                listed.push(id);
            }
        }
    }
    for id in FIRST.iter().copied().filter(|f| listed.contains(f)).chain(listed.iter().copied().filter(|id| !FIRST.contains(id))) {
        if seen.insert(id) {
            out.push(id.to_owned());
        }
    }
    out
}

/// Paragraph, character and object styles in Styles.xml order with their style groups.
fn style_catalog(roots: &[&Element<'_>]) -> folio_model::StyleCatalog {
    fn walk(group: &Element<'_>, style: &str, sub: &str, path: &mut Vec<String>, out: &mut Vec<folio_model::StyleEntry>, groups: &mut Vec<String>) {
        for c in group.elements() {
            if c.name == style {
                let id = c.attr("Self").unwrap_or_default();
                let name = c.attr("Name").map(str::to_owned).unwrap_or_else(|| id.split_once('/').map_or(id, |(_, n)| n).to_owned());
                let (mut based_on, mut settings) = (String::new(), std::collections::BTreeMap::new());
                if style == "CellStyle" || style == "TableStyle" {
                    based_on = c.child("Properties").and_then(|p| p.child("BasedOn")).map(|b| b.text().trim().to_owned()).filter(|b| !b.is_empty() && !b.starts_with("$ID/"))
                        .map(|b| if b.starts_with(style) { b } else { format!("{style}/{b}") }).unwrap_or_default();
                    settings = c.attrs.iter().filter(|(k, _)| !matches!(*k, "Self" | "Name" | "KeyboardShortcut" | "ExtendedKeyboardShortcut")).map(|(k, v)| ((*k).to_owned(), v.to_string())).collect();
                }
                out.push(folio_model::StyleEntry { id: id.to_owned(), name, group: path.clone(), based_on, settings });
            } else if c.name == sub {
                // a group in a group is named with the groups before it ("$ID/Grp:Sub"): its own name is the last part
                let name = c.attr("Name").unwrap_or_default();
                path.push(if path.is_empty() { name } else { name.rsplit(':').next().unwrap_or(name) }.to_owned());
                groups.push(c.attr("Self").unwrap_or_default().to_owned());
                walk(c, style, sub, path, out, groups);
                path.pop();
            }
        }
    }
    let mut cat = folio_model::StyleCatalog::default();
    for r in roots {
        for g in r.elements() {
            let (style, sub, out, groups) = match g.name {
                "RootParagraphStyleGroup" => ("ParagraphStyle", "ParagraphStyleGroup", &mut cat.paragraph, &mut cat.paragraph_groups),
                "RootCharacterStyleGroup" => ("CharacterStyle", "CharacterStyleGroup", &mut cat.character, &mut cat.character_groups),
                "RootObjectStyleGroup" => ("ObjectStyle", "ObjectStyleGroup", &mut cat.object, &mut cat.object_groups),
                "RootCellStyleGroup" => ("CellStyle", "CellStyleGroup", &mut cat.cell, &mut cat.cell_groups),
                "RootTableStyleGroup" => ("TableStyle", "TableStyleGroup", &mut cat.table, &mut cat.table_groups),
                _ => continue,
            };
            walk(g, style, sub, &mut Vec::new(), out, groups);
        }
    }
    cat
}

/// Bleed, slug and black overprint (`DocumentPreference`), colour profiles (designmap) and the
/// Ink Manager's inks (Graphic.xml `Ink`).
fn print_setup(dm: &Element<'_>, prefs: &[&Element<'_>], graphic: &[&Element<'_>]) -> folio_model::PrintSetup {
    let profile = |k: &str| {
        dm.attr(k).map(clean_name).filter(|v| !v.is_empty() && *v != "None" && !is_none_name(v)).map(str::to_owned)
    };
    let mut p = folio_model::PrintSetup {
        cmyk_profile: profile("CMYKProfile"),
        rgb_profile: profile("RGBProfile"),
        solid_color_intent: dm.attr("SolidColorIntent").map(str::to_owned),
        ..Default::default()
    };
    for r in prefs {
        if let Some(d) = r.child("DocumentPreference") {
            let f = |k: &str| d.f64(k).unwrap_or(0.0);
            p.bleed = [
                f("DocumentBleedTopOffset"),
                f("DocumentBleedBottomOffset"),
                f("DocumentBleedInsideOrLeftOffset"),
                f("DocumentBleedOutsideOrRightOffset"),
            ];
            p.slug = [f("SlugTopOffset"), f("SlugBottomOffset"), f("SlugInsideOrLeftOffset"), f("SlugRightOrOutsideOffset")];
            p.facing_pages = d.bool("FacingPages").unwrap_or(false);
            p.facing_pages_stored = d.bool("FacingPages").is_some();
            p.right_to_left = d.attr("PageBinding") == Some("RightToLeft");
            p.overprint_black = d.bool("OverprintBlack").unwrap_or(true);
        }
    }
    for r in graphic {
        for ink in r.children_named("Ink") {
            let name = clean_name(ink.attr("Name").unwrap_or_default()).to_owned();
            p.inks.push(folio_model::Ink {
                process: matches!(name.as_str(), "Process Cyan" | "Process Magenta" | "Process Yellow" | "Process Black"),
                convert_to_process: ink.bool("ConvertToProcess").unwrap_or(false),
                alias: ink
                    .attr("AliasInkName")
                    .map(|a| clean_name(a.strip_prefix("Ink/").unwrap_or(a)))
                    .filter(|a| !a.is_empty() && !is_none_name(a))
                    .map(str::to_owned),
                print: ink.bool("PrintInk").unwrap_or(true),
                ink_type: ink.attr("InkType").unwrap_or("Normal").to_owned(),
                neutral_density: ink.f64("NeutralDensity"),
                name,
            });
        }
        for group in r.children_named("MixedInkGroup") {
            if let Some(id) = group.attr("Self") {
                p.mixed_ink_groups.insert(id.to_owned(), r.children_named("MixedInk")
                    .filter(|m| m.attr("BaseColor") == Some(id)).filter_map(|m| m.attr("Self").map(str::to_owned)).collect());
            }
        }
        for mixed in r.children_named("MixedInk") {
            if let Some(id) = mixed.attr("Self") {
                p.mixed_inks.insert(id.to_owned(), mixed_ink_parts(mixed));
            }
        }
    }
    p
}

/// Inks and percentages of a `MixedInk` (`InkList` as space-separated, percent-encoded
/// `Ink/…` references or a list; `InkPercentages`, older files `InkValues`).
fn mixed_ink_parts(e: &Element<'_>) -> Vec<(String, f64)> {
    let decode = |s: &str| -> String {
        let s = props::percent_decode(s);
        clean_name(s.strip_prefix("Ink/").unwrap_or(&s)).to_owned()
    };
    let names: Vec<String> = match e.get("InkList") {
        Some(props::Val::Str(s)) => s.split_whitespace().map(decode).collect(),
        Some(props::Val::Elem(l)) => l.children_named("ListItem").map(|i| decode(i.text().trim())).collect(),
        None => Vec::new(),
    };
    let amounts = e.get("InkPercentages").or_else(|| e.get("InkValues")).and_then(|v| v.f64s()).unwrap_or_default();
    names.into_iter().zip(amounts).collect()
}

fn is_none_name(v: &str) -> bool {
    props::is_none_ref(v) || v == "[No Profile]" || v == "No CMYK Profile"
}

/// Every item of the document: spread and master items (nested), then items anchored in text.
pub fn for_each_item<'a>(doc: &'a Document, f: &mut dyn FnMut(&'a PageItem)) {
    for s in doc.master_spreads.iter().chain(&doc.spreads) {
        for i in &s.items {
            walk_with_inline(i, f);
        }
    }
    for st in doc.stories.values() {
        for r in st.all_runs() {
            for i in &r.inline {
                walk_with_inline(i, f);
            }
        }
    }
}

fn walk_with_inline<'a>(item: &'a PageItem, f: &mut dyn FnMut(&'a PageItem)) {
    item.walk(f);
}

fn fonts_used(doc: &Document) -> Vec<(String, String)> {
    let mut set = BTreeSet::new();
    for st in doc.stories.values() {
        for r in st.all_runs() {
            if r.text.chars().any(|c| c != '\u{FFFC}' && c != '\u{16}') {
                set.insert((r.attrs.font_family.clone(), r.attrs.font_style.clone()));
                // a nested line style may set the text in another font
                for a in &r.line_attrs {
                    set.insert((a.font_family.clone(), a.font_style.clone()));
                }
            }
        }
        // Bullets and numbers are set in their own font.
        for p in &st.paragraphs {
            if let Some(l) = &p.attrs.list {
                set.insert((l.marker.font_family.clone(), l.marker.font_style.clone()));
            }
        }
    }
    set.into_iter().collect()
}

/// Missing stories, frames, masters and swatches (aggregated into warnings).
fn check_references(doc: &Document, w: &mut Warnings) {
    let mut frames = HashSet::new();
    for_each_item(doc, &mut |i| {
        if i.text_frame().is_some() {
            frames.insert(i.id.as_str());
        }
    });
    let paint_ok = |p: &Paint, w: &mut Warnings| match p {
        Paint::Swatch { id, .. } if !doc.swatches.contains_key(id) => w.add(format!("paint references missing swatch {id}")),
        Paint::Gradient { id, .. } if !doc.gradients.contains_key(id) => {
            w.add(format!("paint references missing gradient {id}"))
        }
        _ => {}
    };
    let mut items: Vec<&PageItem> = Vec::new();
    for_each_item(doc, &mut |i| items.push(i));
    for i in items {
        paint_ok(&i.fill, w);
        paint_ok(&i.stroke.paint, w);
        if let Some(tf) = i.text_frame() {
            if !doc.stories.contains_key(&tf.story) {
                w.add(format!("text frame {} references missing story {}", i.id, tf.story));
            }
            for link in [&tf.previous, &tf.next].into_iter().flatten() {
                if !frames.contains(link.as_str()) {
                    w.add(format!("text frame {} threads to missing frame {link}", i.id));
                }
            }
        }
    }
    let masters: HashSet<&str> = doc.master_spreads.iter().map(|s| s.id.as_str()).collect();
    for p in doc.spreads.iter().chain(&doc.master_spreads).flat_map(|s| &s.pages) {
        if let Some(m) = &p.master
            && !masters.contains(m.as_str())
        {
            w.add(format!("page {} uses missing master {m}", p.name));
        }
    }
    let layers: HashSet<&str> = doc.layers.iter().map(|l| l.id.as_str()).collect();
    let mut bad_layer = 0;
    for_each_item(doc, &mut |i| {
        if i.layer.as_deref().is_some_and(|l| !layers.contains(l)) {
            bad_layer += 1;
        }
    });
    w.add_n("item on a missing layer", bad_layer);
    for r in doc.stories.values().flat_map(|s| s.all_runs()) {
        paint_ok(&r.attrs.fill, w);
        paint_ok(&r.attrs.stroke, w);
    }
}

pub mod numbering;

pub mod anchored;
pub mod style_shortcut;
pub mod inks;
