//! Data Merge QR code fields (oracle: probes `datamerge-c4-w` and `-x`, InDesign 2025 20.5.5.119).
//!
//! A QR field (`#Field` in the data source) is bound to an empty frame like an image field: a `QRC_<field>` destination, a
//! `HyperlinkPageItemSource` with the frame, the `Hyperlink`, and a `DataMergeQrcodePlaceholder` element after the image placeholders.
//! Merging puts into the frame of every record whose value is not empty an `EPS` graphic that InDesign draws itself: the modules as
//! `x y w h rf` rectangles with a four-module quiet zone (`%%BoundingBox` 85.039 pt, or two points a module when that is larger),
//! embedded (`StoredState="Embedded"`) with a `PlainTextQRCode` that holds the value (always plain text, black). The graphic fits the
//! frame proportionally to its smaller side and sits in the middle; a record with no value leaves the frame empty. The modules are the
//! ones `folio_model::qr::encode` makes (measured against 132 InDesign codes) and the EPS text is InDesign's byte for byte (tests).
use super::*;
use crate::ops::attr_edit;
use folio_model::qr;

/// InDesign's bounding box of a small code: 85.039 pt (measured: every coordinate of the EPS is `k * 85.039 / modules` as a float, which
/// `85.039001` and the float `85.03900146484375` do not give).
const MIN_BOX: f64 = 85.039;

/// The EPS InDesign writes for the code of `payload` and its bounding box width; `None` when the text does not fit a code.
pub(crate) fn eps(payload: &str) -> Option<(String, f64)> {
    let modules = qr::encode(payload)?;
    let total = modules.size + 2 * qr::QUIET_ZONE;
    let bound = MIN_BOX.max(2.0 * total as f64);
    // the arithmetic is done in doubles and the numbers are written as floats (measured: `67.000427`, not the float product `67.000420`)
    let step = bound / total as f64;
    let mut out = String::new();
    out.push_str("%!PS-Adobe-3.1 EPSF-3.0
");
    out.push_str(&format!("%%BoundingBox: 0.000000 0.000000 {:.6} {:.6}
", bound as f32, bound as f32));
    out.push_str("%%LanguageLevel: 3
%%Pages: 1 
%%DocumentData: Clean7Bit
%%DocumentProcessColors: (atend)
%%DocumentCustomColors: (atend)
%%EndComments
");
    out.push_str("/rf { rectfill } bind def
0.000000 0.000000 0.000000 1.000000 setcmykcolor
");
    for column in 0..modules.size {
        for row in 0..modules.size {
            if !modules.get(column, row) { continue; }
            let x = ((qr::QUIET_ZONE + column) as f64 * bound / total as f64) as f32;
            let y = ((qr::QUIET_ZONE + modules.size - 1 - row) as f64 * bound / total as f64) as f32;
            let side = step as f32;
            out.push_str(&format!("{x:.6} {y:.6} {side:.6} {side:.6} rf
"));
        }
    }
    out.push_str("%%Trailer
%%DocumentProcessColors: Black 
%%DocumentCustomColors: 
%%CMYKCustomColor: 
%%RGBCustomColor: 
%%EOF
");
    Some((out, bound))
}

/// The top left and size of a frame's path in its own coordinates (the points `PathPointType` lists).
fn frame_box(t: &str, ix: &Index, i: usize) -> Option<[f64; 4]> {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for k in ix.own_descendants(t, i, "PathPointType") {
        let v: Vec<f64> = ix.attr(t, k, "Anchor")?.split_whitespace().filter_map(|n| n.parse().ok()).collect();
        if v.len() != 2 { return None; }
        b = [b[0].min(v[0]), b[1].min(v[1]), b[2].max(v[0]), b[3].max(v[1])];
    }
    (b[0] <= b[2] && b[1] <= b[3]).then_some(b)
}

/// The record's QR code into the empty frame `frame`: an embedded EPS fitted to the frame's smaller side and centred (measured).
pub(crate) fn place(w: &mut Work<'_>, frame: &str, payload: &str) -> Result<(), EditError> {
    let (text, bound) = eps(payload).ok_or_else(|| invalid("The QR code does not fit"))?;
    let (part, ix, i) = w.find(frame)?;
    let t = w.text(&part)?.to_owned();
    if !matches!(ix.local(&t, i), "Rectangle" | "Oval" | "Polygon") || ix.children(i).iter().any(|&n| matches!(ix.local(&t, n), "Image" | "PDF" | "EPS" | "ImportedPage")) {
        return Err(invalid("a QR code goes into an empty frame"));
    }
    let b = frame_box(&t, &ix, i).ok_or_else(|| invalid("the frame has no path"))?;
    let (width, height) = (b[2] - b[0], b[3] - b[1]);
    // the graphic's bounds are the float of the box (`85.03900146484375`), the scale is the frame over that
    let bound = bound as f32 as f64;
    let scale = width.min(height) / bound;
    let (tx, ty) = (b[0] + (width - bound * scale) / 2.0, b[1] + (height - bound * scale) / 2.0);
    let (graphic, link, code) = (w.new_id(), w.new_id(), w.new_id());
    let data = base64_lines(text.as_bytes());
    let size = text.len() as u64;
    let bound = format!("{bound}");
    let xml = format!(concat!(
        "<EPS Self=\"{graphic}\" GrayVectorPolicy=\"IgnoreAll\" RGBVectorPolicy=\"HonorAllProfiles\" CMYKVectorPolicy=\"IgnoreAll\" OverriddenPageItemProps=\"\" LocalDisplaySetting=\"Default\" ImageTypeName=\"$ID/\" AppliedObjectStyle=\"ObjectStyle/$ID/[None]\" ItemTransform=\"{scale} 0 0 {scale} {tx} {ty}\" ",
        "HorizontalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" VerticalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" Visible=\"true\" Name=\"$ID/\">",
        "<Properties><Contents><![CDATA[{data}]]></Contents><GraphicBounds Left=\"0\" Top=\"0\" Right=\"{bound}\" Bottom=\"{bound}\" /></Properties>",
        "<TextWrapPreference Inverse=\"false\" ApplyToMasterPageOnly=\"false\" TextWrapSide=\"BothSides\" TextWrapMode=\"None\"><Properties><TextWrapOffset Top=\"0\" Left=\"0\" Bottom=\"0\" Right=\"0\" /></Properties>",
        "<ContourOption ContourType=\"SameAsClipping\" IncludeInsideEdges=\"false\" ContourPathName=\"$ID/\" /></TextWrapPreference>",
        // the code is embedded: its link names no file of this computer (principle 1)
        "<Link Self=\"{link}\" AssetURL=\"$ID/\" AssetID=\"$ID/\" RenditionData=\"Actual\" LinkResourceURI=\"file:QR%20Code{link}\" LinkResourceFormat=\"$ID/\" StoredState=\"Embedded\" LinkClassID=\"35906\" LinkClientID=\"257\" LinkResourceModified=\"false\" LinkObjectModified=\"false\" ShowInUI=\"false\" CanEmbed=\"true\" CanUnembed=\"true\" CanPackage=\"true\" ImportPolicy=\"NoAutoImport\" ExportPolicy=\"NoAutoExport\" LinkImportStamp=\"file 0 {size}\" LinkResourceSize=\"{hi:x}~{lo:x}\" />",
        "<ClippingPathSettings ClippingType=\"None\" InvertPath=\"false\" IncludeInsideEdges=\"false\" RestrictToFrame=\"false\" UseHighResolutionImage=\"true\" Threshold=\"25\" Tolerance=\"2\" InsetFrame=\"0\" AppliedPathName=\"$ID/\" Index=\"-1\" />",
        "<PlainTextQRCode Self=\"{code}\" PlainText=\"{text}\"><Properties><QrCodeSwatch type=\"string\">Black</QrCodeSwatch></Properties></PlainTextQRCode></EPS>"),
        graphic = graphic, scale = scale, tx = tx, ty = ty, data = data, bound = bound, link = link, size = size, hi = size >> 32, lo = size & 0xffff_ffff,
        code = code, text = esc(payload));
    let at = ix.nodes[i].close_start;
    w.splice(&part, vec![attr_edit(&ix, &t, i, "ContentType", "GraphicType"), (at..at, xml)])
}

/// The bytes as base64 lines of 76 characters, as InDesign writes embedded data.
fn base64_lines(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + data.len() / 57 + 4);
    let mut col = 0;
    for g in data.chunks(3) {
        if col == 76 { out.push('\n'); col = 0; }
        let n = (g[0] as u32) << 16 | (*g.get(1).unwrap_or(&0) as u32) << 8 | *g.get(2).unwrap_or(&0) as u32;
        for k in 0..4 { out.push(if k <= g.len() { A[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' }); }
        col += 4;
    }
    out
}
