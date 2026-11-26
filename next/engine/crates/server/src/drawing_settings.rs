//! Per-user drawing preferences, owned by the engine rather than the renderer.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct PolygonSettings { sides: u32, inset: f64 }

fn path() -> Result<PathBuf, String> {
    std::env::var_os("FOLIO_USER_DATA").filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(p).join("drawing-settings.json"))
        .ok_or_else(|| "FOLIO_USER_DATA is required for drawing settings".into())
}

pub fn run(req: &Value) -> Result<Value, String> {
    let path = path()?;
    let settings: PolygonSettings = if let Some(value) = req.get("polygon") {
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?
    } else {
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PolygonSettings { sides: 6, inset: 0.0 },
            Err(e) => return Err(e.to_string()),
        }
    };
    let points = folio_model::polygon::points([0., 0., 1., 1.], settings.sides, settings.inset)?;
    if req.get("polygon").is_some() {
        std::fs::create_dir_all(path.parent().ok_or("missing settings directory")?).map_err(|e| e.to_string())?;
        std::fs::write(&path, serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(json!({"polygon": settings, "points": points, "aspect": folio_model::polygon::aspect(settings.sides)}))
}
