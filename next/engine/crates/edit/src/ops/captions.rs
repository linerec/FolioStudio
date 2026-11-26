//! Object ▸ Captions (oracle captions; InDesign 2025 measured with probe-a/b/c in `artifacts/captions-20261023/`):
//! - Caption Setup is a state of the document (a new document starts with the defaults, the next dialog shows what was set); it is
//!   not in the IDML. Folio keeps it in a document label `folio.captionSetup` (JSON) of the designmap: rows of Text Before / Metadata /
//!   Text After, Alignment (below, above, left, right of the image), Offset, Paragraph Style, Layer, Group Caption with Image;
//! - Generate Live Caption makes a text frame on the graphic frame's side with the story: one paragraph per row, the text before and
//!   after as plain text and the metadata as an instance of the text variable `Live Metadata Caption: <Metadata>` (a `LiveCaptionType`
//!   variable made on the first use, options only `MetadataProviderName`; the text before and after stay in the story);
//! - Generate Static Caption is the same frame with the text the metadata shows, no variable;
//! - below / above: the frame is as wide as the graphic frame and touches it, the offset is the inset on the side facing the graphic
//!   (top below, bottom above); left / right: the same frame turned a quarter (left -90°, right +90°) along the graphic's side, its
//!   length the graphic's height. The thickness is the offset plus 8.7239 pt (measured at 12 pt Minion: the first line's ascent);
//!   Folio does not compose the lines, so a caption of more lines is thinner here than in InDesign (STATUS);
//! - Group Caption with Image groups the graphic frame and the caption (in that order);
//! - Convert to Static Caption (a text range that holds live caption variables selected; a caret or the frame alone is disabled)
//!   turns the live caption instances of the range into the text they show.
use super::*;
use serde_json::Value;

const DM: &str = "designmap.xml";
const KEY: &str = "folio.captionSetup";
/// The first line's ascent of 12 pt Minion (measured: a one line caption below with offset 1 is 9.7239 pt high).
const LINE: f64 = 8.723876953125;

use folio_model::captions::CaptionSetup as Setup;

fn setup_from_json(v: &Value) -> Result<Setup, EditError> {
    let s: Setup = serde_json::from_value(v.clone()).map_err(|e| EditError::new("invalid", e.to_string()))?;
    validate_setup(&s)?;
    Ok(s)
}
fn validate_setup(s: &Setup) -> Result<(), EditError> {
    if s.rows.is_empty() { return Err(EditError::new("invalid", "a caption needs at least one metadata row")); }
    if !matches!(s.position.as_str(), "below"|"above"|"left"|"right") { return Err(EditError::new("invalid", "unknown caption position")); }
    if !s.offset.is_finite() || !(0.0..=10000.0).contains(&s.offset) { return Err(EditError::new("invalid", "caption offset out of range")); }
    Ok(())
}
fn setup_json(s: &Setup) -> Value { serde_json::to_value(s).expect("caption setup serializes") }

/// The metadata names the dialog offers that were measured, with the provider InDesign stores for them.
fn provider(name: &str) -> String {
    match name {
        "Name" => "$ID/#LinkInfoNameStr".into(),
        "Description" => "$ID/#LinkInfoXMPDescriptionStr".into(),
        "Page" => "$ID/#LinkInfoPageNumberStr".into(),
        other => format!("$ID/#LinkInfo{}Str", other.split_whitespace().collect::<String>()),
    }
}

fn read_setup(w: &Work<'_>) -> Result<Setup, EditError> {
    parse_setup(w.text(DM)?)
}

fn parse_setup(t: &str) -> Result<Setup, EditError> {
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    for i in 0..ix.nodes.len() {
        if ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some(KEY) {
            let json = ix.value(t, i, "Value").unwrap_or_default();
            let v: Value = serde_json::from_str(&json).map_err(|e| EditError::new("invalid", format!("caption setup label: {e}")))?;
            return setup_from_json(&v);
        }
    }
    Ok(Setup::default())
}

impl crate::Editor {
    pub fn set_caption_link_uris(&mut self, uris: std::collections::BTreeMap<String, String>) {
        self.link_uris = uris;
    }

    pub fn caption_setup(&self) -> Result<Value, EditError> {
        let t = self.part(DM).ok_or_else(|| EditError::new("missing", "designmap.xml"))?;
        Ok(setup_json(&parse_setup(t)?))
    }

    pub fn caption_range(&mut self, story: &str, start: usize, end: usize) -> Result<bool, EditError> {
        if start >= end { return Ok(false); }
        let t = self.part(DM).ok_or_else(|| EditError::new("missing", "designmap.xml"))?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        let live: Vec<_> = (0..ix.nodes.len()).filter(|&i| ix.local(t,i)=="TextVariable" && ix.attr(t,i,"VariableType")==Some("LiveCaptionType")).filter_map(|i| ix.value(t,i,"Self")).collect();
        self.range_has_variable(story, start, end, &live)
    }
}

fn write_setup(w: &mut Work<'_>, setup: &Setup) -> Result<(), EditError> {
    let t = w.text(DM)?.to_owned();
    let ix = Index::scan(&t).map_err(|e| EditError::new("xml", e))?;
    let pair = format!("<KeyValuePair Key=\"{KEY}\" Value=\"{}\" />", escape(&setup_json(setup).to_string()));
    let doc = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "Document").ok_or_else(|| EditError::new("xml", "no Document element"))?;
    if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(&t, i) == "KeyValuePair" && ix.attr(&t, i, "Key") == Some(KEY)) {
        return w.splice(DM, vec![(ix.nodes[i].start..ix.nodes[i].end, pair)]);
    }
    let label = ix.named(&t, doc, "Properties").first().and_then(|&p| ix.named(&t, p, "Label").first().copied());
    if let Some(label) = label {
        let end = ix.nodes[label].end;
        let close = t[..end].rfind("</").ok_or_else(|| EditError::new("xml", "label without end"))?;
        return w.splice(DM, vec![(close..close, format!("\t{pair}\n"))]);
    }
    let open_end = t[ix.nodes[doc].start..].find('>').map(|k| ix.nodes[doc].start + k + 1).ok_or_else(|| EditError::new("xml", "Document start tag"))?;
    w.splice(DM, vec![(open_end..open_end, format!("\n\t<Properties><Label>{pair}</Label></Properties>"))])
}

pub(crate) fn set_setup(w: &mut Work<'_>, setup: &Setup) -> Result<(), EditError> {
    validate_setup(setup)?;
    write_setup(w, setup)
}

/// The file name of a link's resource (decoded), the Image Name metadata.
pub(crate) fn link_name(uri: &str) -> String {
    let tail = uri.rsplit('/').next().unwrap_or(uri);
    let bytes = tail.as_bytes();
    let (mut out, mut k) = (Vec::new(), 0);
    while k < bytes.len() {
        if bytes[k] == b'%' && k + 2 < bytes.len() && let Ok(b) = u8::from_str_radix(&tail[k + 1..k + 3], 16) {
            out.push(b);
            k += 3;
        } else {
            out.push(bytes[k]);
            k += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a metadata shows for a graphic (static captions; InDesign's placeholder when the link has no data of that kind). The XMP
/// properties (Description, Title, Author, Copyright Notice, Keywords, Headline, Credit …) come from the graphic's packet
/// (`MetadataPacketPreference`), or the linked file's when the document keeps none (build-a).
fn metadata_value(name: &str, uri: &str, packet: Option<&str>) -> String {
    const NONE: &str = "<No data from link>";
    match name {
        "Name" => link_name(uri),
        "Page" => "1".into(),
        _ => {
            let file_packet = || folio_links::parse_uri(uri).and_then(|l| l.local_path()).and_then(|p| folio_links::probe::xmp::read_packet(&p));
            let packet = packet.map(str::to_owned).or_else(file_packet);
            packet
                .and_then(|p| folio_links::probe::xmp::XmpFields::parse(&p).caption_value(name))
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| NONE.into())
        }
    }
}

/// The XMP packet kept with the graphic of `frame` (`MetadataPacketPreference` ▸ `Contents`).
fn graphic_packet(w: &mut Work<'_>, frame: &str) -> Option<String> {
    let (part, ix, i) = w.find(frame).ok()?;
    let t = w.text(&part).ok()?;
    let pref = (0..ix.nodes.len()).find(|&k| ix.nodes[k].start > ix.nodes[i].start && ix.nodes[k].end <= ix.nodes[i].end && ix.local(t, k) == "MetadataPacketPreference")?;
    let contents = ix.own_descendants(t, pref, "Contents").first().copied()?;
    let body = &t[ix.nodes[contents].start..ix.nodes[contents].end];
    let inner = body.split_once("<![CDATA[").and_then(|(_, r)| r.rsplit_once("]]>")).map(|(c, _)| c.to_owned());
    inner.or_else(|| body.split_once('>').and_then(|(_, r)| r.rsplit_once("</")).map(|(c, _)| crate::xml::unescape(c)))
}

fn page_origin(w: &mut Work<'_>, page: &str) -> Result<(f64, f64), EditError> {
    let (part, ix, pi) = w.find(page)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, pi) != "Page" {
        return Err(EditError::new("invalid", "destination is not a page"));
    }
    let b: Vec<f64> = ix.attr(&t, pi, "GeometricBounds").unwrap_or("0 0 0 0").split_whitespace().filter_map(|v| v.parse().ok()).collect();
    if b.len() != 4 {
        return Err(EditError::new("xml", "invalid page bounds"));
    }
    Ok(item_transform(&ix, &t, pi).apply((b[1], b[0])))
}

/// The graphic frame of `id` (an image id gives its frame): the frame id, its layer and the link's resource.
fn graphic_frame(w: &mut Work<'_>, id: &str) -> Result<(String, Option<String>, String), EditError> {
    let (part, ix, mut i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    if matches!(ix.local(&t, i), "Image" | "PDF" | "EPS" | "WMF" | "PICT" | "ImportedPage") {
        i = ix.nodes[i].parent.ok_or_else(|| EditError::new("xml", "graphic without frame"))?;
    }
    let all = |from: usize| -> Vec<usize> { (0..ix.nodes.len()).filter(|&k| ix.nodes[k].start > ix.nodes[from].start && ix.nodes[k].end <= ix.nodes[from].end).collect() };
    let link = all(i).into_iter().find(|&k| ix.local(&t, k) == "Link").ok_or_else(|| EditError::new("invalid", format!("{id} has no placed graphic")))?;
    let uri = ix.attr(&t, link, "Self").and_then(|id| w.ed.link_uris.get(id)).cloned()
        .unwrap_or_else(|| ix.value(&t, link, "LinkResourceURI").unwrap_or_default());
    let frame = ix.attr(&t, i, "Self").ok_or_else(|| EditError::new("xml", "frame without Self"))?.to_owned();
    Ok((frame, ix.attr(&t, i, "ItemLayer").map(str::to_owned), uri))
}

fn variable_exists(w: &Work<'_>, name: &str) -> Result<bool, EditError> {
    let t = w.text(DM)?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    Ok((0..ix.nodes.len()).any(|i| ix.local(t, i) == "TextVariable" && ix.attr(t, i, "Name") == Some(name)))
}

/// Object ▸ Captions ▸ Generate Live / Static Caption(s) for the graphic frames `ids`, drawn on `page`.
pub(crate) fn generate(w: &mut Work<'_>, ids: &Vec<String>, page: &str, live: &bool) -> Result<(), EditError> {
    let live = *live;
    if ids.is_empty() {
        return Err(EditError::new("invalid", "select a placed graphic"));
    }
    let setup = read_setup(w)?;
    let (ox, oy) = page_origin(w, page)?;
    for id in ids {
        let (frame, image_layer, uri) = graphic_frame(w, id)?;
        let packet = graphic_packet(w, &frame);
        let [x0, y0, x1, y1] = w.bounds_of(&frame)?;
        let ext = setup.offset + LINE;
        let (len, vertical) = if matches!(setup.position.as_str(), "left" | "right") { (y1 - y0, true) } else { (x1 - x0, false) };
        // the frame turned back upright, centred where the finished caption lies
        let (cx, cy) = match setup.position.as_str() {
            "below" => ((x0 + x1) / 2.0, y1 + ext / 2.0),
            "above" => ((x0 + x1) / 2.0, y0 - ext / 2.0),
            "left" => (x0 - ext / 2.0, (y0 + y1) / 2.0),
            _ => (x1 + ext / 2.0, (y0 + y1) / 2.0),
        };
        let rect = [cx - len / 2.0 - ox, cy - ext / 2.0 - oy, cx + len / 2.0 - ox, cy + ext / 2.0 - oy];
        let layer = if setup.layer.is_empty() || setup.group { image_layer.clone() } else { Some(setup.layer.clone()) };
        run(w, &Command::CreateItem { page: page.to_owned(), kind: create::ItemKind::TextFrame, rect, layer, sides: 6 })?;
        let caption = w.created.last().cloned().ok_or_else(|| EditError::new("invalid", "the caption frame was not made"))?;
        let insets = match setup.position.as_str() { "above" => [0.0, 0.0, setup.offset, 0.0], _ => [setup.offset, 0.0, 0.0, 0.0] };
        run(w, &Command::TextFrameOptions { id: caption.clone(), options: textframe::FrameOptions { insets: Some(insets), ..Default::default() } })?;
        if vertical {
            run(w, &Command::Rotate { ids: vec![caption.clone()], by: if setup.position == "left" { -90.0 } else { 90.0 }, anchor: Anchor::Center })?;
        }
        let story = {
            let (part, ix, i) = w.find(&caption)?;
            ix.value(w.text(&part)?, i, "ParentStory").ok_or_else(|| EditError::new("xml", "caption frame without story"))?
        };
        let mut at = 0usize;
        for (k, row) in setup.rows.iter().enumerate() {
            if k > 0 {
                crate::text::insert(w, &story, at, "\r")?;
                at += 1;
            }
            if !row.before.is_empty() {
                crate::text::insert(w, &story, at, &row.before)?;
                at += row.before.chars().count();
            }
            if live {
                let name = format!("Live Metadata Caption: {}", row.metadata);
                if !variable_exists(w, &name)? {
                    let mut options = std::collections::BTreeMap::new();
                    options.insert("MetadataProviderName".to_owned(), provider(&row.metadata));
                    textvariables::define(w, &name, "LiveCaptionType", &options)?;
                }
                // the instance shows the metadata of the graphic (the Name is worked out again by the layout)
                let shown = (row.metadata != "Name").then(|| metadata_value(&row.metadata, &uri, packet.as_deref()));
                crate::text::insert_variable_showing(w, &story, at, &format!("dTextVariablen{}", name.replace(':', "-")), shown.as_deref())?;
                at += 1;
            } else {
                let value = metadata_value(&row.metadata, &uri, packet.as_deref());
                crate::text::insert(w, &story, at, &value)?;
                at += value.chars().count();
            }
            if !row.after.is_empty() {
                crate::text::insert(w, &story, at, &row.after)?;
                at += row.after.chars().count();
            }
        }
        if !setup.paragraph_style.is_empty() {
            run(w, &Command::ParagraphStyle { story: story.clone(), start: 0, end: at, style: setup.paragraph_style.clone(), clear: false })?;
        }
        if setup.group {
            run(w, &Command::Group { ids: vec![frame.clone(), caption.clone()] })?;
        }
    }
    Ok(())
}

/// Object ▸ Captions ▸ Convert to Static Caption: the live caption instances in [start, end) of the story become their text.
pub(crate) fn to_static(w: &mut Work<'_>, story: &str, start: &usize, end: &usize) -> Result<(), EditError> {
    let (start, end) = (*start, *end);
    let live: Vec<String> = {
        let t = w.text(DM)?;
        let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
        (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "TextVariable" && ix.attr(t, i, "VariableType") == Some("LiveCaptionType")).filter_map(|i| ix.value(t, i, "Self")).collect()
    };
    if start >= end {
        return Err(EditError::new("invalid", "select text with a live caption"));
    }
    let n = crate::text::convert_instances(w, story, &|k, inst| (k >= start && k < end && live.contains(&inst.variable)).then(|| inst.result.clone()))?;
    if n == 0 {
        return Err(EditError::new("invalid", "no live caption in the range"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_round_trips_and_validates() {
        let v = serde_json::json!({"rows": [{"before": "Photo: ", "metadata": "Description", "after": " (c)"}], "position": "above", "offset": 5, "group": true});
        let s = setup_from_json(&v).unwrap();
        assert_eq!(s.position, "above");
        assert_eq!(setup_from_json(&setup_json(&s)).unwrap(), s);
        assert!(setup_from_json(&serde_json::json!({"position": "middle"})).is_err());
        assert!(setup_from_json(&serde_json::json!({"rows": []})).is_err());
    }

    #[test]
    fn providers_and_names() {
        assert_eq!(provider("Description"), "$ID/#LinkInfoXMPDescriptionStr");
        assert_eq!(link_name("file:C:/a/b/place%20plain.png"), "place plain.png");
        assert_eq!(metadata_value("Author", "file:x.png", None), "<No data from link>");
        let packet = r#"<x:xmpmeta><rdf:Description dc:x=""><dc:description><rdf:Alt><rdf:li xml:lang="x-default">Rain</rdf:li></rdf:Alt></dc:description><dc:creator><rdf:Seq><rdf:li>Kim</rdf:li></rdf:Seq></dc:creator></rdf:Description></x:xmpmeta>"#;
        assert_eq!(metadata_value("Description", "file:x.png", Some(packet)), "Rain");
        assert_eq!(metadata_value("Author", "file:x.png", Some(packet)), "Kim");
        assert_eq!(metadata_value("Headline", "file:x.png", Some(packet)), "<No data from link>");
    }
}
