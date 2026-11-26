//! Writes font subsets and image objects, including their masks and profiles.

use super::Refs;
use crate::registry::Registry;
use crate::{colors, fonts, images, EmbeddedFont};
use folio_text::FontStore;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Name, Pdf, Ref, Str};
use std::collections::{BTreeMap, HashMap};

pub(super) fn write_fonts(
    pdf: &mut Pdf,
    refs: &mut Refs,
    reg: &mut Registry<'_>,
    fonts: &FontStore,
    font_ids: &[u32],
    subsets: &[Result<fonts::Subset, String>],
    level: u32,
) -> (HashMap<u32, Ref>, Vec<EmbeddedFont>) {
    let mut font_refs: HashMap<u32, Ref> = HashMap::new();
    let mut fonts_embedded = Vec::new();
    for (i, sub) in subsets.iter().enumerate() {
        let fid = font_ids[i];
        match sub {
            Ok(s) => {
                let type0 = refs.next();
                font_refs.insert(fid, type0);
                let (cid, desc, file, tu) = (refs.next(), refs.next(), refs.next(), refs.next());
                let base = format!("{}+{}", s.tag, s.postscript);
                {
                    let mut f = pdf.type0_font(type0);
                    f.base_font(Name(base.as_bytes()));
                    f.encoding_predefined(Name(b"Identity-H"));
                    f.descendant_font(cid);
                    f.to_unicode(tu);
                }
                {
                    let mut c = pdf.cid_font(cid);
                    c.subtype(if s.cff { CidFontType::Type0 } else { CidFontType::Type2 });
                    c.base_font(Name(base.as_bytes()));
                    c.system_info(SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 });
                    c.font_descriptor(desc);
                    c.default_width(0.0);
                    c.widths().consecutive(0, s.widths.iter().copied());
                    if !s.cff {
                        c.cid_to_gid_map_predefined(Name(b"Identity"));
                    }
                }
                {
                    let mut d = pdf.font_descriptor(desc);
                    d.name(Name(base.as_bytes()));
                    d.flags(FontFlags::from_bits_truncate(s.flags));
                    d.bbox(pdf_writer::Rect::new(s.bbox[0], s.bbox[1], s.bbox[2], s.bbox[3]));
                    d.italic_angle(s.italic_angle);
                    d.ascent(s.ascent);
                    d.descent(s.descent);
                    d.cap_height(s.cap_height);
                    d.stem_v(s.stem_v);
                    if s.cff {
                        d.font_file3(file);
                    } else {
                        d.font_file2(file);
                    }
                }
                let data = images::flate(&s.data, 9.min(level + 3));
                {
                    let mut st = pdf.stream(file, &data);
                    st.filter(Filter::FlateDecode);
                    if s.cff {
                        st.pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
                    } else {
                        st.pair(Name(b"Length1"), s.data.len() as i32);
                    }
                }
                let mut cmap = UnicodeCmap::new(Name(b"Custom"), SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"UCS"), supplement: 0 });
                for (gid, text) in s.unicode.iter().enumerate() {
                    if !text.is_empty() {
                        cmap.pair_with_multiple(gid as u16, text.chars());
                    }
                }
                let cmap_data = images::flate(&cmap.finish().into_vec(), level);
                pdf.cmap(tu, &cmap_data).filter(Filter::FlateDecode);
                let notdef = reg.fonts.entries.get(&fid).map_or(0, |e| e.notdef_drawn);
                if notdef > 0 {
                    reg.warn.add("font", format!("{} glyphs missing from {} drawn as .notdef", notdef, s.postscript), None, None);
                }
                fonts_embedded.push(EmbeddedFont {
                    postscript: s.postscript.clone(),
                    file: s.file.clone(),
                    format: if s.cff { "CIDFontType0C".into() } else { "CIDFontType2".into() },
                    glyphs: s.widths.len(),
                    bytes: s.data.len(),
                    notdef,
                });
            }
            Err(e) => {
                let name = fonts.face_info(fid).map(|f| f.postscript.clone()).unwrap_or_default();
                reg.warn.add("font", format!("font {name} could not be embedded ({e}); its text is missing"), None, None);
            }
        }
    }
    (font_refs, fonts_embedded)
}

pub(super) struct ImageColorRefs<'a> {
    pub(super) gray_black: Option<usize>,
    pub(super) cs_refs: &'a [Ref],
    pub(super) rgb_cs_ref: Option<Ref>,
    pub(super) oi_profile_ref: Ref,
    pub(super) image_profile_refs: &'a HashMap<u64, Ref>,
}

pub(super) struct WrittenImages {
    pub(super) refs: Vec<Option<Ref>>,
    pub(super) alpha: Vec<bool>,
    pub(super) counts: BTreeMap<String, usize>,
}

pub(super) fn write_images(
    pdf: &mut Pdf,
    refs: &mut Refs,
    reg: &mut Registry<'_>,
    encoded: &mut [Result<images::Encoded, String>],
    colors: ImageColorRefs<'_>,
    level: u32,
) -> WrittenImages {
    let ImageColorRefs { gray_black, cs_refs, rgb_cs_ref, oi_profile_ref, image_profile_refs } = colors;
    let mut images_written: BTreeMap<String, usize> = BTreeMap::new();
    let mut image_refs: Vec<Option<Ref>> = Vec::new();
    let mut image_alpha: Vec<bool> = Vec::new();
    for (i, e) in encoded.iter_mut().enumerate() {
        match e {
            Ok(img) => {
                let r = refs.next();
                let mask_data = img.mask.take();
                let smask_data = img.smask.take();
                let smask = smask_data.as_ref().map(|_| refs.next());
                let mask = mask_data.as_ref().map(|_| refs.next());
                {
                    let data = std::mem::take(&mut img.data);
                    let mut x = pdf.image_xobject(r, &data);
                    x.width(img.width as i32);
                    x.height(img.height as i32);
                    if matches!(img.space, images::ImgSpace::Rgb(_))
                        && let Some(c) = &reg.graphics.images[i].color {
                        x.intent(crate::image_color::pdf_intent(&c.intent));
                    }
                    match &img.space {
                        images::ImgSpace::Gray => match gray_black {
                            Some(cs) => {
                                x.pair(Name(b"ColorSpace"), cs_refs[cs]);
                            }
                            None => {
                                x.color_space().device_gray();
                            }
                        },
                        images::ImgSpace::Cmyk => {
                            x.color_space().device_cmyk();
                        }
                        images::ImgSpace::Rgb(Some(p)) => {
                            x.color_space().icc_based(image_profile_refs[&colors::fnv(p)]);
                        }
                        images::ImgSpace::Rgb(None) => {
                            x.pair(Name(b"ColorSpace"), rgb_cs_ref.unwrap_or(oi_profile_ref));
                        }
                        images::ImgSpace::Mask => {
                            x.image_mask(true);
                        }
                    }
                    x.bits_per_component(img.bpc as i32);
                    x.filter(match img.filter {
                        images::Filter::Dct => Filter::DctDecode,
                        images::Filter::Flate => Filter::FlateDecode,
                    });
                    if let Some(d) = &img.decode {
                        x.decode(d.iter().copied());
                    } else if gray_black.is_some() && matches!(img.space, images::ImgSpace::Gray) {
                        x.decode([1.0, 0.0]);
                    }
                    if let Some(m) = smask {
                        x.s_mask(m);
                    }
                    if let Some(m) = mask {
                        x.pair(Name(b"Mask"), m);
                    }
                }
                if let (Some(m), Some((mw, mh, data))) = (mask, mask_data) {
                    let mut x = pdf.image_xobject(m, &data);
                    x.width(mw as i32);
                    x.height(mh as i32);
                    x.image_mask(true);
                    x.bits_per_component(1);
                    x.filter(Filter::FlateDecode);
                }
                if let (Some(m), Some(data)) = (smask, smask_data) {
                    let mut x = pdf.image_xobject(m, &data);
                    x.width(img.width as i32);
                    x.height(img.height as i32);
                    x.color_space().device_gray();
                    x.bits_per_component(8);
                    x.filter(Filter::FlateDecode);
                }
                image_refs.push(Some(r));
                image_alpha.push(smask.is_some());
                reg.counts.images += 1;
                *images_written.entry(img.note.clone()).or_default() += 1;
            }
            Err(err) => {
                let job = &reg.graphics.images[i];
                reg.warn.add("image", format!("image could not be written ({err}); not printed"), None, job.owners.first().map(String::as_str));
                // The content stream still draws this image: keep the reference valid with a 1 x 1 stencil whose only sample is 1,
                // which paints nothing (without it the page names an XObject the resources do not have, ISO 32000-1 7.8.3).
                let r = refs.next();
                {
                    let mut x = pdf.image_xobject(r, &[0x80]);
                    x.width(1);
                    x.height(1);
                    x.image_mask(true);
                    x.bits_per_component(1);
                }
                image_refs.push(Some(r));
                image_alpha.push(false);
            }
        }
    }
    for (p, r) in image_profile_refs {
        if let Some(bytes) = encoded.iter().flatten().find_map(|e| match &e.space {
            images::ImgSpace::Rgb(Some(b)) if colors::fnv(b) == *p => Some(b.clone()),
            _ => None,
        }) {
            let data = images::flate(&bytes, level);
            let mut st = pdf.icc_profile(*r, &data);
            st.n(3);
            st.filter(Filter::FlateDecode);
        }
    }
    WrittenImages { refs: image_refs, alpha: image_alpha, counts: images_written }
}
