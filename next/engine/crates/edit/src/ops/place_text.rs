//! Place ▸ a text file (.txt, .docx, .rtf) into a text frame, an empty shape, or a new frame: drawn at `rect`, or made by a
//! click at `point` (oracle placetext-c1, placetext-c2, screens-import). The file is read outside the editor (`folio-textimport`):
//! the command carries what it holds, nothing of the file stays in the document (no link).
//!
//! - A text frame that is part of a thread is refused (InDesign says "Import failed": measured with both frames of a thread).
//! - An empty Rectangle, Oval or Polygon becomes a text frame with a new story, the way Object ▸ Content ▸ Text makes it, and the
//!   text goes in (oracle `placetext-into-rect`, `-oval`: the shape keeps its path, style and fill). A shape that holds a picture is refused.
//! - A click (`page` + `point`) makes the frame InDesign makes for a click with loaded text: the width of the column under the click,
//!   from the click down to the bottom margin (the same frame a click with a loaded thread makes; `ui/threading.ts`). The click itself
//!   was not measured in InDesign (a script has no click), so this is `approximate`: the left edge is the column's, not the click's.
use super::*;
use folio_textimport::TextSource;

pub(super) fn run(w: &mut Work<'_>, source: &TextSource, id: Option<&str>, page: Option<&str>, point: Option<[f64; 2]>, rect: Option<[f64; 4]>, layer: Option<&str>) -> Result<(), EditError> {
    if source.paragraphs.is_empty() {
        return Err(EditError::new("invalid", "가져온 글이 비어 있습니다"));
    }
    let new_frame = page.is_some() || rect.is_some() || point.is_some();
    if id.is_some() == new_frame || page.is_some() != (rect.is_some() || point.is_some()) || (rect.is_some() && point.is_some()) {
        return Err(EditError::new("invalid", "글 가져와 놓기에는 글 상자(id) 하나, 또는 쪽(page)과 사각형(rect) 또는 클릭한 곳(point)이 있어야 합니다"));
    }
    let frame = if let Some(id) = id {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        match ix.local(&t, i) {
            "TextFrame" => {
                // InDesign refuses a text frame that is part of a thread ("Import failed", measured with both frames of a thread)
                if ["PreviousTextFrame", "NextTextFrame"].iter().any(|a| ix.attr(&t, i, a).is_some_and(|v| v != "n")) {
                    return Err(EditError::new("unsupported", "연결된 글 상자에는 글 파일을 놓을 수 없습니다 (InDesign도 같습니다). 연결되지 않은 글 상자나 빈 도형에 놓거나, 빈 곳을 눌러 새 글 상자를 만드세요"));
                }
            }
            "Rectangle" | "Oval" | "Polygon" => {
                if content_of(&ix, &t, i).is_some() {
                    return Err(EditError::new("unsupported", "그림이 든 도형에는 글 파일을 놓을 수 없습니다. 빈 도형이나 글 상자에 놓으세요"));
                }
                create::to_text_frame(w, id)?;
            }
            _ => return Err(EditError::new("unsupported", "이 개체에는 글 파일을 놓을 수 없습니다: 글 상자나 빈 도형(사각형·타원·다각형)에 놓으세요")),
        }
        id.to_owned()
    } else {
        let rect = match (rect, point) {
            (Some(rect), _) => rect,
            (None, Some(point)) => click_frame(w, page.unwrap(), point)?,
            _ => unreachable!(),
        };
        create::run(w, page.unwrap(), ItemKind::TextFrame, rect, layer, 6, None)?;
        w.created.last().unwrap().clone()
    };
    place_styles::bring_in(w, source)?;
    crate::text::place_source(w, &frame, source)
}

/// The frame of a click with loaded text at `point` (page coordinates) on `page`: the column under the click (the nearest one when the
/// click is in a margin or a gutter), from the click down to the bottom margin, at least 12 pt high.
pub(crate) fn click_frame(w: &mut Work<'_>, page: &str, [x, y]: [f64; 2]) -> Result<[f64; 4], EditError> {
    if !x.is_finite() || !y.is_finite() {
        return Err(EditError::new("invalid", "클릭한 곳이 올바르지 않습니다"));
    }
    let (part, ix, pi) = w.find(page)?;
    let t = w.text(&part)?.to_owned();
    if ix.local(&t, pi) != "Page" {
        return Err(EditError::new("invalid", "클릭한 곳은 쪽이어야 합니다"));
    }
    let bounds: Vec<f64> = ix.attr(&t, pi, "GeometricBounds").unwrap_or_default().split_whitespace().filter_map(|v| v.parse().ok()).collect();
    let [top, left, bottom, right] = bounds[..] else { return Err(EditError::new("xml", "쪽의 크기를 읽을 수 없습니다")) };
    let (width, height) = (right - left, bottom - top);
    let facing = w
        .text("Resources/Preferences.xml")
        .ok()
        .and_then(|prefs| {
            let px = Index::scan(prefs).ok()?;
            (0..px.nodes.len()).find(|&i| px.local(prefs, i) == "DocumentPreference").map(|i| px.attr(prefs, i, "FacingPages") == Some("true"))
        })
        .unwrap_or(false);
    // Left and Right of the margins are inside and outside on facing pages: the left page of a spread has them the other way round
    let left_page = facing
        && ix.nodes[pi].parent.is_some_and(|s| {
            let binding = ix.attr(&t, s, "BindingLocation").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            ix.named(&t, s, "Page").iter().position(|&p| p == pi).is_some_and(|n| n < binding)
        });
    let preference = ix.named(&t, pi, "MarginPreference").first().copied();
    let get = |key: &str| preference.and_then(|m| ix.attr(&t, m, key)).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let (margin_left, margin_right) = if left_page { (get("Right"), get("Left")) } else { (get("Left"), get("Right")) };
    let margin_bottom = get("Bottom");
    // ColumnsPositions: the left and right edge of each column, measured from the left margin
    let positions: Vec<f64> = preference.and_then(|m| ix.attr(&t, m, "ColumnsPositions")).map(|v| v.split_whitespace().filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
    let (mut x0, mut x1) = (margin_left, width - margin_right);
    if positions.len() >= 2 {
        let mut best = (f64::INFINITY, (x0, x1));
        for pair in positions.chunks_exact(2) {
            let (a, b) = (margin_left + pair[0], margin_left + pair[1]);
            let distance = if x < a { a - x } else if x > b { x - b } else { 0.0 };
            if distance < best.0 {
                best = (distance, (a, b));
            }
        }
        (x0, x1) = best.1;
    }
    let y = y.clamp(0.0, height);
    Ok([x0, y, x1, (height - margin_bottom).max(y + 12.0)])
}
