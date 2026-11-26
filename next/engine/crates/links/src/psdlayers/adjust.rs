//! Adjustment and fill layers of a PSD: a layer without pixels whose colour is made from what is
//! below it (invert, posterize, threshold) or is one colour (solid fill).
//! InDesign composes these with the Photoshop engine; the formulas below were checked against its
//! PDF output (objlayer-c5, `goldens/objlayer-c5/cells`): the adjusted colour is the layer's
//! colour, which then takes the layer's blend mode, opacity, fill and masks like a pixel layer.
use super::{Rgb, be16};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Adjust {
    Invert,
    /// Number of levels (2..=255).
    Posterize(u16),
    /// Level 1..=255: the luminance at or above it is white, the rest black.
    Threshold(u16),
    /// A solid colour.
    Solid(Rgb),
}

/// What a tagged block makes of a layer, if it is an adjustment or fill this module can compose. `None`: not one
/// of those blocks; `Some(Err)`: one it cannot compose (the reason).
pub(super) fn from_block(key: &[u8], data: &[u8]) -> Option<Result<Adjust, String>> {
    Some(match key {
        b"nvrt" => Ok(Adjust::Invert),
        b"post" => be16(data, 0).filter(|n| (2..=255).contains(n)).map(Adjust::Posterize).ok_or_else(|| "a posterize adjustment with unreadable levels".to_string()),
        b"thrs" => be16(data, 0).filter(|n| (1..=255).contains(n)).map(Adjust::Threshold).ok_or_else(|| "a threshold adjustment with unreadable level".to_string()),
        // measured: InDesign's brightness/contrast is not a per-channel add (a blue 176 became 193 where a red 176 became 218 for +40), so it is not composed
        b"brit" => Err("a brightness/contrast adjustment".to_string()),
        b"SoCo" => solid(data),
        b"levl" | b"curv" | b"expA" | b"hue " | b"hue2" | b"blnc" | b"blwh" | b"phfl" | b"mixr" | b"clrL" | b"grdm" | b"selc" | b"vibA" | b"GdFl" | b"PtFl" => Err("an adjustment or fill layer of a kind that cannot be composed yet".to_string()),
        _ => return None,
    })
}

/// `SoCo`: version 16 and a descriptor whose `Clr ` object holds `Rd  `, `Grn `, `Bl  ` (0..255 doubles).
fn solid(data: &[u8]) -> Result<Adjust, String> {
    let d = super::descriptor::parse_versioned(data).ok_or("unreadable solid colour fill")?;
    let c = d.get("Clr ").ok_or("unreadable solid colour fill")?;
    let channel = |k: &str| match c.get(k) {
        Some(super::descriptor::Value::Double(v)) => Some((*v / 255.0).clamp(0.0, 1.0) as f32),
        _ => None,
    };
    match (channel("Rd  "), channel("Grn "), channel("Bl  ")) {
        (Some(r), Some(g), Some(b)) => Ok(Adjust::Solid([r, g, b])),
        _ => Err("a solid colour fill that is not RGB".into()),
    }
}

impl Adjust {
    /// A fill has a colour of its own and covers the canvas; an adjustment changes what is below it.
    pub(super) fn is_fill(&self) -> bool {
        matches!(self, Adjust::Solid(_))
    }

    /// The adjusted colour of `c` (channels 0..1).
    pub(super) fn apply(&self, c: Rgb) -> Rgb {
        match self {
            Adjust::Invert => [1.0 - c[0], 1.0 - c[1], 1.0 - c[2]],
            Adjust::Posterize(n) => {
                let n = *n as f32;
                // 8-bit steps: the value is cut into n bands and each band shows its own level, spread over 0..255
                let level = |v: f32| ((v * 255.0 + 0.5).floor() * n / 256.0).floor().min(n - 1.0) / (n - 1.0);
                [level(c[0]), level(c[1]), level(c[2])]
            }
            Adjust::Threshold(t) => {
                let l = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
                let v = if (l * 255.0 + 0.5).floor() >= *t as f32 { 1.0 } else { 0.0 };
                [v, v, v]
            }
            Adjust::Solid(rgb) => *rgb,
        }
    }
}
