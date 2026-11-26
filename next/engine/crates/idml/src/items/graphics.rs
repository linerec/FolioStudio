//! Reads placed graphics, their links and previews, and generated QR codes.

use super::{ItemReader, Previews, affine, geometry_paths, local_display};
use crate::props::{Props, clean_name, nums};
use crate::warn::Warnings;
use crate::xml::Element;
use folio_model::{Graphic, ItemKind, PageItem, Paint, Rect, Stroke};

impl ItemReader<'_, '_> {
    /// A `Link` element's facts, with the location a `.folio` resolved for it.
    pub(super) fn link(&self, l: &Element<'_>) -> folio_model::LinkInfo {
        let mut info = link_info(l);
        if let Some(o) = self.links.get(&info.id) {
            info.uri = o.uri.clone();
            if let Some((size, time)) = o.stamp {
                info.size = size;
                info.modified_unix = time;
            }
        }
        info
    }
}

impl<'a> ItemReader<'a, '_> {
    pub(super) fn graphic(&self, g: &'a Element<'a>, w: &mut Warnings) -> Graphic {
        let id = g.attr("Self").unwrap_or_default().to_owned();
        let bounds = match g.child("Properties").and_then(|p| p.child("GraphicBounds")) {
            Some(b) => {
                let f = |k: &str| b.attr(k).and_then(crate::props::num).unwrap_or(0.0);
                Rect { left: f("Left"), top: f("Top"), right: f("Right"), bottom: f("Bottom") }
            }
            None => {
                w.add(format!("{} without GraphicBounds", g.name));
                Rect::default()
            }
        };
        let clip = g.child("ClippingPathSettings").and_then(|c| {
            let kind = c.attr("ClippingType").filter(|t| *t != "None")?;
            let paths = g
                .child("Properties")
                .and_then(|p| p.child("ClippingPathGeometry"))
                .map(|geom| geometry_paths(geom, w))
                .unwrap_or_default();
            if paths.iter().all(|p| p.points.len() < 2) {
                w.add(format!("graphic clipping path ({kind}) without its geometry: whole image shown"));
                return None;
            }
            Some(folio_model::GraphicClip { kind: kind.to_owned(), paths, invert: c.attr("InvertPath") == Some("true") })
        });
        let preview = self.previews.get(&id).cloned();
        // SVG uses its linked vector source; QR EPS uses its modules (`qr_code`).
        if preview.is_none() && g.name != "SVG" && !g.elements().any(|c| c.name.ends_with("QRCode")) {
            w.add(format!("{} without preview file", g.name));
        }
        let actual_ppi = g.str("ActualPpi").and_then(nums).filter(|v| v.len() == 2).map(|v| (v[0], v[1]));
        let effective_ppi = g.str("EffectivePpi").and_then(nums).filter(|v| v.len() == 2).map(|v| (v[0], v[1]));
        let link_el = g.child("Link");
        let link = link_el.map(|l| self.link(l));
        let embedded_data = match self.embedded.get(&id) {
            Some(path) => match std::fs::read(path) {
                Ok(data) => known_embedded(data),
                Err(e) => {
                    w.add(format!("embedded file of a graphic not readable ({e}); its preview is used"));
                    None
                }
            },
            None => embedded_contents(g),
        };
        Graphic {
            id,
            element: g.name.to_owned(),
            transform: affine(g.attr("ItemTransform"), w),
            bounds,
            link_uri: link.as_ref().map(|l| l.uri.clone()).filter(|u| !u.is_empty()),
            preview,
            preview_from_source: false,
            actual_ppi,
            link,
            image_space: g
                .str("Space")
                .map(clean_name)
                .map(|s| s.strip_prefix("#Links_").unwrap_or(s).to_owned())
                .filter(|s| !s.is_empty()),
            effective_ppi,
            image_type: g.str("ImageTypeName").map(|s| clean_name(s).to_owned()).filter(|s| !s.is_empty()),
            pdf: g.child("PDFAttribute").map(|a| folio_model::PlacedPdf {
                page: a.f64("PageNumber").map(|p| p.round().clamp(1.0, u32::MAX as f64) as u32).unwrap_or(1),
                crop: a.attr("PDFCrop").unwrap_or("CropContentVisibleLayers").to_owned(),
                transparent_background: a.bool("TransparentBackground").unwrap_or(true),
            }),
            embedded_data,
            clip,
            clipping_options: crate::clipping::read(g),
            layer_options: crate::graphic_layers::read(g),
            image_color: (g.name == "Image").then(|| folio_model::image_color::ImageColor {
                profile: g.str("Profile").unwrap_or(folio_model::image_color::DOCUMENT_PROFILE).to_owned(),
                intent: g.attr("ImageRenderingIntent").unwrap_or("UseColorSettings").to_owned(),
            }),
            local_display: local_display(g),
        }
    }

    /// The QR code of an EPS graphic that InDesign generated (`PlainTextQRCode`, …), lowered
    /// to its module paths in the graphic's coordinates (`folio_model::qr`): the modules of
    /// InDesign's own EPS when the IDML carries it, else the contents encoded as InDesign
    /// does. Drawn in the code's swatch (`QrCodeSwatch`, a colour name; Black without one)
    /// at the tint InDesign's EPS paints, and not overprinted by the [Black] preference.
    pub(super) fn qr_code(&self, g: &Element<'_>, graphic: &Graphic, w: &mut Warnings) -> Option<PageItem> {
        use folio_model::qr::{self, QrContent, VCard};
        let q = g.elements().find(|c| c.name.ends_with("QRCode"))?;
        let a = |k: &str| q.attr(k).unwrap_or_default().to_owned();
        let content = match q.name {
            "PlainTextQRCode" => QrContent::PlainText(a("PlainText")),
            "HyperlinkQRCode" => QrContent::Hyperlink(a("UrlLink")),
            "EmailQRCode" => QrContent::Email { address: a("EmailAddress"), subject: a("Subject"), body: a("Body") },
            "TextMessageQRCode" => QrContent::TextMessage { number: a("CellNumber"), message: a("TextMessage") },
            "VCardQRCode" => QrContent::VCard(VCard {
                first_name: a("FirstName"),
                last_name: a("LastName"),
                job_title: a("JobTitle"),
                cell_phone: a("CellPhone"),
                phone: a("Phone"),
                email: a("Email"),
                organisation: a("Organisation"),
                street_address: a("StreetAddress"),
                city: a("City"),
                state: a("AdrState"),
                postal_code: a("PostalCode"),
                country: a("Country"),
                website: a("Website"),
            }),
            other => {
                w.add(format!("{other} not known: EPS shown instead"));
                return None;
            }
        };
        let eps = g.child("Properties").and_then(|p| p.child("Contents")).and_then(|c| base64(c.text().trim()));
        let modules = match eps.as_deref().and_then(qr::from_indesign_eps) {
            Some(m) => m,
            None => match qr::encode(&content.payload()) {
                Some(m) => m,
                None => {
                    w.add("QR code too long to encode: EPS shown instead");
                    return None;
                }
            },
        };
        // InDesign's EPS is 85.039 pt or 2 pt per module, the graphic bounds hold it.
        let bounds = if graphic.bounds.width() > 0.0 && graphic.bounds.height() > 0.0 {
            graphic.bounds
        } else {
            let side = (85.039_001f64).max(2.0 * (modules.size + 2 * qr::QUIET_ZONE) as f64);
            Rect { left: 0.0, top: 0.0, right: side, bottom: side }
        };
        let name = q.child("Properties").and_then(|p| p.child("QrCodeSwatch")).map(|s| s.text()).unwrap_or_default();
        let swatch = (!name.is_empty())
            .then(|| {
                let id = format!("Color/{name}");
                self.colors
                    .swatches
                    .get(&id)
                    .or_else(|| self.colors.swatches.values().find(|s| s.name == name && s.base.is_none()))
            })
            .flatten()
            .or_else(|| self.colors.swatches.get("Color/Black"));
        let fill = match swatch {
            Some(s) => {
                let tint = eps.as_deref().and_then(qr::indesign_eps_color).and_then(|(kind, v)| eps_tint(s, kind, &v)).unwrap_or(100.0);
                Paint::Swatch { id: s.id.clone(), tint }
            }
            None => {
                w.add(format!("QR code swatch {name:?} not found (drawn in no colour)"));
                Paint::None
            }
        };
        let mut item = PageItem {
            id: q.attr("Self").unwrap_or_default().to_owned(),
            kind: ItemKind::Polygon { content: None },
            transform: graphic.transform,
            paths: qr::module_paths(&modules, &bounds),
            fill,
            stroke: Stroke { weight: 0.0, ..Stroke::default() },
            ..PageItem::default()
        };
        item.print.placed_art = true;
        Some(item)
    }
}

/// The data of an embedded graphic: `Properties/Contents` holds the file base64-encoded
/// (InDesign's IDML; the INDD reader leaves it out). Only raster and PDF data is kept.
fn embedded_contents(g: &Element<'_>) -> Option<std::sync::Arc<Vec<u8>>> {
    let text = g.child("Properties")?.child("Contents")?.text();
    known_embedded(base64(text.trim())?)
}

/// Embedded file data of the formats print output can use (raster files and PDF).
fn known_embedded(data: Vec<u8>) -> Option<std::sync::Arc<Vec<u8>>> {
    let known = data.starts_with(&[0xFF, 0xD8])
        || data.starts_with(&[0x89, b'P', b'N', b'G'])
        || data.starts_with(&[b'I', b'I', b'*', 0])
        || data.starts_with(&[b'M', b'M', 0, b'*'])
        || data.starts_with(b"8BPS")
        || data.starts_with(b"%PDF");
    known.then(|| std::sync::Arc::new(data))
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The tint of InDesign's QR EPS colour relative to swatch `s`: a separation's tint, or a
/// CMYK colour that is the swatch's ink values scaled by one factor. `None` = full.
fn eps_tint(s: &folio_model::Swatch, kind: &str, v: &[f64]) -> Option<f64> {
    let t = match kind {
        "tint" if s.model == folio_model::ColorModel::Spot && v.len() == 1 => v[0],
        "cmyk" if s.space == folio_model::ColorSpace::Cmyk && s.model == folio_model::ColorModel::Process && v.len() == 4 => {
            let ink: Vec<f64> = s.values.iter().map(|x| x / 100.0 * s.tint.unwrap_or(100.0) / 100.0).collect();
            let i = (0..4).max_by(|&a, &b| ink[a].total_cmp(&ink[b]))?;
            if ink[i] <= 0.0 {
                return None;
            }
            let t = v[i] / ink[i];
            ((0..4).all(|k| (v[k] - ink[k] * t).abs() < 0.005)).then_some(t)?
        }
        _ => return None,
    };
    (t > 0.0 && t < 0.995).then_some(t * 100.0)
}

/// Hex `hi~lo` (`LinkResourceSize`).
fn link_size(s: &str) -> Option<u64> {
    let (hi, lo) = s.split_once('~')?;
    let hi = u64::from_str_radix(hi.trim(), 16).ok()?;
    let lo = u64::from_str_radix(lo.trim(), 16).ok()?;
    hi.checked_mul(1 << 32)?.checked_add(lo)
}

/// `LinkImportStamp="file <FILETIME> <size>"`: modification time as Windows FILETIME (100 ns
/// ticks since 1601, UTC).
fn link_stamp_unix(s: &str) -> Option<f64> {
    let mut it = s.split_ascii_whitespace();
    if it.next()? != "file" {
        return None;
    }
    let ticks: u64 = it.next()?.parse().ok()?;
    (ticks > 0).then(|| ticks as f64 / 1e7 - 11_644_473_600.0)
}

/// What a `Link` element records.
fn link_info(l: &Element<'_>) -> folio_model::LinkInfo {
    let a = |k: &str| l.attr(k).unwrap_or_default();
    folio_model::LinkInfo {
        id: a("Self").to_owned(),
        uri: a("LinkResourceURI").to_owned(),
        format: clean_name(a("LinkResourceFormat")).to_owned(),
        stored_state: a("StoredState").to_owned(),
        size: l.attr("LinkResourceSize").and_then(link_size),
        modified_unix: l.attr("LinkImportStamp").and_then(link_stamp_unix),
        modification_time: l.attr("LinkImportModificationTime").filter(|v| !v.is_empty()).map(str::to_owned),
        resource_modified: a("LinkResourceModified") == "true",
        object_modified: a("LinkObjectModified") == "true",
    }
}

/// Graphic previews from the INDD reader's `result.json` (`previews[].file` / `rawFile`
/// relative to that file, `previews[].graphics` = graphic `Self` ids).
pub fn load_previews(result: &serde_json::Value, dir: &std::path::Path, w: &mut Warnings) -> Previews {
    let mut out = Previews::new();
    let Some(list) = result.get("previews").and_then(|p| p.as_array()) else {
        w.add("reader result has no previews list");
        return out;
    };
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    // One directory listing instead of a stat per preview; anything not listed as a plain
    // file (a link, a case difference) is checked individually.
    let listed: std::collections::HashSet<std::ffi::OsString> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.file_name())
                .collect()
        })
        .unwrap_or_default();
    for p in list {
        let candidates = ["file", "rawFile"].map(|k| p.get(k).and_then(|f| f.as_str()));
        let mut path = None;
        for f in candidates.into_iter().flatten() {
            // Previews are plain file names next to result.json; refuse anything else.
            let name = std::path::Path::new(f);
            let mut parts = name.components();
            let plain = matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none();
            if !plain {
                w.add("reader preview path that is not a plain file name ignored");
                continue;
            }
            let full = dir.join(name);
            if listed.contains(name.as_os_str()) || full.is_file() {
                path = Some(full);
                break;
            }
        }
        let Some(path) = path else {
            w.add("reader preview file missing");
            continue;
        };
        let path = path.to_string_lossy().into_owned();
        for g in p.get("graphics").and_then(|g| g.as_array()).into_iter().flatten() {
            if let Some(id) = g.as_str() {
                out.insert(id.to_owned(), path.clone());
            }
        }
    }
    out
}
