//! Required JSON arguments and the basic error reply on the engine wire.

use serde::Serialize;
use serde_json::{Value, json};

pub(super) fn required_text<'a>(
    request: &'a Value,
    key: &str,
    error: &'static str,
) -> Result<&'a str, &'static str> {
    request[key].as_str().ok_or(error)
}

pub(super) fn required_index(request: &Value, key: &str, error: &'static str) -> Result<usize, &'static str> {
    request[key].as_u64().map(|index| index as usize).ok_or(error)
}

pub(super) fn required_number(request: &Value, key: &str, error: &'static str) -> Result<f64, &'static str> {
    request[key].as_f64().ok_or(error)
}

pub(super) fn required_finite_number(
    request: &Value,
    key: &str,
    error: &'static str,
) -> Result<f64, &'static str> {
    let value = required_number(request, key, error)?;
    value.is_finite().then_some(value).ok_or(error)
}

pub(super) fn error_reply(id: &Value, message: impl Serialize) -> Value {
    json!({"id": id, "ok": false, "error": message})
}
