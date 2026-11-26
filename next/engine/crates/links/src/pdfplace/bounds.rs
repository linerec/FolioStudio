//! Geometric content extent in PDF user space, before its page-to-frame transformation.
use hayro::{hayro_interpret::*, hayro_syntax::Pdf, kurbo::{Affine, BezPath, Rect, Shape, Stroke, StrokeOpts}};
use std::sync::atomic::Ordering;

struct Extent { bounds: Option<Rect>, clips: Vec<Rect> }

impl Extent {
    fn add(&mut self, mut rect: Rect) {
        if let Some(clip) = self.clips.last() { rect = rect.intersect(*clip); }
        if rect.width() > 0. && rect.height() > 0. {
            self.bounds = Some(self.bounds.map_or(rect, |b| b.union(rect)));
        }
    }
}

impl<'a> Device<'a> for Extent {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        if matches!(mode, DrawMode::Invisible) { return; }
        let stroked;
        let path = match mode {
            DrawMode::Stroke(s) | DrawMode::FillAndStroke(_, s) => {
                let style = Stroke::new(s.line_width as f64).with_join(s.line_join).with_caps(s.line_cap).with_miter_limit(s.miter_limit as f64);
                stroked = hayro::kurbo::stroke(path.iter(), &style, &StrokeOpts::default(), 0.001);
                &stroked
            }
            _ => path,
        };
        self.add((props.transform * path).bounding_box());
    }
    fn push_clip_path(&mut self, clip: &ClipPath) {
        let b = clip.path.bounding_box();
        self.clips.push(self.clips.last().map_or(b, |p| p.intersect(b)));
    }
    fn pop_clip(&mut self) { self.clips.pop(); }
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn pop_transparency_group(&mut self) {}
    fn draw_image(&mut self, _: Image<'a, '_>, props: ImageDrawProps<'a>) {
        self.add(props.transform.transform_rect_bbox(Rect::new(0., 0., 1., 1.)));
    }
    fn draw_glyph_run(&mut self, run: &font::GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        for glyph in run.glyphs() {
            match &**glyph {
                font::Glyph::Outline(g) => {
                    let mut p = props.clone(); p.transform *= glyph.transform();
                    self.draw_path(&g.outline(), p, mode);
                }
                font::Glyph::Type3(g) => g.interpret(self, props.transform, glyph.transform(), &props.paint),
            }
        }
    }
}

pub(super) fn measure(data: &[u8], number: u32, crop: [f64; 4]) -> Result<[f64; 4], String> {
    let pdf = Pdf::new(data.to_vec()).map_err(|e| format!("PDF content bounds: {e:?}"))?;
    let page = pdf.pages().get(number as usize - 1).ok_or("PDF page is missing")?;
    let clip = Rect::new(crop[0], crop[1], crop[2], crop[3]);
    let (settings, failed) = super::settings();
    let cache = InterpreterCache::new();
    let mut context = Context::new(Affine::IDENTITY, clip, &cache, page.xref(), settings);
    let mut extent = Extent { bounds: None, clips: vec![clip] };
    interpret_page(page, &mut context, &mut extent);
    if failed.load(Ordering::Relaxed) { return Err("PDF content bounds need supported embedded fonts and images".into()); }
    let b = extent.bounds.ok_or("PDF page has no visible content; choose a page box")?;
    Ok([b.x0, b.y0, b.x1, b.y1])
}
