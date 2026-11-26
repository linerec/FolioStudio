//! Collects source links, previews, embedded files and fonts for an import plan.

use super::messages::Warnings;
use super::names::{Names, file_name, free_document_name, recorded_name};
use super::{Count, FontRow, GraphicPlan, Item, LinkCounts, Plan, Prepared, Renamed, Src, for_each_part};
use crate::format::{FontFacts, RefKind};
use crate::idmlparts::{self, Edits};
use crate::manifest::Project;
use crate::paths::{self, RelPath, fix_name, split_ext};
use folio_links::LinkStatus;
use folio_model::{Document, ItemKind, PageItem};
use folio_text::FontStore;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Every graphic of the document with its frame's owner chain walked (spreads, masters,
/// anchored items, movie and sound posters), plus movie / sound items with a link.
pub(super) fn graphics_of(doc: &Document) -> Vec<(String, Option<folio_model::LinkInfo>)> {
    let mut out = Vec::new();
    let mut visit = |item: &PageItem| {
        item.walk(&mut |i| match &i.kind {
            ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => out.push((g.id.clone(), g.link.clone())),
            ItemKind::Other { .. } => {
                if let Some(l) = &i.print.media_link {
                    out.push((i.id.clone(), Some(l.clone())));
                }
                if let Some(g) = &i.print.poster {
                    out.push((g.id.clone(), g.link.clone()));
                }
            }
            _ => {}
        });
    };
    for s in doc.master_spreads.iter().chain(&doc.spreads) {
        for i in &s.items {
            visit(i);
        }
    }
    for st in doc.stories.values() {
        for r in st.all_runs() {
            for i in &r.inline {
                visit(i);
            }
        }
    }
    let mut seen = HashSet::new();
    out.retain(|(id, _)| seen.insert(id.clone()));
    out
}

/// Previews of the reader's `result.json`: graphic Self → display file, raw file.
fn reader_previews(dir: &Path, result: &Value) -> HashMap<String, (PathBuf, Option<PathBuf>)> {
    let mut out = HashMap::new();
    for p in result.get("previews").and_then(Value::as_array).into_iter().flatten() {
        let plain = |k: &str| p.get(k).and_then(Value::as_str).filter(|f| !f.contains(['/', '\\']) && !f.contains("..")).map(|f| dir.join(f));
        let Some(file) = plain("file").or_else(|| plain("rawFile")).filter(|f| f.is_file()) else { continue };
        let raw = plain("rawFile").filter(|f| f.is_file());
        for g in p.get("graphics").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            out.insert(g.to_string(), (file.clone(), raw.clone()));
        }
    }
    out
}

/// Extracted embedded files (`embedded.json`, folio-embedded 1): graphic Self → file.
fn reader_embedded(dir: &Path, result: &Value, warnings: &mut Warnings) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let Some(entry) = result.get("embedded").filter(|v| !v.is_null()) else { return out };
    let Some(name) = entry.get("file").and_then(Value::as_str).filter(|f| !f.contains(['/', '\\']) && !f.contains("..")) else { return out };
    let sidecar: Value = match std::fs::read_to_string(dir.join(name)).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        Some(v) => v,
        None => {
            warnings.push("import.embedded.unreadable", "embedded.json를 읽을 수 없어 내장 파일을 가져오지 못했습니다".into(), json!({}));
            return out;
        }
    };
    if sidecar.get("format").and_then(Value::as_str) != Some("folio-embedded") || sidecar.get("version").and_then(Value::as_u64) != Some(1) {
        warnings.push("import.embedded.unsupported", "embedded.json 형식을 모릅니다; 내장 파일을 가져오지 않았습니다".into(), json!({}));
        return out;
    }
    for it in sidecar.get("items").and_then(Value::as_array).into_iter().flatten() {
        let f = it.get("file").or_else(|| it.get("previewFile")).and_then(Value::as_str).filter(|f| !f.contains(['/', '\\']) && !f.contains(".."));
        let Some(path) = f.map(|f| dir.join(f)).filter(|p| p.is_file()) else { continue };
        for g in it.get("graphics").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            out.insert(g.to_string(), path.clone());
        }
    }
    out
}

fn add_item(items: &mut Vec<Item>, it: Item) -> usize {
    items.push(it);
    items.len() - 1
}

/// Plans importing `p` into `project` as `document` (root-relative `.folio` path).
pub fn plan(project: &Project, document: &RelPath, p: &Prepared<'_>) -> Plan {
    let started = Instant::now();
    let root = &project.root;
    let mut names = Names::default();
    let mut warnings = Warnings::default();
    let mut renamed = Vec::new();
    let mut long_paths = Vec::new();
    let stem = split_ext(document.file_name()).0.to_string();
    let assets_root = RelPath::parse(&fix_name(&project.file.settings.assets_folder)).unwrap_or_else(|_| RelPath::parse("assets").expect("assets is a valid fixed project folder"));
    let assets = assets_root.join(&fix_name(&stem)).unwrap_or_else(|_| assets_root.clone());
    let previews_dir = assets.join("previews").expect("previews is a valid fixed child name");
    let embedded_dir = assets.join("embedded").expect("embedded is a valid fixed child name");
    let fonts_dir = RelPath::parse(&fix_name(&project.file.settings.fonts_folder)).unwrap_or_else(|_| RelPath::parse("fonts").expect("fonts is a valid fixed project folder"));

    let mut items: Vec<Item> = Vec::new();
    let mut counts = LinkCounts::default();
    let mut by_source: HashMap<String, usize> = HashMap::new();
    let mut by_hint: HashMap<String, usize> = HashMap::new();
    let note_rename = |from: &str, to: &str, renamed: &mut Vec<Renamed>| {
        if from != to {
            renamed.push(Renamed { from: from.to_string(), to: to.to_string() });
        }
    };

    // 1. links
    let entries = p.links.by_owner();
    let graphics = graphics_of(p.doc);
    let mut gplans: Vec<GraphicPlan> = Vec::new();
    let previews = match (p.reader_dir, p.reader_result) {
        (Some(d), Some(r)) => reader_previews(d, r),
        _ => HashMap::new(),
    };
    let embedded = match (p.reader_dir, p.reader_result) {
        (Some(d), Some(r)) => reader_embedded(d, r, &mut warnings),
        _ => HashMap::new(),
    };
    for (self_id, link) in &graphics {
        let mut gp = GraphicPlan { self_id: self_id.clone(), link_self: link.as_ref().map(|l| l.id.clone()).filter(|s| !s.is_empty()), link: None, preview: None, embedded: None };
        if let Some(l) = link {
            counts.placements += 1;
            let entry = entries.get(self_id.as_str());
            let name = folio_links::parse_uri(&l.uri).and_then(|x| x.file_name().map(str::to_string)).unwrap_or_else(|| self_id.clone());
            let status = entry.map(|e| e.status.clone());
            let item = match (l.embedded(), status.flatten()) {
                (true, _) => {
                    counts.embedded += 1;
                    None
                }
                (false, Some(LinkStatus::Found { path, .. })) => {
                    let key = paths::collision_key(&path.to_string_lossy());
                    Some(match by_source.get(&key) {
                        Some(&i) => i,
                        None => {
                            let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
                            // The found file differs from the one InDesign placed when its size
                            // differs from the recorded one (content, not time: times change
                            // with every copy and never decide sameness).
                            let found = if l.size.is_some_and(|s| s != bytes) {
                                counts.modified += 1;
                                if counts.modified_names.len() < 50 {
                                    counts.modified_names.push(name.clone());
                                }
                                "modified"
                            } else {
                                "found"
                            };
                            let it = match paths::relative_to_root(root, &path) {
                                Ok(rel) => {
                                    counts.inside.count += 1;
                                    counts.inside.bytes += bytes;
                                    Item { kind: RefKind::Link, src: Src::Keep(rel.clone()), dest: Some(rel), hash_named: false, bytes, original_name: None, status: Some(found), hint: None, font: None, hash_only: None }
                                }
                                Err(_) => {
                                    counts.outside.count += 1;
                                    counts.outside.bytes += bytes;
                                    let disk = file_name(&path);
                                    let (n, changed) = names.unique(root, &assets, &disk);
                                    if changed {
                                        note_rename(&disk, &n, &mut renamed);
                                    }
                                    let dest = assets.join(&n).unwrap();
                                    Item {
                                        kind: RefKind::Link,
                                        src: Src::Copy(path.clone()),
                                        dest: Some(dest),
                                        hash_named: false,
                                        bytes,
                                        original_name: (n != disk).then(|| recorded_name(&disk)),
                                        status: Some(found),
                                        hint: None,
                                        font: None,
                                        hash_only: None,
                                    }
                                }
                            };
                            counts.files += 1;
                            let i = add_item(&mut items, it);
                            by_source.insert(key, i);
                            i
                        }
                    })
                }
                (false, other) => {
                    let ambiguous = matches!(other, Some(LinkStatus::Ambiguous(_)));
                    if ambiguous {
                        counts.ambiguous += 1;
                        if let Some(LinkStatus::Ambiguous(c)) = &other
                            && counts.ambiguous_rows.len() < 50
                        {
                            counts.ambiguous_rows.push(json!({"name": name, "candidates": c.len()}));
                        }
                    } else {
                        counts.missing += 1;
                        if counts.missing_names.len() < 50 && !counts.missing_names.contains(&name) {
                            counts.missing_names.push(name.clone());
                        }
                    }
                    let hint = l.uri.clone();
                    Some(*by_hint.entry(hint.clone()).or_insert_with(|| {
                        add_item(&mut items, Item { kind: RefKind::Link, src: Src::None, dest: None, hash_named: false, bytes: 0, original_name: None, status: Some(if ambiguous { "ambiguous" } else { "missing" }), hint: Some(hint), font: None, hash_only: None })
                    }))
                }
            };
            gp.link = item;
        }
        if let Some((file, _raw)) = previews.get(self_id) {
            let key = format!("preview:{}", file.display());
            let i = *by_source.entry(key).or_insert_with(|| {
                let bytes = std::fs::metadata(file).map_or(0, |m| m.len());
                add_item(&mut items, Item { kind: RefKind::Preview, src: Src::Move(file.clone()), dest: Some(previews_dir.clone()), hash_named: true, bytes, original_name: None, status: None, hint: None, font: None, hash_only: None })
            });
            gp.preview = Some(i);
        }
        if let Some(file) = embedded.get(self_id) {
            let key = format!("embedded:{}", file.display());
            let i = match by_source.get(&key) {
                Some(&i) => i,
                None => {
                    let bytes = std::fs::metadata(file).map_or(0, |m| m.len());
                    // embedded link: its own file name; pasted graphic: named after its content
                    let named = link.as_ref().filter(|l| l.embedded()).and_then(|l| folio_links::parse_uri(&l.uri)).and_then(|x| x.file_name().map(str::to_string));
                    let (dest, hash_named, original) = match named {
                        Some(n) => {
                            let (fixed, changed) = names.unique(root, &embedded_dir, &n);
                            if changed {
                                note_rename(&n, &fixed, &mut renamed);
                            }
                            (embedded_dir.join(&fixed).unwrap(), false, (fixed != n).then(|| recorded_name(&n)))
                        }
                        None => (embedded_dir.clone(), true, None),
                    };
                    let hint = link.as_ref().filter(|l| l.embedded()).map(|l| l.uri.clone());
                    let i = add_item(&mut items, Item { kind: RefKind::Embedded, src: Src::Move(file.clone()), dest: Some(dest), hash_named, bytes, original_name: original, status: Some("embedded"), hint, font: None, hash_only: None });
                    by_source.insert(key, i);
                    i
                }
            };
            gp.embedded = Some(i);
        }
        gplans.push(gp);
    }

    // 2. other file URIs of the parts (XMP history, data merge sources, file hyperlinks)
    let mut uris: BTreeMap<String, usize> = BTreeMap::new();
    let mut other_files = Vec::new();
    let mut found_uris: Vec<String> = Vec::new();
    // Links of the graphics table are rewritten by Link Self; any other `file:` URI
    // (including Link elements Folio does not model) is collected here.
    let known_links: HashMap<String, String> = gplans.iter().filter_map(|g| g.link_self.clone()).map(|s| (s, String::new())).collect();
    let mut collect = |u: &str| {
        found_uris.push(u.to_string());
        None
    };
    let mut edits = Edits { link_uris: known_links, other_uri: Some(&mut collect), native_paths: true, ..Default::default() };
    let mut limits = crate::limits::Scanner::new();
    if let Err(e) = for_each_part(p.idml, &mut |name, text| {
        limits.feed(name, text);
        if idmlparts::worth_scanning(name, text) {
            let _ = idmlparts::rewrite(name, text, &mut edits);
        }
    }) {
        warnings.push("import.idml.parts", format!("IDML 부분을 읽지 못했습니다: {e}"), json!({"error":e}));
    }
    drop(edits);
    for u in found_uris {
        if uris.contains_key(&u) {
            continue;
        }
        let status = (p.resolve_uri)(&u);
        let idx = match &status {
            // a document another one links to (a content link) is the project's own document or not here: no copy of it, only its address as a hint
            LinkStatus::Found { path, .. } if !is_document_path(&u) => {
                let key = paths::collision_key(&path.to_string_lossy());
                match by_source.get(&key) {
                    Some(&i) => i,
                    None => {
                        let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
                        let it = match paths::relative_to_root(root, path) {
                            Ok(rel) => Item { kind: RefKind::File, src: Src::Keep(rel.clone()), dest: Some(rel), hash_named: false, bytes, original_name: None, status: Some("found"), hint: None, font: None, hash_only: None },
                            Err(_) => {
                                let disk = file_name(path);
                                let (n, changed) = names.unique(root, &assets, &disk);
                                if changed {
                                    note_rename(&disk, &n, &mut renamed);
                                }
                                Item { kind: RefKind::File, src: Src::Copy(path.clone()), dest: Some(assets.join(&n).unwrap()), hash_named: false, bytes, original_name: (n != disk).then(|| recorded_name(&disk)), status: Some("found"), hint: None, font: None, hash_only: None }
                            }
                        };
                        let i = add_item(&mut items, it);
                        by_source.insert(key, i);
                        i
                    }
                }
            }
            _ => *by_hint.entry(u.clone()).or_insert_with(|| add_item(&mut items, Item { kind: RefKind::File, src: Src::None, dest: None, hash_named: false, bytes: 0, original_name: None, status: Some("missing"), hint: Some(u.clone()), font: None, hash_only: None })),
        };
        // names only: absolute paths stay in the reference's hint, nowhere else
        let shown = folio_links::parse_uri(&u).and_then(|l| l.file_name().map(str::to_string)).unwrap_or_default();
        other_files.push(json!({"name": shown, "found": matches!(status, LinkStatus::Found { .. }) && !is_document_path(&u)}));
        uris.insert(u, idx);
    }

    // 3. fonts
    let (font_rows, font_counts) = plan_fonts(project, p, &fonts_dir, &mut names, &mut items, &mut renamed);

    // 4. totals and checks
    let mut copy = Count::default();
    for it in &items {
        if matches!(it.src, Src::Copy(_) | Src::Move(_)) {
            copy.count += 1;
            copy.bytes += it.bytes;
        }
        if let Some(d) = &it.dest
            && !it.hash_named
            && d.is_long()
        {
            long_paths.push(d.to_string());
        }
    }
    let preview_count = Count { count: items.iter().filter(|i| i.kind == RefKind::Preview).count(), bytes: items.iter().filter(|i| i.kind == RefKind::Preview).map(|i| i.bytes).sum() };
    let emb: Vec<&Item> = items.iter().filter(|i| i.kind == RefKind::Embedded).collect();
    let embedded_counts = json!({"files": emb.len(), "bytes": emb.iter().map(|i| i.bytes).sum::<u64>(), "graphics": gplans.iter().filter(|g| g.embedded.is_some()).count()});
    let document_exists = document.on_disk(root).exists();
    let suggestion = document_exists.then(|| free_document_name(root, document)).flatten();
    if document.is_long() {
        long_paths.push(document.to_string());
    }
    let source = json!({"name": file_name(p.source), "kind": p.kind, "bytes": p.source_bytes, "sha256": p.source_sha256,
                        "pages": p.doc.spreads.iter().map(|s| s.pages.len()).sum::<usize>(), "stories": p.doc.stories.len()});
    Plan {
        source,
        document: document.clone(),
        document_exists,
        suggestion,
        assets,
        links: counts,
        previews: preview_count,
        embedded: embedded_counts,
        fonts: font_rows,
        font_counts,
        other_files,
        renamed,
        long_paths,
        copy,
        warnings: warnings.text,
        warning_messages: warnings.messages,
        differences: limits.hits(),
        ms: started.elapsed().as_secs_f64() * 1000.0,
        items,
        graphics: gplans,
        uris,
    }
}

fn font_version(store: &FontStore, family: &str, style: &str, table: &[folio_model::FontInfo]) -> Option<String> {
    let r = store.resolve(family, style, table);
    store
        .with_face(r.font, |f| {
            f.names()
                .into_iter()
                .filter(|n| n.name_id == 5)
                .find_map(|n| n.to_string())
        })
        .flatten()
}

fn plan_fonts(project: &Project, p: &Prepared<'_>, fonts_dir: &RelPath, names: &mut Names, items: &mut Vec<Item>, renamed: &mut Vec<Renamed>) -> (Vec<FontRow>, Value) {
    use folio_package::fonts;
    let store = p.fonts;
    let mut rows = Vec::new();
    struct Group {
        path: PathBuf,
        source: folio_text::FontSource,
        faces: Vec<(u32, bool)>,
        rows: Vec<usize>,
        facts: Vec<FontFacts>,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_file: HashMap<PathBuf, usize> = HashMap::new();
    for (family, style) in &p.doc.fonts_used {
        let version = font_version(store, family, style, &p.doc.font_table);
        let Some((face, cjk)) = fonts::exact_face(store, family, style, &p.doc.font_table) else {
            let installed = store.faces().iter().any(|i| i.family.eq_ignore_ascii_case(family));
            let action = if installed { "substituted" } else { "missing" };
            rows.push(FontRow { family: family.clone(), style: style.clone(), action, code: None, reason: None, file: None, version: None });
            add_item(items, Item { kind: RefKind::Font, src: Src::None, dest: None, hash_named: false, bytes: 0, original_name: None, status: Some(action), hint: None, font: Some(FontFacts { family: family.clone(), style: style.clone(), status: action.into(), ..Default::default() }), hash_only: None });
            continue;
        };
        let cjk_script = cjk || p.doc.font_table.iter().any(|f| &f.family == family && &f.style == style && f.writing_script.is_some_and(fonts::is_cjk_script));
        let path = (*face.path).clone();
        let g = *by_file.entry(path.clone()).or_insert_with(|| {
            groups.push(Group { path: path.clone(), source: face.source, faces: Vec::new(), rows: Vec::new(), facts: Vec::new() });
            groups.len() - 1
        });
        let group = &mut groups[g];
        match group.faces.iter_mut().find(|f| f.0 == face.index) {
            Some(f) => f.1 |= cjk_script,
            None => group.faces.push((face.index, cjk_script)),
        }
        group.rows.push(rows.len());
        group.facts.push(FontFacts { family: family.clone(), style: style.clone(), postscript: face.postscript.clone(), version: version.clone().unwrap_or_default(), status: String::new(), reason: None });
        rows.push(FontRow { family: family.clone(), style: style.clone(), action: "copy", code: None, reason: None, file: Some(file_name(&face.path)), version });
    }
    let (mut copy, mut record) = (0, 0);
    for g in groups {
        let bytes = std::fs::metadata(&g.path).map_or(0, |m| m.len());
        match fonts::decide(&g.path, g.source, &g.faces, false) {
            Ok(_) => {
                let disk = file_name(&g.path);
                let (n, changed) = names.unique(&project.root, fonts_dir, &disk);
                if changed {
                    renamed.push(Renamed { from: disk.clone(), to: n.clone() });
                }
                let dest = fonts_dir.join(&n).unwrap();
                let mut facts = g.facts[0].clone();
                facts.status = "copied".into();
                for &r in &g.rows {
                    rows[r].file = Some(dest.to_string());
                }
                // one reference per file; the faces it serves are in the rows
                let src = match paths::relative_to_root(&project.root, &g.path) {
                    Ok(rel) => Src::Keep(rel),
                    Err(_) => Src::Copy(g.path.clone()),
                };
                let dest = match &src {
                    Src::Keep(rel) => rel.clone(),
                    _ => dest,
                };
                add_item(items, Item { kind: RefKind::Font, src, dest: Some(dest), hash_named: false, bytes, original_name: (n != disk).then(|| recorded_name(&disk)), status: Some("copied"), hint: None, font: Some(facts), hash_only: None });
                copy += g.rows.len();
            }
            Err(skip) => {
                for &r in &g.rows {
                    rows[r].action = "record";
                    rows[r].code = Some(skip.code);
                    rows[r].reason = Some(skip.reason.clone());
                }
                for mut facts in g.facts {
                    facts.status = skip.code.into();
                    facts.reason = Some(skip.reason.clone());
                    add_item(items, Item { kind: RefKind::Font, src: Src::None, dest: None, hash_named: false, bytes, original_name: None, status: Some("notCopied"), hint: None, font: Some(facts), hash_only: Some(g.path.clone()) });
                }
                record += g.rows.len();
            }
        }
    }
    let missing = rows.iter().filter(|r| r.action == "missing").count();
    let substituted = rows.iter().filter(|r| r.action == "substituted").count();
    (rows, json!({"used": p.doc.fonts_used.len(), "copy": copy, "record": record, "missing": missing, "substituted": substituted}))
}

/// Whether a path names an InDesign (or Folio) document: what a content link to another document points at.
fn is_document_path(u: &str) -> bool {
    let lower = u.to_lowercase();
    [".indd", ".indt", ".indb", ".idml", ".inx", ".folio"].iter().any(|e| lower.ends_with(e))
}
