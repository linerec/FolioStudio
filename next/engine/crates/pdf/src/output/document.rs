//! Serializes assembled pages, resources, and metadata in PDF object order.

use super::assets::{ImageColorRefs, WrittenImages, write_fonts, write_images};
use super::color_spaces::{ColorSpaceInput, WrittenColorSpaces, write_color_spaces};
use super::page_tree::{PageTree, write_page_tree};
use super::{RefTables, Refs, blend, write_ramp, write_resources};
use crate::colors;
use crate::pages::PageOut;
use crate::registry::{FormDef, GsDef, Registry, ShadingSpace};
use crate::{EmbeddedFont, ExportError, ExportLayers, ExportOptions, FlattenReport, Preset};
use crate::{flatten, fonts, images, meta, title};
use folio_model::Document;
use folio_text::FontStore;
use pdf_writer::{Filter, Name, Pdf, Ref, Str, TextStr};
use std::collections::{BTreeMap, HashMap};

pub(crate) struct WriteInput<'a, 'd> {
    pub(crate) doc: &'d Document,
    pub(crate) opts: &'a ExportOptions,
    pub(crate) fonts: &'d FontStore,
    pub(crate) preset: Preset,
    pub(crate) flattening: bool,
    pub(crate) cmyk_only: bool,
    pub(crate) level: u32,
    pub(crate) profile: &'a [u8],
    pub(crate) facts: &'a meta::IccFacts,
    pub(crate) rgb_profile: Option<&'a [u8]>,
    pub(crate) ocgs: &'a [LayerGroup<'d>],
    pub(crate) outs: &'a [PageOut],
    pub(crate) page_data: &'a [Vec<u8>],
    pub(crate) form_data: &'a [Vec<u8>],
    pub(crate) font_ids: &'a [u32],
    pub(crate) subsets: &'a [Result<fonts::Subset, String>],
    pub(crate) encoded: &'a mut [Result<images::Encoded, String>],
    pub(crate) flat_jobs: &'a [flatten::FlatJob],
    pub(crate) flat_tiles: &'a mut [Vec<flatten::TileImage>],
    pub(crate) flat_report: &'a FlattenReport,
    pub(crate) doc_inks: Option<&'a folio_render::sep::DocInks>,
    pub(crate) export_secs: i64,
    pub(crate) date: &'a str,
}

pub(crate) struct Written {
    pub(crate) bytes: Vec<u8>,
    pub(crate) fonts_embedded: Vec<EmbeddedFont>,
    pub(crate) images_written: BTreeMap<String, usize>,
    pub(crate) placed_converted: usize,
    pub(crate) identifier: &'static str,
}

pub(crate) fn write_document<'d>(reg: &mut Registry<'d>, input: WriteInput<'_, 'd>) -> Result<Written, ExportError> {
    let WriteInput {
        doc,
        opts,
        fonts,
        preset,
        flattening,
        cmyk_only,
        level,
        profile,
        facts,
        rgb_profile,
        ocgs,
        outs,
        page_data,
        form_data,
        font_ids,
        subsets,
        encoded,
        flat_jobs,
        flat_tiles,
        flat_report,
        doc_inks,
        export_secs,
        date,
    } = input;
    let import_files = reg.graphics.take_import_files();
    let mut pdf = if flattening {
        let streams = encoded.iter().flatten().map(|e| {
            e.data.len() + e.mask.as_ref().map_or(0, |(_, _, data)| data.len()) + e.smask.as_ref().map_or(0, Vec::len)
        }).sum::<usize>();
        let placed = import_files.iter().map(|f| f.chunk.len()).sum::<usize>();
        let content = page_data.iter().chain(form_data).map(Vec::len).sum::<usize>();
        let payload = streams + placed + content + flat_report.bytes as usize;
        Pdf::with_capacity(payload + payload / 16 + (1 << 20))
    } else {
        Pdf::new()
    };
    let (vmaj, vmin) = preset.version();
    // optional content is PDF 1.5: [High Quality Print] and [Press Quality] are written as PDF 1.5 with Acrobat layers
    let (vmaj, vmin) = if ocgs.is_empty() { (vmaj, vmin) } else { (vmaj, vmin).max((1, 5)) };
    pdf.set_version(vmaj as u8, vmin as u8);
    let mut refs = Refs(0);
    let catalog_ref = refs.next();
    let pages_ref = refs.next();
    let info_ref = refs.next();
    let meta_ref = refs.next();
    let oi_profile_ref = refs.next();
    let ocg_refs: Vec<Ref> = ocgs.iter().map(|_| refs.next()).collect();

    let WrittenColorSpaces { gray_black, extra_ink_state, flat_cs, cs_refs, rgb_cs_ref, image_profile_refs } = write_color_spaces(
        &mut pdf,
        &mut refs,
        reg,
        ColorSpaceInput {
            document_rgb: doc.print.rgb_profile.as_deref(),
            cmyk_only,
            encoded,
            flat_tiles,
            doc_inks,
            rgb_profile,
            oi_profile_ref,
            level,
        },
    )?;

    // Forms are referenced by soft masks in graphics states: number them first.
    let mask_form_refs: Vec<Ref> = reg
        .forms
        .iter()
        .map(|f| match f {
            FormDef::Imported(_) => Ref::new(i32::MAX),
            _ => refs.next(),
        })
        .collect();
    // Flattened rasters.
    let mut flat_refs = Vec::with_capacity(flat_tiles.len());
    for (k, tiles) in flat_tiles.iter_mut().enumerate() {
        let mut rasters = Vec::with_capacity(flat_cs[k].len());
        let layers = tiles.iter_mut().enumerate().flat_map(|(j, t)| {
            let (w, h) = (t.w, t.h);
            t.layers.iter_mut().enumerate().map(move |(l, layer)| (j, l, w, h, layer))
        });
        for ((j, l, w, h, layer), &cs) in layers.zip(&flat_cs[k]) {
            let r = refs.next();
            let data = std::mem::take(&mut layer.data);
            let mut x = pdf.image_xobject(r, &data);
            x.width(w as i32);
            x.height(h as i32);
            match cs {
                None => {
                    x.color_space().device_cmyk();
                }
                Some(cs) => {
                    x.pair(Name(b"ColorSpace"), cs_refs[cs]);
                }
            }
            x.bits_per_component(8);
            match layer.encoding {
                flatten::TileEncoding::Dct => {
                    x.filter(Filter::DctDecode);
                    x.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
                }
                flatten::TileEncoding::Flate => {
                    x.filter(Filter::FlateDecode);
                }
                flatten::TileEncoding::Png => {
                    x.filter(Filter::FlateDecode);
                    x.decode_parms().predictor(pdf_writer::types::Predictor::PngOptimum).colors(layer.plates.len() as i32).columns(w as i32);
                }
            }
            let name = if l == 0 { format!("R{j}") } else { format!("R{j}_{l}") };
            rasters.push((name, r));
        }
        flat_refs.push(rasters);
    }
    // Effect rasters (alpha images of soft masks).
    let raster_refs: Vec<Ref> = reg.rasters.iter().map(|_| refs.next()).collect();
    for (i, a) in reg.rasters.iter().enumerate() {
        let data = images::flate(&a.data, level);
        let mut x = pdf.image_xobject(raster_refs[i], &data);
        x.width(a.w as i32);
        x.height(a.h as i32);
        x.color_space().device_gray();
        x.bits_per_component(8);
        x.filter(Filter::FlateDecode);
    }

    // Graphics states.
    let gs_refs: Vec<Ref> = reg.gs.iter().map(|_| refs.next()).collect();
    for (i, g) in reg.gs.iter().enumerate() {
        let mut e = pdf.ext_graphics(gs_refs[i]);
        match g {
            GsDef::Overprint { fill, stroke } => {
                e.overprint(*stroke);
                e.overprint_fill(*fill);
                e.overprint_mode(pdf_writer::types::OverprintMode::IgnoreZeroChannel);
            }
            GsDef::Blend { alpha, mode } => {
                let a = *alpha as f32 / 1000.0;
                e.stroking_alpha(a);
                e.non_stroking_alpha(a);
                e.blend_mode(blend(*mode));
            }
            GsDef::Mask { form, alpha, mode } => {
                let a = *alpha as f32 / 1000.0;
                e.stroking_alpha(a);
                e.non_stroking_alpha(a);
                e.blend_mode(blend(*mode));
                e.soft_mask().subtype(pdf_writer::types::MaskType::Luminosity).group(mask_form_refs[*form]);
            }
        }
    }

    let (font_refs, fonts_embedded) = write_fonts(&mut pdf, &mut refs, reg, fonts, font_ids, subsets, level);
    let WrittenImages { refs: image_refs, alpha: image_alpha, counts: images_written } = write_images(
        &mut pdf,
        &mut refs,
        reg,
        encoded,
        ImageColorRefs {
            gray_black,
            cs_refs: &cs_refs,
            rgb_cs_ref,
            oi_profile_ref,
            image_profile_refs: &image_profile_refs,
        },
        level,
    );

    // Imported pages: each source file's chunk renumbered into ours once.
    let rgb_for_imports = rgb_cs_ref.unwrap_or(oi_profile_ref);
    let mut file_maps: Vec<HashMap<Ref, Ref>> = Vec::new();
    let mut placed_converted = 0;
    for (fi, fc) in import_files.into_iter().enumerate() {
        // a placed PDF only the flattener's rasters show is not written (PDF 1.3 cannot hold it)
        if flattening && !reg.graphics.file_used(fi) {
            file_maps.push(HashMap::new());
            continue;
        }
        placed_converted += usize::from(fc.rgb_converted);
        let mut map: HashMap<Ref, Ref> = HashMap::new();
        fc.chunk.renumber_into(&mut pdf, |old| {
            if old == fc.default_rgb {
                return rgb_for_imports;
            }
            *map.entry(old).or_insert_with(|| refs.next())
        });
        file_maps.push(map);
    }
    let mut imported_refs: Vec<Ref> = Vec::new();
    for imp in &reg.graphics.imported {
        imported_refs.push(file_maps[imp.file].get(&imp.form).copied().unwrap_or(rgb_for_imports));
        reg.counts.imported_pdf_pages += 1;
    }

    // Shadings and patterns.
    let shading_refs: Vec<Ref> = reg.shadings.iter().map(|_| refs.next()).collect();
    for (i, sh) in reg.shadings.iter().enumerate() {
        let func = write_ramp(&mut pdf, &mut refs, &sh.stops);
        let mut d = pdf.indirect(shading_refs[i]).dict();
        d.pair(Name(b"ShadingType"), if sh.radial { 3 } else { 2 });
        match &sh.space {
            ShadingSpace::Cmyk => d.pair(Name(b"ColorSpace"), Name(b"DeviceCMYK")),
            ShadingSpace::Space(cs) => d.pair(Name(b"ColorSpace"), cs_refs[*cs]),
        };
        d.insert(Name(b"Coords")).array().items(sh.coords.iter().copied());
        d.pair(Name(b"Function"), func);
        d.insert(Name(b"Extend")).array().items([true, true]);
    }
    let pattern_refs: Vec<Ref> = reg.patterns.iter().map(|_| refs.next()).collect();
    for (i, p) in reg.patterns.iter().enumerate() {
        let m = &p.matrix;
        let mut d = pdf.indirect(pattern_refs[i]).dict();
        d.pair(Name(b"Type"), Name(b"Pattern"));
        d.pair(Name(b"PatternType"), 2);
        d.pair(Name(b"Shading"), shading_refs[p.shading]);
        d.insert(Name(b"Matrix")).array().items([m.a, m.b, m.c, m.d, m.tx, m.ty].map(|v| v as f32));
    }

    // Forms (transparency groups).
    let form_refs: Vec<Ref> = reg
        .forms
        .iter()
        .enumerate()
        .map(|(i, f)| match f {
            FormDef::Group { .. } | FormDef::Mask { .. } | FormDef::Flat(_) => mask_form_refs[i],
            FormDef::Imported(k) => imported_refs[*k],
        })
        .collect();
    let refs_tables = RefTables {
        cs: &cs_refs,
        gs: &gs_refs,
        fonts: &font_refs,
        forms: &form_refs,
        images: &image_refs,
        shadings: &shading_refs,
        patterns: &pattern_refs,
        rasters: &raster_refs,
        ocgs: &ocg_refs,
    };
    for (i, f) in reg.forms.iter().enumerate() {
        if let FormDef::Group { used, bbox, group, .. } = f {
            let mut x = pdf.form_xobject(form_refs[i], &form_data[i]);
            x.filter(Filter::FlateDecode);
            x.bbox(pdf_writer::Rect::new(bbox.left as f32, bbox.top as f32, bbox.right as f32, bbox.bottom as f32));
            {
                let mut g = x.group();
                g.transparency();
                g.isolated(group.isolated);
                g.knockout(group.knockout);
                g.color_space().device_cmyk();
            }
            write_resources(x.resources(), used, &refs_tables);
        }
        if let FormDef::Flat(k) = f {
            let b = flat_jobs[*k].cluster.bbox;
            let mut x = pdf.form_xobject(form_refs[i], &form_data[i]);
            x.filter(Filter::FlateDecode);
            x.bbox(pdf_writer::Rect::new((b.x0 - 2.0) as f32, (b.y0 - 2.0) as f32, (b.x1 + 2.0) as f32, (b.y1 + 2.0) as f32));
            let mut res = x.resources();
            if let Some(gs) = extra_ink_state.filter(|_| flat_tiles[*k].iter().any(|t| t.layers.len() > 1)) {
                res.ext_g_states().pair(Name(b"Inks"), gs_refs[gs]);
            }
            let mut xo = res.x_objects();
            for (name, r) in &flat_refs[*k] {
                xo.pair(Name(name.as_bytes()), *r);
            }
        }
        if let FormDef::Mask { used, bbox, .. } = f {
            let mut x = pdf.form_xobject(form_refs[i], &form_data[i]);
            x.filter(Filter::FlateDecode);
            x.bbox(pdf_writer::Rect::new(bbox.left as f32, bbox.top as f32, bbox.right as f32, bbox.bottom as f32));
            {
                let mut g = x.group();
                g.transparency();
                g.color_space().device_gray();
            }
            write_resources(x.resources(), used, &refs_tables);
        }
    }

    let transparent_pages = write_page_tree(
        &mut pdf,
        &mut refs,
        reg,
        &refs_tables,
        PageTree {
            root: pages_ref,
            pages: outs,
            content: page_data,
            image_alpha: &image_alpha,
            document_id: opts.document_id.as_deref(),
            flattening,
        },
    );
    reg.counts.pages_with_transparency = if flattening { flat_report.pages } else { transparent_pages };

    // Output intent, metadata, info, identity.
    let identifier = meta::condition_identifier(&facts.description).unwrap_or("Custom");
    {
        let data = images::flate(profile, level);
        let mut st = pdf.icc_profile(oi_profile_ref, &data);
        st.n(4);
        st.filter(Filter::FlateDecode);
    }
    let title = title::document_title(opts.title.as_deref(), &opts.document_name);
    let producer = format!("folio-pdf {}", env!("CARGO_PKG_VERSION"));
    let seed = colors::fnv(title.as_bytes()) ^ (meta::now_secs() as u64).wrapping_mul(0x9E37_79B9) ^ (std::process::id() as u64) << 32;
    let document_id = meta::uuid(seed);
    let instance_id = meta::uuid(seed.rotate_left(17) ^ 0xA5A5);
    let xmp = meta::xmp(&meta::XmpFields {
        title: &title,
        creator_tool: "Folio Studio",
        producer: &producer,
        date,
        document_id: &document_id,
        instance_id: &instance_id,
        pdfx_version: preset.info_version().0,
        pdfx_conformance: preset.info_version().1,
    });
    pdf.metadata(meta_ref, xmp.as_bytes());
    {
        let (y, mo, d, hh, mi, ss) = meta::civil(export_secs);
        let pdate = pdf_writer::Date::new(y as u16).month(mo as u8).day(d as u8).hour(hh as u8).minute(mi as u8).second(ss as u8).utc_offset_hour(0).utc_offset_minute(0);
        let mut info = pdf.document_info(info_ref);
        info.title(TextStr(&title));
        info.creator(TextStr("Folio Studio"));
        info.producer(TextStr(&producer));
        info.creation_date(pdate);
        info.modified_date(pdate);
        info.trapped(pdf_writer::types::TrappingStatus::NotTrapped);
        let (version, conformance) = preset.info_version();
        if preset.is_pdfx() { info.pair(Name(b"GTS_PDFXVersion"), Str(version.as_bytes())); }
        if let Some(c) = conformance {
            info.pair(Name(b"GTS_PDFXConformance"), Str(c.as_bytes()));
        }
    }
    // Acrobat layers: the groups, then the catalog's OCProperties (default configuration "Default": ON/OFF by Export Layers, the
    // Order a labelled list of the layers from the top one down with "Guides and Grids" last, no radio-button groups; no /AS)
    for (g, r) in ocgs.iter().zip(&ocg_refs) {
        let mut d = pdf.indirect(*r).dict();
        d.pair(Name(b"Type"), Name(b"OCG"));
        d.pair(Name(b"Name"), TextStr(g.name));
        d.insert(Name(b"Intent")).array().items([Name(b"View"), Name(b"Design")]);
        let mut usage = d.insert(Name(b"Usage")).dict();
        let mut creator = usage.insert(Name(b"CreatorInfo")).dict();
        creator.pair(Name(b"Creator"), TextStr("Folio Studio"));
        creator.pair(Name(b"Subtype"), Name(b"Artwork"));
    }
    {
        let mut cat = pdf.catalog(catalog_ref);
        cat.pages(pages_ref);
        cat.metadata(meta_ref);
        if !ocgs.is_empty() {
            let mut oc = cat.insert(Name(b"OCProperties")).dict();
            oc.insert(Name(b"OCGs")).array().items(ocg_refs.iter().copied());
            let mut d = oc.insert(Name(b"D")).dict();
            d.pair(Name(b"Name"), TextStr("Default"));
            d.insert(Name(b"ON")).array().items(ocgs.iter().zip(&ocg_refs).filter(|(g, _)| g.on).map(|(_, r)| *r));
            d.insert(Name(b"OFF")).array().items(ocgs.iter().zip(&ocg_refs).filter(|(g, _)| !g.on).map(|(_, r)| *r));
            {
                let mut order = d.insert(Name(b"Order")).array();
                let mut list = order.push().array();
                // the label is the document's file name (InDesign: chapter1.indd)
                list.item(TextStr(if opts.document_name.is_empty() { "Document" } else { &opts.document_name }));
                list.items(ocg_refs.iter().skip(1).rev().copied());
                list.item(ocg_refs[0]);
            }
            d.insert(Name(b"RBGroups")).array();
        }
        if preset.is_pdfx() {
        let mut ois = cat.insert(Name(b"OutputIntents")).array();
        let mut oi = ois.push().dict();
        oi.pair(Name(b"Type"), Name(b"OutputIntent"));
        oi.pair(Name(b"S"), Name(b"GTS_PDFX"));
        oi.pair(Name(b"OutputConditionIdentifier"), TextStr(identifier));
        oi.pair(Name(b"OutputCondition"), TextStr(&facts.description));
        oi.pair(Name(b"Info"), TextStr(&facts.description));
        oi.pair(Name(b"RegistryName"), TextStr("http://www.color.org"));
        oi.pair(Name(b"DestOutputProfile"), oi_profile_ref);
        }
    }
    let id_a = meta::uuid(seed ^ 1).replace('-', "");
    let id_b = meta::uuid(seed ^ 2).replace('-', "");
    let hex = |s: &str| (0..s.len()).step_by(2).filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect::<Vec<u8>>();
    pdf.set_file_id((hex(&id_a), hex(&id_b)));
    let bytes = pdf.finish();
    Ok(Written { bytes, fonts_embedded, images_written, placed_converted, identifier })
}

/// An Acrobat layer of the PDF (an optional content group): a document layer, or InDesign's "Guides and Grids" (`layer` none).
pub(crate) struct LayerGroup<'d> {
    name: &'d str,
    pub(crate) layer: Option<&'d str>,
    /// On in the default view (Export Layers).
    on: bool,
}

/// The PDF's Acrobat layers as InDesign 2025 writes them (buildc2-ocg-probe): "Guides and Grids" first (off), then the document's
/// layers from the bottom one up, on when Export Layers takes them (visible & printable, visible, all).
pub(crate) fn layer_groups(doc: &Document, which: ExportLayers) -> Vec<LayerGroup<'_>> {
    let mut layers: Vec<&folio_model::Layer> = doc.layers.iter().collect();
    layers.sort_by_key(|l| l.order);
    let mut out = vec![LayerGroup { name: "Guides and Grids", layer: None, on: false }];
    out.extend(layers.into_iter().map(|l| LayerGroup { name: &l.name, layer: Some(&l.id), on: which.includes(l.visible, l.printable) }));
    out
}
