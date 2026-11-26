//! Evaluates a parsed expression against a record.

mod format;

pub use format::{parse_date, parse_number};

use self::format::{Dec, EXCEL_EPOCH, civil_from_days, fmt_num, format_date, format_number, is_date_format, iso, serial};
use super::parser::{Compiled, Kind, Node, Op};
use super::{ExprError, FUNCS, Func, MAX_TEXT_BYTES, err};
use regex::RegexBuilder;
use serde::Serialize;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum V {
    Empty,
    /// The text of a field of the data source: a number or a date where one is needed, text as it is otherwise.
    Raw(String),
    /// A text written in the expression or made by a function: never read as a number by a comparison.
    Text(String),
    Num(f64),
    Bool(bool),
    /// Days since 1970-01-01.
    Date(i64),
}

/// What an expression gave for a record: the text the field gets and what it was.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evaluated {
    pub text: String,
    /// `empty`, `text`, `number`, `date` or `bool`.
    pub kind: &'static str,
}

struct Rt { code: &'static str, args: Vec<String>, pos: usize, end: usize }
type R<T> = Result<T, Rt>;

fn rt(code: &'static str, n: &Node, args: Vec<String>) -> Rt { Rt { code, args, pos: n.pos, end: n.end } }

impl From<Rt> for ExprError {
    fn from(r: Rt) -> ExprError { err(r.code, r.pos, r.end, r.args) }
}

pub(super) type RegexCache = RefCell<HashMap<String, Result<regex::Regex, String>>>;

struct Cx<'a> { values: &'a [String], regexes: &'a RegexCache }

fn to_text(v: &V) -> String {
    match v {
        V::Empty => String::new(),
        V::Raw(s) | V::Text(s) => s.clone(),
        V::Num(x) => fmt_num(*x),
        V::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        V::Date(d) => iso(*d),
    }
}

fn kind_of(v: &V) -> &'static str {
    match v { V::Empty => "empty", V::Raw(_) | V::Text(_) => "text", V::Num(_) => "number", V::Bool(_) => "bool", V::Date(_) => "date" }
}

fn is_blank(v: &V) -> bool {
    match v { V::Empty => true, V::Raw(s) | V::Text(s) => s.trim().is_empty(), _ => false }
}

/// What a value is in arithmetic: a number, or a date (a date plus or minus days is a date).
#[derive(Debug, Clone, Copy)]
enum Ar { Num(f64), Date(i64) }

fn ar_f64(a: Ar) -> f64 { match a { Ar::Num(x) => x, Ar::Date(d) => serial(d) } }

fn arith(v: &V, n: &Node) -> R<Ar> {
    match v {
        V::Empty => Ok(Ar::Num(0.0)),
        V::Num(x) => Ok(Ar::Num(*x)),
        V::Bool(b) => Ok(Ar::Num(if *b { 1.0 } else { 0.0 })),
        V::Date(d) => Ok(Ar::Date(*d)),
        V::Raw(s) | V::Text(s) => {
            if s.trim().is_empty() { return Ok(Ar::Num(0.0)); }
            if let Some(x) = parse_number(s) { Ok(Ar::Num(x)) }
            else if let Some(d) = parse_date(s) { Ok(Ar::Date(d)) }
            else { Err(rt("notNumber", n, vec![shown(s)])) }
        }
    }
}

fn shown(s: &str) -> String {
    let mut t: String = s.chars().take(40).collect();
    if s.chars().count() > 40 { t.push('…'); }
    format!("\"{t}\"")
}

fn number(v: &V, n: &Node) -> R<f64> { Ok(ar_f64(arith(v, n)?)) }

fn finite(x: f64, n: &Node) -> R<V> { if x.is_finite() { Ok(V::Num(x)) } else { Err(rt("overflow", n, vec![])) } }

fn truthy(v: &V, n: &Node) -> R<bool> {
    match v {
        V::Bool(b) => Ok(*b),
        V::Num(x) => Ok(*x != 0.0),
        V::Date(_) => Ok(true),
        V::Empty => Ok(false),
        V::Raw(s) | V::Text(s) => {
            let t = s.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("false") { Ok(false) }
            else if t.eq_ignore_ascii_case("true") { Ok(true) }
            else if let Some(x) = parse_number(t) { Ok(x != 0.0) }
            else { Err(rt("notBool", n, vec![shown(s)])) }
        }
    }
}

/// A value as a date for `YEAR`, `TEXT`...: `None` for a blank.
fn date_of(v: &V, n: &Node) -> R<Option<i64>> {
    match v {
        V::Date(d) => Ok(Some(*d)),
        V::Num(x) => Ok(Some(x.floor() as i64 - EXCEL_EPOCH as i64)),
        V::Bool(_) => Err(rt("notDate", n, vec![to_text(v)])),
        V::Empty => Ok(None),
        V::Raw(s) | V::Text(s) => {
            if s.trim().is_empty() { return Ok(None); }
            if let Some(d) = parse_date(s) { return Ok(Some(d)); }
            if let Some(x) = parse_number(s) { return Ok(Some(x.floor() as i64 - EXCEL_EPOCH as i64)); }
            Err(rt("notDate", n, vec![shown(s)]))
        }
    }
}

// comparison -----------------------------------------------------------------------------------------------------------

enum Cv { Blank, Num(f64), Date(i64), Text(String), Bool(bool) }

/// `strict`: the other side is a text written in the expression, so a field is compared as the text it is.
fn cv(v: &V, strict: bool) -> Cv {
    match v {
        V::Empty => Cv::Blank,
        V::Raw(s) => {
            if s.trim().is_empty() { Cv::Blank }
            else if strict { Cv::Text(s.clone()) }
            else if let Some(x) = parse_number(s) { Cv::Num(x) }
            else if let Some(d) = parse_date(s) { Cv::Date(d) }
            else { Cv::Text(s.clone()) }
        }
        V::Text(s) => Cv::Text(s.clone()),
        V::Num(x) => Cv::Num(*x),
        V::Bool(b) => Cv::Bool(*b),
        V::Date(d) => Cv::Date(*d),
    }
}

fn rank(c: &Cv) -> u8 { match c { Cv::Blank | Cv::Num(_) | Cv::Date(_) => 0, Cv::Text(_) => 1, Cv::Bool(_) => 2 } }

fn value_of(c: &Cv) -> f64 { match c { Cv::Num(x) => *x, Cv::Date(d) => serial(*d), _ => 0.0 } }

fn order(a: &Cv, b: &Cv) -> Ordering {
    match (a, b) {
        (Cv::Blank, Cv::Blank) => Ordering::Equal,
        (Cv::Blank, Cv::Text(t)) => "".cmp(t.to_lowercase().as_str()),
        (Cv::Text(t), Cv::Blank) => t.to_lowercase().as_str().cmp(""),
        (Cv::Blank, Cv::Bool(x)) => false.cmp(x),
        (Cv::Bool(x), Cv::Blank) => x.cmp(&false),
        (Cv::Text(x), Cv::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Cv::Bool(x), Cv::Bool(y)) => x.cmp(y),
        _ if rank(a) != rank(b) => rank(a).cmp(&rank(b)),
        _ => value_of(a).partial_cmp(&value_of(b)).unwrap_or(Ordering::Equal),
    }
}

fn compare(op: Op, a: &V, b: &V) -> bool {
    let strict_a = matches!(b, V::Text(_));
    let strict_b = matches!(a, V::Text(_));
    let o = order(&cv(a, strict_a), &cv(b, strict_b));
    match op { Op::Eq => o == Ordering::Equal, Op::Ne => o != Ordering::Equal, Op::Lt => o == Ordering::Less, Op::Gt => o == Ordering::Greater, Op::Le => o != Ordering::Greater, _ => o != Ordering::Less }
}

// evaluation -----------------------------------------------------------------------------------------------------------

fn check_len(s: String, n: &Node) -> R<V> {
    if s.len() > MAX_TEXT_BYTES { Err(rt("tooLongResult", n, vec![])) } else { Ok(V::Text(s)) }
}

fn eval(n: &Node, cx: &Cx<'_>) -> R<V> {
    match &n.kind {
        Kind::Num(x) => Ok(V::Num(*x)),
        Kind::Text(s) => Ok(V::Text(s.clone())),
        Kind::Bool(b) => Ok(V::Bool(*b)),
        Kind::Field(i) => {
            let s = cx.values.get(*i).map(String::as_str).unwrap_or("");
            Ok(if s.is_empty() { V::Empty } else { V::Raw(s.to_owned()) })
        }
        Kind::Neg(a) => {
            let v = eval(a, cx)?;
            finite(-number(&v, a)?, n)
        }
        Kind::Bin(op, a, b) => {
            let (va, vb) = (eval(a, cx)?, eval(b, cx)?);
            match op {
                Op::Concat => check_len(to_text(&va) + &to_text(&vb), n),
                Op::Eq | Op::Ne | Op::Lt | Op::Gt | Op::Le | Op::Ge => Ok(V::Bool(compare(*op, &va, &vb))),
                _ => {
                    let (x, y) = (arith(&va, a)?, arith(&vb, b)?);
                    match (op, x, y) {
                        (Op::Add, Ar::Date(d), Ar::Num(k)) | (Op::Add, Ar::Num(k), Ar::Date(d)) => Ok(V::Date(d + k.floor() as i64)),
                        (Op::Sub, Ar::Date(d), Ar::Num(k)) => Ok(V::Date(d - k.floor() as i64)),
                        (Op::Sub, Ar::Date(d), Ar::Date(e)) => Ok(V::Num((d - e) as f64)),
                        _ => {
                            let (x, y) = (ar_f64(x), ar_f64(y));
                            match op {
                                Op::Add => finite(x + y, n),
                                Op::Sub => finite(x - y, n),
                                Op::Mul => finite(x * y, n),
                                _ => if y == 0.0 { Err(rt("divideByZero", n, vec![])) } else { finite(x / y, n) },
                            }
                        }
                    }
                }
            }
        }
        Kind::Call(f, args) => call(*f, args, n, cx),
    }
}

/// A count or position argument: a whole number from `min` (an error says which function and what).
fn whole(v: &V, node: &Node, func: &str, what: &str, min: i64) -> R<i64> {
    let x = number(v, node)?.floor();
    if x < min as f64 { return Err(rt("badArgument", node, vec![func.into(), format!("{what} must be {min} or more")])); }
    Ok(x.min(1e9) as i64)
}

fn call(f: Func, args: &[Node], n: &Node, cx: &Cx<'_>) -> R<V> {
    let arg = |i: usize| eval(&args[i], cx);
    let text = |i: usize| -> R<String> { Ok(to_text(&arg(i)?)) };
    let name = FUNCS.iter().find(|d| d.func == f).map_or("", |d| d.name);
    match f {
        Func::Concat => {
            let mut out = String::new();
            for i in 0..args.len() { out.push_str(&text(i)?); }
            check_len(out, n)
        }
        Func::TextJoin => {
            let delimiter = text(0)?;
            let skip = truthy(&arg(1)?, &args[1])?;
            let mut parts = Vec::new();
            for i in 2..args.len() {
                let s = text(i)?;
                if skip && s.is_empty() { continue; }
                parts.push(s);
            }
            check_len(parts.join(&delimiter), n)
        }
        Func::Left | Func::Right => {
            let s = text(0)?;
            let count = (if args.len() > 1 { whole(&arg(1)?, &args[1], name, "the count", 0)? } else { 1 }) as usize;
            let total = s.chars().count();
            let take = count.min(total);
            Ok(V::Text(if f == Func::Left { s.chars().take(take).collect() } else { s.chars().skip(total - take).collect() }))
        }
        Func::Mid => {
            let s = text(0)?;
            let start = whole(&arg(1)?, &args[1], name, "the start", 1)? as usize;
            let count = whole(&arg(2)?, &args[2], name, "the count", 0)? as usize;
            Ok(V::Text(s.chars().skip(start - 1).take(count).collect()))
        }
        Func::Len => Ok(V::Num(text(0)?.chars().count() as f64)),
        Func::Upper => Ok(V::Text(text(0)?.to_uppercase())),
        Func::Lower => Ok(V::Text(text(0)?.to_lowercase())),
        Func::Trim => Ok(V::Text(text(0)?.split_whitespace().collect::<Vec<_>>().join(" "))),
        Func::Substitute => {
            let (s, old, new) = (text(0)?, text(1)?, text(2)?);
            if old.is_empty() { return Ok(V::Text(s)); }
            if args.len() == 4 {
                let which = whole(&arg(3)?, &args[3], name, "the occurrence", 1)? as usize;
                return Ok(V::Text(match s.match_indices(old.as_str()).nth(which - 1) {
                    Some((at, _)) => format!("{}{new}{}", &s[..at], &s[at + old.len()..]),
                    None => s,
                }));
            }
            check_len(s.replace(old.as_str(), &new), n)
        }
        Func::RegexReplace => {
            let (s, pattern, replacement) = (text(0)?, text(1)?, text(2)?);
            let compiled = {
                let mut cache = cx.regexes.borrow_mut();
                cache.entry(pattern.clone()).or_insert_with(|| RegexBuilder::new(&pattern).size_limit(1 << 20).nest_limit(64).build().map_err(|e| e.to_string().lines().last().unwrap_or("").trim().to_owned())).clone()
            };
            match compiled {
                Ok(re) => check_len(re.replace_all(&s, replacement.as_str()).into_owned(), n),
                Err(e) => Err(rt("badRegex", &args[1], vec![e])),
            }
        }
        Func::SplitPart => {
            let (s, delimiter) = (text(0)?, text(1)?);
            let which = number(&arg(2)?, &args[2])?.floor() as i64;
            if which == 0 { return Err(rt("badArgument", &args[2], vec![name.into(), "n counts from 1 (or from the end with a negative number)".into()])); }
            let parts: Vec<&str> = if delimiter.is_empty() { vec![s.as_str()] } else { s.split(delimiter.as_str()).collect() };
            let index = if which > 0 { which as usize - 1 } else { parts.len().wrapping_sub(which.unsigned_abs() as usize) };
            Ok(parts.get(index).map_or(V::Empty, |p| V::Text((*p).to_owned())))
        }
        Func::If => {
            if truthy(&arg(0)?, &args[0])? { arg(1) } else if args.len() > 2 { arg(2) } else { Ok(V::Empty) }
        }
        Func::Ifs => {
            if !args.len().is_multiple_of(2) { return Err(rt("badArgument", n, vec![name.into(), "give a value for every condition".into()])); }
            for k in 0..args.len() / 2 {
                if truthy(&arg(2 * k)?, &args[2 * k])? { return arg(2 * k + 1); }
            }
            Err(rt("noMatch", n, vec![]))
        }
        Func::And => {
            for node in args {
                if !truthy(&eval(node, cx)?, node)? {
                    return Ok(V::Bool(false));
                }
            }
            Ok(V::Bool(true))
        }
        Func::Or => {
            for node in args {
                if truthy(&eval(node, cx)?, node)? {
                    return Ok(V::Bool(true));
                }
            }
            Ok(V::Bool(false))
        }
        Func::Not => Ok(V::Bool(!truthy(&arg(0)?, &args[0])?)),
        Func::IsBlank => Ok(V::Bool(is_blank(&arg(0)?))),
        Func::IfBlank => { let v = arg(0)?; if is_blank(&v) { arg(1) } else { Ok(v) } }
        Func::Value => {
            let v = arg(0)?;
            if is_blank(&v) { return Ok(V::Empty); }
            Ok(V::Num(number(&v, &args[0])?))
        }
        Func::Round => {
            let x = number(&arg(0)?, &args[0])?;
            let digits = if args.len() > 1 { number(&arg(1)?, &args[1])?.trunc() } else { 0.0 };
            if !(-15.0..=15.0).contains(&digits) { return Err(rt("badArgument", &args[1], vec![name.into(), "the digits must be from -15 to 15".into()])); }
            finite(Dec::of(x).round_to(digits as i32).to_f64(), n)
        }
        Func::Int => finite(number(&arg(0)?, &args[0])?.floor(), n),
        Func::Abs => finite(number(&arg(0)?, &args[0])?.abs(), n),
        Func::Year | Func::Month | Func::Day => {
            let Some(days) = date_of(&arg(0)?, &args[0])? else { return Ok(V::Empty) };
            let (y, m, d) = civil_from_days(days);
            Ok(V::Num(match f { Func::Year => y, Func::Month => m, _ => d } as f64))
        }
        Func::Text => {
            let v = arg(0)?;
            let format = text(1)?;
            if is_blank(&v) { return Ok(V::Text(String::new())); }
            if is_date_format(&format) {
                let Some(days) = date_of(&v, &args[0])? else { return Ok(V::Text(String::new())) };
                return Ok(V::Text(format_date(days, &format)));
            }
            let x = number(&v, &args[0])?;
            Ok(V::Text(format_number(x, &format)))
        }
    }
}

pub(super) fn evaluate_with(c: &Compiled, values: &[String], regexes: &RegexCache) -> Result<Evaluated, ExprError> {
    let v = eval(&c.root, &Cx { values, regexes })?;
    Ok(Evaluated { text: to_text(&v), kind: kind_of(&v) })
}

/// The value of an expression for one record (`values` are the record's fields in the order `compile` was given them).
pub fn evaluate(c: &Compiled, values: &[String]) -> Result<Evaluated, ExprError> {
    evaluate_with(c, values, &RefCell::new(HashMap::new()))
}
