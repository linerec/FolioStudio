//! `idml-dump <document.idml> [result.json] [--full out.json] [--stories N]`
//!
//! Reads an IDML package (with the INDD reader's result.json, found next to the package when
//! not given) and prints a JSON summary: counts per kind, the first stories in reading order,
//! missing references, warnings and the read time. `--full` also writes the whole Document.

use folio_model::{Document, ItemKind, Paint, PageItem};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn usage() -> ! {
    eprintln!("usage: idml-dump <document.idml> [result.json] [--full out.json] [--stories N]");
    std::process::exit(2);
}

fn kind_name(i: &PageItem) -> &str {
    match &i.kind {
        ItemKind::Rectangle { .. } => "Rectangle",
        ItemKind::Oval { .. } => "Oval",
        ItemKind::Polygon { .. } => "Polygon",
        ItemKind::GraphicLine => "GraphicLine",
        ItemKind::TextFrame(_) => "TextFrame",
        ItemKind::Group { .. } => "Group",
        ItemKind::Other { element } => element,
    }
}

fn graphic(i: &PageItem) -> Option<&folio_model::Graphic> {
    match &i.kind {
        ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => content.as_ref(),
        _ => None,
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut positional: Vec<PathBuf> = Vec::new();
    let mut full: Option<PathBuf> = None;
    let mut story_count = 5usize;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--full" => full = Some(args.next().unwrap_or_else(|| usage()).into()),
            "--stories" => story_count = args.next().and_then(|n| n.parse().ok()).unwrap_or_else(|| usage()),
            "-h" | "--help" => usage(),
            _ => positional.push(a.into()),
        }
    }
    let Some(idml) = positional.first().cloned() else { usage() };
    let result = positional.get(1).cloned().or_else(|| {
        let r = idml.parent().unwrap_or(Path::new(".")).join("result.json");
        r.is_file().then_some(r)
    });

    let t = Instant::now();
    let doc = match folio_idml::read_package(&idml, result.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("idml-dump: {e}");
            std::process::exit(1);
        }
    };
    let elapsed = t.elapsed();

    let summary = summarize(&doc, result.as_deref(), elapsed.as_secs_f64() * 1000.0, story_count);
    println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
    if let Some(out) = full {
        let text = serde_json::to_string(&doc).unwrap_or_default();
        if let Err(e) = std::fs::write(&out, text) {
            eprintln!("idml-dump: {}: {e}", out.display());
            std::process::exit(1);
        }
        eprintln!("idml-dump: full document written to {}", out.display());
    }
    eprintln!("idml-dump: read in {:.1} ms", elapsed.as_secs_f64() * 1000.0);
}

fn summarize(doc: &Document, result: Option<&Path>, ms: f64, story_count: usize) -> Value {
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut total, mut inline, mut clipped, mut graphics, mut previews, mut wraps) = (0, 0, 0, 0, 0, 0);
    let mut graphic_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut no_preview: Vec<String> = Vec::new();
    let mut missing_story: Vec<String> = Vec::new();
    let mut missing_paint: BTreeSet<String> = BTreeSet::new();
    let mut count = |i: &PageItem, is_inline: bool| {
        i.walk(&mut |x| {
            total += 1;
            *kinds.entry(kind_name(x).to_owned()).or_default() += 1;
            clipped += x.clipped.len();
            wraps += x.wraps as usize;
            if let Some(g) = graphic(x) {
                graphics += 1;
                *graphic_kinds.entry(g.element.clone()).or_default() += 1;
                if g.preview.is_some() {
                    previews += 1;
                } else {
                    no_preview.push(g.id.clone());
                }
            }
            if let ItemKind::TextFrame(tf) = &x.kind
                && !doc.stories.contains_key(&tf.story)
            {
                missing_story.push(format!("{} → {}", x.id, tf.story));
            }
            for p in [&x.fill, &x.stroke.paint] {
                match p {
                    Paint::Swatch { id, .. } if !doc.swatches.contains_key(id) => {
                        missing_paint.insert(id.clone());
                    }
                    Paint::Gradient { id, .. } if !doc.gradients.contains_key(id) => {
                        missing_paint.insert(id.clone());
                    }
                    _ => {}
                }
            }
        });
        if is_inline {
            inline += 1;
        }
    };
    for s in doc.master_spreads.iter().chain(&doc.spreads) {
        for i in &s.items {
            count(i, false);
        }
    }
    for st in doc.stories.values() {
        for r in st.paragraphs.iter().flat_map(|p| &p.runs) {
            for i in &r.inline {
                count(i, true);
            }
        }
    }

    let (mut paragraphs, mut runs, mut chars, mut variables) = (0, 0, 0, 0);
    for st in doc.stories.values() {
        paragraphs += st.paragraphs.len();
        runs += st.paragraphs.iter().map(|p| p.runs.len()).sum::<usize>();
        variables += st.paragraphs.iter().flat_map(|p| &p.runs).filter(|r| r.variable.is_some()).count();
        chars += folio_idml::story_chars(st);
    }

    // Stories in reading order: as their first frame appears in the spreads.
    let mut order: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    for s in &doc.spreads {
        for i in &s.items {
            i.walk(&mut |x| {
                if let ItemKind::TextFrame(tf) = &x.kind
                    && seen.insert(tf.story.as_str())
                {
                    order.push(tf.story.as_str());
                }
            });
        }
    }
    let first_stories: Vec<Value> = order
        .iter()
        .filter_map(|id| doc.stories.get(*id))
        .filter(|s| folio_idml::story_chars(s) > 1)
        .take(story_count)
        .map(|s| {
            let text: String = s
                .paragraphs
                .iter()
                .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
                .join("¶");
            let first = s.paragraphs.iter().flat_map(|p| p.runs.iter().map(move |r| (p, r))).find(|(_, r)| !r.text.is_empty());
            json!({
                "id": s.id,
                "paragraphs": s.paragraphs.len(),
                "characters": folio_idml::story_chars(s),
                "first_run": first.map(|(p, r)| json!({
                    "paragraph_style": p.style,
                    "justification": format!("{:?}", p.attrs.justification),
                    "font": format!("{} {}", r.attrs.font_family, r.attrs.font_style),
                    "size": r.attrs.point_size,
                    "leading": format!("{:?}", r.attrs.leading),
                    "tracking": r.attrs.tracking,
                    "horizontal_scale": r.attrs.horizontal_scale,
                })),
                "text": text.chars().take(240).collect::<String>(),
            })
        })
        .collect();

    let declared: HashSet<(&str, &str)> = doc.font_table.iter().map(|f| (f.family.as_str(), f.style.as_str())).collect();
    let undeclared_fonts: Vec<String> = doc
        .fonts_used
        .iter()
        .filter(|(f, s)| !declared.contains(&(f.as_str(), s.as_str())))
        .map(|(f, s)| format!("{f} {s}"))
        .collect();
    let masters: HashSet<&str> = doc.master_spreads.iter().map(|s| s.id.as_str()).collect();
    let missing_masters: Vec<String> = doc
        .spreads
        .iter()
        .flat_map(|s| &s.pages)
        .filter_map(|p| p.master.as_ref().filter(|m| !masters.contains(m.as_str())).map(|m| format!("page {} → {m}", p.name)))
        .collect();

    let mut counts = Map::new();
    counts.insert("spreads".into(), json!(doc.spreads.len()));
    counts.insert("master_spreads".into(), json!(doc.master_spreads.len()));
    counts.insert("pages".into(), json!(doc.spreads.iter().map(|s| s.pages.len()).sum::<usize>()));
    counts.insert("master_pages".into(), json!(doc.master_spreads.iter().map(|s| s.pages.len()).sum::<usize>()));
    counts.insert("page_items".into(), json!(total));
    counts.insert("page_items_by_kind".into(), json!(kinds));
    counts.insert("inline_items".into(), json!(inline));
    counts.insert("clipped_items".into(), json!(clipped));
    counts.insert("wrapping_items".into(), json!(wraps));
    counts.insert("graphics".into(), json!(graphics));
    counts.insert("graphics_by_kind".into(), json!(graphic_kinds));
    counts.insert("graphics_with_preview".into(), json!(previews));
    counts.insert("stories".into(), json!(doc.stories.len()));
    counts.insert("paragraphs".into(), json!(paragraphs));
    counts.insert("runs".into(), json!(runs));
    counts.insert("characters".into(), json!(chars));
    counts.insert("computed_text_markers".into(), json!(variables));
    counts.insert("swatches".into(), json!(doc.swatches.len()));
    counts.insert("gradients".into(), json!(doc.gradients.len()));
    counts.insert("layers".into(), json!(doc.layers.len()));
    counts.insert("fonts_used".into(), json!(doc.fonts_used.len()));
    counts.insert("font_table".into(), json!(doc.font_table.len()));

    json!({
        "source": {
            "path": doc.source.path,
            "bytes": doc.source.bytes,
            "reader": doc.source.reader,
            "dom_version": doc.source.dom_version,
            "reader_result": result.map(|r| r.display().to_string()),
        },
        "read_ms": (ms * 10.0).round() / 10.0,
        "counts": counts,
        "first_stories": first_stories,
        "fonts_used": doc.fonts_used.iter().map(|(f, s)| format!("{f} {s}")).collect::<Vec<_>>(),
        "missing": {
            "stories_of_text_frames": missing_story,
            "paint_swatches": missing_paint,
            "graphic_previews": no_preview,
            "masters": missing_masters,
            "fonts_not_in_font_table": undeclared_fonts,
        },
        "warnings": doc.warnings,
    })
}
