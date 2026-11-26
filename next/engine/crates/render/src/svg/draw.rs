use super::Scene;
use crate::{build_path, solid, to_skia};
use folio_color::{BlendSpace, Colors};
use folio_model::{Affine, LineCap, LineJoin};
use tiny_skia::{FillRule, Mask, PathBuilder, Pixmap, Stroke};

impl Scene {
    pub fn draw(&self, pixmap: &mut Pixmap, transform: &Affine, clip: Option<&Mask>, colors: &Colors, blend: BlendSpace) {
        let Some(rect) = tiny_skia::Rect::from_xywh(0., 0., self.width as f32, self.height as f32) else { return };
        let mut mask = clip.cloned().unwrap_or_else(|| {
            let mut m = Mask::new(pixmap.width(), pixmap.height()).expect("pixmap dimensions are valid");
            m.data_mut().fill(255); m
        });
        mask.intersect_path(&PathBuilder::from_rect(rect), FillRule::Winding, true, to_skia(transform));
        let rgb = |v| if blend == BlendSpace::Cmyk { colors.rgb_through_cmyk(v) } else { colors.rgb_to_srgb(v) };
        if let Some(rich) = &self.rich {
            rich.draw(pixmap, transform, &mask, &rgb);
            return;
        }
        for shape in &self.shapes {
            let Some(path) = build_path(&shape.paths) else { continue };
            let t = to_skia(&transform.then_inner(&shape.transform));
            if let Some(fill) = shape.fill {
                pixmap.fill_path(&path, &solid(rgb(fill)), if shape.evenodd { FillRule::EvenOdd } else { FillRule::Winding }, t, Some(&mask));
            }
            if let Some(stroke) = shape.stroke.filter(|_| shape.width > 0.) {
                let style = Stroke {
                    width: shape.width as f32,
                    line_cap: match shape.cap { LineCap::Butt => tiny_skia::LineCap::Butt, LineCap::Round => tiny_skia::LineCap::Round, LineCap::Projecting => tiny_skia::LineCap::Square },
                    line_join: match shape.join { LineJoin::Miter => tiny_skia::LineJoin::Miter, LineJoin::Round => tiny_skia::LineJoin::Round, LineJoin::Bevel => tiny_skia::LineJoin::Bevel },
                    miter_limit: shape.miter as f32,
                    dash: shape.dash.as_ref().and_then(|d| tiny_skia::StrokeDash::new(d.array.iter().map(|v| *v as f32).collect(), d.phase as f32)),
                };
                pixmap.stroke_path(&path, &solid(rgb(stroke)), &style, t, Some(&mask));
            }
        }
    }
}
