//! Font and colour tables shared by RTF text and style readers.

use super::tokens::Tok;

pub(super) struct Resources {
    fonts: Vec<(i32, String)>,
    colors: Vec<Option<[u8; 3]>>,
}

impl Resources {
    pub(super) fn font_name(&self, index: i32) -> Option<&str> {
        self.fonts.iter().find(|(id, _)| *id == index).map(|(_, name)| name.as_str())
    }

    pub(super) fn swatch(&self, index: usize) -> Option<([u8; 3], String)> {
        if index == 0 {
            return None;
        }
        let rgb = self.colors.get(index).copied().flatten()?;
        Some((rgb, colour_name(rgb)?))
    }
}

/// The font table and the colour table (read before the text, so a table after the first text works too).
pub(super) fn read(toks: &[Tok<'_>]) -> Resources {
    let mut resources = Resources { fonts: Vec::new(), colors: Vec::new() };
    let mut depth = 0i32;
    let mut font_depth: Option<i32> = None;
    let mut color_depth: Option<i32> = None;
    let (mut font, mut name, mut ignore_below): (i32, Vec<u8>, Option<i32>) = (0, Vec::new(), None);
    let (mut rgb, mut seen) = ([0u8; 3], false);
    let mut prev_open = false;
    for tok in toks {
        match tok {
            Tok::Open => {
                depth += 1;
                prev_open = true;
                continue;
            }
            Tok::Close => {
                if font_depth == Some(depth) {
                    font_depth = None;
                }
                if color_depth == Some(depth) {
                    color_depth = None;
                }
                if ignore_below == Some(depth) {
                    ignore_below = None;
                }
                depth -= 1;
            }
            Tok::Word(w, p) => {
                if *w == b"fonttbl" && prev_open {
                    font_depth = Some(depth);
                } else if *w == b"colortbl" && prev_open {
                    color_depth = Some(depth);
                    (rgb, seen) = ([0; 3], false);
                } else if font_depth.is_some() {
                    if let (b"f", Some(n)) = (*w, p) {
                        font = *n;
                    }
                } else if color_depth.is_some() {
                    let v = p.unwrap_or(0).clamp(0, 255) as u8;
                    match *w {
                        b"red" => (rgb[0], seen) = (v, true),
                        b"green" => (rgb[1], seen) = (v, true),
                        b"blue" => (rgb[2], seen) = (v, true),
                        _ => {}
                    }
                }
            }
            Tok::Symbol(b'*') if font_depth.is_some() => ignore_below = Some(depth),
            Tok::Text(text) => {
                if font_depth.is_some() && ignore_below.is_none() {
                    for &c in *text {
                        if c == b';' {
                            let n = String::from_utf8_lossy(&name).trim().to_owned();
                            if !n.is_empty() {
                                resources.fonts.push((font, n));
                            }
                            name.clear();
                        } else {
                            name.push(c);
                        }
                    }
                } else if color_depth.is_some() {
                    for &c in *text {
                        if c == b';' {
                            resources.colors.push(seen.then_some(rgb));
                            (rgb, seen) = ([0; 3], false);
                        }
                    }
                }
            }
            _ => {}
        }
        prev_open = false;
    }
    resources
}

/// The swatch name InDesign gives a colour of the colour table (measured: rtf-colors, rtf-colors2).
fn colour_name(rgb: [u8; 3]) -> Option<String> {
    Some(match rgb {
        [0, 0, 0] => return None,
        [255, 0, 0] => "Red".to_owned(),
        [0, 0, 255] => "Blue".to_owned(),
        [0, 255, 0] => "Bright Green".to_owned(),
        [128, 128, 128] => "Gray-50%".to_owned(),
        [255, 0, 255] => "Pink".to_owned(),
        [0, 255, 255] => "Turquoise".to_owned(),
        [255, 255, 0] => "R=255 G=255 B=0".to_owned(),
        [255, 255, 255] => "White".to_owned(),
        [0, 0, 128] => "Dark Blue".to_owned(),
        [0, 128, 128] => "Teal".to_owned(),
        [0, 128, 0] => "Green".to_owned(),
        [128, 0, 128] => "Violet".to_owned(),
        [128, 0, 0] => "Dark Red".to_owned(),
        [128, 128, 0] => "Dark Yellow".to_owned(),
        [192, 192, 192] => "Gray-25%".to_owned(),
        [r, g, b] => format!("RTF r{r} g{g} b{b}"),
    })
}
