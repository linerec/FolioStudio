//! The text variables that read the document's file and clock (oracle textvars; InDesign 2025 measured with probe-a/b/c in
//! `artifacts/textvars-20261023/`):
//! - Creation Date, Modification Date, Output Date: the document's `xmp:CreateDate`, `xmp:ModifyDate` and the last output (none yet: the
//!   creation date), written with the variable's `Format`, an ICU date pattern (`MM/dd/yy`, `MMMM d, yyyy h:mm aa`, quoted text in `'…'`);
//!   the text before and after go around it. The wall-clock time is the one the date is written with (its own offset), not this
//!   computer's, so a document shows the same everywhere;
//! - File Name: the file's name without its extension (`IncludeExtension`: with it), with the folder in front when `IncludePath` is
//!   set. InDesign writes the absolute path there; Folio writes the folder relative to the project root, `/` between the names
//!   (principle 1), and nothing in front of a document that is in no project;
//! - Image Name (`LiveCaptionType`, metadata `$ID/#LinkInfoNameStr`): the file name of the link of the one graphic whose frame meets the text
//!   frame on its spread; `<No intersecting link>` for none, `<Multiple intersecting links>` for two or more (text before and after go
//!   around these too). Other metadata (description, author …) is not worked out: the result InDesign stored stays.
use super::*;
use folio_model::{DocumentFacts, VariableDef};

/// The fields of an ISO 8601 date-time (`2026-10-23T07:24:51-04:00`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct When {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    /// Minutes east of UTC.
    offset: i64,
}

fn parse_when(iso: &str) -> Option<When> {
    let iso = iso.trim();
    let (date, rest) = iso.split_once('T').unwrap_or((iso, "00:00:00"));
    let mut d = date.split('-');
    let (year, month, day) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    // time then offset: `07:24:51.5-04:00`, `…Z`, `…+0900`
    let at = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
    let (time, zone) = rest.split_at(at);
    let mut t = time.split(':');
    let hour = t.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let minute = t.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let second = t.next().and_then(|v| v.split('.').next().and_then(|s| s.parse().ok())).unwrap_or(0);
    let offset = match zone.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let digits: String = zone[1..].chars().filter(char::is_ascii_digit).collect();
            let (h, m) = (digits.get(0..2)?.parse::<i64>().ok()?, digits.get(2..4).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0));
            (h * 60 + m) * if sign == '-' { -1 } else { 1 }
        }
        _ => 0,
    };
    ((1..=12).contains(&month) && (1..=31).contains(&day) && hour < 25 && minute < 60 && second < 62).then_some(When { year, month, day, hour, minute, second, offset })
}

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// 0 = Sunday.
fn weekday(y: i64, m: i64, d: i64) -> i64 {
    (days_from_civil(y, m, d) + 4).rem_euclid(7)
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn number(out: &mut String, v: i64, width: usize) {
    out.push_str(&format!("{v:0width$}"));
}

/// The date written with an ICU pattern (the letters InDesign's variable accepts; measured for each in textvars probe-b: `d dd M MM MMM
/// MMMM MMMMM yy yyyy EEE EEEE h hh H HH K k m mm s ss a aa z Z G D w W F Q e c`, `'quoted'`, other characters as they are). English names.
pub fn format_date(iso: &str, pattern: &str) -> Option<String> {
    let w = parse_when(iso)?;
    let wd = weekday(w.year, w.month, w.day);
    let doy = days_from_civil(w.year, w.month, w.day) - days_from_civil(w.year, 1, 1) + 1;
    let jan1 = weekday(w.year, 1, 1);
    let first = weekday(w.year, w.month, 1);
    let (h12, pm) = ((w.hour + 11) % 12 + 1, w.hour % 24 >= 12);
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            // `''` is an apostrophe, `'…'` text as it is
            if chars.get(i + 1) == Some(&'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        out.push('\'');
                        i += 2;
                        continue;
                    }
                    break;
                }
                out.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        if !c.is_ascii_alphabetic() {
            out.push(c);
            i += 1;
            continue;
        }
        let n = chars[i..].iter().take_while(|&&x| x == c).count();
        i += n;
        match c {
            'y' | 'u' | 'Y' => {
                if n == 2 { number(&mut out, w.year.rem_euclid(100), 2) } else { number(&mut out, w.year, n.max(1)) }
            }
            'M' | 'L' => match n {
                1 | 2 => number(&mut out, w.month, n),
                3 => out.push_str(&MONTHS[w.month as usize - 1][..3]),
                4 => out.push_str(MONTHS[w.month as usize - 1]),
                _ => out.push_str(&MONTHS[w.month as usize - 1][..1]),
            },
            'd' => number(&mut out, w.day, n),
            'D' => number(&mut out, doy, n),
            'E' | 'c' | 'e' if n >= 3 || c == 'E' => match n {
                1..=3 => out.push_str(&DAYS[wd as usize][..3]),
                4 => out.push_str(DAYS[wd as usize]),
                _ => out.push_str(&DAYS[wd as usize][..1]),
            },
            'e' | 'c' => number(&mut out, wd + 1, n),
            'h' => number(&mut out, h12, n),
            'H' => number(&mut out, w.hour % 24, n),
            'K' => number(&mut out, w.hour % 12, n),
            'k' => number(&mut out, if w.hour % 24 == 0 { 24 } else { w.hour % 24 }, n),
            'm' => number(&mut out, w.minute, n),
            's' => number(&mut out, w.second, n),
            'S' => out.push_str(&"0".repeat(n)),
            'a' => out.push_str(if pm { "PM" } else { "AM" }),
            'G' => out.push_str("AD"),
            'w' => number(&mut out, (doy - 1 + jan1) / 7 + 1, n),
            'W' => number(&mut out, (w.day - 1 + first) / 7 + 1, n),
            'F' => number(&mut out, (w.day - 1) / 7 + 1, n),
            'Q' => match n {
                1 | 2 => number(&mut out, (w.month - 1) / 3 + 1, n),
                _ => out.push_str(&format!("Q{}", (w.month - 1) / 3 + 1)),
            },
            'Z' => out.push_str(&format!("{}{:02}{:02}", if w.offset < 0 { '-' } else { '+' }, w.offset.abs() / 60, w.offset.abs() % 60)),
            'z' => out.push_str(&format!("GMT{}{:02}:{:02}", if w.offset < 0 { '-' } else { '+' }, w.offset.abs() / 60, w.offset.abs() % 60)),
            _ => (0..n).for_each(|_| out.push(c)),
        }
    }
    Some(out)
}

/// An ISO 8601 date-time of `secs` since 1970 (UTC) written with `offset` minutes east of UTC: `2026-10-23T16:24:51+09:00`.
pub fn iso_from_unix(secs: i64, offset: i64) -> String {
    let local = secs + offset * 60;
    let (days, rem) = (local.div_euclid(86400), local.rem_euclid(86400));
    // civil date of a day number (Howard Hinnant's algorithm, the inverse of `days_from_civil`)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let sign = if offset < 0 { '-' } else { '+' };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60, offset.abs() / 60, offset.abs() % 60)
}

fn around(def: &VariableDef, shown: &str) -> String {
    format!("{}{}{}", def.text_before, shown, def.text_after)
}

/// The Creation / Modification / Output Date variable `def` for a document with `facts`; `None` when the document does not say (the
/// result the file stores stays).
pub fn date_value(def: &VariableDef, facts: &DocumentFacts) -> Option<String> {
    let at = match def.kind.as_str() {
        "CreationDateType" => &facts.created,
        "ModificationDateType" => &facts.modified,
        // no output yet: InDesign shows the creation date (measured: unchanged until the first PDF export)
        "OutputDateType" if facts.output.is_empty() => &facts.created,
        "OutputDateType" => &facts.output,
        _ => return None,
    };
    let pattern = def.settings.get("Format").map(String::as_str).unwrap_or("MM/dd/yy");
    format_date(at, pattern).map(|d| around(def, &d))
}

/// The File Name variable `def` (name, extension and folder from the document's `facts`).
pub fn file_name_value(def: &VariableDef, facts: &DocumentFacts) -> Option<String> {
    if facts.file_name.is_empty() {
        return None;
    }
    let (stem, ext) = match facts.file_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, Some(e)),
        _ => (facts.file_name.as_str(), None),
    };
    let on = |k: &str| def.settings.get(k).is_some_and(|v| v == "true");
    let mut name = String::new();
    if on("IncludePath") && !facts.folder.is_empty() {
        name.push_str(facts.folder.trim_end_matches('/'));
        name.push('/');
    }
    name.push_str(stem);
    if let Some(e) = ext.filter(|_| on("IncludeExtension")) {
        name.push('.');
        name.push_str(e);
    }
    Some(around(def, &name))
}

/// The link names of the graphics on `frame`'s spread whose frames meet the text frame: the Image Name variable `def` for it.
pub(super) fn image_name_value(def: &VariableDef, doc: &Document, frame: &FrameRef<'_>) -> Option<String> {
    if def.settings.get("MetadataProviderName").is_some_and(|p| p != "$ID/#LinkInfoNameStr") {
        return None;
    }
    let spread = frame.spread?;
    let spreads: Vec<&folio_model::Spread> = doc.spreads.iter().chain(&doc.master_spreads).collect();
    let items = &spreads.get(spread)?.items;
    let mine = frame.to_spread.apply_rect(&paths_bounds(&frame.item.paths)?);
    let mut names = Vec::new();
    graphics_meeting(items, folio_model::Affine::IDENTITY, &mine, &mut names);
    let shown = match names.len() {
        0 => "<No intersecting link>".to_owned(),
        1 => names.remove(0),
        _ => "<Multiple intersecting links>".to_owned(),
    };
    Some(around(def, &shown))
}

fn graphics_meeting(items: &[PageItem], parent: folio_model::Affine, mine: &Rect, out: &mut Vec<String>) {
    for it in items {
        let here = parent.then_inner(&it.transform);
        match &it.kind {
            ItemKind::Group { children } => graphics_meeting(children, here, mine, out),
            ItemKind::Rectangle { content: Some(g) } | ItemKind::Oval { content: Some(g) } | ItemKind::Polygon { content: Some(g) } => {
                let Some(uri) = g.link.as_ref().map(|l| l.uri.as_str()).or(g.link_uri.as_deref()) else { continue };
                let Some(own) = paths_bounds(&it.paths) else { continue };
                let b = here.apply_rect(&own);
                let meets_horizontal = b.left < mine.right && mine.left < b.right && b.top <= mine.bottom && mine.top <= b.bottom;
                let meets_vertical = b.left <= mine.right && mine.left <= b.right && b.top < mine.bottom && mine.top < b.bottom;
                if meets_horizontal || meets_vertical {
                    out.push(link_name(uri));
                }
            }
            _ => {}
        }
    }
}

/// The file name of a link's `file:` URI, percent-decoded.
fn link_name(uri: &str) -> String {
    let last = uri.rsplit(['/', '\\']).next().unwrap_or(uri);
    let bytes = last.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && let Some(v) = last.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The instant of textvars probe-b (InDesign 2025, a Friday): the same patterns gave these texts.
    const AT: &str = "2026-10-23T07:26:26-04:00";

    #[test]
    fn patterns_read_as_indesign_wrote_them() {
        for (pattern, want) in [
            ("d", "23"), ("MM", "10"), ("MMM", "Oct"), ("MMMM", "October"), ("MMMMM", "O"), ("yy", "26"), ("yyyy", "2026"), ("EEE", "Fri"),
            ("EEEE", "Friday"), ("h", "7"), ("hh", "07"), ("HH", "07"), ("a", "AM"), ("aa", "AM"), ("Z", "-0400"), ("G", "AD"), ("D", "296"),
            ("w", "43"), ("W", "4"), ("F", "4"), ("Q", "4"), ("QQQ", "Q4"), ("e", "6"), ("ee", "06"), ("ccc", "Fri"),
            ("MM/dd/yy", "10/23/26"), ("MMM d, yyyy", "Oct 23, 2026"), ("MMMM d, yyyy", "October 23, 2026"), ("d MMMM yyyy", "23 October 2026"),
            ("EEEE, MMMM d, yyyy", "Friday, October 23, 2026"), ("yyyy-MM-dd HH:mm:ss", "2026-10-23 07:26:26"), ("h:mm:ss aa", "7:26:26 AM"),
            ("yyyy'\u{b144}' M'\u{c6d4}' d'\u{c77c}'", "2026\u{b144} 10\u{c6d4} 23\u{c77c}"), ("'Date' d", "Date 23"), ("d'th'", "23th"),
            ("MMMM d, yyyy h:mm aa", "October 23, 2026 7:26 AM"), ("yy. M. d.", "26. 10. 23."),
        ] {
            assert_eq!(format_date(AT, pattern).as_deref(), Some(want), "{pattern}");
        }
    }

    #[test]
    fn afternoon_and_midnight() {
        assert_eq!(format_date("2025-12-25T00:03:04+09:00", "h:mm aa, H, K, k").as_deref(), Some("12:03 AM, 0, 0, 24"));
        assert_eq!(format_date("2025-12-25T14:03:04+09:00", "h aa Z").as_deref(), Some("2 PM +0900"));
        assert_eq!(format_date("nonsense", "d"), None);
    }

    #[test]
    fn file_names_follow_the_options() {
        let facts = DocumentFacts { file_name: "probe-doc.indd".into(), folder: "issues/12".into(), ..Default::default() };
        let mut def = VariableDef { kind: "FileNameType".into(), ..Default::default() };
        let name = |def: &VariableDef| file_name_value(def, &facts).unwrap();
        assert_eq!(name(&def), "probe-doc");
        def.settings.insert("IncludeExtension".into(), "true".into());
        assert_eq!(name(&def), "probe-doc.indd");
        def.settings.insert("IncludePath".into(), "true".into());
        assert_eq!(name(&def), "issues/12/probe-doc.indd");
        def.text_before = "<<".into();
        def.text_after = ">>".into();
        assert_eq!(name(&def), "<<issues/12/probe-doc.indd>>");
        assert_eq!(file_name_value(&def, &DocumentFacts::default()), None);
    }

    #[test]
    fn output_date_is_the_creation_date_until_an_output() {
        let mut facts = DocumentFacts { created: AT.into(), ..Default::default() };
        let def = VariableDef { kind: "OutputDateType".into(), ..Default::default() };
        assert_eq!(date_value(&def, &facts).as_deref(), Some("10/23/26"));
        facts.output = "2027-01-02T03:04:05+00:00".into();
        assert_eq!(date_value(&def, &facts).as_deref(), Some("01/02/27"));
    }

    #[test]
    fn unix_seconds_become_the_iso_date_of_the_zone() {
        // 2026-10-23 11:24:51 UTC
        assert_eq!(iso_from_unix(1_792_754_691, 0), "2026-10-23T11:24:51+00:00");
        assert_eq!(iso_from_unix(1_792_754_691, 9 * 60), "2026-10-23T20:24:51+09:00");
        assert_eq!(iso_from_unix(1_792_754_691, -4 * 60), "2026-10-23T07:24:51-04:00");
        assert_eq!(iso_from_unix(0, 0), "1970-01-01T00:00:00+00:00");
        assert_eq!(parse_when(&iso_from_unix(1_792_754_691, 9 * 60)).map(|w| (w.year, w.day, w.hour, w.offset)), Some((2026, 23, 20, 540)));
    }

    #[test]
    fn link_names_are_decoded() {
        assert_eq!(link_name("file:///C:/Links/place%20plain.png"), "place plain.png");
        assert_eq!(link_name("file:///C:/Links/%ED%95%9C.jpg"), "\u{d55c}.jpg");
    }
}
