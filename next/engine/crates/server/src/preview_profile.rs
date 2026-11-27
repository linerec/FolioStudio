use folio_engine::performance::{self, Hardware, Options, Plan};
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

const KEYS: [(&str, &str); 4] = [
    ("folio.performanceProfile", "FOLIO_PROFILE"),
    ("folio.threads", "FOLIO_THREADS"),
    ("folio.memBudgetMb", "FOLIO_MEM_BUDGET_MB"),
    ("folio.accel", "FOLIO_ACCEL"),
];

fn parsed(key: &str, value: &Value) -> Option<String> {
    let s = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))?;
    let valid = match key {
        "folio.performanceProfile" => matches!(s.as_str(), "auto" | "low" | "standard" | "high"),
        "folio.accel" => matches!(s.as_str(), "auto" | "off"),
        "folio.threads" | "folio.memBudgetMb" => {
            s == "auto" || s.parse::<u32>().is_ok_and(|n| n > 0)
        }
        _ => false,
    };
    valid.then_some(s)
}

struct Selection {
    hardware: Hardware,
    plan: Plan,
    inputs: Value,
}

static ACTIVE: LazyLock<Selection> = LazyLock::new(|| {
    let settings = crate::user_settings::run(&json!({})).unwrap_or(Value::Null);
    let mut inputs = json!({});
    let values = KEYS.map(|(key, env)| {
        let (raw, source) = if let Ok(v) = std::env::var(env) {
            (Value::String(v), "environment")
        } else if let Some(v) = settings["settings"]["preferences"].get(key) {
            (v.clone(), "saved")
        } else {
            (json!("auto"), "default")
        };
        let parsed = parsed(key, &raw);
        inputs[key] = json!({"source": source, "valid": parsed.is_some(), "value": parsed});
        parsed.unwrap_or_else(|| "auto".into())
    });
    let [profile, threads, budget, accel] = values;
    let options = Options {
        profile,
        threads: threads.parse().ok(),
        mem_budget_mb: budget.parse().ok(),
        accel_off: accel == "off",
    };
    let hardware = Hardware::detect();
    let plan = performance::select(&hardware, &options, cfg!(windows));
    Selection {
        hardware,
        plan,
        inputs,
    }
});

pub fn reuse_pdf_image_rows() -> bool {
    ACTIVE.plan.pdf_reuse_image_rows
}

pub fn workers() -> usize {
    ACTIVE.plan.render_workers
}

pub fn preview_image_allowance() -> Option<u64> {
    ACTIVE.plan.budget_mb.map(|mb| mb * 1024 * 1024 / workers() as u64)
}

pub fn query(request: &Value) -> Result<Value, String> {
    let mut write = json!({});
    if let Some(value) = request.get("values") {
        let values = value.as_object().ok_or("values must be an object")?;
        if values.iter().any(|(key, v)| parsed(key, v).is_none()) {
            return Err("invalid performance preference or value".into());
        }
        write["values"] = value.clone();
    }
    let settings = match crate::user_settings::run(&write) {
        Ok(settings) => settings["settings"].clone(),
        Err(_) if write["values"].is_null() && std::env::var_os("FOLIO_USER_DATA").is_none() => {
            Value::Null
        }
        Err(e) => return Err(e),
    };
    let saved: Map<_, _> = KEYS
        .iter()
        .filter_map(|(k, _)| {
            settings["preferences"]
                .get(*k)
                .map(|v| (k.to_string(), v.clone()))
        })
        .collect();
    let mut reply = json!({"hardware": ACTIVE.hardware, "adapters": performance::adapters(),
        "backend": "cpu", "active": ACTIVE.plan, "previewImageAllowanceBytes": preview_image_allowance(), "inputs": ACTIVE.inputs, "saved": saved});
    if write.get("values").is_some() {
        reply["_settingsEvent"] = crate::user_settings::event(&settings, &request["origin"]);
    }
    Ok(reply)
}
