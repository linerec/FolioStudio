//! StoryPreference optical margin values shared by reading, editing and queries.
use folio_model::optical_margin::OpticalMargin;
use serde_json::Value;

pub fn read<'a>(get: impl Fn(&str) -> Option<&'a str>) -> OpticalMargin {
    let default = OpticalMargin::default();
    OpticalMargin {
        enabled: get("OpticalMarginAlignment").map_or(default.enabled, |v| v == "true"),
        size: get("OpticalMarginSize").and_then(|v|v.parse().ok()).unwrap_or(default.size),
    }
}

pub fn encode(key: &str, value: &Value) -> Option<(&'static str, String)> {
    match key {
        "enabled" => value.as_bool().map(|v|("OpticalMarginAlignment",v.to_string())),
        "size" => value.as_f64().filter(|v|v.is_finite() && *v >= 0.1 && *v <= 1296.0)
            .map(|v|("OpticalMarginSize",v.to_string())),
        _ => None,
    }
}
