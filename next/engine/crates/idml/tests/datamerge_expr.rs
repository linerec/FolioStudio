//! Data Merge computed columns (`folio_idml::datamerge_expr`): the language, the conversions, the formats, the error positions
//! and what `apply` adds to a data source.
use folio_idml::datamerge::{parse, Source};
use folio_idml::datamerge_expr::*;

fn utf16(text: &str) -> Vec<u8> {
    let mut bytes = vec![255, 254];
    for word in text.encode_utf16() { bytes.extend(word.to_le_bytes()); }
    bytes
}

/// A data source from rows of a table (the first row is the header; Korean needs UTF-16 as a real file does).
fn source(rows: &[&[&str]]) -> Source {
    let text: String = rows.iter().map(|r| r.join("\t") + "\r\n").collect();
    parse(&utf16(&text)).unwrap()
}

/// The value of `expr` for the first record of a table.
fn value(expr: &str, header: &[&str], row: &[&str]) -> String {
    // a last column that is never empty keeps a record of blanks from being an empty line, and the tab as the delimiter
    let (header, row): (Vec<&str>, Vec<&str>) = (header.iter().copied().chain(["_"]).collect(), row.iter().copied().chain(["."]).collect());
    let s = source(&[&header, &row]);
    let c = compile(expr, Some(&s.fields), &[]).unwrap_or_else(|e| panic!("{expr}: {e:?}"));
    evaluate(&c, &s.records[0]).unwrap_or_else(|e| panic!("{expr}: {e:?}")).text
}

fn plain(expr: &str) -> String { value(expr, &["x"], &["x"]) }

fn compile_error(expr: &str, header: &[&str]) -> ExprError {
    let s = source(&[header, &vec!["1"; header.len()]]);
    compile(expr, Some(&s.fields), &[]).unwrap_err()
}

fn run_error(expr: &str, header: &[&str], row: &[&str]) -> ExprError {
    let (header, row): (Vec<&str>, Vec<&str>) = (header.iter().copied().chain(["_"]).collect(), row.iter().copied().chain(["."]).collect());
    let s = source(&[&header, &row]);
    let c = compile(expr, Some(&s.fields), &[]).unwrap();
    evaluate(&c, &s.records[0]).unwrap_err()
}

#[test]
fn columns_texts_and_joining_with_korean_names() {
    assert_eq!(value("{성} & \" \" & {이름}", &["성", "이름"], &["김", "철수"]), "김 철수");
    assert_eq!(value("{ 성 }&{이름}", &["성", "이름"], &["김", "철수"]), "김철수");
    assert_eq!(plain("\"He said \"\"hi\"\"\""), "He said \"hi\"");
    // the text of a column is kept as it is: a code with leading zeros joins as it is
    assert_eq!(value("{코드} & \"x\"", &["코드"], &["007"]), "007x");
    assert_eq!(value("{코드} + 0", &["코드"], &["007"]), "7");
    // a column named in other case is found; the exact name wins
    assert_eq!(value("{Name}", &["name"], &["a"]), "a");
    assert_eq!(value("{Name}", &["name", "Name"], &["a", "b"]), "b");
    // an image field: the name without its @
    assert_eq!(value("{사진}", &["@사진"], &["a.jpg"]), "a.jpg");
    assert_eq!(value("{@사진}", &["사진", "@사진"], &["t", "a.jpg"]), "a.jpg");
}

#[test]
fn numbers_and_arithmetic() {
    assert_eq!(value("{가격} * 2 + 1", &["가격"], &["15,000"]), "30001");
    assert_eq!(value("{a} / {b}", &["a", "b"], &["1", "3"]), "0.333333333333333");
    assert_eq!(plain("0.1 + 0.2"), "0.3");
    assert_eq!(plain("-(2 + 3) * 2"), "-10");
    assert_eq!(plain("2 + 3 * 4"), "14");
    assert_eq!(plain("10 - 4 - 3"), "3");
    assert_eq!(plain("1.5e3"), "1500");
    assert_eq!(plain("\"5\" + 1"), "6");
    assert_eq!(plain("TRUE + 1"), "2");
    assert_eq!(value("{a} + 1", &["a"], &["12%"]), "1.12");
    assert_eq!(value("{빈} + 1", &["빈", "x"], &["", "y"]), "1");
    assert_eq!(plain("123456789012345678"), "1.23456789012346E+17");
    assert_eq!(plain("0.000000001234"), "0.000000001234");
    assert_eq!(plain("0.0000000001234"), "1.234E-10");
}

#[test]
fn comparisons_follow_a_spreadsheet() {
    assert_eq!(value("{재고} > 0", &["재고"], &["5"]), "TRUE");
    assert_eq!(value("{재고} > 0", &["재고"], &["0"]), "FALSE");
    assert_eq!(value("{재고} > 0", &["재고"], &[""]), "FALSE");
    assert_eq!(value("{a} = {b}", &["a", "b"], &["10", "10.0"]), "TRUE");
    assert_eq!(value("{a} < {b}", &["a", "b"], &["9", "10"]), "TRUE");
    // texts compare without regard to case; a text written in the expression is compared as text
    assert_eq!(value("{a} = \"abc\"", &["a"], &["ABC"]), "TRUE");
    assert_eq!(value("{a} = \"10\"", &["a"], &["10.0"]), "FALSE");
    assert_eq!(value("{a} = 10", &["a"], &["10.0"]), "TRUE");
    assert_eq!(value("{a} <> \"x\"", &["a"], &["y"]), "TRUE");
    assert_eq!(value("{a} != \"y\"", &["a"], &["y"]), "FALSE");
    // a text is more than any number, as in a spreadsheet
    assert_eq!(value("{a} > 100", &["a"], &["품절"]), "TRUE");
    assert_eq!(value("{a} = \"\"", &["a"], &[""]), "TRUE");
    assert_eq!(value("{a} <= {b}", &["a", "b"], &["2026-11-19", "2026/11/20"]), "TRUE");
    assert_eq!(value("{a} >= 5", &["a"], &["5"]), "TRUE");
}

#[test]
fn text_functions() {
    assert_eq!(plain("CONCAT(\"a\", 1, TRUE)"), "a1TRUE");
    assert_eq!(plain("TEXTJOIN(\", \", TRUE, \"a\", \"\", \"b\")"), "a, b");
    assert_eq!(plain("TEXTJOIN(\"-\", FALSE, \"a\", \"\", \"b\")"), "a--b");
    assert_eq!(plain("LEFT(\"가나다라\", 2)"), "가나");
    assert_eq!(plain("LEFT(\"가나다라\")"), "가");
    assert_eq!(plain("RIGHT(\"가나다라\", 3)"), "나다라");
    assert_eq!(plain("RIGHT(\"ab\", 10)"), "ab");
    assert_eq!(plain("MID(\"가나다라마\", 2, 3)"), "나다라");
    assert_eq!(plain("MID(\"abc\", 5, 2)"), "");
    assert_eq!(plain("LEN(\"가나다\")"), "3");
    assert_eq!(plain("UPPER(\"abc가\") & LOWER(\"ABC\")"), "ABC가abc");
    assert_eq!(plain("TRIM(\"  a   b \u{3000} c \")"), "a b c");
    assert_eq!(plain("SUBSTITUTE(\"a-b-c\", \"-\", \"+\")"), "a+b+c");
    assert_eq!(plain("SUBSTITUTE(\"a-b-c\", \"-\", \"+\", 2)"), "a-b+c");
    assert_eq!(plain("SUBSTITUTE(\"a-b-c\", \"-\", \"+\", 5)"), "a-b-c");
    assert_eq!(plain("SUBSTITUTE(\"abc\", \"\", \"+\")"), "abc");
    assert_eq!(plain("SPLITPART(\"서울,강남,역삼\", \",\", 2)"), "강남");
    assert_eq!(plain("SPLITPART(\"서울,강남,역삼\", \",\", -1)"), "역삼");
    assert_eq!(plain("SPLITPART(\"서울,강남\", \",\", 3)"), "");
    assert_eq!(plain("REGEXREPLACE(\"tel 010-1234-5678\", \"[^0-9]\", \"\")"), "01012345678");
    assert_eq!(plain("REGEXREPLACE(\"홍 길동\", \"(.) (.+)\", \"$2 $1\")"), "길동 홍");
    assert_eq!(value("LEFT({a}, 2) & \"*\"", &["a"], &["12345"]), "12*");
}

#[test]
fn logic_and_blanks() {
    assert_eq!(value("IF({재고} > 0, \"판매 중\", \"품절\")", &["재고"], &["12"]), "판매 중");
    assert_eq!(value("IF({재고} > 0, \"판매 중\", \"품절\")", &["재고"], &["0"]), "품절");
    // without the false value the field is blank (a spreadsheet gives FALSE)
    assert_eq!(value("IF({재고} > 0, \"판매 중\")", &["재고"], &["0"]), "");
    assert_eq!(value("IFS({a} > 90, \"A\", {a} > 80, \"B\", TRUE, \"C\")", &["a"], &["85"]), "B");
    assert_eq!(value("IFS({a} > 90, \"A\", TRUE, \"C\")", &["a"], &["10"]), "C");
    assert_eq!(run_error("IFS({a} > 90, \"A\")", &["a"], &["10"]).code, "noMatch");
    assert_eq!(plain("AND(1 < 2, 2 < 3) & OR(FALSE, 1 > 2) & NOT(FALSE)"), "TRUEFALSETRUE");
    // AND and OR stop at the first answer: the failing VALUE is not evaluated
    assert_eq!(plain("OR(TRUE, VALUE(\"x\") > 1)"), "TRUE");
    assert_eq!(plain("IF(TRUE, \"ok\", VALUE(\"x\"))"), "ok");
    assert_eq!(value("ISBLANK({a})", &["a", "b"], &["", "x"]), "TRUE");
    assert_eq!(value("ISBLANK({a})", &["a", "b"], &["x", "x"]), "FALSE");
    assert_eq!(plain("ISBLANK(\"  \")"), "TRUE");
    assert_eq!(value("IFBLANK({a}, \"없음\")", &["a", "b"], &["", "x"]), "없음");
    assert_eq!(value("IFBLANK({b}, \"없음\")", &["a", "b"], &["", "x"]), "x");
    assert_eq!(value("{a} & \"x\"", &["a", "b"], &["", "x"]), "x");
    assert_eq!(plain("IF(\"TRUE\", 1, 2)"), "1");
    assert_eq!(plain("IF(\"0\", 1, 2)"), "2");
    assert_eq!(run_error("IF(\"abc\", 1, 2)", &["x"], &["x"]).code, "notBool");
}

#[test]
fn value_round_int_abs() {
    assert_eq!(plain("VALUE(\"1,234.5\") + 1"), "1235.5");
    assert_eq!(plain("VALUE(\"\") & \"|\""), "|");
    assert_eq!(plain("ROUND(2.5) & \" \" & ROUND(-2.5) & \" \" & ROUND(2.4)"), "3 -3 2");
    assert_eq!(plain("ROUND(1.005, 2)"), "1.01");
    assert_eq!(plain("ROUND(2.675, 2)"), "2.68");
    assert_eq!(plain("ROUND(1234.5678, -2)"), "1200");
    assert_eq!(plain("ROUND(0.4999, 0)"), "0");
    assert_eq!(plain("ROUND(9.995, 2)"), "10");
    assert_eq!(plain("ROUND(0.5)"), "1");
    assert_eq!(plain("INT(-1.5) & INT(1.9)"), "-21");
    assert_eq!(plain("ABS(-3) & ABS(\"4\")"), "34");
    assert_eq!(run_error("ROUND(1, 30)", &["x"], &["x"]).code, "badArgument");
}

#[test]
fn text_formats_for_numbers() {
    let t = |x: &str, f: &str| value(&format!("TEXT({{n}}, \"{f}\")"), &["n"], &[x]);
    assert_eq!(t("15000", "#,##0"), "15,000");
    assert_eq!(t("1234567.891", "#,##0"), "1,234,568");
    assert_eq!(t("999", "#,##0"), "999");
    assert_eq!(t("0", "#,##0"), "0");
    assert_eq!(t("0", "#"), "");
    assert_eq!(t("3.14159", "0.0"), "3.1");
    assert_eq!(t("2.5", "0.00"), "2.50");
    assert_eq!(t("2", "0.##"), "2.");
    assert_eq!(t("2.5", "0.##"), "2.5");
    assert_eq!(t("1234.5", "#,##0.00"), "1,234.50");
    assert_eq!(t("0.256", "0%"), "26%");
    assert_eq!(t("0.256", "0.0%"), "25.6%");
    assert_eq!(t("42", "00000"), "00042");
    assert_eq!(t("123456", "000"), "123456");
    assert_eq!(t("-1234", "#,##0"), "-1,234");
    assert_eq!(t("-0.001", "0.0"), "0.0");
    assert_eq!(t("-5", "#,##0;(#,##0)"), "(5)");
    assert_eq!(t("5", "#,##0;(#,##0)"), "5");
    assert_eq!(t("0", "#,##0;(#,##0);\"\"zero\"\""), "zero");
    assert_eq!(t("1500", "\"\"₩\"\"#,##0"), "₩1,500");
    assert_eq!(t("1500", "#,##0\"\"원\"\""), "1,500원");
    assert_eq!(t("1500", "#,##0원"), "1,500원");
    assert_eq!(t("1500000", "0.0,,"), "1.5");
    assert_eq!(t("1500", "0,"), "2");
    assert_eq!(t("0.5", "#.##"), ".5");
    assert_eq!(t("3.7", ".00"), "3.70");
    assert_eq!(t("1234", "#,##0.00"), "1,234.00");
    assert_eq!(t("5", "$0.00"), "$5.00");
    assert_eq!(t("12", "@"), "@");
    assert_eq!(run_error("TEXT({n}, \"0\")", &["n"], &["abc"]).code, "notNumber");
    // a blank gives a blank
    assert_eq!(t("", "#,##0"), "");
    assert_eq!(plain("TEXT(1234.5, \"#,##0.0\") & \"원\""), "1,234.5원");
}

#[test]
fn text_formats_for_dates_and_the_korean_weekday() {
    let t = |x: &str, f: &str| value(&format!("TEXT({{d}}, \"{f}\")"), &["d"], &[x]);
    assert_eq!(t("2026-11-19", "yyyy년 m월 d일"), "2026년 11월 19일");
    assert_eq!(t("2026-01-05", "yyyy년 m월 d일"), "2026년 1월 5일");
    assert_eq!(t("2026-01-05", "yyyy-mm-dd"), "2026-01-05");
    assert_eq!(t("2026/1/5", "yyyy-mm-dd"), "2026-01-05");
    assert_eq!(t("2026.1.5", "yyyy-mm-dd"), "2026-01-05");
    assert_eq!(t("2026.1.5.", "yyyy-mm-dd"), "2026-01-05");
    assert_eq!(t("2026년 1월 5일", "yyyy-mm-dd"), "2026-01-05");
    assert_eq!(t("2026-11-19 13:45:00", "yyyy-mm-dd"), "2026-11-19");
    assert_eq!(t("2026-11-19", "aaa"), "목");
    assert_eq!(t("2026-11-19", "aaaa"), "목요일");
    assert_eq!(t("2026-11-19", "yyyy.mm.dd (aaa)"), "2026.11.19 (목)");
    assert_eq!(t("2026-11-22", "aaa"), "일");
    assert_eq!(t("2026-11-19", "dddd ddd"), "Thursday Thu");
    assert_eq!(t("2026-11-19", "mmmm mmm"), "November Nov");
    assert_eq!(t("2026-11-19", "yy.m.d"), "26.11.19");
    assert_eq!(t("2026-11-19", "\"\"월\"\" m"), "월 11");
    assert_eq!(t("2000-02-29", "yyyy-mm-dd"), "2000-02-29");
    assert_eq!(t("", "yyyy"), "");
    assert_eq!(run_error("TEXT({d}, \"yyyy\")", &["d"], &["내일"]).code, "notDate");
    assert_eq!(run_error("TEXT({d}, \"yyyy\")", &["d"], &["2026-02-30"]).code, "notDate");
    assert_eq!(run_error("TEXT({d}, \"yyyy\")", &["d"], &["2026-13-01"]).code, "notDate");
    assert_eq!(value("YEAR({d}) & \"/\" & MONTH({d}) & \"/\" & DAY({d})", &["d"], &["2026-11-09"]), "2026/11/9");
    assert_eq!(value("YEAR({d}) & \"|\"", &["d"], &[""]), "|");
    // a number is a spreadsheet's date serial (45000 is 2023-03-15)
    assert_eq!(plain("TEXT(45000, \"yyyy-mm-dd\")"), "2023-03-15");
    // a date plus days is a date; two dates make days
    assert_eq!(value("TEXT({d} + 30, \"yyyy-mm-dd\")", &["d"], &["2026-11-19"]), "2026-12-19");
    assert_eq!(value("{d} + 1", &["d"], &["2026-12-31"]), "2027-01-01");
    assert_eq!(value("{e} - {d}", &["d", "e"], &["2026-12-01", "2026-12-25"]), "24");
    assert_eq!(value("{d} - 1", &["d"], &["2026-03-01"]), "2026-02-28");
}

#[test]
fn errors_carry_their_position() {
    let e = compile_error("{성} &", &["성"]);
    assert_eq!((e.code, e.pos), ("unexpectedEnd", 5));
    let e = compile_error("{없는} & \"x\"", &["성"]);
    assert_eq!((e.code, e.pos, e.end), ("unknownField", 0, 4));
    assert_eq!(e.args, ["없는"]);
    let e = compile_error("FOO(1)", &["성"]);
    assert_eq!((e.code, e.pos, e.end), ("unknownFunction", 0, 3));
    let e = compile_error("성 & \"x\"", &["성"]);
    assert_eq!((e.code, e.pos, e.end), ("unknownName", 0, 1));
    let e = compile_error("1 + LEFT()", &["성"]);
    assert_eq!((e.code, e.pos, e.end), ("argCount", 4, 10));
    let e = compile_error("TEXT(1)", &["성"]);
    assert_eq!(e.code, "argCount");
    let e = compile_error("\"abc", &["성"]);
    assert_eq!((e.code, e.pos), ("unterminatedString", 0));
    let e = compile_error("1 + {abc", &["성"]);
    assert_eq!((e.code, e.pos), ("unterminatedField", 4));
    let e = compile_error("(1 + 2", &["성"]);
    assert_eq!((e.code, e.pos), ("unexpectedEnd", 6));
    let e = compile_error("1 2", &["성"]);
    assert_eq!((e.code, e.pos, e.end), ("unexpectedToken", 2, 3));
    let e = compile_error("1 + )", &["성"]);
    assert_eq!((e.code, e.pos), ("unexpectedToken", 4));
    let e = compile_error("1 + @", &["성"]);
    assert_eq!((e.code, e.pos), ("unexpectedChar", 4));
    let e = compile_error("12abc", &["성"]);
    assert_eq!((e.code, e.pos, e.end), ("badNumber", 0, 5));
    let e = compile_error("{ }", &["성"]);
    assert_eq!(e.code, "emptyField");
    assert_eq!(compile_error("   ", &["성"]).code, "empty");
    assert_eq!(compile_error(&"(".repeat(200), &["성"]).code, "tooDeep");
    assert_eq!(compile_error(&"1+".repeat(3000), &["성"]).code, "tooLong");
    // positions are characters, not bytes
    let e = compile_error("\"가나다\" & {없음}", &["성"]);
    assert_eq!((e.pos, e.end), (8, 12));
    let e = compile_error("1 + ", &["성"]);
    assert_eq!(e.code, "unexpectedEnd");
    assert!(!e.message.is_empty());
    // the syntax alone is checked without a source
    assert!(compile("{아무거나} & 1", None, &[]).is_ok());
    assert_eq!(compile("{아무거나} &", None, &[]).unwrap_err().code, "unexpectedEnd");
}

#[test]
fn evaluation_errors_point_at_the_operand() {
    let e = run_error("1 / {a}", &["a"], &["0"]);
    assert_eq!((e.code, e.pos, e.end), ("divideByZero", 0, 7));
    let e = run_error("{a} * 2", &["a"], &["가격"]);
    assert_eq!((e.code, e.pos, e.end), ("notNumber", 0, 3));
    assert_eq!(e.args, ["\"가격\""]);
    let e = run_error("LEFT({a}, -1)", &["a"], &["abc"]);
    assert_eq!((e.code, e.pos), ("badArgument", 10));
    assert_eq!(run_error("MID(\"abc\", 0, 1)", &["a"], &["abc"]).code, "badArgument");
    assert_eq!(run_error("SPLITPART(\"a\", \",\", 0)", &["a"], &["a"]).code, "badArgument");
    assert_eq!(run_error("REGEXREPLACE(\"a\", \"(\", \"\")", &["a"], &["a"]).code, "badRegex");
    assert_eq!(run_error("1e308 * 10", &["a"], &["a"]).code, "overflow");
}

#[test]
fn every_function_is_listed_and_named_in_any_case() {
    let names: Vec<_> = functions().iter().map(|f| f.name).collect();
    for want in ["CONCAT", "TEXTJOIN", "LEFT", "RIGHT", "MID", "LEN", "UPPER", "LOWER", "TRIM", "SUBSTITUTE", "REGEXREPLACE", "SPLITPART", "IF", "IFS", "AND", "OR", "NOT", "ISBLANK", "IFBLANK", "VALUE", "ROUND", "INT", "ABS", "TEXT", "YEAR", "MONTH", "DAY"] {
        assert!(names.contains(&want), "{want}");
    }
    assert_eq!(names.len(), 27);
    assert_eq!(plain("left(\"abc\", 1) & Upper(\"d\")"), "aD");
    assert_eq!(plain("true & False"), "TRUEFALSE");
}

#[test]
fn apply_adds_computed_fields_to_every_record() {
    let s = source(&[&["성", "이름", "가격", "코드"], &["김", "철수", "15000", "A1"], &["이", "영희", "8000", "B2"]]);
    let computed = vec![
        ComputedField { name: "이름 전체".into(), expr: "{성} & \" \" & {이름}".into() },
        ComputedField { name: "가격표".into(), expr: "TEXT({가격}, \"#,##0\") & \"원\"".into() },
        ComputedField { name: "@사진".into(), expr: "\"images/\" & {코드} & \".jpg\"".into() },
        ComputedField { name: "#링크".into(), expr: "\"https://example.com/\" & {코드}".into() },
        // a computed field may use the ones above it
        ComputedField { name: "라벨".into(), expr: "{이름 전체} & \" - \" & {가격표}".into() },
    ];
    let applied = apply(&s, &computed);
    assert!(applied.definition_errors.is_empty() && applied.record_errors.is_empty(), "{applied:?}");
    let names: Vec<_> = applied.source.fields.iter().map(|f| (f.name.as_str(), f.kind.as_str())).collect();
    assert_eq!(names, [("성", "text"), ("이름", "text"), ("가격", "text"), ("코드", "text"), ("이름 전체", "text"), ("가격표", "text"), ("사진", "image"), ("링크", "qrcode"), ("라벨", "text")]);
    assert_eq!(applied.source.records[0][4..], ["김 철수", "15,000원", "images/A1.jpg", "https://example.com/A1", "김 철수 - 15,000원"]);
    assert_eq!(applied.source.records[1][4..], ["이 영희", "8,000원", "images/B2.jpg", "https://example.com/B2", "이 영희 - 8,000원"]);
    // the source's own parts are as they were
    assert_eq!(applied.source.encoding, s.encoding);
    assert_eq!(applied.source.options, s.options);
    assert_eq!(applied.source.records[0][..4], s.records[0][..]);
    // no computed fields: the same source
    assert_eq!(apply(&s, &[]).source, s);
}

#[test]
fn a_failing_record_gets_an_empty_cell_and_an_error() {
    let s = source(&[&["이름", "가격"], &["가", "100"], &["나", "무료"], &["다", "300"], &["라", "abc"]]);
    let computed = vec![ComputedField { name: "두 배".into(), expr: "{가격} * 2".into() }, ComputedField { name: "표시".into(), expr: "{이름} & {두 배}".into() }];
    let applied = apply(&s, &computed);
    let cells: Vec<_> = applied.source.records.iter().map(|r| (r[2].as_str(), r[3].as_str())).collect();
    assert_eq!(cells, [("200", "가200"), ("", "나"), ("600", "다600"), ("", "라")]);
    assert_eq!(applied.error_count, 2);
    let errors: Vec<_> = applied.record_errors.iter().map(|e| (e.record, e.field.as_str(), e.error.code, e.error.pos, e.error.end)).collect();
    assert_eq!(errors, [(2, "두 배", "notNumber", 0, 4), (4, "두 배", "notNumber", 0, 4)]);
    assert!(applied.definition_errors.is_empty());
}

#[test]
fn definition_errors_leave_the_other_fields_alone() {
    let s = source(&[&["a", "b"], &["1", "2"]]);
    let computed = vec![
        ComputedField { name: "ok".into(), expr: "{a} + {b}".into() },
        ComputedField { name: "broken".into(), expr: "{a} +".into() },
        ComputedField { name: "a".into(), expr: "1".into() },
        ComputedField { name: "@".into(), expr: "1".into() },
        ComputedField { name: "x{y}".into(), expr: "1".into() },
        ComputedField { name: "ok".into(), expr: "2".into() },
        ComputedField { name: "early".into(), expr: "{late}".into() },
        ComputedField { name: "late".into(), expr: "5".into() },
        ComputedField { name: "ghost".into(), expr: "{nothing}".into() },
    ];
    let applied = apply(&s, &computed);
    let codes: Vec<_> = applied.definition_errors.iter().map(|d| (d.field.as_str(), d.error.code)).collect();
    assert_eq!(codes, [("broken", "unexpectedEnd"), ("a", "duplicateName"), ("@", "badName"), ("x{y}", "badName"), ("ok", "duplicateName"), ("early", "laterField"), ("ghost", "unknownField")]);
    let names: Vec<_> = applied.source.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "ok", "broken", "early", "late", "ghost"]);
    // a field whose definition is wrong is there, empty
    assert_eq!(applied.source.records[0], ["1", "2", "3", "", "", "5", ""]);
    // a name that is an image field and a text field of the same name are different fields
    let s = source(&[&["사진"], &["x"]]);
    let applied = apply(&s, &[ComputedField { name: "@사진".into(), expr: "{사진} & \".jpg\"".into() }]);
    assert!(applied.definition_errors.is_empty());
    assert_eq!(applied.source.records[0], ["x", "x.jpg"]);
    assert_eq!(applied.source.fields[1].kind, "image");
}

#[test]
fn many_records_and_a_regex_pattern_that_is_compiled_once() {
    let mut rows: Vec<Vec<String>> = vec![vec!["n".into()]];
    for i in 0..3000 { rows.push(vec![format!("tel {i:04}-1234")]); }
    let text: String = rows.iter().map(|r| r.join("\t") + "\r\n").collect();
    let s = parse(&utf16(&text)).unwrap();
    let applied = apply(&s, &[ComputedField { name: "digits".into(), expr: "REGEXREPLACE({n}, \"[^0-9]\", \"\")".into() }]);
    assert_eq!(applied.source.records[2999][1], "29991234");
    assert_eq!(applied.error_count, 0);
}

#[test]
fn the_result_kind_says_what_it_was() {
    let kind = |expr: &str| { let c = compile(expr, None, &[]).unwrap(); evaluate(&c, &[]).unwrap().kind };
    assert_eq!(kind("1"), "number");
    assert_eq!(kind("\"a\""), "text");
    assert_eq!(kind("TRUE"), "bool");
    assert_eq!(kind("IF(FALSE, 1)"), "empty");
}

#[test]
fn the_export_csv_is_read_back_by_the_source_reader() {
    // Korean: UTF-16 little endian with a byte order mark; commas and quotes inside a value are quoted; a line break becomes a space
    let mut s = source(&[&["이름", "메모", "@사진", "#링크"], &["김, 철수", "He said \"hi\" next", "a.png", "https://x"], &["이", "-", "-", "-"]]);
    // a value with a line break (a table's cell cannot hold one: the source reader reads physical lines)
    s.records[0][1] = "He said \"hi\"\r\nnext".into();
    s.records[1][1..].fill(String::new());
    let (bytes, kind) = export_csv(&s, &|v| format!("C:/data/{v}"));
    assert!(bytes.starts_with(&[0xff, 0xfe]) && kind.is_none());
    let back = parse(&bytes).unwrap();
    assert_eq!(back.fields, s.fields);
    assert_eq!(back.records[0], ["김, 철수", "He said \"hi\" next", "C:/data/a.png", "https://x"]);
    assert_eq!(back.records[1], ["이", "", "", ""]);
    // only ASCII: an ASCII file and the type InDesign writes for it
    let s = source(&[&["a", "b"], &["1", "x,y"]]);
    let (bytes, kind) = export_csv(&s, &|v| v.to_owned());
    assert_eq!(bytes, b"a,b\r\n1,\"x,y\"\r\n");
    assert_eq!(kind, Some("CommaSeparated"));
    assert_eq!(parse(&bytes).unwrap().records[0], ["1", "x,y"]);
}
