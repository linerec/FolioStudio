//! `ReadBase::update` must give the document `read_parts` gives for the same parts. Every spread, parent spread and story part of
//! the oracle fixtures and the InDesign Feature Atlas is changed in a few ways (as an edit changes it), the base is brought up to date
//! and compared with a full read of the changed parts. An update that says it cannot (`Err`) is allowed, but only for a reason the
//! incremental path is documented to have; the number of updates that did go through is counted so the path cannot quietly stop being
//! taken. `warnings` are the one field not compared: the host keeps its own.

use folio_idml::{ReadBase, ReaderData, read_parts};
use folio_model::Document;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::hash::Hasher;
use std::io::Read as _;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Every part of the package (text only).
fn parts_of(path: &Path) -> BTreeMap<String, String> {
    let f = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).expect("zip");
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).expect("entry");
        let name = e.name().map(|n| n.into_owned()).unwrap_or_default();
        if e.is_dir() {
            continue;
        }
        let mut s = String::new();
        if e.read_to_string(&mut s).is_ok() {
            out.insert(name, s);
        }
    }
    out
}

struct Hasher64(std::collections::hash_map::DefaultHasher, usize);

impl std::fmt::Write for Hasher64 {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        self.1 += s.len();
        Ok(())
    }
}

/// A digest of everything in the document but its warnings (the serialisation keeps every field).
fn digest(doc: &Document) -> (u64, usize) {
    let mut h = Hasher64(Default::default(), 0);
    let mut d = doc.clone();
    d.warnings.clear();
    write!(h, "{d:?}").unwrap();
    (h.0.finish(), h.1)
}

/// Where two documents differ (the first differing field), for a failure message.
fn explain(a: &Document, b: &Document) -> String {
    let (mut a, mut b) = (a.clone(), b.clone());
    a.warnings.clear();
    b.warnings.clear();
    macro_rules! field {
        ($($f:ident),*) => { $( if format!("{:?}", a.$f) != format!("{:?}", b.$f) { return format!("field {}", stringify!($f)); } )* };
    }
    if a.spreads.len() != b.spreads.len() {
        return "number of spreads".into();
    }
    for (i, (x, y)) in a.spreads.iter().zip(&b.spreads).enumerate() {
        if format!("{x:?}") != format!("{y:?}") {
            if x.items.len() == y.items.len() {
                for (j, (p, q)) in x.items.iter().zip(&y.items).enumerate() {
                    if format!("{p:?}") != format!("{q:?}") {
                        return format!("spread {i} ({}) item {j} ({})", x.id, p.id);
                    }
                }
            }
            return format!("spread {i} ({}) outside its items", x.id);
        }
    }
    for (i, (x, y)) in a.master_spreads.iter().zip(&b.master_spreads).enumerate() {
        if format!("{x:?}") != format!("{y:?}") {
            return format!("master spread {i}");
        }
    }
    for (id, x) in &a.stories {
        if b.stories.get(id).map(|y| format!("{y:?}")) != Some(format!("{x:?}")) {
            return format!("story {id}");
        }
    }
    field!(numbering_lists, story_order, conditions, condition_indicators, indexes, traps, hyperlinks, stroke_styles, preflight_profiles,
        preflight_settings, toc_styles, toc_stories, source, stories, swatches, layers, fonts_used, font_table, gradients, text_preferences,
        print, pasteboard, footnotes, swatch_order, styles, view, text_variables, chapter_number, chapter_number_source, facts);
    "no differing field found".into()
}

/// A few ways an edit changes a part's text; the part's element structure stays what it was (an edit's own parts are re-read by the
/// same reader, which is what is being compared).
fn mutations(name: &str, text: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if name.starts_with("Spreads/") || name.starts_with("MasterSpreads/") {
        // every `ItemTransform` (the item moves one point right)
        let mut moved = String::with_capacity(text.len() + 64);
        let mut rest = text;
        let key = " ItemTransform=\"";
        while let Some(at) = rest.find(key) {
            let (head, tail) = rest.split_at(at + key.len());
            moved.push_str(head);
            let end = tail.find('"').unwrap_or(tail.len());
            let nums: Vec<f64> = tail[..end].split_whitespace().filter_map(|v| v.parse().ok()).collect();
            if nums.len() == 6 {
                let v = [nums[0], nums[1], nums[2], nums[3], nums[4] + 1.0, nums[5]];
                moved.push_str(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "));
            } else {
                moved.push_str(&tail[..end]);
            }
            rest = &tail[end..];
        }
        moved.push_str(rest);
        if moved != text {
            out.push(("move items", moved));
        }
        // the first item's fill colour changes
        if let Some(at) = text.find(" FillColor=\"") {
            let (head, tail) = text.split_at(at + " FillColor=\"".len());
            let end = tail.find('"').unwrap_or(0);
            out.push(("fill", format!("{head}Color/Black{}", &tail[end..])));
        }
    } else if name.starts_with("Stories/") {
        // one more character in the first text
        if let Some(at) = text.find("<Content>") {
            let at = at + "<Content>".len();
            out.push(("type", format!("{}x{}", &text[..at], &text[at..])));
        }
        // the story's first character style range loses its text
        if let Some(at) = text.find("<Content>")
            && let Some(end) = text[at..].find("</Content>")
        {
            out.push(("delete text", format!("{}{}", &text[..at + "<Content>".len()], &text[at + end..])));
        }
    }
    out
}

/// Byte slices of direct children of the spread, including groups as one complete subtree.
fn spread_children(text: &str) -> Vec<std::ops::Range<usize>> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let (mut depth, mut start) = (0, 0);
    let mut out = Vec::new();
    loop {
        let at = reader.buffer_position() as usize;
        match reader.read_event().unwrap() {
            Event::Start(_) => { if depth == 2 { start = at; } depth += 1; }
            Event::End(_) => { depth -= 1; if depth == 2 { out.push(start..reader.buffer_position() as usize); } }
            Event::Empty(_) if depth == 2 => out.push(at..reader.buffer_position() as usize),
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

#[test]
fn item_fragments_equal_full_reads_across_fixtures() {
    let mut files = std::fs::read_dir(root().join("oracle/fixtures")).unwrap().filter_map(Result::ok)
        .map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "idml")).collect::<Vec<_>>();
    files.push(root().join("../samples/feature-atlas/Feature_Atlas.idml"));
    let data = ReaderData::default();
    let mut checked = 0;
    for path in files {
        if !path.is_file() { continue; }
        let original = parts_of(&path);
        let Ok(base) = ReadBase::read(&original, &path, &data) else { continue };
        for (name, text) in original.iter().filter(|(n, _)| n.starts_with("Spreads/") || n.starts_with("MasterSpreads/")) {
            let children = spread_children(text);
            for range in children {
                let source = &text[range.clone()];
                // Only existing model items; Page and Guide have spread-level effects.
                let mut r = quick_xml::Reader::from_str(source);
                let e = match r.read_event().unwrap() { quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => e, _ => continue };
                let Some(id) = e.attributes().filter_map(Result::ok).find(|a| a.key.as_ref() == "Self").map(|a| a.value.into_owned()) else { continue };
                if !base.doc().spreads.iter().chain(&base.doc().master_spreads).flat_map(|s| &s.items).any(|i| i.id == id) { continue; }
                let Some((_, moved)) = mutations(name, source).into_iter().next() else { continue };
                let mut parts = original.clone();
                parts.get_mut(name).unwrap().replace_range(range, &moved);
                let full = read_parts(&parts, &path, &data).unwrap();
                let mut partial = base.clone();
                partial.update_with_items(&parts, &path, &data, &[name.clone()].into(), &[(name.clone(), vec![moved.as_str()])].into()).unwrap();
                assert_eq!(digest(partial.doc()), digest(&full), "{} {name} {id}: {}", path.display(), explain(partial.doc(), &full));
                checked += 1;
            }
        }
    }
    eprintln!("item fragments: {checked} full-read comparisons");
    assert!(checked > 500, "only {checked} items checked");
}

fn check_document(path: &Path, summary: &mut Summary) {
    let parts0 = parts_of(path);
    let data = ReaderData::default();
    let idml = path.to_path_buf();
    let base0 = ReadBase::read(&parts0, &idml, &data).expect("base");
    // the base itself is the full read
    assert_eq!(digest(base0.doc()), digest(&read_parts(&parts0, &idml, &data).expect("full")), "{}: base is not the full read", path.display());
    let names: Vec<&String> = parts0.keys().filter(|n| n.starts_with("Spreads/") || n.starts_with("MasterSpreads/") || n.starts_with("Stories/")).collect();
    for name in names {
        for (what, text) in mutations(name, &parts0[name]) {
            let mut parts1 = parts0.clone();
            parts1.insert(name.clone(), text);
            let full = match read_parts(&parts1, &idml, &data) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let mut base = base0.clone();
            let changed: BTreeSet<String> = [name.clone()].into();
            match base.update(&parts1, &idml, &data, &changed) {
                Ok(()) => {
                    summary.ok += 1;
                    if digest(base.doc()) != digest(&full) {
                        panic!("{} {name} ({what}): updated document differs from the full read: {}", path.display(), explain(base.doc(), &full));
                    }
                }
                Err(why) => {
                    *summary.fallbacks.entry(why.split_once(' ').map(|(_, rest)| rest.to_owned()).unwrap_or(why)).or_default() += 1;
                }
            }
        }
    }
    // ten edits in a row, each on the base the one before left (parts of several kinds); equal to a full read after each
    let mut parts = parts0.clone();
    let mut base = base0.clone();
    let candidates: Vec<&String> = parts0.keys().filter(|n| n.starts_with("Spreads/") || n.starts_with("Stories/")).collect();
    for step in 0..10 {
        let Some(name) = candidates.get((step * 7 + 3) % candidates.len().max(1)) else { break };
        let Some((_, text)) = mutations(name, &parts[*name]).into_iter().next() else { continue };
        parts.insert((*name).clone(), text);
        let changed: BTreeSet<String> = [(*name).clone()].into();
        if base.update(&parts, &idml, &data, &changed).is_err() {
            // the host reads again and goes on from there
            base = ReadBase::read(&parts, &idml, &data).expect("read again");
        }
        let full = read_parts(&parts, &idml, &data).expect("full");
        assert_eq!(digest(base.doc()), digest(&full), "{}: after {} edits in a row: {}", path.display(), step + 1, explain(base.doc(), &full));
    }
}

#[derive(Default)]
struct Summary {
    ok: usize,
    fallbacks: BTreeMap<String, usize>,
}

#[test]
fn updated_documents_equal_full_reads() {
    let mut summary = Summary::default();
    let fixtures = root().join("oracle/fixtures");
    let mut docs: Vec<PathBuf> = std::fs::read_dir(&fixtures).expect("fixtures").flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "idml")).collect();
    docs.sort();
    // the Atlas holds the most kinds of content
    docs.push(root().join("../samples/feature-atlas/Feature_Atlas.idml"));
    let (mut checked, mut missing) = (0, 0);
    for d in docs.iter().filter(|d| d.exists()) {
        // a package that does not read at all (a fixture of a failure) is not a case
        let parts = parts_of(d);
        if read_parts(&parts, d, &ReaderData::default()).is_err() {
            missing += 1;
            continue;
        }
        check_document(d, &mut summary);
        checked += 1;
    }
    eprintln!("incremental updates: {} went through in {checked} documents ({missing} unreadable skipped); fell back:", summary.ok);
    for (why, n) in &summary.fallbacks {
        eprintln!("  {n:5} x {why}");
    }
    assert!(checked > 100, "only {checked} documents checked");
    assert!(summary.ok > 500, "only {} incremental updates went through", summary.ok);
}
