//! Linked offset, angle and scale fields in the transform dialogs.

use serde_json::{Value, json};
use super::fields::number;

fn clean(x: f64) -> f64 { let y = (x * 1e6).round() / 1e6; if y == 0. { 0. } else { y } }

pub(super) fn link_move(values: &mut Value, changed: &str) {
    match changed {
        "h" | "v" => {
            let (h, v) = (number(values, "h"), number(values, "v"));
            values["distance"] = json!(clean(h.hypot(v)));
            values["angle"] = json!(clean((-v).atan2(h).to_degrees()));
        }
        "distance" | "angle" => {
            let (d, a) = (number(values, "distance"), number(values, "angle").to_radians());
            values["h"] = json!(clean(d * a.cos()));
            values["v"] = json!(clean(-d * a.sin()));
        }
        _ => {}
    }
}

pub(super) fn link_scale(values: &mut Value, changed: &str) {
    // turning "keep proportions" off starts the vertical scale from the horizontal one
    if changed == "uniform" && values["uniform"] == false { values["sy"] = values["sx"].clone(); }
}

pub(super) fn prepare_scale(values: &mut Value) {
    if values["uniform"] == true { values["sy"] = values["sx"].clone(); }
}
