//! Assigns and writes the color spaces used by pages, images, and flattened plates.

use super::{Refs, devicen_code, write_alt};
use crate::colors::{self, Alternate, CsDef};
use crate::registry::{GsDef, Registry};
use crate::{ExportError, flatten, images};
use pdf_writer::{Filter, Name, Pdf, Ref};
use std::collections::HashMap;

pub(super) struct ColorSpaceInput<'a> {
    pub(super) document_rgb: Option<&'a str>,
    pub(super) cmyk_only: bool,
    pub(super) encoded: &'a [Result<images::Encoded, String>],
    pub(super) flat_tiles: &'a [Vec<flatten::TileImage>],
    pub(super) doc_inks: Option<&'a folio_render::sep::DocInks>,
    pub(super) rgb_profile: Option<&'a [u8]>,
    pub(super) oi_profile_ref: Ref,
    pub(super) level: u32,
}

pub(super) struct WrittenColorSpaces {
    pub(super) gray_black: Option<usize>,
    pub(super) extra_ink_state: Option<usize>,
    pub(super) flat_cs: Vec<Vec<Option<usize>>>,
    pub(super) cs_refs: Vec<Ref>,
    pub(super) rgb_cs_ref: Option<Ref>,
    pub(super) image_profile_refs: HashMap<u64, Ref>,
}

pub(super) fn write_color_spaces(
    pdf: &mut Pdf,
    refs: &mut Refs,
    reg: &mut Registry<'_>,
    input: ColorSpaceInput<'_>,
) -> Result<WrittenColorSpaces, ExportError> {
    let ColorSpaceInput {
        document_rgb,
        cmyk_only,
        encoded,
        flat_tiles,
        doc_inks,
        rgb_profile,
        oi_profile_ref,
        level,
    } = input;
    // Colour spaces (images may add ICC profiles to the list below).
    let rgb_needed = reg.res.spaces.rgb_used
        || (!reg.graphics.imported.is_empty() && !cmyk_only)
        || encoded.iter().any(|e| matches!(e, Ok(images::Encoded { space: images::ImgSpace::Rgb(None), .. })));
    // PDF/X-1a writes gray images on the black plate as InDesign does: DeviceN [Black]
    // (same samples, inverted by Decode), which viewers show as the black ink it prints as
    let gray_black = (cmyk_only && encoded.iter().any(|e| matches!(e, Ok(images::Encoded { space: images::ImgSpace::Gray, .. }))))
        .then(|| reg.res.spaces.intern(CsDef::DeviceN { inks: vec!["Black".into()], cmyk: vec![[0.0, 0.0, 0.0, 1.0]] }));
    let extra_ink_state = flat_tiles.iter().flatten().any(|t| t.layers.len() > 1)
        .then(|| reg.gs(GsDef::Overprint { fill: true, stroke: false }));
    // the colour spaces of flattened rasters: DeviceCMYK, or the plates they hold
    let flat_cs: Vec<Vec<Option<usize>>> = flat_tiles
        .iter()
        .map(|tiles| {
            tiles
                .iter()
                .flat_map(|t| &t.layers)
                .map(|t| {
                    let inks = doc_inks?;
                    if t.plates == [0, 1, 2, 3] {
                        return None;
                    }
                    let names: Vec<String> = t.plates.iter().map(|&p| inks.inks[p].name.clone()).collect();
                    let alts: Vec<[f32; 4]> = t.plates.iter().map(|&p| inks.inks[p].cmyk).collect();
                    Some(if names.len() == 1 && t.plates[0] >= 4 {
                        reg.res.spaces.intern(CsDef::Separation { ink: names[0].clone(), alt: Alternate::Cmyk(alts[0]) })
                    } else {
                        reg.res.spaces.intern(CsDef::DeviceN { inks: names, cmyk: alts })
                    })
                })
                .collect()
        })
        .collect();
    if rgb_needed {
        reg.res.spaces.intern(CsDef::IccRgb);
    }
    // PDF/X-4 allows RGB only ICC-tagged: without the document RGB profile there is no
    // compliant way to write it.
    if rgb_needed && rgb_profile.is_none() {
        return Err(ExportError::OutputProfile(format!(
            "the document has RGB content but its RGB profile {:?} is not installed: pass --rgb-profile",
            document_rgb.unwrap_or("sRGB IEC61966-2.1")
        )));
    }
    let rgb_profile_ref = rgb_needed.then(|| refs.next());
    let mut cs_refs: Vec<Ref> = Vec::new();
    let mut image_profile_refs: HashMap<u64, Ref> = HashMap::new();
    for e in encoded.iter().flatten() {
        if let images::ImgSpace::Rgb(Some(p)) = &e.space {
            image_profile_refs.entry(colors::fnv(p)).or_insert_with(|| refs.next());
        }
    }
    for _ in &reg.res.spaces.defs {
        cs_refs.push(refs.next());
    }
    let rgb_cs_ref = reg.res.spaces.defs.iter().position(|d| *d == CsDef::IccRgb).map(|i| cs_refs[i]);
    for (i, d) in reg.res.spaces.defs.iter().enumerate() {
        let r = cs_refs[i];
        match d {
            CsDef::Separation { ink, alt } => {
                let f = refs.next();
                {
                    let mut arr = pdf.indirect(r).array();
                    arr.item(Name(b"Separation"));
                    arr.item(Name(ink.as_bytes()));
                    write_alt(&mut arr, alt, rgb_cs_ref);
                    arr.item(f);
                }
                let mut ef = pdf.exponential_function(f);
                ef.domain([0.0, 1.0]);
                ef.c0(alt.zero());
                ef.c1(alt.at(1.0));
                ef.n(1.0);
            }
            CsDef::DeviceN { inks, cmyk } => {
                let f = refs.next();
                {
                    let mut arr = pdf.indirect(r).array();
                    arr.item(Name(b"DeviceN"));
                    {
                        let mut names = arr.push().array();
                        for n in inks {
                            names.item(Name(n.as_bytes()));
                        }
                    }
                    arr.item(Name(b"DeviceCMYK"));
                    arr.item(f);
                }
                let code = devicen_code(cmyk);
                let mut pf = pdf.post_script_function(f, code.as_bytes());
                pf.domain(std::iter::repeat_n([0.0f32, 1.0], inks.len()).flatten());
                pf.range([0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]);
            }
            CsDef::Lab => {
                let mut arr = pdf.indirect(r).array();
                arr.item(Name(b"Lab"));
                let mut dict = arr.push().dict();
                dict.insert(Name(b"WhitePoint")).array().items([0.964203f32, 1.0, 0.824905]);
                dict.insert(Name(b"BlackPoint")).array().items([0.0f32, 0.0, 0.0]);
                dict.insert(Name(b"Range")).array().items([-128.0f32, 127.0, -128.0, 127.0]);
            }
            CsDef::IccRgb => {
                let mut arr = pdf.indirect(r).array();
                arr.item(Name(b"ICCBased"));
                arr.item(rgb_profile_ref.unwrap_or(oi_profile_ref));
            }
        }
    }
    if let (Some(r), Some(p)) = (rgb_profile_ref, rgb_profile) {
        let data = images::flate(p, level);
        let mut st = pdf.icc_profile(r, &data);
        st.n(3);
        st.filter(Filter::FlateDecode);
    }

    Ok(WrittenColorSpaces { gray_black, extra_ink_state, flat_cs, cs_refs, rgb_cs_ref, image_profile_refs })
}
