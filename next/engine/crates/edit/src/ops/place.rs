//! Ordinary placement with content-aware fit disabled (basic/place-*, basic-place-formats:
//! TIFF, PSD, CMYK and grayscale, EXIF orientation). The image's colour space is the Image's
//! `Space` (`$ID/#Links_RGB`, `…_CMYK`, `…_Grayscale`); an EXIF rotation becomes the graphic's
//! transform (orientation 6: a quarter turn clockwise, the turned image centred in the frame).
//! Unassigned frames centre at 100%; replacement preserves the old graphic's displayed
//! size and position. Page.place makes a [None] frame centred on its path, with no fitting
//! override. The application's content-aware setting is deliberately not emulated.
use super::*;
use folio_links::probe::{ColorSpace, ImageFormat};
mod pdf;
mod svg;
mod eps;
pub(crate) mod inline;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicSource {
    pub uri: String,
    pub bytes: u64,
    /// Natural size in points, before any frame fitting.
    pub size: [f64; 2],
    pub ppi: [f64; 2],
    pub format: String,
    pub embedded_profile: bool,
    /// "RGB", "CMYK", "Grayscale", "LAB", "Indexed RGB" or "Black and White" (a 1-bit BMP) (the Image's `Space`).
    #[serde(default = "rgb")]
    pub space: String,
    /// EXIF orientation: 1 as stored, 3 a half turn, 6 a quarter turn clockwise, 8 anticlockwise. The mirrored values (2, 4, 5, 7)
    /// are kept but InDesign ignores them: the image sits as stored (placeimg-c1: place-jpeg-exif2/4/5).
    #[serde(default = "upright")]
    pub orientation: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf: Option<folio_links::pdfplace::Source>,
    /// The XMP packet InDesign's toolkit makes from the file's own tags (`MetadataPacketPreference`), none for files without such tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xmp: Option<String>,
    /// Import option "Rendering Intent" (`Image/@ImageRenderingIntent`, one of `folio_model::image_color::INTENTS`); none = `UseColorSettings`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendering_intent: Option<String>,
}

fn rgb() -> String {
    "RGB".into()
}

fn upright() -> u8 {
    1
}

impl GraphicSource {
    pub fn extension(&self) -> &'static str {
        match self.format.as_str() { "SVG" => "svg", "EPS" => "eps", "EMF" => "emf", "WMF" => "wmf", "PNG" => "png", "TIFF" => "tif", "Photoshop" => "psd", "PDF" => "pdf", "GIF" => "gif", "BMP" => "bmp", _ => "jpg" }
    }

    /// The image's size as placed (turned by its orientation).
    fn shown(&self) -> [f64; 2] {
        if matches!(self.orientation, 6 | 8) { [self.size[1], self.size[0]] } else { self.size }
    }

    /// The graphic's transform for its natural size with its top left of the turned image at
    /// (`left`, `top`).
    fn turned(&self, left: f64, top: f64) -> Mat {
        let [w, h] = self.size;
        match self.orientation {
            6 => Mat([0., 1., -1., 0., left + h, top]),
            8 => Mat([0., -1., 1., 0., left, top + w]),
            3 => Mat([-1., 0., 0., -1., left + w, top + h]),
            _ => Mat([1., 0., 0., 1., left, top]),
        }
    }

    /// InDesign's name of the file format (`ImageTypeName`, `LinkResourceFormat`).
    fn type_name(&self) -> &'static str {
        match self.format.as_str() {
            "PNG" => "Portable Network Graphics (PNG)",
            "TIFF" => "TIFF",
            "Photoshop" => "Photoshop",
            "GIF" => "CompuServe GIF",
            "BMP" => "Windows Bitmap",
            _ => "JPEG",
        }
    }
}

/// The refusal of a picture format Folio does not place: said by name, so a person (and the import notes) know what is not carried.
fn unsupported_format(_format: ImageFormat) -> String {
    "이 형식은 아직 지원하지 않습니다: PDF로 읽을 수 없는 AI·PDF 파일. JPEG·PNG·TIFF·PSD·GIF·BMP·PDF를 놓을 수 있습니다".into()
}

impl GraphicSource {
    pub fn read(path: &std::path::Path, uri: String) -> Result<Self, String> {
        Self::read_with_pdf(path, uri, None)
    }

    pub fn read_with_pdf(path: &std::path::Path, uri: String, options: Option<folio_links::pdfplace::Options>) -> Result<Self, String> {
        let p = folio_links::probe::probe(path).map_err(|e| e.to_string())?;
        if matches!(p.format, ImageFormat::Pdf | ImageFormat::Ai) && !p.page_boxes.is_empty() {
            let pdf = folio_links::pdfplace::inspect(path, &p, options.unwrap_or_default())?;
            let b = pdf.bounds;
            return Ok(Self { uri, bytes: p.file_size, size: [b[2]-b[0], b[3]-b[1]], ppi: [72.,72.],
                format: "PDF".into(), embedded_profile: false, space: "RGB".into(), orientation: 1, pdf: Some(pdf), xmp: None, rendering_intent: None });
        }
        if p.format == ImageFormat::Svg {
            let size = folio_links::probe::svg_size(path).map_err(|e| e.to_string())?;
            return Ok(Self { uri, bytes:p.file_size, size, ppi:[72.,72.], format:"SVG".into(), embedded_profile:false,
                space:"RGB".into(), orientation:1, pdf:None, xmp:None, rendering_intent:None });
        }
        if options.is_some() { return Err("PDF import options require a PDF-compatible file".into()); }
        if p.format == ImageFormat::Wmf {
            // a Windows metafile: drawn from its records as vectors (folio_render::metafile), its size the header's
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let kind = folio_links::probe::metafile::looks_like_metafile(&bytes).unwrap_or("WMF");
            let size = folio_links::probe::metafile::size(&bytes).ok_or("메타파일(EMF·WMF)의 크기를 읽을 수 없습니다")?;
            return Ok(Self { uri, bytes: p.file_size, size, ppi: [72., 72.], format: kind.into(), embedded_profile: false, space: "RGB".into(),
                orientation: 1, pdf: None, xmp: None, rendering_intent: None });
        }
        if p.format == ImageFormat::Eps {
            // the bounding box (a Photoshop EPS: its pixels at its resolution); shown and printed through the preview it carries
            let (x, y) = p.ppi.unwrap_or((72., 72.));
            let size = if p.vector { [p.width as f64, p.height as f64] } else { [p.width as f64 * 72. / x, p.height as f64 * 72. / y] };
            if size.iter().any(|v| !v.is_finite() || *v <= 0.) {
                return Err("EPS 의 경계 상자(BoundingBox)가 올바르지 않습니다".into());
            }
            let space = match p.color_space { ColorSpace::Cmyk => "CMYK", ColorSpace::Gray => "Grayscale", ColorSpace::Lab => "LAB", _ => "RGB" };
            return Ok(Self { uri, bytes: p.file_size, size, ppi: [x.round(), y.round()], format: "EPS".into(), embedded_profile: false, space: space.into(),
                orientation: 1, pdf: None, xmp: own_packet(path), rendering_intent: None });
        }
        let format = match p.format {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Tiff => "TIFF",
            ImageFormat::Psd => "Photoshop",
            ImageFormat::Gif => "GIF",
            ImageFormat::Bmp => "BMP",
            other => return Err(unsupported_format(other)),
        };
        // InDesign's Space of a palette image: a PNG palette is read as RGB, a GIF or TIFF palette stays "Indexed RGB" (placeimg-c1)
        // a BMP: 1 bit is "Black and White", 4 and 8 bits are palette pictures, 24 and 32 bits are RGB (screens-import: bmp-*)
        if format == "BMP" && !(matches!(p.color_space, ColorSpace::Indexed) && matches!(p.bits, 1 | 4 | 8) || matches!(p.color_space, ColorSpace::Rgb) && matches!(p.channels, 3 | 4)) {
            return Err("이 BMP의 색 깊이는 아직 지원하지 않습니다: 1·4·8·24·32비트 BMP를 놓을 수 있습니다".into());
        }
        let space = match (p.color_space, format) {
            (ColorSpace::Indexed, "BMP") if p.bits == 1 => "Black and White",
            (ColorSpace::Indexed, "BMP") => "Indexed RGB",
            (ColorSpace::Rgb, _) | (ColorSpace::Indexed, "PNG") => "RGB",
            (ColorSpace::Cmyk, _) => "CMYK",
            (ColorSpace::Gray, _) => "Grayscale",
            // a Lab PSD (8 and 16 bits; build-a2) as a Lab TIFF
            (ColorSpace::Lab, "TIFF" | "Photoshop") => "LAB",
            (ColorSpace::Indexed, "GIF" | "TIFF") => "Indexed RGB",
            _ => return Err("이 그림의 색 모델은 아직 지원하지 않습니다: RGB·CMYK·회색 음영·Lab(TIFF·PSD)·색인(PNG·GIF·TIFF·BMP) 그림을 놓을 수 있습니다".into()),
        };
        let orientation = p.orientation.unwrap_or(1);
        // a GIF with a transparent colour: its IDML is the same as an opaque one's (screens-import: place-gif-transparent); the
        // transparent pixels show what is behind the picture, on the screen and in the PDF (`render::gif`, `pdf::images`)
        // 16 bits per channel: measured for RGB and grayscale PNG / TIFF / flat PSD; Lab, indexed, CMYK and JPEG stay at 8
        // a palette picture of 1, 2 or 4 bits (PNG, BMP) is read as the 8-bit palette picture it is (screens-import: place-png-indexed*, place-bmp*)
        let low_palette = matches!(p.color_space, ColorSpace::Indexed) && matches!(p.bits, 1 | 2 | 4) && matches!(format, "PNG" | "BMP");
        // 16-bit CMYK and CIELab (TIFF, PSD CMYK; build-a): drawn and printed from the high byte of each sample, the colour through the
        // document CMYK profile (CMYK) or CIELab D50 (Lab), as the 8-bit pictures
        let deep = p.bits == 16 && (matches!(format, "PNG" | "TIFF" | "Photoshop") && matches!(space, "RGB" | "Grayscale") || matches!((format, space), ("TIFF" | "Photoshop", "CMYK" | "LAB")));
        let depth_ok = p.bits == 8 || low_palette || deep;
        if !depth_ok || !(1..=8).contains(&orientation) {
            return Err("이 그림의 비트 수는 아직 지원하지 않습니다: 8비트 그림, 16비트 RGB·회색 음영 PNG·TIFF·PSD, 16비트 CMYK·Lab TIFF·PSD, 1·2·4비트 색인 PNG·BMP를 놓을 수 있습니다".into());
        }
        // an embedded CMYK or gray profile: measured for JPEG and TIFF (placeimg-c1): the output keeps the numbers (InDesign's default
        // colour policy for CMYK, Preserve Numbers), the Image's Profile string differs. A PSD or PNG profile is treated the same (build-a).
        let (x, y) = p.ppi.unwrap_or((72., 72.));
        let size = [p.width as f64 * 72. / x, p.height as f64 * 72. / y];
        if size.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("그림 크기 또는 해상도가 올바르지 않습니다".into());
        }
        Ok(Self {
            uri,
            bytes: p.file_size,
            size,
            ppi: [x.round(), y.round()],
            format: format.into(),
            embedded_profile: p.icc.is_some(),
            space: space.into(),
            orientation: orientation as u8,
            pdf: None,
            xmp: folio_links::probe::raster_xmp(path).or_else(|| own_packet(path)),
            rendering_intent: None,
        })
    }
}

/// The XMP packet the file carries, kept with the graphic as it is (`MetadataPacketPreference`): captions and the Links panel read
/// it from the document, and it goes to InDesign with the IDML. InDesign writes the same properties in its own layout (place-xmp-jpg,
/// place-xmp-tif); the properties are the same, the text is the file's (build-a).
pub(crate) fn own_packet(path: &std::path::Path) -> Option<String> {
    let packet = folio_links::probe::xmp::read_packet(path)?;
    if packet.contains("]]>") {
        return None;
    }
    Some(format!("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n{packet}\n<?xpacket end=\"r\"?>"))
}

/// Links panel ▸ Embed Link: `<Contents>` in the graphic's `Properties` before its `GraphicBounds`, the link's `StoredState` Embedded.
pub(super) fn embed(w: &mut Work<'_>, id: &str, data: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let graphic = if GRAPHICS.contains(&ix.local(&t, i)) {
        i
    } else {
        content_of(&ix, &t, i).ok_or_else(|| EditError::new("invalid", format!("{id} holds no linked graphic")))?.graphic
    };
    let link = ix.named(&t, graphic, "Link").first().copied().ok_or_else(|| EditError::new("invalid", "the graphic has no link"))?;
    if ix.attr(&t, link, "StoredState") == Some("Embedded") || !ix.own_descendants(&t, graphic, "Contents").is_empty() {
        return Err(EditError::new("invalid", "the graphic is embedded already"));
    }
    if data.trim().is_empty() || !data.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'\n')) {
        return Err(EditError::new("invalid", "the embedded data is not base64"));
    }
    let props = ix.named(&t, graphic, "Properties").first().copied().ok_or_else(|| EditError::new("xml", "a graphic without Properties"))?;
    let bounds = ix.named(&t, props, "GraphicBounds").first().copied().ok_or_else(|| EditError::new("xml", "a graphic without GraphicBounds"))?;
    let indent = indent_before(&t, ix.nodes[bounds].start).to_owned();
    let at = ix.nodes[bounds].start;
    let mut changes = vec![(at..at, format!("<Contents><![CDATA[{data}]]></Contents>\n{indent}"))];
    changes.push(attr_edit(&ix, &t, link, "StoredState", "Embedded"));
    w.splice(&part, changes)
}

/// The graphic (and its link) `id` names: the graphic itself, or the one in the frame.
fn graphic_and_link(w: &mut Work<'_>, id: &str) -> Result<(String, String, Index, usize, usize), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let graphic = if GRAPHICS.contains(&ix.local(&t, i)) {
        i
    } else {
        content_of(&ix, &t, i).ok_or_else(|| EditError::new("invalid", format!("{id} holds no linked graphic")))?.graphic
    };
    let link = ix.named(&t, graphic, "Link").first().copied().ok_or_else(|| EditError::new("invalid", "the graphic has no link"))?;
    Ok((part, t, ix, graphic, link))
}

/// The embedded data (base64 as written) of the graphic `id` names, none if it is not embedded.
pub(crate) fn embedded_data(ed: &crate::Editor, id: &str) -> Option<String> {
    for t in ed.parts().values() {
        if !t.contains(&format!(" Self=\"{id}\"")) {
            continue;
        }
        let ix = Index::scan(t).ok()?;
        let i = ix.by_self(t, id)?;
        let graphic = if GRAPHICS.contains(&ix.local(t, i)) { i } else { content_of(&ix, t, i)?.graphic };
        let props = ix.named(t, graphic, "Properties").first().copied()?;
        let c = ix.named(t, props, "Contents").first().copied()?;
        let n = &ix.nodes[c];
        let body = t[n.open_end..n.close_start].trim();
        return Some(body.strip_prefix("<![CDATA[").and_then(|b| b.strip_suffix("]]>")).unwrap_or(body).to_owned());
    }
    None
}

/// Links panel ▸ Unembed Link: only `Contents` goes and the link changes (oracle: unembed-link).
pub(super) fn unembed(w: &mut Work<'_>, id: &str, uri: &str) -> Result<(), EditError> {
    let (part, t, ix, graphic, link) = graphic_and_link(w, id)?;
    let props = ix.named(&t, graphic, "Properties").first().copied().ok_or_else(|| EditError::new("xml", "a graphic without Properties"))?;
    let contents = ix.named(&t, props, "Contents").first().copied().ok_or_else(|| EditError::new("invalid", "the graphic is not embedded"))?;
    if uri.is_empty() {
        return Err(EditError::new("invalid", "the unembedded file needs an address"));
    }
    let changes = vec![
        (element_span(&ix, &t, contents), String::new()),
        attr_edit(&ix, &t, link, "StoredState", "Normal"),
        attr_edit(&ix, &t, link, "LinkResourceURI", uri),
    ];
    w.splice(&part, changes)
}

/// Links panel ▸ Relink: `id` is the frame holding the linked graphic, or the graphic.
pub(super) fn relink(w: &mut Work<'_>, source: &GraphicSource, id: &str) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    let frame = if GRAPHICS.contains(&ix.local(&t, i)) {
        let p = ix.nodes[i].parent.ok_or_else(|| EditError::new("invalid", "a graphic outside a frame"))?;
        ix.attr(&t, p, "Self").ok_or_else(|| EditError::new("invalid", "a graphic outside a frame"))?.to_owned()
    } else {
        id.to_owned()
    };
    if content_of(&ix, &t, ix.by_self(&t, &frame).ok_or_else(|| EditError::new("notFound", frame.clone()))?).is_none() {
        return Err(EditError::new("invalid", format!("{frame} holds no linked graphic")));
    }
    run(w, source, Some(&frame), None, None, None, None, true)
}

/// The `<Image>` element of a placed raster picture. `m` places it in its frame; `embedded` is the picture file in base64 when the document
/// holds the bytes itself (a Word picture: the link is then `Embedded`, `Contents` sits before the graphic bounds).
pub(crate) fn image_element(source: &GraphicSource, graphic: &str, link: &str, m: Mat, embedded: Option<&str>) -> String {
    if source.format == "SVG" { return svg::element(source, graphic, link, m, embedded); }
    if source.format == "EPS" { return eps::element(source, graphic, link, m); }
    if matches!(source.format.as_str(), "EMF" | "WMF") { return eps::metafile_element(source, graphic, link, m); }
    let name = source.type_name();
    let space = &source.space;
    // a grayscale image takes a colour, black at first (place-tiff-gray, place-jpeg-gray)
    let fill = if space == "Grayscale" || space == "Black and White" { r#" FillColor="Color/Black""# } else { "" };
    // a CMYK picture's embedded profile is not kept under "preserve numbers": the Image says "Use Document Default" (place-tiff-cmyk-icc)
    let profile = if !source.embedded_profile {
        "None"
    } else if space == "CMYK" {
        "Use Document Default"
    } else {
        "Embedded"
    };
    let (sx, sy) = (m.0[0].hypot(m.0[1]), m.0[2].hypot(m.0[3]));
    format!(
        r#"<Image Self="{graphic}" Space="$ID/#Links_{space}"{fill} ActualPpi="{ppi}" EffectivePpi="{effective}" ImageRenderingIntent="{intent}" OverriddenPageItemProps="" LocalDisplaySetting="Default" ImageTypeName="$ID/{name}" AppliedObjectStyle="ObjectStyle/$ID/[None]" ItemTransform="{matrix}" HorizontalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" VerticalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" GradientFillStart="0 0" GradientFillLength="0" GradientFillAngle="0" GradientFillHiliteLength="0" GradientFillHiliteAngle="0" Visible="true" Name="$ID/"><Properties><Profile type="string">$ID/{profile}</Profile>{contents}<GraphicBounds Left="0" Top="0" Right="{width}" Bottom="{height}"/></Properties>{WRAP}{metadata}<Link Self="{link}" AssetURL="$ID/" AssetID="$ID/" RenditionData="Actual" LinkResourceURI="{uri}" LinkResourceFormat="$ID/{name}" StoredState="{stored}" LinkClassID="35906" LinkClientID="257" LinkResourceModified="false" LinkObjectModified="false" ShowInUI="true" CanEmbed="true" CanUnembed="true" CanPackage="true" ImportPolicy="NoAutoImport" ExportPolicy="NoAutoExport" LinkResourceSize="{bytes}"/>{CLIP}</Image>"#,
        intent = source.rendering_intent.as_deref().unwrap_or("UseColorSettings"),
        metadata = source.xmp.as_deref().map(|x| format!("<MetadataPacketPreference><Properties><Contents><![CDATA[{x}]]></Contents></Properties></MetadataPacketPreference>")).unwrap_or_default(),
        ppi = geom::pair((source.ppi[0], source.ppi[1])),
        effective = geom::pair(((source.ppi[0] / sx).round(), (source.ppi[1] / sy).round())),
        matrix = m.text(),
        width = geom::num(source.size[0]),
        height = geom::num(source.size[1]),
        uri = escape(&source.uri),
        stored = if embedded.is_some() { "Embedded" } else { "Normal" },
        contents = embedded.map(|d| format!("<Contents><![CDATA[{d}]]></Contents>")).unwrap_or_default(),
        bytes = format!("{:x}~{:x}", source.bytes >> 32, source.bytes & 0xffffffff)
    )
}

const CONTOUR: &str = r#"<ContourOption ContourType="SameAsClipping" IncludeInsideEdges="false" ContourPathName="$ID/"/>"#;
const WRAP: &str = r#"<TextWrapPreference Inverse="false" ApplyToMasterPageOnly="false" TextWrapSide="BothSides" TextWrapMode="None"><Properties><TextWrapOffset Top="0" Left="0" Bottom="0" Right="0"/></Properties><ContourOption ContourType="SameAsClipping" IncludeInsideEdges="false" ContourPathName="$ID/"/></TextWrapPreference>"#;
const CLIP: &str = r#"<ClippingPathSettings ClippingType="None" InvertPath="false" IncludeInsideEdges="false" RestrictToFrame="false" UseHighResolutionImage="true" Threshold="25" Tolerance="2" InsetFrame="0" AppliedPathName="$ID/" Index="-1"/><ImageIOPreference ApplyPhotoshopClippingPath="true" AllowAutoEmbedding="true" AlphaChannelName="$ID/"/>"#;

/// A picture with an embedded profile is measured only in a document with the default colour policies.
pub(crate) fn check_profile_policy(w: &mut Work<'_>, source: &GraphicSource) -> Result<(), EditError> {
    if source.embedded_profile {
        let dm = w.text("designmap.xml")?;
        let dx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
        let document = (0..dx.nodes.len()).find(|&i| dx.local(dm, i) == "Document");
        let policy = |name| document.and_then(|i| dx.attr(dm, i, name));
        // measured with the default policies only: RGB keeps embedded profiles, CMYK keeps numbers (ignores linked profiles)
        if policy("RGBPolicy") != Some("PreserveEmbeddedProfiles") || (source.space == "CMYK" && policy("CMYKPolicy") != Some("CombinationOfPreserveAndSafeCmyk")) {
            return Err(EditError::new(
                "unsupported",
                "내장 RGB 프로필을 보존하고 CMYK 숫자를 유지하는 기본 색상 정책의 문서에서만 내장 프로필이 있는 그림을 놓을 수 있습니다",
            ));
        }
    }
    Ok(())
}

// Placement callers supply independent source, geometry and fitting inputs; retain their call contract.
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    w: &mut Work<'_>,
    source: &GraphicSource,
    id: Option<&str>,
    page: Option<&str>,
    point: Option<[f64; 2]>,
    rect: Option<[f64; 4]>,
    layer: Option<&str>,
    relink: bool,
) -> Result<(), EditError> {
    check_profile_policy(w, source)?;
    if source.rendering_intent.as_deref().is_some_and(|i| !folio_model::image_color::INTENTS.contains(&i)) {
        return Err(EditError::new("invalid", "알 수 없는 렌더링 의도입니다"));
    }
    let invalid = || EditError::new("invalid", "invalid graphic placement");
    if source
        .size
        .iter()
        .chain(&source.ppi)
        .any(|v| !v.is_finite() || *v <= 0.)
        || !["PNG", "JPEG", "TIFF", "Photoshop", "PDF", "GIF", "BMP", "SVG", "EPS", "EMF", "WMF"].contains(&source.format.as_str())
        || (source.format == "PDF") != source.pdf.is_some()
        || source.pdf.as_ref().is_some_and(|p| p.options.page_number == 0 || p.bounds.iter().any(|v| !v.is_finite())
            || (p.bounds[2]-p.bounds[0]-source.size[0]).abs()>1e-6 || (p.bounds[3]-p.bounds[1]-source.size[1]).abs()>1e-6)
        || !["RGB", "CMYK", "Grayscale", "LAB", "Indexed RGB", "Black and White"].contains(&source.space.as_str())
        || !(1..=8).contains(&source.orientation)
        || source.uri.is_empty()
        || id.is_some() && (page.is_some() || point.is_some() || rect.is_some())
        || id.is_none() && (page.is_none() || point.is_some() == rect.is_some())
    {
        return Err(invalid());
    }
    let id = if let Some(id) = id {
        id.to_owned()
    } else {
        let r = rect.unwrap_or_else(|| {
            // the image's size as stored, even when its EXIF turns it (place-point-exif6)
            let [x, y] = point.unwrap();
            [x, y, x + source.size[0], y + source.size[1]]
        });
        create::run(
            w,
            page.unwrap(),
            if rect.is_some() {
                ItemKind::Rectangle
            } else {
                ItemKind::RectangleFrame
            },
            r,
            layer,
            6,
            None,
        )?;
        let id = w.created.last().unwrap().clone();
        if point.is_some() {
            // Page.place uses natural-size centred geometry, not add(geometricBounds).
            let (part, ix, i) = w.find(&id)?;
            let t = w.text(&part)?.to_owned();
            let [x0, y0, x1, y1] = frame_box(&ix, &t, i).ok_or_else(invalid)?;
            let (cx, cy) = ((x0 + x1) / 2., (y0 + y1) / 2.);
            let old = item_transform(&ix, &t, i);
            let mut changes = vec![attr_edit(
                &ix,
                &t,
                i,
                "ItemTransform",
                &Mat([1., 0., 0., 1., old.0[4] + cx, old.0[5] + cy]).text(),
            )];
            for k in ["GradientFillStart", "GradientStrokeStart"] {
                changes.push(attr_edit(&ix, &t, i, k, "0 0"));
            }
            for k in ["GradientFillLength", "GradientStrokeLength"] {
                changes.push(attr_edit(&ix, &t, i, k, "0"));
            }
            for n in 0..ix.nodes.len() {
                if ix.local(&t, n) == "PathPointType" && ix.ancestors(n).any(|a| a == i) {
                    for k in ["Anchor", "LeftDirection", "RightDirection"] {
                        let (x, y) = ix
                            .attr(&t, n, k)
                            .and_then(geom::parse_pair)
                            .ok_or_else(invalid)?;
                        changes.push(attr_edit(&ix, &t, n, k, &geom::pair((x - cx, y - cy))));
                    }
                }
            }
            w.splice(&part, changes)?;
            // an EXIF-turned image: the frame's outline is the turned image's box from the
            // stored image's top left; its centre stays (place-point-exif6)
            let [sw, sh] = source.shown();
            if [sw, sh] != source.size {
                let (l, t0) = (x0 - cx, y0 - cy);
                resize(w, &id, Target::Box([l, t0, l + sw, t0 + sh]))?;
            }
        }
        id
    };
    let (part, ix, i) = w.find(&id)?;
    let t = w.text(&part)?.to_owned();
    if (!relink && !part.starts_with("Spreads/")) || !["Rectangle", "Oval", "Polygon"].contains(&ix.local(&t, i))
    {
        return Err(EditError::new(
            "unsupported",
            "그림은 사각형·타원·다각형 프레임에 놓을 수 있습니다",
        ));
    }
    for n in std::iter::once(i).chain(ix.ancestors(i)).filter(|_| !relink) {
        if ix.attr(&t, n, "Locked") == Some("true") || ix.attr(&t, n, "Visible") == Some("false") {
            return Err(EditError::new(
                "locked",
                "잠기거나 숨긴 개체에 놓을 수 없습니다",
            ));
        }
    }
    if let Some(layer) = ix.attr(&t, i, "ItemLayer").filter(|_| !relink) {
        let dm = w.text("designmap.xml")?;
        let dx = Index::scan(dm).map_err(|e| EditError::new("xml", e))?;
        if let Some(li) = dx.by_self(dm, layer)
            && (dx.attr(dm, li, "Locked") == Some("true")
                || dx.attr(dm, li, "Visible") == Some("false"))
        {
            return Err(EditError::new(
                "locked",
                "잠기거나 숨긴 레이어에 놓을 수 없습니다",
            ));
        }
    }
    let b = frame_box(&ix, &t, i).ok_or_else(invalid)?;
    let old = content_of(&ix, &t, i);
    let replacing = old.is_some();
    let [sw, sh] = source.shown();
    // into a frame: centred; at a click: the turned image from the frame's top left, which
    // InDesign leaves the size of the image as stored (place-point-exif6)
    let mut m = if point.is_some() { source.turned(b[0], b[1]) } else { source.turned((b[0] + b[2] - sw) / 2., (b[1] + b[3] - sh) / 2.) };
    if old.is_some() && matches!(source.orientation, 3 | 6 | 8) {
        return Err(EditError::new("unsupported", "replacing a graphic with a turned (EXIF) image is not measured yet"));
    }
    if let Some(c) = &old {
        // place-img: replacing keeps the displayed rectangle, including nonuniform scales.
        let sx = (c.bounds[2] - c.bounds[0]) / source.size[0];
        let sy = if source.pdf.is_some() { sx } else { (c.bounds[3] - c.bounds[1]) / source.size[1] };
        let (tx, ty) = c.m.apply((c.bounds[0], c.bounds[1]));
        m = Mat([
            c.m.0[0] * sx,
            c.m.0[1] * sx,
            c.m.0[2] * sy,
            c.m.0[3] * sy,
            tx,
            ty,
        ]);
    }
    if let Some(pdf) = &source.pdf {
        let [left, top, _, _] = pdf.bounds;
        m.0[4] -= m.0[0] * left + m.0[2] * top;
        m.0[5] -= m.0[1] * left + m.0[3] * top;
    }
    let fitting = ix
        .named(&t, i, "FrameFittingOption")
        .first()
        .and_then(|&f| ix.attr(&t, f, "FittingOnEmptyFrame"))
        .unwrap_or("None")
        .to_owned();
    let graphic = w.new_id();
    // a relink keeps the link's own id (oracle: relink-png: the Link element changes, a placement over the graphic makes a new one)
    let link = match old.as_ref().filter(|_| relink) {
        Some(c) => {
            let l = ix.named(&t, c.graphic, "Link").first().and_then(|&l| ix.attr(&t, l, "Self")).map(str::to_owned);
            l.ok_or_else(|| EditError::new("invalid", "the graphic has no link"))?
        }
        None => w.new_id(),
    };
    let xml = if source.pdf.is_some() { pdf::element(source, &graphic, &link, m) } else { image_element(source, &graphic, &link, m, None) };
    let mut changes = vec![attr_edit(&ix, &t, i, "ContentType", "GraphicType")];
    if let Some(c) = old {
        changes.push((element_span(&ix, &t, c.graphic), xml));
    } else {
        let at = ix.nodes[i].close_start;
        changes.push((at..at, xml));
    }
    if let Some(&wrap) = ix.named(&t, i, "TextWrapPreference").first()
        && ix.named(&t, wrap, "ContourOption").is_empty() {
        let at = ix.nodes[wrap].close_start;
        changes.push((at..at, CONTOUR.into()));
    }
    if ix.named(&t, i, "FrameFittingOption").is_empty() && point.is_none() {
        // place-placeholder: [None] has no fitting override; placing adds just the crops.
        let at = ix
            .named(&t, i, "ObjectExportOption")
            .first()
            .map(|&o| ix.nodes[o].start)
            .unwrap_or(ix.nodes[i].close_start);
        changes.push((
            at..at,
            r#"<FrameFittingOption LeftCrop="0" TopCrop="0" RightCrop="0" BottomCrop="0"/>"#.into(),
        ));
    }
    w.splice(&part, changes)?;
    let (part, ix, i) = w.find(&id)?;
    let t = w.text(&part)?.to_owned();
    let c = content_of(&ix, &t, i).ok_or_else(invalid)?;
    write_content(w, &part, &ix, &t, &c, m, None, true)?;
    let mode = if rect.is_some() {
        Some(Fit::Proportionally)
    } else if replacing {
        None
    } else {
        match fitting.as_str() {
            "None" => None,
            "Proportionally" => Some(Fit::Proportionally),
            "FillProportionally" => Some(Fit::Fill),
            "ContentToFrame" => Some(Fit::ContentToFrame),
            _ => {
                return Err(EditError::new(
                    "unsupported",
                    "이 프레임의 자동 맞춤 방식은 아직 지원하지 않습니다",
                ));
            }
        }
    };
    if let Some(how) = mode {
        fit(w, &id, how)?;
        if rect.is_none() {
            let (part, ix, i) = w.find(&id)?;
            let t = w.text(&part)?.to_owned();
            let c = content_of(&ix, &t, i).ok_or_else(invalid)?;
            write_content(w, &part, &ix, &t, &c, c.m, None, true)?;
        }
    }
    Ok(())
}
