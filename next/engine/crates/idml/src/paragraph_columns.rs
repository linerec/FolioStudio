//! Paragraph column properties shared by IDML reading and value commands.
use crate::props::Props;
use folio_model::paragraph_columns::{ColumnLayout, ParagraphColumns};
use serde_json::Value;

pub struct Property {
    pub api: &'static str,
    pub xml: &'static str,
    default: &'static str,
}
pub const PROPERTIES: &[Property] = &[
    Property { api: "spanColumnType", xml: "SpanColumnType", default: "SingleColumn" },
    Property { api: "spanSplitColumnCount", xml: "SpanSplitColumnCount", default: "All" },
    Property { api: "spanColumnMinSpaceBefore", xml: "SpanColumnMinSpaceBefore", default: "0" },
    Property { api: "spanColumnMinSpaceAfter", xml: "SpanColumnMinSpaceAfter", default: "0" },
    Property { api: "splitColumnInsideGutter", xml: "SplitColumnInsideGutter", default: "6" },
    Property { api: "splitColumnOutsideGutter", xml: "SplitColumnOutsideGutter", default: "0" },
];
const LAYOUTS: &[(&str, &str)] = &[("SINGLE_COLUMN", "SingleColumn"), ("SPAN_COLUMNS", "SpanColumns"), ("SPLIT_COLUMNS", "SplitColumns")];
pub fn property(api: &str) -> Option<&'static Property> { PROPERTIES.iter().find(|p| p.api == api) }
impl Property {
    pub fn encode(&self, value: &Value) -> Option<String> {
        match self.xml {
            "SpanColumnType" => LAYOUTS.iter().find(|(api, _)| Some(*api) == value.as_str()).map(|(_, xml)| (*xml).into()),
            "SpanSplitColumnCount" if value.as_str() == Some("ALL") => Some("All".into()),
            "SpanSplitColumnCount" => value.as_u64().filter(|n| (1..=40).contains(n)).map(|n| if n == 1 { "All".into() } else { n.to_string() }),
            _ => value.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map(|n| n.to_string()),
        }
    }
    pub fn property_type(&self, value: &str) -> Option<&'static str> {
        (self.xml == "SpanSplitColumnCount").then_some(if value == "All" { "enumeration" } else { "short" })
    }
    pub fn decode(&self, value: &str) -> Value {
        let value = if value.is_empty() { self.default } else { value };
        match self.xml {
            "SpanColumnType" => LAYOUTS.iter().find(|(_, xml)| *xml == value).map(|(api, _)| Value::from(*api)).unwrap_or(Value::Null),
            "SpanSplitColumnCount" if value == "All" => Value::from("ALL"),
            _ => value.parse::<f64>().ok().map(Value::from).unwrap_or(Value::Null),
        }
    }
}
/// Entering split layout supplies two columns; explicitly setting All is invalid.
pub fn resolve_count<'a>(layout: &str, count: &'a str, explicit: bool) -> Option<&'a str> {
    if layout == "SplitColumns" && matches!(count, "" | "All" | "1") {
        if explicit { None } else { Some("2") }
    } else { Some(count) }
}
pub(crate) fn read<'a>(a: &mut ParagraphColumns, p: &impl Props<'a>) {
    if let Some(v) = p.str("SpanColumnType") {
        a.layout = match v { "SpanColumns" => ColumnLayout::Span, "SplitColumns" => ColumnLayout::Split, _ => ColumnLayout::Single };
    }
    if let Some(v) = p.str("SpanSplitColumnCount") { a.count = v.parse().ok(); }
    for (key, field) in [("SpanColumnMinSpaceBefore", &mut a.before), ("SpanColumnMinSpaceAfter", &mut a.after),
        ("SplitColumnInsideGutter", &mut a.inside), ("SplitColumnOutsideGutter", &mut a.outside)] {
        if let Some(v) = p.f64(key) { *field = v; }
    }
}
