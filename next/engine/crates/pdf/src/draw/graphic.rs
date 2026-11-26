//! Draws placed graphics and their clipping paths.

use super::Drawer;
use crate::colors::Col;
use crate::content::Stream;
use crate::graphics::Placement;
use folio_model::{Affine, Graphic, PageItem, Rect};
use pdf_writer::types::RenderingIntent;

impl<'a, 'd> Drawer<'a, 'd> {
    pub(super) fn draw_graphic(&mut self, s: &mut Stream, frame: &'d PageItem, g: &'d Graphic) {
        let page = self.page;
        // InDesign composites a page that places a PDF, AI or EPS (or its proxy) in its
        // transparency blend space: the page gets a DeviceCMYK transparency group.
        if matches!(g.element.as_str(), "PDF" | "EPS" | "ImportedPage") {
            s.transparency = true;
        }
        s.concat(&g.transform);
        // The image's clipping path (Photoshop path, alpha channel, detected edges), in the
        // graphic's own coordinates: nonzero as InDesign writes it (an inverted path's
        // geometry already holds the image bounds, `GraphicClip::invert`).
        if let Some(c) = &g.clip
            && c.paths.iter().any(|p| p.points.len() > 1)
        {
            s.path(&c.paths);
            s.clip(false);
        }
        // a Windows metafile with its file at hand is drawn as the vectors of its records (build-a); without it, as its preview
        let metafile = g.element == "WMF" && g.preview_from_source && self.reg.graphics.svg_source(g).is_some_and(|p| folio_render::svg::read(&p).is_ok());
        if g.element == "SVG" || metafile {
            let scene = self.reg.graphics.svg_source(g).ok_or_else(|| "SVG source is missing".to_owned()).and_then(|p| folio_render::svg::read(&p).map(|scene| (p, scene)));
            match scene {
                Ok((path, scene)) => {
                    self.reg.graphics.note_svg(g, page, &path);
                    s.concat(&scene.placement(&g.bounds));
                    s.rect(&Rect {left:0.,top:0.,right:scene.width,bottom:scene.height}); s.clip(false);
                    // pixels for what is not plain vectors (svg/rich.rs): 300 ppi at the size it is placed
                    let t = s.ctm();
                    let px = (t.a.hypot(t.b).max(t.c.hypot(t.d)) * 300.0 / 72.0).clamp(0.01, 1.0e4);
                    for part in scene.parts(px) {
                        match part {
                            folio_render::svg::Part::Shape(shape) => {
                                s.push(); s.concat(&shape.transform);
                                s.set_overprint(self.reg, Some(false), Some(false));
                                if let Some(rgb) = shape.fill {
                                    let r=self.reg.res.rgb(rgb); s.set_fill(&r.col); s.path(&shape.paths); s.fill(shape.evenodd);
                                }
                                if let Some(rgb) = shape.stroke.filter(|_|shape.width>0.) {
                                    let r=self.reg.res.rgb(rgb); s.set_stroke(&r.col);
                                    s.line_style(shape.width,shape.join,shape.cap,Some(shape.miter)); s.set_dash(shape.dash.as_ref());
                                    s.path(&shape.paths); s.stroke();
                                }
                                s.pop();
                            }
                            folio_render::svg::Part::Clip { paths, transform, evenodd } => {
                                s.push(); s.concat(&transform); s.path(&paths); s.clip(evenodd);
                                if let Some(back) = transform.invert() { s.concat(&back); }
                            }
                            folio_render::svg::Part::EndClip => s.pop(),
                            folio_render::svg::Part::Raster { pixmap, rect } => {
                                let alpha = pixmap.pixels().iter().any(|p| p.alpha() < 255);
                                let Ok(png) = pixmap.encode_png() else { continue };
                                let unit = Affine::new(rect.width(), 0.0, 0.0, -rect.height(), rect.left, rect.bottom);
                                if alpha && self.flat.is_some() {
                                    self.flat_site(s, &rect, "SVG content drawn as pixels with transparency");
                                    continue;
                                }
                                let index = self.reg.graphics.svg_pixels(g, png);
                                s.push();
                                s.concat(&unit);
                                let placed = s.ctm();
                                self.reg.graphics.placed(index, &placed);
                                s.xobject_image(index);
                                s.pop();
                                if alpha { s.transparency = true; }
                            }
                        }
                    }
                    self.reg.counts.image_placements += 1;
                    return;
                },
                Err(error) => self.reg.warn.add("link", error, Some(page), Some(&g.id)),
            }
        }
        let placed = {
            let reg = &mut *self.reg;
            reg.graphics.place(g, page, &mut reg.res, &mut reg.warn, &frame.id)
        };
        // flattening: a placed PDF that PDF 1.3 cannot hold as it is
        let flat_import = match &placed {
            Some(Placement::Imported { index }) if self.flat.is_some() => self.reg.graphics.imported_needs_flattening(*index),
            _ => None,
        };
        match placed {
            Some(Placement::Image { unit, transparency: true, .. }) if self.flat.is_some() => {
                self.flat_site(s, &unit.apply_rect(&Rect { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 }), "image with transparency");
            }
            Some(Placement::Imported { .. }) if flat_import.is_some() => {
                self.flat_site(s, &g.bounds, flat_import.unwrap_or_default());
                if let Some(f) = self.flat.as_mut() {
                    *f.kinds.entry("placed PDF drawn from its preview").or_default() += 1;
                }
            }
            Some(Placement::Image { index, unit, transparency }) => {
                // 1-bit images are stencils painted in the fill colour: black, as InDesign.
                s.set_fill(&Col::Cmyk([0.0, 0.0, 0.0, 1.0]));
                if self.solid_color_intent != RenderingIntent::default() {
                    let intent = self.reg.graphics.images[index].color.as_ref()
                        .map(|color| crate::image_color::pdf_intent(&color.intent))
                        .unwrap_or_default();
                    s.c.set_rendering_intent(intent);
                }
                s.concat(&unit);
                let placed = s.ctm();
                self.reg.graphics.placed(index, &placed);
                s.xobject_image(index);
                self.reg.counts.image_placements += 1;
                if transparency {
                    s.transparency = true;
                }
            }
            Some(Placement::Imported { index }) => {
                let form = self.reg.graphics.form_of(index, &mut self.reg.forms);
                s.xobject_form(form);
                self.reg.counts.image_placements += 1;
                if self.reg.graphics.imported_transparency(index) {
                    s.transparency = true;
                }
            }
            None => {}
        }
    }
}
