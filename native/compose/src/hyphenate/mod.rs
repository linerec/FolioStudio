mod patterns;
mod supplement;
use crate::{Item, SOFT_HYPHEN, Settings, breaks::is_latin_letter};
use patterns::Patterns;
use std::sync::LazyLock;

pub struct Dictionary {
    patterns: Patterns,
    supplement: bool,
}
macro_rules! dictionary {
    ($name:ident, $file:literal, $extra:expr, $supplement:expr) => {
        static $name: LazyLock<Dictionary> = LazyLock::new(|| Dictionary {
            patterns: Patterns::load(include_str!(concat!("../../data/", $file)), $extra),
            supplement: $supplement,
        });
    };
}
dictionary!(US, "hyph-en-us.tex", None, true);
dictionary!(UK, "hyph-en-us.tex", Some("5zation"), true);
dictionary!(BRITISH_PATTERNS, "hyph-en-gb.tex", None, false);
dictionary!(GERMAN, "hyph-de-1996.tex", None, false);
dictionary!(TRADITIONAL, "hyph-de-1901.tex", None, false);
dictionary!(FRENCH, "hyph-fr.tex", None, false);
dictionary!(SPANISH, "hyph-es.tex", None, false);

impl Dictionary {
    pub fn english_us() -> &'static Self {
        &US
    }
    pub fn english_uk() -> &'static Self {
        &BRITISH_PATTERNS
    }
    pub fn for_language(name: &str) -> Option<&'static Self> {
        let name = name.strip_prefix("$ID/").unwrap_or(name);
        match name {
            "English: UK" => Some(&UK),
            "German: Traditional" | "German: Old Rules" => Some(&TRADITIONAL),
            _ if name.starts_with("English") || name == "Korean" => Some(&US),
            _ if name.starts_with("German") || name.starts_with("de_") => Some(&GERMAN),
            _ if name.starts_with("French") => Some(&FRENCH),
            _ if name.starts_with("Spanish") => Some(&SPANISH),
            _ => None,
        }
    }

    pub fn points(&self, word: &str) -> Vec<usize> {
        let length = word.chars().count();
        let lower = word.to_lowercase();
        if length < 2 || lower.chars().count() != length {
            return Vec::new();
        }
        if let Some(points) = self.patterns.exception(&lower) {
            return points.to_vec();
        }
        let mut points = self.patterns.points(&lower);
        if self.supplement && !points.iter().any(|&p| p >= 2 && p + 2 <= length) {
            supplement::add_points(&lower, &mut points);
        }
        points
    }
}

pub fn points_with_width(
    items: &[Item],
    s: &Settings,
    _line_width: f64,
    dictionary: &dyn Fn(usize) -> Option<&'static Dictionary>,
) -> Vec<usize> {
    let mut result = Vec::new();
    if !s.hyphenation {
        return result;
    }
    let mut i = 0;
    while i < items.len() {
        let start = i;
        i += 1;
        if !is_latin_letter(items[start].ch) {
            continue;
        }
        while i < items.len() && (is_latin_letter(items[i].ch) || items[i].ch == SOFT_HYPHEN) {
            i += 1;
        }
        let length = i - start;
        if length < s.hyphen.words_longer_than
            || items[start..i].iter().any(|x| x.ch == SOFT_HYPHEN)
        {
            continue;
        }
        if let Some(dict) = dictionary(start) {
            let word: String = items[start..i].iter().map(|x| x.ch).collect();
            result.extend(
                dict.points(&word)
                    .into_iter()
                    .filter(|&p| p >= s.hyphen.after_first && length - p >= s.hyphen.before_last)
                    .map(|p| start + p),
            );
        }
    }
    result
}
pub fn points(
    items: &[Item],
    s: &Settings,
    dictionary: &dyn Fn(usize) -> Option<&'static Dictionary>,
) -> Vec<usize> {
    points_with_width(items, s, f64::INFINITY, dictionary)
}

pub struct Hyphenated {
    pub items: Vec<Item>,
    pub origin: Vec<Option<usize>>,
}
impl Hyphenated {
    pub fn original_breaks(&self, breaks: &[usize]) -> Vec<usize> {
        breaks
            .iter()
            .filter_map(|&j| self.origin.get(j..)?.iter().flatten().next().copied())
            .collect()
    }
    pub fn hyphen_before(&self, b: usize) -> bool {
        b.checked_sub(1)
            .and_then(|j| self.items.get(j))
            .is_some_and(|x| x.ch == SOFT_HYPHEN)
    }
}
pub fn insert(items: &[Item], points: &[usize], hyphen_width: &dyn Fn(usize) -> f64) -> Hyphenated {
    let mut out = Hyphenated {
        items: Vec::with_capacity(items.len()),
        origin: Vec::with_capacity(items.len()),
    };
    let mut points = points
        .iter()
        .copied()
        .filter(|&p| p > 0 && p < items.len())
        .peekable();
    for (i, item) in items.iter().enumerate() {
        if points.peek() == Some(&i) {
            out.items.push(Item::soft_hyphen(
                hyphen_width(i - 1),
                items[i - 1].point_size,
            ));
            out.origin.push(None);
            while points.peek() == Some(&i) {
                points.next();
            }
        }
        out.items.push(*item);
        out.origin.push(Some(i));
    }
    out
}
