//! Data Merge computed columns: a small expression language (Excel-like) that turns the columns of a data source into more fields.
//! Rules: `artifacts/dbmerge-20261119/DESIGN.ko.md` section 2 ("계산 열 식") and `artifacts/mergeexpr-c1-20261023/STATUS.ko.md`.
//!
//! The language is deterministic: no clock, no randomness, no file or network access, no loops. An expression is parsed once
//! (`compile`, errors carry the character position) and evaluated for every record (`evaluate`, `apply`).
//! - Columns are `{name}` (spaces and Korean are fine; `{@name}` / `{#name}` pick the image / QR code column of that name), texts
//!   `"..."` (`""` is a quote), numbers as they are; operators `&` `+ - * /` `= <> < > <= >=` (`!=` too), unary `-`.
//! - A column's value keeps its text (`{코드}` of `007` joins as `007`) and turns into a number or a date only where one is needed
//!   (arithmetic, comparison with a number, `TEXT`, `YEAR`...), as a cell of a spreadsheet would: `{재고} > 0`, `{가격} * 2`.
//!   Dates are read as `yyyy-mm-dd`, `yyyy/mm/dd`, `yyyy.mm.dd` or `yyyy년 m월 d일` and join as `yyyy-mm-dd`.
//! - Texts compare without regard to case (as a spreadsheet does). A blank is 0 in arithmetic and `""` in a text.
//! - Deviations from Excel, on purpose: `IF(c, a)` is blank (not FALSE) when `c` is false; `VALUE`, `TEXT`, `YEAR`, `MONTH` and `DAY`
//!   of a blank are blank; `TEXT` of something that is not a number (or a date) is an error, not the text as it is.
//!   A record whose expression fails gets an empty cell and an error (`RecordError`); the other records are not touched.
use crate::datamerge::{Field, Source};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;

mod eval;
mod lexer;
mod parser;

pub use eval::{Evaluated, evaluate, parse_date, parse_number};
pub use parser::{Compiled, compile};

use eval::{RegexCache, evaluate_with};

pub const MAX_EXPR_CHARS: usize = 4000;
pub const MAX_COMPUTED_FIELDS: usize = 64;
/// The record errors `apply` lists (the number of all of them is `error_count`).
pub const MAX_RECORD_ERRORS: usize = 500;
const MAX_DEPTH: usize = 48;
const MAX_TEXT_BYTES: usize = 4 << 20;

pub use folio_model::datamerge::ComputedField;

/// Where and why an expression is wrong. `pos` and `end` are character positions (0-based, `end` is exclusive) in the expression;
/// `code` is stable (the screen translates it from `code` and `args`), `message` is the English text.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExprError {
    pub code: &'static str,
    pub pos: usize,
    pub end: usize,
    pub args: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionError {
    pub field: String,
    #[serde(flatten)]
    pub error: ExprError,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordError {
    /// 1-based record number.
    pub record: usize,
    pub field: String,
    #[serde(flatten)]
    pub error: ExprError,
}

fn err(code: &'static str, pos: usize, end: usize, args: Vec<String>) -> ExprError {
    ExprError { code, pos, end: end.max(pos), message: message(code, &args), args }
}

fn message(code: &str, a: &[String]) -> String {
    let g = |i: usize| a.get(i).map(String::as_str).unwrap_or("");
    match code {
        "empty" => "The expression is empty".into(),
        "tooLong" => format!("The expression is longer than {MAX_EXPR_CHARS} characters"),
        "tooDeep" => "The expression is nested too deeply".into(),
        "unexpectedChar" => format!("Unexpected character {}", g(0)),
        "unterminatedString" => "A text in quotes is not closed (a closing \" is missing)".into(),
        "unterminatedField" => "A field name in braces is not closed (a closing } is missing)".into(),
        "emptyField" => "A field name in braces is empty".into(),
        "badNumber" => format!("{} is not a valid number", g(0)),
        "unexpectedToken" => format!("Unexpected {}", g(0)),
        "unexpectedEnd" => format!("The expression ends too early: {} is missing", g(0)),
        "unknownFunction" => format!("Unknown function {}", g(0)),
        "unknownName" => format!("Unknown name {0}: put a field name in braces, {{{0}}}, and a text in quotes, \"{0}\"", g(0)),
        "unknownField" => format!("The data source has no field {}", g(0)),
        "laterField" => format!("The computed field {} is defined below this one; a computed field can only use the fields above it", g(0)),
        "argCount" => format!("{} takes {} arguments, not {}", g(0), g(1), g(2)),
        "badName" => format!("{}: a field name cannot be empty or contain braces or control characters", g(0)),
        "duplicateName" => format!("{}: the data source or another computed field already has a field with this name", g(0)),
        "tooManyFields" => format!("At most {MAX_COMPUTED_FIELDS} computed fields"),
        "notNumber" => format!("{} is not a number", g(0)),
        "notDate" => format!("{} is not a date (write yyyy-mm-dd, yyyy/mm/dd or yyyy.mm.dd)", g(0)),
        "notBool" => format!("{} is not TRUE or FALSE", g(0)),
        "divideByZero" => "Division by zero".into(),
        "overflow" => "The number is too large".into(),
        "badArgument" => format!("{}: {}", g(0), g(1)),
        "badRegex" => format!("The pattern is not a valid regular expression: {}", g(0)),
        "noMatch" => "None of the conditions of IFS is true".into(),
        "tooLongResult" => "The result is too long".into(),
        other => other.to_owned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Func { Concat, TextJoin, Left, Right, Mid, Len, Upper, Lower, Trim, Substitute, RegexReplace, SplitPart, If, Ifs, And, Or, Not, IsBlank, IfBlank, Value, Round, Int, Abs, Text, Year, Month, Day }

struct Def { name: &'static str, func: Func, min: usize, max: usize, syntax: &'static str }

const MANY: usize = usize::MAX;
const FUNCS: &[Def] = &[
    Def { name: "CONCAT", func: Func::Concat, min: 1, max: MANY, syntax: "CONCAT(text1, text2, ...)" },
    Def { name: "TEXTJOIN", func: Func::TextJoin, min: 3, max: MANY, syntax: "TEXTJOIN(delimiter, ignore_empty, text1, text2, ...)" },
    Def { name: "LEFT", func: Func::Left, min: 1, max: 2, syntax: "LEFT(text, count)" },
    Def { name: "RIGHT", func: Func::Right, min: 1, max: 2, syntax: "RIGHT(text, count)" },
    Def { name: "MID", func: Func::Mid, min: 3, max: 3, syntax: "MID(text, start, count)" },
    Def { name: "LEN", func: Func::Len, min: 1, max: 1, syntax: "LEN(text)" },
    Def { name: "UPPER", func: Func::Upper, min: 1, max: 1, syntax: "UPPER(text)" },
    Def { name: "LOWER", func: Func::Lower, min: 1, max: 1, syntax: "LOWER(text)" },
    Def { name: "TRIM", func: Func::Trim, min: 1, max: 1, syntax: "TRIM(text)" },
    Def { name: "SUBSTITUTE", func: Func::Substitute, min: 3, max: 4, syntax: "SUBSTITUTE(text, old, new, occurrence)" },
    Def { name: "REGEXREPLACE", func: Func::RegexReplace, min: 3, max: 3, syntax: "REGEXREPLACE(text, pattern, replacement)" },
    Def { name: "SPLITPART", func: Func::SplitPart, min: 3, max: 3, syntax: "SPLITPART(text, delimiter, n)" },
    Def { name: "IF", func: Func::If, min: 2, max: 3, syntax: "IF(condition, if_true, if_false)" },
    Def { name: "IFS", func: Func::Ifs, min: 2, max: MANY, syntax: "IFS(condition1, value1, condition2, value2, ...)" },
    Def { name: "AND", func: Func::And, min: 1, max: MANY, syntax: "AND(condition1, condition2, ...)" },
    Def { name: "OR", func: Func::Or, min: 1, max: MANY, syntax: "OR(condition1, condition2, ...)" },
    Def { name: "NOT", func: Func::Not, min: 1, max: 1, syntax: "NOT(condition)" },
    Def { name: "ISBLANK", func: Func::IsBlank, min: 1, max: 1, syntax: "ISBLANK(value)" },
    Def { name: "IFBLANK", func: Func::IfBlank, min: 2, max: 2, syntax: "IFBLANK(value, instead)" },
    Def { name: "VALUE", func: Func::Value, min: 1, max: 1, syntax: "VALUE(text)" },
    Def { name: "ROUND", func: Func::Round, min: 1, max: 2, syntax: "ROUND(number, digits)" },
    Def { name: "INT", func: Func::Int, min: 1, max: 1, syntax: "INT(number)" },
    Def { name: "ABS", func: Func::Abs, min: 1, max: 1, syntax: "ABS(number)" },
    Def { name: "TEXT", func: Func::Text, min: 2, max: 2, syntax: "TEXT(value, format)" },
    Def { name: "YEAR", func: Func::Year, min: 1, max: 1, syntax: "YEAR(date)" },
    Def { name: "MONTH", func: Func::Month, min: 1, max: 1, syntax: "MONTH(date)" },
    Def { name: "DAY", func: Func::Day, min: 1, max: 1, syntax: "DAY(date)" },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: &'static str,
    pub min_args: usize,
    /// `None`: any number of arguments.
    pub max_args: Option<usize>,
    pub syntax: &'static str,
}

/// The functions the language has (for a help list); the names are what is written in an expression, case does not matter.
pub fn functions() -> Vec<FunctionInfo> {
    FUNCS.iter().map(|d| FunctionInfo { name: d.name, min_args: d.min, max_args: (d.max != MANY).then_some(d.max), syntax: d.syntax }).collect()
}

// computed fields on a data source ------------------------------------------------------------------------------------

/// The field a computed field's name makes (`@name` image, `#name` QR code, else text).
pub fn field_of_name(name: &str) -> Result<Field, ExprError> {
    let (kind, base) = if let Some(b) = name.strip_prefix('@') { ("image", b) } else if let Some(b) = name.strip_prefix('#') { ("qrcode", b) } else { ("text", name) };
    let base = base.trim();
    if base.is_empty() || base.chars().any(|c| c.is_control() || c == '{' || c == '}') { return Err(err("badName", 0, 0, vec![name.to_owned()])); }
    Ok(Field { name: base.to_owned(), kind: kind.into() })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedField { pub computed: usize, pub column: usize }

/// A data source with its computed fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    /// The source with the computed fields after its own (a field whose definition is wrong is there with empty values; one
    /// whose name is taken, or not a name, is left out).
    pub source: Source,
    /// The computed fields that are in `source`: which of the given (`computed`, 0-based) and the column it is (`column`).
    pub added: Vec<AddedField>,
    pub definition_errors: Vec<DefinitionError>,
    /// The first `MAX_RECORD_ERRORS` of the records whose expression failed.
    pub record_errors: Vec<RecordError>,
    pub error_count: usize,
}

/// Adds the computed fields to the data source: every record gets the values of the expressions (in order, so one may use the
/// fields above it). A record whose expression fails gets an empty cell and an entry in `record_errors`.
pub fn apply(source: &Source, computed: &[ComputedField]) -> Applied {
    let mut fields = source.fields.clone();
    let mut definition_errors = Vec::new();
    let mut plan: Vec<(&ComputedField, Option<Compiled>)> = Vec::new();
    let mut added = Vec::new();
    for (n, cf) in computed.iter().enumerate() {
        if n >= MAX_COMPUTED_FIELDS {
            definition_errors.push(DefinitionError { field: cf.name.clone(), error: err("tooManyFields", 0, 0, vec![]) });
            continue;
        }
        let field = match field_of_name(&cf.name) {
            Ok(f) => f,
            Err(e) => { definition_errors.push(DefinitionError { field: cf.name.clone(), error: e }); continue; }
        };
        if fields.iter().any(|f| f.name == field.name && f.kind == field.kind) {
            definition_errors.push(DefinitionError { field: cf.name.clone(), error: err("duplicateName", 0, 0, vec![cf.name.clone()]) });
            continue;
        }
        let later: Vec<String> = computed[n + 1..].iter().filter_map(|c| field_of_name(&c.name).ok().map(|f| f.name)).collect();
        let compiled = match compile(&cf.expr, Some(&fields), &later) {
            Ok(c) => Some(c),
            Err(e) => { definition_errors.push(DefinitionError { field: cf.name.clone(), error: e }); None }
        };
        added.push(AddedField { computed: n, column: fields.len() });
        fields.push(field);
        plan.push((cf, compiled));
    }
    let regexes: RegexCache = RefCell::new(HashMap::new());
    let mut records = Vec::with_capacity(source.records.len());
    let mut record_errors = Vec::new();
    let mut error_count = 0;
    for (r, record) in source.records.iter().enumerate() {
        let mut row = record.clone();
        row.resize(source.fields.len(), String::new());
        for (cf, compiled) in &plan {
            let value = match compiled.as_ref().map(|c| evaluate_with(c, &row, &regexes)) {
                Some(Ok(v)) => v.text,
                Some(Err(e)) => {
                    error_count += 1;
                    if record_errors.len() < MAX_RECORD_ERRORS { record_errors.push(RecordError { record: r + 1, field: cf.name.clone(), error: e }); }
                    String::new()
                }
                None => String::new(),
            };
            row.push(value);
        }
        records.push(row);
    }
    Applied {
        source: Source { encoding: source.encoding.clone(), delimiter: source.delimiter.clone(), fields, records, options: source.options },
        added, definition_errors, record_errors, error_count,
    }
}

/// The data source as a CSV file InDesign reads (the export of a document with computed fields: InDesign cannot compute them, so
/// the values go into the file). Comma separated (a header with a comma, or one column, is always read as comma separated), one
/// physical line per record (a line break inside a value becomes a space: InDesign reads physical lines), ASCII when every
/// character is, else UTF-16 little endian with a byte order mark. Returns the bytes and the `DataSourceFileType` for them (none
/// for UTF-16: InDesign writes none for such a file). `image_path` gives what is written for the value of an image field.
pub fn export_csv(source: &Source, image_path: &dyn Fn(&str) -> String) -> (Vec<u8>, Option<&'static str>) {
    fn cell(value: &str) -> String {
        let one_line: String = value.replace("\r\n", " ").chars().filter(|c| *c != '\0').map(|c| if matches!(c, '\r' | '\n') { ' ' } else { c }).collect();
        if one_line.contains([',', '"']) { format!("\"{}\"", one_line.replace('"', "\"\"")) } else { one_line }
    }
    let prefix = |f: &Field| match f.kind.as_str() { "image" => "@", "qrcode" => "#", _ => "" };
    let mut text = String::new();
    let header: Vec<String> = source.fields.iter().map(|f| cell(&format!("{}{}", prefix(f), f.name))).collect();
    text.push_str(&header.join(","));
    text.push_str("\r\n");
    for record in &source.records {
        let row: Vec<String> = source.fields.iter().enumerate().map(|(i, f)| {
            let value = record.get(i).map(String::as_str).unwrap_or("");
            cell(&if f.kind == "image" && !value.is_empty() { image_path(value) } else { value.to_owned() })
        }).collect();
        text.push_str(&row.join(","));
        text.push_str("\r\n");
    }
    if text.is_ascii() { return (text.into_bytes(), Some("CommaSeparated")); }
    let mut bytes = vec![0xff, 0xfe];
    for unit in text.encode_utf16() { bytes.extend(unit.to_le_bytes()); }
    (bytes, None)
}
