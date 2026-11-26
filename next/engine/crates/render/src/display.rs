//! Display Performance on screen: Fast Display draws a placed graphic as a grey box.
//!
//! Only the screen render uses it (`render_page_display`); printing, PDF export and the separation preview
//! draw every graphic whatever the setting (oracle displayperf: the printed page does not change).
//! Which setting counts follows InDesign 2025 (artifacts/displayperf-20261023/STATUS.ko.md): the object-level
//! setting of the graphic or its frame when it has one and the application allows them, the view's otherwise.
//! Typical and High Quality differ in InDesign by the resolution of the preview it draws; Folio draws the
//! same image for both. The grey of the box is an approximation (not measured).
use folio_model::{Affine, Graphic, LocalDisplay, Rect};
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap};

/// View ▸ Display Performance: how the window shows graphics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewDisplay {
    /// Fast Display (`OPTIMIZED`).
    Optimized,
    /// Typical Display (`TYPICAL`).
    Typical,
    /// High Quality Display (`HIGH_QUALITY`).
    #[default]
    HighQuality,
}

impl ViewDisplay {
    /// The DOM name the preferences store (`OPTIMIZED`, `TYPICAL`, `HIGH_QUALITY`).
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "OPTIMIZED" => Self::Optimized,
            "TYPICAL" => Self::Typical,
            "HIGH_QUALITY" => Self::HighQuality,
            _ => return None,
        })
    }
}

/// How a screen render shows graphics: the window's view setting, and whether the application ignores object-level settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayOptions {
    pub view: ViewDisplay,
    /// Preferences ▸ Display Performance ▸ Ignore Object-Level Display Settings (View ▸ Allow Object-Level Display Settings off).
    pub ignore_local: bool,
}

impl DisplayOptions {
    /// How a graphic in a frame is shown: its own setting, else its frame's, else the view's.
    pub fn shown(&self, frame: LocalDisplay, graphic: LocalDisplay) -> ViewDisplay {
        if self.ignore_local {
            return self.view;
        }
        match if graphic != LocalDisplay::UseView { graphic } else { frame } {
            LocalDisplay::UseView => self.view,
            LocalDisplay::HighQuality => ViewDisplay::HighQuality,
            LocalDisplay::Typical => ViewDisplay::Typical,
            LocalDisplay::Optimized => ViewDisplay::Optimized,
        }
    }
}

/// The mid grey Fast Display fills a graphic with.
const BOX_GREY: u8 = 0x99;

/// The graphic's box (its bounds in the frame's inner space) filled grey where it shows inside the frame.
pub(crate) fn grey_box(pixmap: &mut Pixmap, g: &Graphic, frame: Option<&tiny_skia::Path>, rule: FillRule, frame_t: &Affine, clip: Option<&Mask>) {
    let b: &Rect = &g.bounds;
    let Some(rect) = tiny_skia::Rect::from_ltrb(b.left as f32, b.top as f32, b.right as f32, b.bottom as f32) else { return };
    let Some(path) = PathBuilder::from_rect(rect).transform(crate::to_skia(&g.transform)) else { return };
    let mut paint = Paint::default();
    paint.set_color_rgba8(BOX_GREY, BOX_GREY, BOX_GREY, 255);
    paint.anti_alias = true;
    let to_device = crate::to_skia(frame_t);
    match frame {
        Some(f) => {
            crate::with_clip_layer(pixmap, clip, f, rule, frame_t, |layer, offset, mask| {
                layer.fill_path(&path, &paint, FillRule::Winding, crate::to_skia(&offset.then_inner(frame_t)), Some(mask));
            });
        }
        None => pixmap.fill_path(&path, &paint, FillRule::Winding, to_device, clip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_graphic_the_frame_then_the_view_decide() {
        let fast = DisplayOptions { view: ViewDisplay::Optimized, ignore_local: false };
        assert_eq!(fast.shown(LocalDisplay::UseView, LocalDisplay::UseView), ViewDisplay::Optimized);
        assert_eq!(fast.shown(LocalDisplay::HighQuality, LocalDisplay::UseView), ViewDisplay::HighQuality);
        assert_eq!(fast.shown(LocalDisplay::Optimized, LocalDisplay::Typical), ViewDisplay::Typical);
        let ignoring = DisplayOptions { view: ViewDisplay::Typical, ignore_local: true };
        assert_eq!(ignoring.shown(LocalDisplay::Optimized, LocalDisplay::Optimized), ViewDisplay::Typical);
        assert_eq!(ViewDisplay::from_name("OPTIMIZED"), Some(ViewDisplay::Optimized));
        assert_eq!(ViewDisplay::from_name("FAST"), None);
    }
}
