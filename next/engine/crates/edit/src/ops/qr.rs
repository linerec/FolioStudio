use super::*;
use create::ItemKind;
use folio_model::{Rect, SubPath, qr};

// R11
pub(crate) fn create(
    w: &mut Work<'_>,
    page: &str,
    kind: &str,
    content: &str,
    rect: &[f64; 4],
) -> Result<(), EditError> {
    let [left, top, right, bottom] = *rect;
    let bounds = Rect {
        left,
        top,
        right,
        bottom,
    };
    if content.is_empty()
        || content.len() > 2000
        || (bounds.width() - bounds.height()).abs() > 0.001
    {
        return Err(EditError::new(
            "invalid",
            "QR content must contain 1–2000 bytes and its bounds must be square",
        ));
    }
    let content = match kind {
        "text" => qr::QrContent::PlainText(content.to_owned()),
        "url" => qr::QrContent::Hyperlink(content.to_owned()),
        _ => return Err(EditError::new("invalid", "unsupported QR content type")),
    };
    let modules = qr::encode(&content.payload())
        .ok_or_else(|| EditError::new("invalid", "QR content is too long"))?;
    if !bounds.width().is_finite() || !bounds.height().is_finite() {
        return Err(EditError::new("invalid", "invalid new item geometry"));
    }

    let created_before = w.created.len();
    create::run(w, page, ItemKind::Rectangle, *rect, None, 4, None)?;
    create::run(w, page, ItemKind::Polygon, *rect, None, 4, None)?;
    let members = w.created.split_off(created_before);
    let [background, foreground] = members.as_slice() else {
        unreachable!("each frame creation returns one item");
    };
    set_paint(
        w,
        background,
        Some("Color/Paper"),
        None,
        Some("Swatch/None"),
        None,
        Some(0.0),
    )?;
    set_paint(
        w,
        foreground,
        Some("Color/Black"),
        None,
        Some("Swatch/None"),
        None,
        Some(0.0),
    )?;
    write_paths(w, foreground, &qr::module_paths(&modules, &bounds))?;
    group(w, &members)
}

fn write_paths(w: &mut Work<'_>, id: &str, paths: &[SubPath]) -> Result<(), EditError> {
    let (part, ix, item) = w.find(id)?;
    let text = w.text(&part)?;
    let geometry = ix
        .path(text, item, &["Properties", "PathGeometry"])
        .ok_or_else(|| EditError::new("xml", "no path geometry"))?;
    let inner = ix.nodes[geometry].open_end..ix.nodes[geometry].close_start;
    // R11
    let local = |p: folio_model::Point| (p.x - 5.0, p.y - 5.0);
    let xml = paths
        .iter()
        .map(|path| {
            let points: Vec<_> = path
                .points
                .iter()
                .map(|p| PathPoint {
                    anchor: local(p.anchor),
                    left: local(p.left),
                    right: local(p.right),
                })
                .collect();
            compound::path_xml(&points, !path.open, "\t\t\t\t")
        })
        .collect::<String>();
    w.splice(&part, vec![(inner, xml)])
}
