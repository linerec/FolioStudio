//! Conditional text definitions; membership stays with the original story ranges.
use serde::{Deserialize, Serialize};

/// `ConditionalTextPreference/@ShowConditionIndicators`: whether the marks of conditional text show on screen and print.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Indicators {
    Hide,
    #[default]
    Show,
    ShowAndPrint,
}

impl Indicators {
    pub fn from_idml(v: &str) -> Indicators {
        match v {
            "HideIndicators" => Indicators::Hide,
            "ShowAndPrintIndicators" => Indicators::ShowAndPrint,
            _ => Indicators::Show,
        }
    }
}

/// The RGB (0-255) InDesign 2025 draws a condition's indicator in: its interface colour measured from the PDF it exports
/// (scenario condx-ind-colors), or `r,g,b` of an own colour.
pub fn indicator_rgb(color: &str) -> Option<[u8; 3]> {
    if let Some((r, rest)) = color.split_once(',') {
        let (g, b) = rest.split_once(',')?;
        let v = |s: &str| s.trim().parse::<f64>().ok().map(|x| x.round().clamp(0.0, 255.0) as u8);
        return Some([v(r)?, v(g)?, v(b)?]);
    }
    Some(match color {
        "LightBlue" => [79, 153, 255], "Red" => [255, 0, 0], "Green" => [79, 255, 79], "Blue" => [0, 0, 255],
        "Yellow" => [255, 255, 79], "Magenta" => [255, 79, 255], "Cyan" => [0, 255, 255], "Gray" => [128, 128, 128],
        "Black" => [0, 0, 0], "Orange" => [255, 102, 0], "DarkGreen" => [0, 84, 0], "Teal" => [0, 153, 153],
        "Tan" => [204, 153, 102], "Brown" => [153, 51, 0], "Violet" => [153, 51, 255], "Gold" => [255, 153, 0],
        "DarkBlue" => [0, 0, 135], "Pink" => [255, 153, 204], "Lavender" => [153, 153, 255], "BrickRed" => [153, 0, 0],
        "OliveGreen" => [102, 102, 0], "Peach" => [255, 153, 153], "Burgundy" => [153, 0, 51], "GrassGreen" => [153, 204, 0],
        "Ochre" => [153, 102, 0], "Purple" => [102, 0, 102], "LightGray" => [186, 186, 186], "Charcoal" => [171, 163, 181],
        "GridBlue" => [122, 186, 217], "GridOrange" => [255, 181, 107], "Fiesta" => [247, 89, 107], "LightOlive" => [140, 166, 107],
        "Lipstick" => [207, 130, 181], "CuteTeal" => [130, 207, 194], "Sulphur" => [207, 207, 130], "GridGreen" => [156, 222, 156],
        "White" => [255, 255, 255],
        _ => return None,
    })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Condition {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub indicator_method: String,
    pub underline_appearance: String,
    pub indicator_color: String,
}
