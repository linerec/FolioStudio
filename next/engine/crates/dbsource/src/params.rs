//! Query parameters: the value given for each, else its default; relative values (`today`, `thisMonth`, `lastMonth`,
//! `nextMonth`, `monthOffset:±n`) are worked out from the "today" the caller gives, so the result is the same on every run.
use crate::error::{DbError, Result};
use crate::model::{Param, ParamType};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date { pub y: i32, pub m: u32, pub d: u32 }

fn days_in(y: i32, m: u32) -> u32 {
    match m { 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, 4 | 6 | 9 | 11 => 30, _ => if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 } }
}

impl Date {
    pub fn parse(s: &str) -> Option<Date> {
        let mut it = s.trim().splitn(3, '-');
        let y: i32 = it.next()?.parse().ok()?;
        let m: u32 = it.next()?.parse().ok()?;
        let d: u32 = it.next()?.parse().ok()?;
        (s.trim().len() == 10 && (1..=12).contains(&m) && d >= 1 && d <= days_in(y, m) && (1..=9999).contains(&y)).then_some(Date { y, m, d })
    }
    pub fn first_of_month(self) -> Date { Date { d: 1, ..self } }
    /// The same day `n` months later (the last day of the month when that month is shorter).
    pub fn add_months(self, n: i32) -> Date {
        let total = self.y * 12 + self.m as i32 - 1 + n;
        let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
        Date { y, m, d: self.d.min(days_in(y, m)) }
    }
    pub fn next_day(self) -> Date {
        if self.d < days_in(self.y, self.m) { Date { d: self.d + 1, ..self } } else { Date { d: 1, ..self }.add_months(1) }
    }
    /// Today from the system clock (UTC): only when the caller gives no "today".
    pub fn today_utc() -> Date {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        // days since 1970-01-01 to a civil date (Howard Hinnant's algorithm)
        let z = secs.div_euclid(86_400) + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = (yoe + era * 400 + i64::from(m <= 2)) as i32;
        Date { y, m, d }
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{:04}-{:02}-{:02}", self.y, self.m, self.d) }
}

/// A month: `YYYY-MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Month { pub y: i32, pub m: u32 }

impl Month {
    pub fn parse(s: &str) -> Option<Month> {
        let (y, m) = s.trim().split_once('-')?;
        let (y, m): (i32, u32) = (y.parse().ok()?, m.parse().ok()?);
        (s.trim().len() == 7 && (1..=12).contains(&m) && (1..=9999).contains(&y)).then_some(Month { y, m })
    }
    pub fn first(self) -> Date { Date { y: self.y, m: self.m, d: 1 } }
    pub fn of(d: Date) -> Month { Month { y: d.y, m: d.m } }
}

impl std::fmt::Display for Month {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{:04}-{:02}", self.y, self.m) }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    Text(String),
    /// The number as it was written (bound as text: the database reads it in the column's type).
    Number(String),
    Date(Date),
    Month(Month),
    List(Vec<String>),
}

impl Resolved {
    pub fn json(&self) -> Value {
        match self {
            Resolved::Text(s) | Resolved::Number(s) => json!(s),
            Resolved::Date(d) => json!(d.to_string()),
            Resolved::Month(m) => json!(m.to_string()),
            Resolved::List(l) => json!(l),
        }
    }
}

/// `today`, `thisMonth`, `lastMonth`, `nextMonth`, `monthOffset:±n` (None when `text` is none of them).
fn relative(text: &str, today: Date) -> Option<Result<Date>> {
    Some(Ok(match text {
        "today" => today,
        "thisMonth" => today.first_of_month(),
        "lastMonth" => today.first_of_month().add_months(-1),
        "nextMonth" => today.first_of_month().add_months(1),
        other => {
            let n = other.strip_prefix("monthOffset:")?;
            match n.trim().trim_start_matches('+').parse::<i32>() {
                Ok(n) if n.abs() <= 1200 => today.first_of_month().add_months(n),
                _ => return Some(Err(DbError::invalid(format!("monthOffset needs a whole number of months, not {n}")))),
            }
        }
    }))
}

fn scalar(v: &Value) -> Option<String> {
    match v { Value::String(s) => Some(s.clone()), Value::Number(n) => Some(n.to_string()), Value::Bool(b) => Some(b.to_string()), _ => None }
}

fn one(p: &Param, v: &Value, today: Date) -> Result<Resolved> {
    let bad = |what: &str| DbError::invalid(format!("parameter {}: {what}", p.name));
    Ok(match p.kind {
        ParamType::Text => Resolved::Text(scalar(v).ok_or_else(|| bad("needs a text"))?),
        ParamType::Number => {
            let s = scalar(v).ok_or_else(|| bad("needs a number"))?;
            let t = s.trim();
            if t.parse::<f64>().map_or(true, |x| !x.is_finite()) { return Err(bad("needs a number")); }
            Resolved::Number(t.to_owned())
        }
        ParamType::Date => {
            let s = scalar(v).ok_or_else(|| bad("needs a date (YYYY-MM-DD)"))?;
            match relative(&s, today) { Some(d) => Resolved::Date(d?), None => Resolved::Date(Date::parse(&s).ok_or_else(|| bad("needs a date (YYYY-MM-DD)"))?) }
        }
        ParamType::Month => {
            let s = scalar(v).ok_or_else(|| bad("needs a month (YYYY-MM)"))?;
            match relative(&s, today) { Some(d) => Resolved::Month(Month::of(d?)), None => Resolved::Month(Month::parse(&s).ok_or_else(|| bad("needs a month (YYYY-MM)"))?) }
        }
        ParamType::List => match v {
            Value::Array(items) => Resolved::List(items.iter().map(|i| scalar(i).ok_or_else(|| bad("a list holds texts or numbers"))).collect::<Result<_>>()?),
            other => Resolved::List(scalar(other).ok_or_else(|| bad("needs a list"))?.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect()),
        },
    })
}

/// Every parameter's value: the one given (`given`), else its default; a parameter with neither is an error.
pub fn resolve(params: &[Param], given: &Map<String, Value>, today: Date) -> Result<BTreeMap<String, Resolved>> {
    let mut out = BTreeMap::new();
    for p in params {
        if p.name.is_empty() || p.name.chars().any(|c| !(c.is_alphanumeric() || c == '_')) {
            return Err(DbError::invalid(format!("parameter name {:?}: letters, digits and _ only", p.name)));
        }
        if out.contains_key(&p.name) { return Err(DbError::invalid(format!("parameter {} is defined twice", p.name))); }
        let value = given.get(&p.name).filter(|v| !v.is_null()).or(p.default.as_ref())
            .ok_or_else(|| DbError::invalid(format!("parameter {} has no value", p.name)))?;
        out.insert(p.name.clone(), one(p, value, today)?);
    }
    if let Some(unknown) = given.keys().find(|k| !params.iter().any(|p| &p.name == *k)) {
        return Err(DbError::invalid(format!("no parameter is called {unknown}")));
    }
    Ok(out)
}

/// The values as the document remembers them (`paramValues`).
pub fn values_json(resolved: &BTreeMap<String, Resolved>) -> Map<String, Value> {
    resolved.iter().map(|(k, v)| (k.clone(), v.json())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Param, ParamType};

    fn p(name: &str, kind: ParamType, default: Option<Value>) -> Param { Param { name: name.into(), kind, default } }
    const TODAY: Date = Date { y: 2026, m: 1, d: 31 };

    #[test]
    fn relative_months_from_the_given_today() {
        let params = [p("월", ParamType::Month, Some(json!("thisMonth"))), p("지난달", ParamType::Month, Some(json!("lastMonth"))),
            p("다음", ParamType::Month, Some(json!("nextMonth"))), p("뒤", ParamType::Month, Some(json!("monthOffset:-13"))),
            p("오늘", ParamType::Date, Some(json!("today"))), p("석달", ParamType::Date, Some(json!("monthOffset:+3")))];
        let r = resolve(&params, &Map::new(), TODAY).unwrap();
        assert_eq!(r["월"], Resolved::Month(Month { y: 2026, m: 1 }));
        assert_eq!(r["지난달"], Resolved::Month(Month { y: 2025, m: 12 }));
        assert_eq!(r["다음"], Resolved::Month(Month { y: 2026, m: 2 }));
        assert_eq!(r["뒤"], Resolved::Month(Month { y: 2024, m: 12 }));
        assert_eq!(r["오늘"], Resolved::Date(TODAY));
        assert_eq!(r["석달"], Resolved::Date(Date { y: 2026, m: 4, d: 1 }));
        assert_eq!(Date { y: 2024, m: 1, d: 31 }.add_months(1), Date { y: 2024, m: 2, d: 29 });
        assert_eq!(Date { y: 2026, m: 12, d: 31 }.next_day(), Date { y: 2027, m: 1, d: 1 });
    }

    #[test]
    fn given_values_win_and_are_checked() {
        let params = [p("월", ParamType::Month, Some(json!("thisMonth"))), p("최소", ParamType::Number, None), p("상태", ParamType::List, None)];
        let given: Map<String, Value> = serde_json::from_value(json!({"월": "2025-07", "최소": 12.5, "상태": ["게재", "확정"]})).unwrap();
        let r = resolve(&params, &given, TODAY).unwrap();
        assert_eq!(values_json(&r), serde_json::from_value::<Map<String, Value>>(json!({"월": "2025-07", "최소": "12.5", "상태": ["게재", "확정"]})).unwrap());
        for bad in [json!({"최소": "abc", "상태": []}), json!({"월": "2025-13", "최소": 1, "상태": []}), json!({"최소": 1, "상태": [], "없는것": 1})] {
            assert!(resolve(&params, &serde_json::from_value(bad.clone()).unwrap(), TODAY).is_err(), "{bad}");
        }
        assert_eq!(resolve(&params, &Map::new(), TODAY).unwrap_err().kind, crate::Kind::Invalid);
    }
}
