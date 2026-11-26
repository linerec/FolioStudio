//! Converts numbers and dates and formats their calculated-field values.

pub(super) const EXCEL_EPOCH: f64 = 25569.0;

pub(super) fn serial(days: i64) -> f64 { days as f64 + EXCEL_EPOCH }

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub(super) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn leap(y: i64) -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 }

fn days_in_month(y: i64, m: i64) -> i64 { match m { 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, 4 | 6 | 9 | 11 => 30, _ => if leap(y) { 29 } else { 28 } } }

fn make_date(y: i64, m: i64, d: i64) -> Option<i64> {
    ((1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then(|| days_from_civil(y, m, d))
}

/// A date written as `yyyy-mm-dd`, `yyyy/mm/dd`, `yyyy.mm.dd` (a time after it is ignored) or `yyyy년 m월 d일`.
pub fn parse_date(text: &str) -> Option<i64> {
    let s = text.trim();
    let chars: Vec<char> = s.chars().collect();
    let digits = |i: &mut usize, min: usize, max: usize| -> Option<i64> {
        let from = *i;
        while *i < chars.len() && chars[*i].is_ascii_digit() && *i - from < max { *i += 1; }
        if *i - from < min { return None; }
        chars[from..*i].iter().collect::<String>().parse().ok()
    };
    let mut i = 0;
    let y = digits(&mut i, 4, 4)?;
    let sep = *chars.get(i)?;
    let m;
    let d;
    if sep == '년' {
        i += 1;
        while chars.get(i) == Some(&' ') { i += 1; }
        m = digits(&mut i, 1, 2)?;
        if chars.get(i) != Some(&'월') { return None; }
        i += 1;
        while chars.get(i) == Some(&' ') { i += 1; }
        d = digits(&mut i, 1, 2)?;
        if chars.get(i) == Some(&'일') { i += 1; }
    } else {
        if !matches!(sep, '-' | '/' | '.') { return None; }
        i += 1;
        m = digits(&mut i, 1, 2)?;
        if chars.get(i) != Some(&sep) { return None; }
        i += 1;
        d = digits(&mut i, 1, 2)?;
        if sep == '.' && chars.get(i) == Some(&'.') { i += 1; }
    }
    let rest: String = chars[i..].iter().collect();
    let rest = rest.trim();
    if !rest.is_empty() {
        // a time of day after the date is not used
        let time = rest.strip_prefix('T').unwrap_or(rest).trim_start();
        let ok = time.split(':').count() >= 2 && time.chars().all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | ' ' | 'Z' | '+' | '-'));
        if !ok { return None; }
    }
    make_date(y, m, d)
}

/// A number as a spreadsheet reads it: an optional sign, digits (commas as thousands separators only in groups of three), a decimal
/// part, an exponent, a trailing `%`.
pub fn parse_number(text: &str) -> Option<f64> {
    let s = text.trim();
    let (body, percent) = match s.strip_suffix('%') { Some(b) => (b.trim_end(), true), None => (s, false) };
    let (sign, digits) = match body.strip_prefix('-') { Some(r) => (-1.0, r), None => (1.0, body.strip_prefix('+').unwrap_or(body)) };
    let first = digits.chars().next()?;
    if !(first.is_ascii_digit() || first == '.') { return None; }
    let (int_part, rest) = match digits.find(['.', 'e', 'E']) { Some(i) => digits.split_at(i), None => (digits, "") };
    let int_clean = if int_part.contains(',') {
        let groups: Vec<&str> = int_part.split(',').collect();
        let ok = !groups[0].is_empty() && groups[0].len() <= 3 && groups[1..].iter().all(|g| g.len() == 3) && groups.iter().all(|g| g.chars().all(|c| c.is_ascii_digit()));
        if !ok { return None; }
        groups.concat()
    } else { int_part.to_owned() };
    let x: f64 = format!("{int_clean}{rest}").parse().ok()?;
    if !x.is_finite() { return None; }
    Some(sign * if percent { x / 100.0 } else { x })
}

/// The shortest text of a number that keeps 15 significant digits (what a spreadsheet's General format gives).
pub(super) fn fmt_num(x: f64) -> String {
    if x == 0.0 { return "0".into(); }
    let a = x.abs();
    if !(1e-9..1e15).contains(&a) {
        let s = format!("{:.14e}", x);
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
        let e: i32 = e.parse().unwrap_or(0);
        return format!("{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    let exp = a.log10().floor() as i32;
    let decimals = (14 - exp).clamp(0, 30) as usize;
    let s = format!("{:.*}", decimals, x);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_owned() } else { s };
    if s == "-0" { "0".into() } else { s }
}

pub(super) fn iso(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// A number as 15 significant decimal digits (the value of a spreadsheet's cell), so that rounding is by the decimal that was
/// typed (`1.005` rounds to `1.01`), half away from zero.
pub(super) struct Dec { neg: bool, digits: Vec<u8>, point: i32 }

impl Dec {
    /// `value = 0.d1d2d3... * 10^point`
    pub(super) fn of(x: f64) -> Dec {
        if x == 0.0 || !x.is_finite() { return Dec { neg: false, digits: vec![], point: 0 }; }
        let s = format!("{:.14e}", x.abs());
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let digits: Vec<u8> = m.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
        Dec { neg: x < 0.0, digits, point: e.parse::<i32>().unwrap_or(0) + 1 }
    }

    pub(super) fn round_to(mut self, places: i32) -> Dec {
        let keep = self.point + places;
        if keep >= self.digits.len() as i32 { return self; }
        if keep < 0 { self.digits.clear(); self.point = 0; return self; }
        let keep = keep as usize;
        let up = self.digits[keep] >= 5;
        self.digits.truncate(keep);
        if up {
            let mut i = keep;
            let mut carry = true;
            while carry && i > 0 {
                i -= 1;
                if self.digits[i] == 9 { self.digits[i] = 0; } else { self.digits[i] += 1; carry = false; }
            }
            if carry { self.digits.insert(0, 1); self.point += 1; }
        }
        self
    }

    fn is_zero(&self) -> bool { self.digits.iter().all(|&d| d == 0) }

    /// The integer digits (without leading zeros, empty for zero) and `places` decimals.
    fn parts(&self, places: usize) -> (String, String) {
        let digit = |i: i32| -> char { if i < 0 { '0' } else { (b'0' + self.digits.get(i as usize).copied().unwrap_or(0)) as char } };
        let int: String = (0..self.point.max(0)).map(digit).collect::<String>().trim_start_matches('0').to_owned();
        let frac: String = (0..places as i32).map(|k| digit(self.point + k)).collect();
        (int, frac)
    }

    pub(super) fn to_f64(&self) -> f64 {
        let places = (self.digits.len() as i32 - self.point).max(0) as usize;
        let (int, frac) = self.parts(places);
        let text = format!("{}.{}", if int.is_empty() { "0" } else { &int }, if frac.is_empty() { "0" } else { &frac });
        let x: f64 = text.parse().unwrap_or(0.0);
        if self.neg { -x } else { x }
    }
}

// TEXT formats ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Fmt { Lit(String), Date(String), Int(char), Point, Dec(char), Comma, Percent }

/// Date tokens are runs of `y`, `m`, `d` (any case) and of three or four `a` (the Korean weekday); `"..."` and `\x` are texts.
pub(super) fn is_date_format(format: &str) -> bool { lex_format(format).iter().any(|t| matches!(t, Fmt::Date(_))) }

fn lex_format(format: &str) -> Vec<Fmt> {
    let chars: Vec<char> = format.chars().collect();
    let mut out: Vec<Fmt> = Vec::new();
    let mut i = 0;
    let lit = |out: &mut Vec<Fmt>, s: &str| {
        if let Some(Fmt::Lit(last)) = out.last_mut() { last.push_str(s); } else { out.push(Fmt::Lit(s.to_owned())); }
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' { s.push(chars[i]); i += 1; }
                i += 1;
                lit(&mut out, &s);
            }
            '\\' | '_' | '*' => {
                i += 1;
                if let Some(&x) = chars.get(i) { if c == '\\' { lit(&mut out, &x.to_string()); } else if c == '_' { lit(&mut out, " "); } }
                i += 1;
            }
            'y' | 'Y' | 'm' | 'M' | 'd' | 'D' => {
                let lower = c.to_ascii_lowercase();
                let from = i;
                while i < chars.len() && chars[i].to_ascii_lowercase() == lower { i += 1; }
                out.push(Fmt::Date(chars[from..i].iter().collect::<String>().to_lowercase()));
            }
            'a' | 'A' => {
                let from = i;
                while i < chars.len() && chars[i].eq_ignore_ascii_case(&'a') { i += 1; }
                if i - from >= 3 { out.push(Fmt::Date("a".repeat((i - from).min(4)))); } else { lit(&mut out, &chars[from..i].iter().collect::<String>()); }
            }
            '0' | '#' | '?' => { out.push(Fmt::Int(c)); i += 1; }
            '.' => { out.push(Fmt::Point); i += 1; }
            ',' => { out.push(Fmt::Comma); i += 1; }
            '%' => { out.push(Fmt::Percent); i += 1; }
            other => { lit(&mut out, &other.to_string()); i += 1; }
        }
    }
    // the digit placeholders after the point are decimals
    let mut after = false;
    for t in out.iter_mut() {
        match t { Fmt::Point => after = true, Fmt::Int(c) if after => *t = Fmt::Dec(*c), _ => {} }
    }
    out
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAYS_KO: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

pub(super) fn format_date(days: i64, format: &str) -> String {
    let (y, m, d) = civil_from_days(days);
    let weekday = (days + 4).rem_euclid(7) as usize;
    let mut out = String::new();
    for t in lex_format(format) {
        match t {
            Fmt::Date(run) => match run.as_bytes()[0] {
                b'y' => if run.len() <= 2 { out.push_str(&format!("{:02}", y.rem_euclid(100))); } else { out.push_str(&format!("{y:04}")); },
                b'm' => match run.len() {
                    1 => out.push_str(&m.to_string()),
                    2 => out.push_str(&format!("{m:02}")),
                    3 => out.push_str(&MONTHS[m as usize - 1][..3]),
                    4 => out.push_str(MONTHS[m as usize - 1]),
                    _ => out.push_str(&MONTHS[m as usize - 1][..1]),
                },
                b'd' => match run.len() {
                    1 => out.push_str(&d.to_string()),
                    2 => out.push_str(&format!("{d:02}")),
                    3 => out.push_str(&DAYS[weekday][..3]),
                    _ => out.push_str(DAYS[weekday]),
                },
                _ => { out.push_str(DAYS_KO[weekday]); if run.len() >= 4 { out.push_str("요일"); } }
            },
            Fmt::Lit(s) => out.push_str(&s),
            Fmt::Int(c) => out.push(c),
            Fmt::Dec(c) => out.push(c),
            Fmt::Point => out.push('.'),
            Fmt::Comma => out.push(','),
            Fmt::Percent => out.push('%'),
        }
    }
    out
}

/// `"a;b;c"` as sections (a `;` inside quotes does not split).
fn sections(format: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    let mut escaped = false;
    for c in format.chars() {
        if escaped { out.last_mut().expect("one section").push(c); escaped = false; continue; }
        match c {
            '\\' => { escaped = true; out.last_mut().expect("one section").push(c); }
            '"' => { quoted = !quoted; out.last_mut().expect("one section").push(c); }
            ';' if !quoted => out.push(String::new()),
            _ => out.last_mut().expect("one section").push(c),
        }
    }
    out
}

fn group3(digits: &str) -> String {
    let n = digits.len();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) { out.push(','); }
        out.push(c);
    }
    out
}

/// A number by a spreadsheet format: `0` and `#` and `?` digits, `,` thousands (between digits) or thousands scale (after them),
/// `.` decimals, `%`, texts in quotes, `\x`, other characters as they are; up to three sections (positive; negative; zero).
pub(super) fn format_number(x: f64, format: &str) -> String {
    let parts = sections(format);
    let (section, signed) = if x < 0.0 && parts.len() > 1 { (&parts[1], false) } else if x == 0.0 && parts.len() > 2 { (&parts[2], false) } else { (&parts[0], true) };
    let mut tokens = lex_format(section);
    let digit_at: Vec<usize> = tokens.iter().enumerate().filter(|(_, t)| matches!(t, Fmt::Int(_) | Fmt::Dec(_))).map(|(i, _)| i).collect();
    let (Some(&first), Some(&last)) = (digit_at.first(), digit_at.last()) else {
        // no digit placeholder: the format is only text
        return tokens.iter().map(|t| match t { Fmt::Lit(s) => s.clone(), Fmt::Point => ".".into(), Fmt::Comma => ",".into(), Fmt::Percent => "%".into(), _ => String::new() }).collect();
    };
    let point_at = tokens.iter().position(|t| matches!(t, Fmt::Point));
    let mut thousands = false;
    let mut scale = 0;
    let mut chain = true;
    for (k, token) in tokens.iter_mut().enumerate() {
        if *token == Fmt::Comma {
            if k < first || point_at.is_some_and(|p| k > p && k < last) { *token = Fmt::Lit(",".into()); }
            else if k < last { thousands = true; *token = Fmt::Lit(String::new()); }
            else if chain { scale += 1; *token = Fmt::Lit(String::new()); }
            else { *token = Fmt::Lit(",".into()); }
        } else if k > last && *token != Fmt::Point { chain = false; }
    }
    let percents = tokens.iter().filter(|t| **t == Fmt::Percent).count() as i32;
    let max_decimals = tokens.iter().filter(|t| matches!(t, Fmt::Dec(_))).count();
    let mut dec = Dec::of(x);
    dec.point += 2 * percents - 3 * scale;
    let mut dec = dec.round_to(max_decimals as i32);
    if dec.is_zero() { dec.neg = false; }
    let (int, mut frac) = dec.parts(max_decimals);
    // decimals: `#` drops its trailing zero, `?` pads with a space
    let decimals: Vec<char> = tokens.iter().filter_map(|t| if let Fmt::Dec(c) = t { Some(*c) } else { None }).collect();
    let mut frac_chars: Vec<char> = frac.chars().collect();
    let mut keep = frac_chars.len();
    while keep > 0 && decimals[keep - 1] != '0' && frac_chars[keep - 1] == '0' { keep -= 1; }
    for k in keep..frac_chars.len() { frac_chars[k] = if decimals[k] == '?' { ' ' } else { '\0' }; }
    frac = frac_chars.iter().filter(|c| **c != '\0').collect();
    let mut out = String::new();
    if signed && dec.neg { out.push('-'); }
    let int_tokens: Vec<char> = tokens.iter().filter_map(|t| if let Fmt::Int(c) = t { Some(*c) } else { None }).collect();
    let mut int_text = String::new();
    // digits go to the placeholders from the right; more digits than placeholders go to the left-most one
    let digits: Vec<char> = int.chars().collect();
    let mut assigned: Vec<String> = vec![String::new(); int_tokens.len()];
    let mut d = digits.len();
    for (slot, c) in int_tokens.iter().enumerate().rev() {
        if d > 0 { d -= 1; assigned[slot] = digits[d].to_string(); } else { assigned[slot] = match c { '0' => "0".into(), '?' => " ".into(), _ => String::new() }; }
    }
    if d > 0 {
        let extra: String = digits[..d].iter().collect();
        if let Some(a) = assigned.first_mut() { *a = format!("{extra}{a}"); } else { int_text = extra; }
    }
    if thousands {
        let run: String = assigned.concat();
        let plain: String = run.chars().filter(char::is_ascii_digit).collect();
        let pad: String = run.chars().take_while(|c| *c == ' ').collect();
        assigned = vec![format!("{pad}{}", group3(&plain))];
        assigned.resize(int_tokens.len().max(1), String::new());
    }
    let mut slot = 0;
    let mut emitted_int_text = false;
    for t in &tokens {
        match t {
            Fmt::Lit(s) => out.push_str(s),
            Fmt::Int(_) => { out.push_str(assigned.get(slot).map_or("", String::as_str)); slot += 1; }
            Fmt::Point => {
                if !emitted_int_text && int_tokens.is_empty() { out.push_str(&int_text); emitted_int_text = true; }
                out.push('.');
            }
            Fmt::Dec(_) => {}
            Fmt::Percent => out.push('%'),
            Fmt::Comma | Fmt::Date(_) => {}
        }
        if let Fmt::Point = t { out.push_str(&frac); }
    }
    out
}
