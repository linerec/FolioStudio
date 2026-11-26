//! Runs the phases of PDF export and builds the export report.

use crate::colors::{CsDef, Resolver};
use crate::output::document::{WriteInput, Written, layer_groups, write_document};
use crate::pages::content::{DrawnPages, MarkGlyphs, PageDrawInput, draw_pages};
use crate::pages::{page_info_text, plan, simple_shape};
use crate::settings::OutputSettings;
use crate::registry::{FormDef, Registry};
use crate::{ExportError, ExportOptions, Exported, FontReport, InkReport, LinkReport, LinkUsage, OutputIntentReport, Preset, Report, ConvertReport};
use crate::{flatten, fonts, graphics, images, memory, meta, spotconv};
use folio_model::layout::DocumentLayout;
use folio_model::Document;
use folio_text::FontStore;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

/// `parallel`, with each job holding `cost(i)` of a shared `budget` while it runs (large
/// images decode one at a time instead of all at once).
fn parallel_budget<T: Send>(n: usize, threads: usize, budget: u64, cost: impl Fn(usize) -> u64 + Sync, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let budget = memory::Budget::new(budget);
    parallel(n, threads, |i| budget.run(cost(i), || f(i)))
}

/// Runs `f` over `0..n` on up to `threads` workers; results in index order.
fn parallel<T: Send>(n: usize, threads: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    parallel_with_state(n, threads, || (), |_, i| f(i))
}

fn parallel_with_state<T: Send, S>(n: usize, threads: usize, init: impl Fn() -> S + Sync, f: impl Fn(&mut S, usize) -> T + Sync) -> Vec<T> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = threads.clamp(1, n.max(1));
    let mut slots: Vec<Option<T>> = (0..n).map(|_| None).collect();
    let results = std::sync::Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let mut state = init();
                let mut local = Vec::new();
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    local.push((i, f(&mut state, i)));
                }
                results.lock().expect("PDF worker/cache lock is not poisoned").extend(local);
            });
        }
    });
    for (i, v) in results.into_inner().expect("PDF worker result lock is not poisoned") {
        slots[i] = Some(v);
    }
    slots.into_iter().map(|s| s.expect("worker result")).collect()
}

/// Writes the document as PDF/X-4.
pub fn export(
    doc: &Document,
    layout: &DocumentLayout,
    fonts: &FontStore,
    links: Option<LinkReport>,
    opts: &ExportOptions,
) -> Result<Exported, ExportError> {
    if opts.preset == Preset::Small { return Err(ExportError::OutputProfile("smallest-file RGB conversion is not supported".into())); }
    let started = Instant::now();
    let mut timings = BTreeMap::new();
    let threads = if opts.threads == 0 { std::thread::available_parallelism().map_or(4, |n| n.get()) } else { opts.threads };

    let OutputSettings {
        profile_path,
        profile,
        facts,
        rgb_profile,
        preset,
        flattening,
        cmyk_only,
        extra_warnings,
        colors,
        conv,
    } = OutputSettings::resolve(doc, opts)?;

    if opts.booklet.is_some() && (flattening || opts.marks.any()) {
        return Err(ExportError::Unsupported("a booklet is written as PDF/X-4 without printer's marks (flattening presets and marks are not supported)".into()));
    }
    let pages = plan(doc, opts);
    if pages.is_empty() {
        return Err(ExportError::NoPages);
    }
    let export_secs = meta::now_secs();
    let date = meta::xmp_date(export_secs);

    // Page information glyphs.
    let info_font = if opts.marks.page_info { fonts.latin_fallback() } else { None };
    let stamp = format!("{}  {} UTC", &date[..10], &date[11..16]);
    let info_glyphs: Vec<MarkGlyphs> = pages
        .iter()
        .map(|p| {
            let (l, r) = page_info_text(opts, p, &stamp);
            info_font.map(|f| (simple_shape(fonts, f, &l), simple_shape(fonts, f, &r))).unwrap_or_default()
        })
        .collect();
    let extra: Vec<(u32, u16)> =
        info_font.map(|f| info_glyphs.iter().flat_map(|(a, b)| a.iter().chain(b)).map(|(g, _)| (f, *g)).collect()).unwrap_or_default();

    let t = Instant::now();
    let mut font_reg = fonts::FontRegistry::prepare(layout, fonts, &extra);
    let ids = font_reg.embeddable();
    let files = parallel(ids.len(), threads, |i| fonts::subset_file(&font_reg.entries[&ids[i]], fonts));
    font_reg.take_files(files);
    timings.insert("subset".into(), t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    let resolver = match (&colors, cmyk_only) {
        (Some(c), true) => Resolver::with_cmyk_output(doc, c.clone()),
        _ => Resolver::new(doc),
    };
    let mut gfx = graphics::Graphics::new(links, fonts, spotconv::InkRules::from_doc(doc));
    gfx.set_output(flattening, colors.clone().filter(|_| cmyk_only));
    let mut reg = Registry::new(resolver, font_reg, gfx);
    // Export Layers and Create Acrobat Layers (InDesign 2025, buildc2-ocg-probe): the groups are "Guides and Grids" (always off) and
    // then the document layers from the bottom one up; with Acrobat layers every layer's content is written in its group
    reg.export_layers = opts.export_layers;
    let ocgs = if opts.acrobat_layers && preset.layers_allowed() { layer_groups(doc, opts.export_layers) } else { Vec::new() };
    if !ocgs.is_empty() {
        reg.ocg_of_layer = Some(ocgs.iter().enumerate().filter_map(|(i, g)| g.layer.map(|l| (l.to_owned(), i))).collect());
    }
    reg.counts.layout_glyphs = layout.frames.values().flat_map(|f| &f.lines).map(|l| l.glyphs.len()).sum();
    // Phase 1: page content in document order.
    let DrawnPages { outs, mut flat_jobs, mut flat_report, flat_kinds, flat_page_slot } = draw_pages(
        &mut reg,
        PageDrawInput { doc, layout, fonts, opts, pages: &pages, flattening, info_font, info_glyphs: &info_glyphs },
    );
    timings.insert("content".into(), t.elapsed().as_secs_f64() * 1000.0);
    reg.graphics.release_sources();

    // Phase 2: fonts, images, compression on worker threads.
    let t = Instant::now();
    let font_ids: Vec<u32> = reg.fonts.entries.values().filter(|e| e.drawn && e.embeddable).map(|e| e.font).collect();
    let subsets: Vec<Result<fonts::Subset, String>> =
        parallel(font_ids.len(), threads, |i| fonts::build_subset(&reg.fonts.entries[&font_ids[i]], fonts));
    timings.insert("fonts".into(), t.elapsed().as_secs_f64() * 1000.0);
    let t = Instant::now();
    let level = opts.compression;
    let settings = images::Settings {
        doc_rgb: rgb_profile.clone().map(Arc::new),
        level,
        image_compression: opts.image_compression.clone(),
        jpeg_quality: opts.jpeg_quality,
        downsample: opts.downsample.map(|(above, to)| images::Downsample { above, to }),
        convert: conv.clone().filter(|_| cmyk_only || preset == Preset::X3),
        no_smask: flattening,
    };
    let jobs = &reg.graphics.images;
    let mut encoded: Vec<Result<images::Encoded, String>> = parallel_budget(
        jobs.len(),
        threads,
        if flattening { opts.image_memory.min(384 << 20) } else { opts.image_memory },
        // decoded samples, their working copies and the compressed result
        |i| if jobs[i].uses == 0 { 0 } else { jobs[i].decoded.map(|d| d * 3).unwrap_or(jobs[i].bytes * 8) },
        // an image only flattened regions show is not written
        |i| if jobs[i].uses == 0 { Err(String::new()) } else { images::encode(&jobs[i], &settings) },
    );
    timings.insert("images".into(), t.elapsed().as_secs_f64() * 1000.0);

    // Flattened regions rendered as ink rasters, row by row of tiles.
    let t = Instant::now();
    let mut flat_tiles: Vec<Vec<flatten::TileImage>> = (0..flat_jobs.len()).map(|_| Vec::new()).collect();
    let doc_inks = (!flat_jobs.is_empty()).then(|| folio_render::sep::document_inks(doc, layout, colors.as_ref().expect("colours when flattening")));
    let mut flat_failures: Vec<String> = Vec::new();
    if let (Some(inks), Some(colors), Some(conv)) = (&doc_inks, &colors, &conv) {
        let glyph_cache = folio_render::GlyphCache::new();
        let image_cache = folio_render::ImageCache::new();
        let scene = folio_render::Scene { doc, colors, layout, glyphs: fonts, glyph_cache: &glyph_cache, images: &image_cache };
        let ink_images = folio_render::sep::InkImages::new();
        let decode_budget = (opts.image_memory / 6).max(1);
        let tile_budget = (opts.image_memory / 6).max(1);
        let source = flatten::PrintImages::new(reg.graphics.entries(), conv, decode_budget);
        let ctx = flatten::RenderCtx {
            scene: &scene,
            inks,
            ink_images: &ink_images,
            source: Some(&source),
            smooth_ppi: opts.flatten.smooth_ppi,
            line_art_ppi: opts.flatten.line_art_ppi,
            level,
        };
        // effect rasters shared by the regions of a page
        let fx: Vec<folio_render::sep::FxCache> = (0..outs.len()).map(|_| folio_render::sep::FxCache::new()).collect();
        // pass 1: which tiles hard edges of line art and text cross (smooth resolution)
        let rows: Vec<(usize, (i32, i32, i32))> = flat_jobs.iter().enumerate().flat_map(|(k, j)| j.cluster.runs.iter().map(move |r| (k, *r))).collect();
        let edges = parallel_budget(rows.len(), threads, tile_budget, |i| {
            let (_, (_, i0, i1)) = rows[i];
            flatten::work_bytes(i1 - i0 + 1, 1, 1.0, inks.len())
        }, |i| {
            let (k, run) = rows[i];
            let started = Instant::now();
            let hard = flatten::hard_tiles(&ctx, &flat_jobs[k], run, &fx[flat_jobs[k].out_page]);
            (hard, started.elapsed().as_secs_f64() * 1000.0)
        });
        let mut hard: Vec<HashMap<(i32, i32), bool>> = (0..flat_jobs.len()).map(|_| HashMap::new()).collect();
        let mut detail: Vec<(f64, bool)> = vec![(0.0, false); flat_jobs.len()];
        for (i, (h, ms)) in edges.into_iter().enumerate() {
            flat_report.render_ms += ms;
            let (k, (j, i0, _)) = rows[i];
            detail[k] = (detail[k].0.max(h.image_detail), detail[k].1 || h.soft);
            for (n, v) in h.hard.into_iter().enumerate() {
                hard[k].insert((i0 + n as i32, j), v);
            }
        }
        for (k, job) in flat_jobs.iter_mut().enumerate() {
            job.smooth_ppi = Some(flatten::smooth_ppi_for(opts.flatten.smooth_ppi, detail[k].0, detail[k].1));
        }
        let flat_jobs = &flat_jobs;
        // pass 2: rectangles of tiles at their resolution
        let rects: Vec<(usize, flatten::TileRect)> = flat_jobs.iter().enumerate().flat_map(|(k, j)| flatten::plan_rects(j, &hard[k]).into_iter().map(move |r| (k, r))).collect();
        let budget = memory::Budget::new(tile_budget);
        let rendered = parallel_with_state(rects.len(), threads, flatten::SampleBuffer::default, |samples, i| {
            let (k, r) = rects[i];
            let ppi = if r.line_art { ctx.line_art_ppi } else { flat_jobs[k].smooth_ppi.unwrap_or(ctx.smooth_ppi) };
            let bytes = flatten::work_bytes(r.i1 - r.i0 + 1, r.j1 - r.j0 + 1, ppi / ctx.smooth_ppi, inks.len());
            budget.run(bytes, || {
                let started = Instant::now();
                let image = flatten::render_rect(&ctx, &flat_jobs[k], r, &fx[flat_jobs[k].out_page], samples);
                (image, started.elapsed().as_secs_f64() * 1000.0)
            })
        });
        for (i, (image, ms)) in rendered.into_iter().enumerate() {
            flat_report.render_ms += ms;
            if let Some(image) = image {
                flat_tiles[rects[i].0].push(image);
            }
        }
        flat_failures = source.failures.lock().expect("PDF worker/cache lock is not poisoned").iter().take(8).cloned().collect();
    }
    for e in &flat_failures {
        reg.warn.add("image", format!("image could not be decoded for a flattened area ({e}); its preview was used"), None, None);
    }
    flat_report.kinds = flat_kinds.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let preview_graphics = flat_kinds.get("placed PDF drawn from its preview").copied().unwrap_or(0);
    flat_report.preview_graphics = preview_graphics;
    if preview_graphics > 0 {
        reg.warn.add("transparency", format!("{preview_graphics} placed PDF(s) with transparency or colour PDF 1.3 cannot hold were flattened from their low-resolution preview"), None, None);
    }
    for (k, tiles) in flat_tiles.iter().enumerate() {
        let slot = flat_page_slot.get(&flat_jobs[k].out_page).copied();
        for t in tiles {
            let bytes = t.layers.iter().map(|l| l.data.len() as u64).sum::<u64>();
            flat_report.images += t.layers.len();
            flat_report.bytes += bytes;
            if let Some(s) = slot {
                flat_report.per_page[s].2 += bytes;
            }
            if t.line_art {
                flat_report.line_art_images += t.layers.len();
                flat_report.line_art_area_pt2 += t.area;
            } else {
                flat_report.smooth_area_pt2 += t.area;
            }
        }
    }
    if flattening {
        timings.insert("flatten".into(), t.elapsed().as_secs_f64() * 1000.0);
    }
    let t = Instant::now();
    let page_data: Vec<Vec<u8>> = parallel(outs.len(), threads, |i| images::flate(&outs[i].content, level));
    let form_data: Vec<Vec<u8>> = parallel(reg.forms.len(), threads, |i| match &reg.forms[i] {
        FormDef::Group { content, .. } | FormDef::Mask { content, .. } => images::flate(content, level),
        FormDef::Imported(_) => Vec::new(),
        FormDef::Flat(k) => images::flate(&flatten::form_content(&flat_tiles[*k]), level),
    });
    timings.insert("compress".into(), t.elapsed().as_secs_f64() * 1000.0);

    // Phase 3: objects.
    let t = Instant::now();
    let Written {
        bytes,
        fonts_embedded,
        images_written,
        placed_converted,
        identifier,
    } = write_document(&mut reg, WriteInput {
        doc,
        opts,
        fonts,
        preset,
        flattening,
        cmyk_only,
        level,
        profile: &profile,
        facts: &facts,
        rgb_profile: rgb_profile.as_deref(),
        ocgs: &ocgs,
        outs: &outs,
        page_data: &page_data,
        form_data: &form_data,
        font_ids: &font_ids,
        subsets: &subsets,
        encoded: &mut encoded,
        flat_jobs: &flat_jobs,
        flat_tiles: &mut flat_tiles,
        flat_report: &flat_report,
        doc_inks: doc_inks.as_ref(),
        export_secs,
        date: &date,
    })?;
    timings.insert("write".into(), t.elapsed().as_secs_f64() * 1000.0);
    timings.insert("total".into(), started.elapsed().as_secs_f64() * 1000.0);

    // Report.
    for (k, m) in reg.res.warnings.drain(..).chain(extra_warnings) {
        reg.warn.add(&k, m, None, None);
    }
    // What the document reader could not model is missing from the print as well.
    // Effects, opacity and blend modes are reported per drawn item instead (the writer draws
    // drop shadows, glows, feathers and partial transparency itself).
    let per_item = |w: &str| {
        w.starts_with("effect ") || w.starts_with("blend mode ") || w.ends_with(" opacity not modelled")
    };
    for w in doc.warnings.iter().filter(|w| !per_item(w.as_str())) {
        reg.warn.add("document", w.clone(), None, None);
    }
    let mut inks: Vec<InkReport> = Vec::new();
    for ink in &doc.print.inks {
        let used = reg.res.ink_use.get(&ink.name);
        inks.push(InkReport {
            name: ink.name.clone(),
            kind: if ink.process { "process".into() } else { "spot".into() },
            output: match (ink.process, used) {
                (true, _) => "own plate".into(),
                (false, Some(u)) => u.output.clone(),
                (false, None) => if ink.convert_to_process { "converted to process (unused)".into() } else { "own plate (unused)".into() },
            },
            uses: used.map_or(0, |u| u.uses),
        });
    }
    for (name, u) in &reg.res.ink_use {
        if !doc.print.inks.iter().any(|i| &i.name == name) {
            inks.push(InkReport { name: name.clone(), kind: "spot".into(), output: u.output.clone(), uses: u.uses });
        }
    }
    reg.counts.separation_spaces = reg.res.spaces.defs.iter().filter(|d| matches!(d, CsDef::Separation { .. })).count();
    reg.counts.devicen_spaces = reg.res.spaces.defs.iter().filter(|d| matches!(d, CsDef::DeviceN { .. })).count();
    let mut fonts_missing = Vec::new();
    for (family, style) in &layout.missing_fonts {
        let r = fonts.resolve(family, style, &doc.font_table);
        let characters = doc
            .stories
            .values()
            .flat_map(|s| &s.paragraphs)
            .flat_map(|p| &p.runs)
            .filter(|r| &r.attrs.font_family == family && &r.attrs.font_style == style)
            .map(|r| r.text.chars().count())
            .sum();
        fonts_missing.push(FontReport {
            family: family.clone(),
            style: style.clone(),
            embedded: fonts.face_info(r.font).map(|f| f.postscript.clone()),
            exact: r.exact,
            glyphs: 0,
            characters,
        });
    }
    if !fonts_missing.is_empty() {
        reg.warn.add("font", format!("{} font(s) missing: text set and embedded with substitutes (a print shop will reject this)", fonts_missing.len()), None, None);
    }
    let mut links_summary: BTreeMap<String, usize> = BTreeMap::new();
    let links: Vec<LinkUsage> = reg
        .graphics
        .links
        .values()
        .map(|l| {
            *links_summary.entry(l.used.clone()).or_default() += 1;
            LinkUsage {
                uri: l.uri.clone(),
                used: l.used.clone(),
                how: l.how.clone(),
                path: l.path.clone(),
                pages: l.pages.clone(),
                placements: l.placements,
                written: l.written.clone(),
            }
        })
        .collect();
    let converted = cmyk_only.then(|| ConvertReport {
        destination: facts.description.clone(),
        rgb_images: conv.as_ref().map_or(0, |c| c.converted.load(std::sync::atomic::Ordering::Relaxed)),
        rgb_images_own_profile: conv.as_ref().map_or(0, |c| c.own_profile.load(std::sync::atomic::Ordering::Relaxed)),
        placed_pdfs: placed_converted,
    });
    let report = Report {
        ok: true,
        preset: preset.key().into(),
        standard: preset.standard().into(),
        output: String::new(),
        bytes: bytes.len() as u64,
        output_intent: OutputIntentReport {
            profile: profile_path.display().to_string(),
            description: facts.description.clone(),
            identifier: identifier.into(),
            bytes: profile.len(),
            class: String::from_utf8_lossy(&facts.class).into_owned(),
            version: format!("{}.{}", facts.version.0, facts.version.1),
        },
        images_written,
        counts: reg.counts.clone(),
        inks,
        fonts_missing,
        fonts_embedded,
        links,
        links_summary,
        warnings: reg.warn.map.into_values().collect(),
        timings_ms: timings,
        flatten: flattening.then_some(flat_report),
        converted,
    };
    Ok(Exported { pdf: bytes, report })
}
