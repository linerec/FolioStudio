use super::{QUIET_ZONE, QrModules};

const MAX_SIZE: usize = 177;

fn four_numbers(line: &str) -> Option<[f64; 4]> {
    let mut values = line.split_whitespace().filter_map(|word| word.parse().ok());
    let numbers = [
        values.next()?,
        values.next()?,
        values.next()?,
        values.next()?,
    ];
    (values.next().is_none() && numbers.iter().all(|value: &f64| value.is_finite()))
        .then_some(numbers)
}

fn module_index(position: f64, step: f64, size: usize) -> Option<usize> {
    let index = position / step - QUIET_ZONE as f64;
    let rounded = index.round();
    ((index - rounded).abs() <= 0.01 && (0.0..size as f64).contains(&rounded))
        .then_some(rounded as usize)
}

// R8
pub fn from_indesign_eps(data: &[u8]) -> Option<QrModules> {
    let mut bound = None;
    let mut rectangles = Vec::new();
    for line in std::str::from_utf8(data).ok()?.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("%%BoundingBox:") {
            if let Some([0.0, 0.0, width, height]) = four_numbers(rest)
                && width > 0.0
                && (width - height).abs() < 0.001
            {
                bound = Some(width);
            }
        } else if let Some(rest) = line.strip_suffix(" rf") {
            if rectangles.len() == MAX_SIZE * MAX_SIZE {
                return None;
            }
            rectangles.push(four_numbers(rest)?);
        }
    }
    let bound = bound?;
    let step = rectangles.first()?[2];
    if step <= 0.0 {
        return None;
    }
    let total = (bound / step).round();
    let size = total - (2 * QUIET_ZONE) as f64;
    if !(21.0..=MAX_SIZE as f64).contains(&size)
        || !(size as usize - 17).is_multiple_of(4)
        || (bound - total * step).abs() > 0.01
    {
        return None;
    }
    let size = size as usize;
    let mut dark = vec![false; size * size];
    for [x, y, width, height] in rectangles {
        if (width - step).abs() > 0.001 || (height - step).abs() > 0.001 {
            return None;
        }
        let column = module_index(x, step, size)?;
        let row = size - 1 - module_index(y, step, size)?;
        dark[row * size + column] = true;
    }
    Some(QrModules { size, dark })
}

// R9
pub fn indesign_eps_color(data: &[u8]) -> Option<(&'static str, Vec<f64>)> {
    std::str::from_utf8(data).ok()?.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let kind = match words.next_back()? {
            "setcmykcolor" => "cmyk",
            "setrgbcolor" => "rgb",
            "setgray" => "gray",
            "setcolor" => "tint",
            _ => return None,
        };
        let values = words.map(str::parse).collect::<Result<Vec<_>, _>>().ok()?;
        (!values.is_empty()).then_some((kind, values))
    })
}
