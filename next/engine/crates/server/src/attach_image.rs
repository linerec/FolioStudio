use crate::{Profiles, Session};
use folio_model::Rect;
use folio_render::{Raster, Scene, bounded_png, locate_page, page_region, render_page};
use serde_json::{Value, json};

fn stroke(r: &mut Raster, horizontal: bool, at: i64, start: i64, end: i64, color: [u8; 4]) {
    let (along, across) = if horizontal {
        (r.width, r.height)
    } else {
        (r.height, r.width)
    };
    if !(0..i64::from(across)).contains(&at) {
        return;
    }
    for p in start.max(0)..=end.min(i64::from(along) - 1) {
        let (x, y) = if horizontal { (p, at) } else { (at, p) };
        let index = ((y as usize) * r.width as usize + x as usize) * 4;
        r.rgba[index..index + 4].copy_from_slice(&color);
    }
}

pub fn render(req: &Value, s: &Session, profiles: &Profiles) -> Result<Value, String> {
    render_selected(req,s,profiles,None)
}

pub fn render_selected(req: &Value,s:&Session, profiles:&Profiles,ids:Option<&[String]>) -> Result<Value,String> {
    let page = req
        .get("page")
        .map_or(Some(0), Value::as_u64)
        .ok_or("invalid page")? as usize;
    let edge = req
        .get("longEdge")
        .map_or(Some(1200), Value::as_u64)
        .filter(|n| (1..=2000).contains(n))
        .ok_or("longEdge must be 1 to 2000")?;
    let region = req
        .get("region")
        .map(|v| serde_json::from_value::<[f64; 4]>(v.clone()))
        .transpose()
        .map_err(|_| "region needs four coordinates")?
        .map(|[left, top, right, bottom]| Rect {
            left,
            top,
            right,
            bottom,
        });
    let area = page_region(&s.doc, page, region).ok_or("missing page or empty region")?;
    let cap = edge as f64 / area.width().max(area.height());
    let scale = req
        .get("scale")
        .map_or(Some(cap), Value::as_f64)
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or("scale must be positive")?
        .min(cap);
    let flag = |key: &str| {
        req.get(key)
            .map_or(Some(false), Value::as_bool)
            .ok_or("invalid overlay option")
    };
    let guides = flag("guides")?;
    let selection = flag("selection")?;
    let (proof, _) = crate::proof_setup::colors_for(s, &profiles.get());
    let scene = Scene {
        doc: &s.doc,
        colors: proof.as_deref().unwrap_or(&s.colors),
        layout: &s.layout,
        glyphs: &*s.fonts,
        glyph_cache: &s.glyph_cache,
        images: &s.images,
    };
    let mut raster = render_page(&scene, page, scale, Some(area)).ok_or("cannot render page")?;
    let x = |v: f64| ((v - area.left) * scale).round() as i64;
    let y = |v: f64| ((v - area.top) * scale).round() as i64;
    if guides && let Some((spread, _, bbox)) = locate_page(&s.doc, page) {
        for g in &spread.guides {
            if g.fit_to_page
                && g.page
                    .and_then(|i| spread.pages.get(i))
                    .is_some_and(|p| p.index != page)
            {
                continue;
            }
            if g.view_threshold.is_some_and(|min| min > scale * 100.0)
                || g.layer
                    .as_ref()
                    .is_some_and(|id| s.doc.layers.iter().any(|l| &l.id == id && !l.visible))
            {
                continue;
            }
            let at = if g.horizontal {
                y(g.position - bbox.top)
            } else {
                x(g.position - bbox.left)
            };
            stroke(
                &mut raster,
                g.horizontal,
                at,
                0,
                i64::MAX,
                [0, 210, 220, 255],
            );
        }
    }
    if selection {
        let from_view;
        let selected=if let Some(ids)=ids {ids} else {
            from_view=s.view.lock().map_err(|_|"view unavailable")?.selection.iter().map(|s|s.id.clone()).collect::<Vec<_>>();
            &from_view
        };
        for id in selected {
            if let Some(loc) = s.locations().get(id).filter(|loc| loc.page == page) {
                let [l, t, r, b] = loc.bounds;
                for at in [y(t), y(b)] {
                    stroke(&mut raster, true, at, x(l), x(r), [48, 107, 255, 255]);
                }
                for at in [x(l), x(r)] {
                    stroke(&mut raster, false, at, y(t), y(b), [48, 107, 255, 255]);
                }
            }
        }
    }
    let (png, width, height) = bounded_png(raster, 720 * 1024)?;
    Ok(
        json!({"ok":true,"type":"pageImage","page":page,"width":width,"height":height,
        "region":[area.left,area.top,area.right,area.bottom],"selection":selection,"guides":guides,
        "png":folio_project::idmlparts::base64_lines(&png).split_whitespace().collect::<String>()}),
    )
}
