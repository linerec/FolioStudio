//! Equations for the Insert MathML dialog and the Math Expressions panel (rules/mathml.ko.md).
use serde_json::{json, Value};

/// `mathmlLayout` `{mathml, size, display?}`: the drawing of a MathML source at once, for the live
/// preview. `notice` is `noMathFont` when the installed fonts have no OpenType MATH table (a text
/// font stands in, the shapes are approximate).
pub fn layout(req: &Value) -> Result<Value, String> {
    let mathml = req["mathml"].as_str().ok_or("mathml required")?;
    let size = req["size"].as_f64().unwrap_or(10.0);
    let display = req["display"].as_bool().unwrap_or(false);
    match folio_math::layout(mathml, &folio_math::Options { size, display, color: [0.0; 3] }) {
        Ok(eq) => Ok(json!({
            "ok": true,
            "width": eq.width,
            "height": eq.height,
            "depth": eq.depth,
            "svg": eq.preview_svg,
            "font": eq.font,
            "approximate": eq.approximate,
            "notice": if eq.approximate { json!("noMathFont") } else { Value::Null },
            "notes": eq.notes,
        })),
        Err(e) => {
            let kind = if e.0.contains("no installed font") { "noFont" } else { "invalid" };
            Ok(json!({"ok": false, "error": e.to_string(), "errorKind": kind, "notice": if kind == "noFont" { json!("noFont") } else { Value::Null }}))
        }
    }
}

/// `mathExpressions`: the equations of the document, in document order. `page` is the id of the page
/// that holds the frame's middle (the spread's first page when it lies off the pages).
pub fn expressions(session: Option<&crate::Session>) -> Result<Value, String> {
    let doc = &session.ok_or("no document open")?.doc;
    let mut items = Vec::new();
    for spread in &doc.spreads {
        for top in &spread.items {
            top.walk(&mut |item| {
                let Some(math) = &item.math else { return };
                let Some(b) = folio_model::paths_bounds(&item.paths) else { return };
                let middle = folio_model::Point { x: (b.left + b.right) / 2.0, y: (b.top + b.bottom) / 2.0 };
                // nested items (in a group) are placed by their top-level item's transform: good enough to pick the page
                let at = top.transform.apply(if std::ptr::eq(top, item) { middle } else { folio_model::Point { x: 0.0, y: 0.0 } });
                let page = spread
                    .pages
                    .iter()
                    .find(|p| {
                        p.transform.invert().is_some_and(|inv| {
                            let q = inv.apply(at);
                            q.x >= p.bounds.left && q.x <= p.bounds.right && q.y >= p.bounds.top && q.y <= p.bounds.bottom
                        })
                    })
                    .or(spread.pages.first())
                    .map(|p| p.id.clone());
                items.push(json!({
                    "id": item.id,
                    "page": page,
                    "mathml": math.source,
                    "size": math.font_size.unwrap_or(10.0),
                    "color": math.rgb.unwrap_or([0.0; 3]),
                    "width": b.right - b.left,
                    "height": b.bottom - b.top,
                    "drawn": math.svg_data.is_some(),
                }));
            });
        }
    }
    Ok(json!({ "items": items }))
}
