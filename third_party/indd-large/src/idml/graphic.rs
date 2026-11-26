//! Placed graphics (images, PDF, EPS, SVG) with their settings, clipping
//! path, import options, layers and links.
//!
//! Evidence: `docs/format/objects.md` (placed graphics, links, clipping
//! path settings, graphic layers) and `idml-values.md`.

use super::*;
use crate::model::{ClippingPath, GraphicLayer, Name, QrCode, QrKind};

/// `Space` of an image by its colour space code (and whether it has a
/// colour table).
fn color_space(code: u32, indexed: bool) -> Option<&'static str> {
    match (code, indexed) {
        (1, _) => Some("$ID/#Links_Grayscale"),
        (2, false) => Some("$ID/#Links_RGB"),
        (2, true) => Some("$ID/#Links_Indexed RGB"),
        (4, _) => Some("$ID/#Links_CMYK"),
        _ => None,
    }
}

/// `Properties/Profile` of an image by the code of chunk 0x7C0F.
fn profile(code: Option<u32>) -> Option<&'static str> {
    match code {
        None | Some(0) => Some("$ID/None"),
        Some(1) => Some("$ID/Use Document Default"),
        Some(3) => Some("$ID/Embedded"),
        _ => None,
    }
}

/// A vector colour policy code of chunk 0x7C42.
fn vector_policy(code: u32) -> Option<&'static str> {
    match code {
        1 => Some("IgnoreAll"),
        3 => Some("HonorAllProfiles"),
        _ => None,
    }
}

/// `PDFCrop` by the crop code of chunk 0x251B.
fn pdf_crop(code: u32) -> Option<&'static str> {
    match code {
        0 => Some("CropContentVisibleLayers"),
        2 => Some("CropArt"),
        3 => Some("CropPDF"),
        4 => Some("CropTrim"),
        // Folio fork, folio.5: the Atlas and Folio's crop specimen.
        5 => Some("CropBleed"),
        6 => Some("CropMedia"),
        7 => Some("CropContentAllLayers"),
        _ => None,
    }
}

/// A value rounded half up, as IDML rounds resolutions.
fn round_half_up(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// The horizontal and vertical scale of a transform: √(a² + b²), and
/// |ad − bc| divided by the horizontal scale.
fn scales(m: &Matrix) -> (f64, f64) {
    let [a, b, c, d, ..] = m.0;
    let h = a.hypot(b);
    (h, (a * d - b * c).abs() / h)
}

impl Writer<'_> {
    /// `ClippingPathSettings` of an image, PDF, EPS or placed page, from
    /// chunk 0x2C1A; a graphic without the chunk has the values every such
    /// graphic has in IDML. A value that the stored fields do not settle is
    /// left out. See `docs/format/objects.md`, clipping path settings.
    pub(super) fn clipping(x: &mut Xml, g: &Graphic) {
        let default = ClippingPath {
            kind: 0,
            tolerance: 2.0,
            inset: 0.0,
            threshold: 25,
            index: -1,
            flags: 2,
        };
        let c = g.clipping.as_ref().unwrap_or(&default);
        let kind = match c.kind {
            0 => "None",
            1 => "DetectEdges",
            2 => "AlphaChannel",
            3 => "PhotoshopPath",
            _ => return,
        };
        // An alpha channel clip of an image without named channels (the
        // transparency of a PNG) is `$ID/` with index 0 (Folio fork, folio.5:
        // the Atlas and Folio's clipping specimen, both PNG).
        let path_name = match c.kind {
            0 | 1 => Some("$ID/"),
            2 => Some(g.alpha_channel.as_deref().unwrap_or("$ID/")),
            _ => g
                .image
                .as_ref()
                .and_then(|i| i.clipping_path_name.as_deref()),
        };
        // Bit 0x01 is `InvertPath` and 0x08 `RestrictToFrame` (folio.5: a
        // specimen with invert alone stores 0x03).
        let invert = (c.flags & 0x01 != 0).to_string();
        let restrict = (c.flags & 0x08 != 0).to_string();
        x.start("ClippingPathSettings")
            .attr("UseHighResolutionImage", (c.flags & 0x02 != 0).to_string())
            .attr("Threshold", c.threshold.to_string())
            .attr("Tolerance", num(c.tolerance))
            .attr("InsetFrame", num(c.inset));
        // The i16 is not the index of an alpha channel (objects.md); a PNG's
        // transparency is index 0 (above). A named channel's index is not
        // known.
        if c.kind != 2 {
            x.attr("Index", c.index.to_string());
        } else if g.alpha_channel.is_none() {
            x.attr("Index", "0");
        }
        x.attr("ClippingType", kind);
        x.attr("InvertPath", invert.clone());
        x.attr("IncludeInsideEdges", "false");
        x.attr("RestrictToFrame", restrict);
        if let Some(name) = path_name {
            x.attr("AppliedPathName", name);
        }
        x.end();
    }

    /// `ImageIOPreference` of an image, from chunk 0x1714; without the
    /// chunk, the values every image without it has.
    fn image_io(x: &mut Xml, g: &Graphic) {
        let (clipping, alpha) = match &g.import {
            Some((clipping, alpha)) => (*clipping, alpha.idml()),
            None => (true, "$ID/".to_string()),
        };
        x.empty(
            "ImageIOPreference",
            &[
                ("ApplyPhotoshopClippingPath", clipping.to_string()),
                ("AllowAutoEmbedding", "true".into()),
                ("AlphaChannelName", alpha),
            ],
        );
    }

    /// A `Link` element. Values without an INDD field are those of every
    /// link in the corpus IDML files (`idml-values.md`, links). See
    /// `docs/format/objects.md`, links.
    pub(super) fn link(&self, x: &mut Xml, link: &Link) {
        let major = self.doc.version.major;
        x.start("Link").attr("Self", uref(Some(link.uid)));
        // The values every link has from DOM 21.5 (idml-values.md, links).
        let dom = self.dom_version();
        if dom >= (21, 5) {
            x.attr("AiGeneratedAsset", "false")
                .attr("AiFullyGeneratedAsset", "false")
                .attr("AiC2paArchiveData", "$ID/");
        }
        x.attr("AssetURL", "$ID/")
            .attr("AssetID", "$ID/")
            .attr("LinkResourceURI", link.uri.clone());
        if let Some(f) = &link.format {
            x.attr("LinkResourceFormat", builtin_key(f));
        }
        let state = if link.embedded {
            "Embedded"
        } else if link.contained {
            "Contained"
        } else {
            "Normal"
        };
        // The class of the link object (objects.md, links).
        let class = if link.class == 0 { 0x8C42 } else { link.class };
        x.attr("StoredState", state)
            .attr("LinkClassID", class.to_string())
            .attr("LinkClientID", "257");
        if let Some(m) = link.modified {
            x.attr("LinkResourceModified", m.to_string());
        }
        x.attr("LinkObjectModified", "false");
        if let Some(s) = link.shown {
            x.attr("ShowInUI", s.to_string());
        }
        x.attr("CanEmbed", "true")
            .attr("CanUnembed", "true")
            .attr("CanPackage", "true")
            .attr("ImportPolicy", "NoAutoImport")
            .attr("ExportPolicy", "NoAutoExport");
        if let Some(stamp) = &link.stamp {
            x.attr("LinkImportStamp", stamp.clone());
        }
        if let Some([modified, imported]) = link.times {
            if link.stamp.is_some() {
                x.attr(
                    "LinkImportModificationTime",
                    link_time(modified, &self.doc.xmp_dates),
                );
            }
            x.attr("LinkImportTime", link_time(imported, &self.doc.xmp_dates));
        }
        if let Some([high, low]) = link.size {
            x.attr("LinkResourceSize", format!("{high:x}~{low:x}"));
        }
        if major >= 13 {
            x.attr("RenditionData", "Actual");
        }
        if major >= 21 {
            x.attr(
                "PDFIdentifier",
                link.pdf_identifier.unwrap_or(0).to_string(),
            );
        }
        // Every link has it from DOM 21.4 (idml-values.md, links).
        if dom >= (21, 4) {
            x.attr("SourceObjectName", "");
        }
        x.end();
    }

    /// The vector colour policies of a PDF or EPS: from chunk 0x7C42, or
    /// without it from the document's colour policies (`objects.md`,
    /// vector colour policies).
    fn vector_policies(&self, g: &Graphic) -> [Option<&'static str>; 2] {
        let document = |name: &str| {
            self.doc
                .prefs
                .values
                .iter()
                .find(|v| v.element == "Document" && v.name == name)
                .map(|v| v.value.as_str())
        };
        let rgb = match document("RGBPolicy") {
            Some("ColorPolicyOff") => Some("IgnoreAll"),
            Some(_) => Some("HonorAllProfiles"),
            None => None,
        };
        let cmyk = match document("CMYKPolicy") {
            Some("ColorPolicyOff" | "CombinationOfPreserveAndSafeCmyk") => Some("IgnoreAll"),
            Some("PreserveEmbeddedProfiles" | "ConvertToWorkingSpace") => Some("HonorAllProfiles"),
            _ => None,
        };
        match g.vector_policies {
            Some([_, r, _, c]) => [vector_policy(r).or(rgb), vector_policy(c).or(cmyk)],
            None => [rgb, cmyk],
        }
    }

    /// `outer` is the transform from the graphic's parent to the spread.
    /// The contour of a graphic's text wrap: its own (chunk 0x373D), else
    /// that of new graphics in the preferences (objects.md, text wrap).
    pub(super) fn graphic_contour(&self, g: &Graphic) -> Option<ContourOption> {
        let c = g.contour.or(self.doc.prefs.contour)?;
        // An `AlphaChannel` contour names the image's first channel
        // (objects.md, text wrap, contour).
        let path_name = match &g.alpha_channel {
            Some(name) if c.kind == 2 => name.clone(),
            _ => "$ID/".into(),
        };
        Some(ContourOption {
            contour: Some(c),
            path_name,
            params: self.dom_version() >= (21, 4),
        })
    }

    pub(super) fn placed_graphic(&self, x: &mut Xml, g: &Graphic, outer: &Matrix) {
        let major = self.doc.version.major;
        let tag = match g.kind {
            GraphicKind::Image => "Image",
            GraphicKind::Pdf => "PDF",
            GraphicKind::Eps => "EPS",
            GraphicKind::Svg => "SVG",
            GraphicKind::ImportedPage => "ImportedPage",
        };
        let [left, top, right, bottom] = g.bounds;
        x.start(tag).attr("Self", uref(Some(g.uid)));
        // The page number is the stored index + 1 (objects.md, placed
        // InDesign pages).
        if let Some(i) = g.page_index.and_then(|i| i.checked_add(1)) {
            x.attr("PageNumber", i.to_string());
        }
        match g.kind {
            GraphicKind::Image => {
                if g.link.is_some()
                    && let Some(p) = &g.image
                {
                    if let Some(space) = p.color_space.and_then(|c| color_space(c, p.indexed)) {
                        x.attr("Space", space);
                    }
                    if let Some([h, v]) = p.resolution {
                        let actual = [round_half_up(h), round_half_up(v)];
                        x.attr("ActualPpi", nums(&actual));
                        let (sh, sv) = scales(&g.transform.then(outer));
                        if sh > 0.0 && sv > 0.0 {
                            let effective =
                                [round_half_up(actual[0] / sh), round_half_up(actual[1] / sv)];
                            if effective.iter().all(|e| e.is_finite()) {
                                x.attr("EffectivePpi", nums(&effective));
                            }
                        }
                    }
                }
                x.attr("ImageRenderingIntent", "UseColorSettings");
            }
            GraphicKind::Pdf | GraphicKind::Eps => {
                x.attr("GrayVectorPolicy", "IgnoreAll");
                let [rgb, cmyk] = self.vector_policies(g);
                if let Some(v) = rgb {
                    x.attr("RGBVectorPolicy", v);
                }
                if let Some(v) = cmyk {
                    x.attr("CMYKVectorPolicy", v);
                }
            }
            GraphicKind::Svg => {
                x.attr("UseSVGAs", "EmbedCode");
            }
            GraphicKind::ImportedPage => {}
        }
        x.attr("LocalDisplaySetting", "Default");
        if let Some(f) = g.link.as_ref().and_then(|l| l.format.as_ref()) {
            x.attr("ImageTypeName", builtin_key(f));
        }
        if let Some(style) = g.object_style.and_then(|u| self.object_style_ref(u)) {
            x.attr("AppliedObjectStyle", style);
        }
        x.attr("ItemTransform", matrix(&g.transform));
        // From 21.4; -1 without the chunk (objects.md, page item settings).
        if self.saved_by((21, 4)) {
            if let Some(layer) = g.props.grouping_layer {
                x.attr("BeforeGroupingLayerReference", uref(Some(layer)));
            }
            x.attr(
                "BeforeGroupingLayerPosition",
                g.props.grouping_layer_position.unwrap_or(-1).to_string(),
            );
        }
        // IDML has no `AllowOverrides` on placed graphics.
        let props = crate::model::ItemProps {
            allow_overrides: None,
            ..g.props.clone()
        };
        self.settings(x, &props, None);
        if g.kind == GraphicKind::Image {
            for (name, v) in self.item_attr_values(&g.attrs) {
                if name == "FillColor" {
                    x.attr(name, v);
                }
            }
            for (name, v) in Self::gradient_values(&g.attrs) {
                if name.starts_with("GradientFill") {
                    x.attr(name, v);
                }
            }
        }
        if major >= 21 && !matches!(g.kind, GraphicKind::Eps | GraphicKind::ImportedPage) {
            x.attr("FlexItemWidthMode", "FlexFixed")
                .attr("FlexItemHeightMode", "FlexFixed");
        }
        if g.kind == GraphicKind::Svg && (major, self.doc.version.minor) >= (21, 2) {
            x.attr("IsMathMLObject", "false");
        }
        x.start("Properties");
        if g.kind == GraphicKind::Image
            && let Some(p) = profile(g.profile)
        {
            x.start("Profile").attr("type", "string").text(p).end();
        }
        if let Some(data) = &g.contents {
            x.start("Contents")
                .cdata(&base64_lines(data), CDATA_SECTION)
                .end();
        }
        if !g.clipping_paths.is_empty() {
            Self::geometry_paths(x, "ClippingPathGeometry", &g.clipping_paths);
        }
        x.empty(
            "GraphicBounds",
            &[
                ("Left", num(left)),
                ("Top", num(top)),
                ("Right", num(right)),
                ("Bottom", num(bottom)),
            ],
        );
        x.end();
        if g.kind != GraphicKind::Svg {
            Self::clipping(x, g);
        }
        if g.kind == GraphicKind::Image {
            Self::image_io(x, g);
        }
        if g.kind == GraphicKind::Pdf
            || g.kind == GraphicKind::Image && g.layers.as_ref().is_some_and(|l| l.option)
        {
            self.graphic_layers(x, g);
        }
        Self::text_wrap_preference(x, g.text_wrap.as_ref(), self.graphic_contour(g));
        if let Some(link) = &g.link {
            self.link(x, link);
        }
        if g.kind == GraphicKind::Pdf {
            let (page, transparent, crop) = match g.pdf {
                Some((page, transparent, crop)) => (
                    page.to_string(),
                    match transparent {
                        0 => Some("false"),
                        1 => Some("true"),
                        _ => None,
                    },
                    pdf_crop(crop),
                ),
                None => (
                    "1".to_string(),
                    Some("true"),
                    Some("CropContentVisibleLayers"),
                ),
            };
            x.start("PDFAttribute").attr("PageNumber", page);
            if let Some(c) = crop {
                x.attr("PDFCrop", c);
            }
            if let Some(t) = transparent {
                x.attr("TransparentBackground", t);
            }
            x.end();
        }
        if let Some(q) = &g.qr {
            self.qr_code(x, q);
        }
        x.end();
    }

    /// The QR code element of an EPS graphic (Folio fork, folio.5): its
    /// contents as stored, and `QrCodeSwatch` with the swatch's colour name
    /// (a tint's base colour, as InDesign writes it). Without a swatch
    /// InDesign writes no `Properties`, except an empty swatch for a vCard.
    fn qr_code(&self, x: &mut Xml, q: &QrCode) {
        let tag = match &q.kind {
            QrKind::PlainText(_) => "PlainTextQRCode",
            QrKind::Hyperlink(_) => "HyperlinkQRCode",
            QrKind::Email { .. } => "EmailQRCode",
            QrKind::TextMessage { .. } => "TextMessageQRCode",
            QrKind::VCard { .. } => "VCardQRCode",
        };
        x.start(tag).attr("Self", uref(Some(q.uid)));
        match &q.kind {
            QrKind::PlainText(t) => {
                x.attr("PlainText", t.clone());
            }
            QrKind::Hyperlink(u) => {
                x.attr("UrlLink", u.clone());
            }
            QrKind::Email { address, subject, body } => {
                x.attr("EmailAddress", address.clone())
                    .attr("Subject", subject.clone())
                    .attr("Body", body.clone());
            }
            QrKind::TextMessage { number, message } => {
                x.attr("TextMessage", message.clone()).attr("CellNumber", number.clone());
            }
            QrKind::VCard { fields, state } => {
                let f = |i: usize| fields.get(i).cloned().unwrap_or_default();
                // Stored order: first, last, title, phone, cell, e-mail,
                // website, organisation, street, city, postal code, country.
                x.attr("Website", f(6))
                    .attr("PostalCode", f(10))
                    .attr("Country", f(11))
                    .attr("City", f(9))
                    .attr("AdrState", state.clone().unwrap_or_default())
                    .attr("StreetAddress", f(8))
                    .attr("Organisation", f(7))
                    .attr("Email", f(5))
                    .attr("Phone", f(3))
                    .attr("CellPhone", f(4))
                    .attr("JobTitle", f(2))
                    .attr("LastName", f(1))
                    .attr("FirstName", f(0));
            }
        }
        let swatch = self.qr_swatch_name(q.swatch);
        if swatch.is_some() || matches!(q.kind, QrKind::VCard { .. }) {
            x.start("Properties")
                .start("QrCodeSwatch")
                .attr("type", "string")
                .text(&swatch.unwrap_or_default())
                .end()
                .end();
        }
        x.end();
    }

    /// The colour name of swatch `uid` (a tint names its base colour).
    fn qr_swatch_name(&self, uid: u32) -> Option<String> {
        if uid == 0 {
            return None;
        }
        let color = |u: u32| self.doc.colors.iter().find(|c| c.uid == u);
        let base = match self.doc.tints.iter().find(|(t, ..)| t.uid == uid) {
            Some((t, ..)) => t.base,
            None => uid,
        };
        color(base).map(|c| c.name.clone())
    }

    /// `GraphicLayerOption` with the graphic's layers, and for an image
    /// `LayerCompOption`. See `docs/format/objects.md`, graphic layers.
    fn graphic_layers(&self, x: &mut Xml, g: &Graphic) {
        let layers = g.layers.as_ref().map_or(&[][..], |l| &l.layers[..]);
        x.start("GraphicLayerOption")
            .attr("UpdateLinkOption", "KeepOverrides");
        let base = format!("{}GraphicLayerOption1", uref(Some(g.uid)));
        // Top-level layers, then each layer's children, in record order. A
        // layer whose parent is not listed is left out.
        let mut path = Vec::new();
        Self::layer_children(x, layers, -1, &base, &mut path);
        x.end();
        if g.kind == GraphicKind::Image
            && let Some(comp) = g.layer_comp
        {
            x.empty("LayerCompOption", &[("AppliedLayerComp", comp.to_string())]);
        }
    }

    fn layer_children(
        x: &mut Xml,
        layers: &[GraphicLayer],
        parent: i32,
        base: &str,
        path: &mut Vec<u32>,
    ) {
        for l in layers.iter().filter(|l| l.parent == parent) {
            // A layer that contains itself would never end.
            if path.contains(&l.id) || i32::try_from(l.id).is_err() {
                continue;
            }
            path.push(l.id);
            let id: String = path.iter().map(|i| format!("i{i:x}")).collect();
            x.start("GraphicLayer")
                .attr("Self", format!("{base}{id}"))
                .attr("Name", layer_name(&l.name))
                .attr("OriginalVisibility", l.original_visibility.to_string())
                .attr("CurrentVisibility", l.current_visibility.to_string())
                .attr("SeparatorLayer", (l.flags & 0x01 != 0).to_string())
                .attr("AdjustmentLayer", "false")
                .attr("FXLayer", (l.flags & 0x04 != 0).to_string())
                .attr("Locked", (l.flags & 0x08 != 0).to_string());
            for name in [
                "HasViewState",
                "ViewState",
                "HasExportState",
                "ExportState",
                "HasPrintState",
                "PrintState",
            ] {
                x.attr(name, "false");
            }
            x.attr("Id", l.id.to_string());
            Self::layer_children(x, layers, l.id as i32, base, path);
            x.end();
            path.pop();
        }
    }
}

/// A layer name: a key (flag 1) is written with `$ID/`.
fn layer_name(n: &Name) -> String {
    n.idml()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_scales_of_skewed_transforms() {
        let (h, v) = scales(&Matrix([2.0, 0.0, 0.0, 3.0, 5.0, 5.0]));
        assert_eq!((h, v), (2.0, 3.0));
        // A skew keeps the area: the vertical scale is the determinant
        // over the horizontal scale.
        let (h, v) = scales(&Matrix([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]));
        assert_eq!((h, v), (1.0, 1.0));
    }

    #[test]
    fn rounds_resolutions_half_up() {
        assert_eq!(round_half_up(76.5), 77.0);
        assert_eq!(round_half_up(72.009), 72.0);
    }

    #[test]
    fn names_colour_spaces() {
        assert_eq!(color_space(2, true), Some("$ID/#Links_Indexed RGB"));
        assert_eq!(color_space(4, false), Some("$ID/#Links_CMYK"));
        assert_eq!(color_space(3, false), None);
        assert_eq!(profile(None), Some("$ID/None"));
        assert_eq!(profile(Some(2)), None);
    }
}
