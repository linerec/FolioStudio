//! A picture of a Word file as the inline frame InDesign makes of it (oracle `docxtext-c3-pic*`; measurements in
//! `artifacts/docxtext-c3-20261023/measurement/g1.json`).
//!
//! - The frame is a rectangle `wp:extent` big, sitting on the baseline of its line (`ItemTransform` 0 -height); a picture on its own in the
//!   paragraph is the same.
//! - The picture is stretched to the frame, each way by its own factor (a crop of the Word file does not count; neither does a turn the
//!   picture file asks for by its EXIF orientation).
//! - The document holds the picture's bytes itself (`Contents`, the link is `Embedded`, named `Image<number>.<EXT>` after the
//!   graphic's id), so no file name or path of the Word file's computer goes into the document.
//! - The description of the picture becomes the frame's alternate text (`SourceCustom`).
use super::*;
use folio_textimport::{Picture, decode_base64};

/// What Folio can read of a picture file: it goes through the same reader as a placed file (through a short-lived file, which the reader
/// takes by path).
fn read_source(bytes: &[u8], format: &str, uri: String) -> Result<GraphicSource, EditError> {
    let extension = match format {
        "PNG" => "png",
        "JPEG" => "jpg",
        "GIF" => "gif",
        "TIFF" => "tif",
        other => return Err(EditError::new("unsupported", format!("{other} 그림은 아직 지원하지 않습니다"))),
    };
    // a name no other picture of this process shares (the temporary folder is shared with other programs)
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("folio-docx-picture-{}-{n}.{extension}", std::process::id()));
    std::fs::write(&path, bytes).map_err(|e| EditError::new("io", format!("그림을 읽을 수 없습니다: {e}")))?;
    let source = GraphicSource::read(&path, uri);
    let _ = std::fs::remove_file(&path);
    source.map_err(|e| EditError::new("unsupported", e))
}

/// The `<Rectangle>` of the picture `pic`, to stand in a character range of the story. Its ids come from `w`.
pub(crate) fn element(w: &mut Work<'_>, pic: &Picture) -> Result<String, EditError> {
    let bytes = decode_base64(&pic.data).ok_or_else(|| EditError::new("invalid", "그림 내용을 읽을 수 없습니다"))?;
    if !(pic.width.is_finite() && pic.height.is_finite() && pic.width > 0.0 && pic.height > 0.0) {
        return Err(EditError::new("invalid", "그림의 크기가 올바르지 않습니다"));
    }
    let (frame, graphic, link) = (w.new_id(), w.new_id(), w.new_id());
    // InDesign names the embedded link after the graphic's id in decimal
    let number = u64::from_str_radix(graphic.trim_start_matches(|c: char| !c.is_ascii_hexdigit()), 16).unwrap_or(0);
    let mut source = read_source(&bytes, &pic.format, format!("file:Image{number}.{}", match pic.format.as_str() { "JPEG" => "JPG".to_owned(), "TIFF" => "TIF".to_owned(), f => f.to_owned() }))?;
    check_profile_policy(w, &source)?;
    // the picture's own tags (EXIF ...) make no `MetadataPacketPreference` here (measured: docxtext-c3-pic18)
    source.xmp = None;
    // the EXIF orientation is not applied (measured: docxtext-c3-pic18): the stored picture is stretched to the frame
    let [sw, sh] = source.size;
    let m = Mat([pic.width / sw, 0.0, 0.0, pic.height / sh, 0.0, 0.0]);
    let image = image_element(&source, &graphic, &link, m, Some(&pic.data));
    let (width, height) = (geom::num(pic.width), geom::num(pic.height));
    let export = match &pic.alt {
        Some(alt) => super::super::create::EXPORT
            .replacen("AltTextSourceType=\"SourceXMLStructure\"", "AltTextSourceType=\"SourceCustom\"", 1)
            .replacen("CustomAltText=\"$ID/\"", &format!("CustomAltText=\"{}\"", escape(alt)), 1),
        None => super::super::create::EXPORT.to_owned(),
    };
    Ok(format!(
        "<Rectangle Self=\"{frame}\" ContentType=\"GraphicType\" StoryTitle=\"$ID/\" ParentInterfaceChangeCount=\"\" TargetInterfaceChangeCount=\"\" LastUpdatedInterfaceChangeCount=\"\" OverriddenPageItemProps=\"\" \
HorizontalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" VerticalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" \
GradientFillStart=\"0 0\" GradientFillLength=\"0\" GradientFillAngle=\"0\" GradientStrokeStart=\"0 0\" GradientStrokeLength=\"0\" GradientStrokeAngle=\"0\" Locked=\"false\" LocalDisplaySetting=\"Default\" \
GradientFillHiliteLength=\"0\" GradientFillHiliteAngle=\"0\" GradientStrokeHiliteLength=\"0\" GradientStrokeHiliteAngle=\"0\" AppliedObjectStyle=\"ObjectStyle/$ID/[None]\" Visible=\"true\" Name=\"$ID/\" \
ItemTransform=\"1 0 0 1 0 -{height}\"><Properties><PathGeometry><GeometryPathType PathOpen=\"false\"><PathPointArray>\
<PathPointType Anchor=\"0 0\" LeftDirection=\"0 0\" RightDirection=\"0 0\" />\
<PathPointType Anchor=\"{width} 0\" LeftDirection=\"{width} 0\" RightDirection=\"{width} 0\" />\
<PathPointType Anchor=\"{width} {height}\" LeftDirection=\"{width} {height}\" RightDirection=\"{width} {height}\" />\
<PathPointType Anchor=\"0 {height}\" LeftDirection=\"0 {height}\" RightDirection=\"0 {height}\" />\
</PathPointArray></GeometryPathType></PathGeometry></Properties>{WRAP}{incopy}{export}{image}</Rectangle>",
        incopy = super::super::create::INCOPY_SPACED,
    ))
}

/// Leaves out the pictures of a text source that Folio cannot put in a document (a format or colour mode it has not been measured for) and
/// names them in the source's notes. Without this the whole placement would be refused for one picture. The pictures are checked as
/// the placement will read them; a document whose colour settings are not the default can still refuse one at placement.
pub fn prune_unreadable(source: &mut folio_textimport::TextSource) {
    use folio_textimport::Inline;
    fn walk(paragraphs: &mut [folio_textimport::Paragraph], notes: &mut Vec<String>) {
        for paragraph in paragraphs {
            paragraph.runs.retain_mut(|run| {
                match &mut run.inline {
                    Some(Inline::Picture(pic)) => {
                        let ok = decode_base64(&pic.data).ok_or_else(|| "그림 내용을 읽을 수 없습니다".to_owned()).and_then(|bytes| read_source(&bytes, &pic.format, String::new()).map(|_| ()).map_err(|e| e.message));
                        match ok {
                            Ok(()) => true,
                            Err(why) => {
                                notes.push(format!("그림 한 개를 가져오지 않았습니다: {why}"));
                                false
                            }
                        }
                    }
                    Some(Inline::Table(table)) => {
                        for cell in &mut table.cells {
                            walk(&mut cell.paragraphs, notes);
                        }
                        true
                    }
                    _ => true,
                }
            });
        }
    }
    let mut notes = Vec::new();
    walk(&mut source.paragraphs, &mut notes);
    for note in notes {
        if !source.warnings.contains(&note) {
            source.warnings.push(note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_textimport::{Inline, Paragraph, Run, TextSource};

    fn source_with(pictures: Vec<Picture>) -> TextSource {
        let runs = pictures.into_iter().map(|p| Run { text: "\u{fffc}".into(), inline: Some(Inline::Picture(p)), ..Run::default() }).collect();
        TextSource {
            format: "docx".into(),
            paragraphs: vec![Paragraph { runs, ..Paragraph::default() }],
            paragraph_styles: vec![],
            character_styles: vec![],
            colors: vec![],
            formatted: true,
            ends_with_return: false,
            numbering_lists: vec![],
            warnings: vec![],
            warning_messages: vec![],
        }
    }

    #[test]
    fn a_picture_that_cannot_be_read_is_left_out_with_a_note() {
        let bad = Picture { data: "not*base64".into(), format: "PNG".into(), width: 10.0, height: 10.0, alt: None };
        // a file the reader refuses: a PNG header with nothing behind it
        let broken = Picture { data: folio_textimport::decode_base64("iVBORw0KGgo=").map(|_| "iVBORw0KGgo=".to_owned()).unwrap(), format: "PNG".into(), width: 10.0, height: 10.0, alt: None };
        let mut source = source_with(vec![bad, broken]);
        prune_unreadable(&mut source);
        assert!(source.paragraphs[0].runs.is_empty());
        // each reason is named once
        assert_eq!(source.warnings.len(), 2, "{:?}", source.warnings);
        assert!(source.warnings.iter().all(|w| w.contains("그림 한 개를 가져오지 않았습니다")));
    }
}
