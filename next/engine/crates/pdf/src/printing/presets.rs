//! Print presets: named sets of print choices kept in the application's user settings (never in a document or a project).
//!
//! Measured in InDesign 2025 (`printing-presets.json`, rules in `next/oracle/rules/printing.ko.md`): a preset is created, duplicated
//! (`<name> Copy 1`, the first free number), renamed and deleted; a name that is taken is refused; applying a preset to a print job moves
//! its values over but keeps the job's own page range (a preset has no page range). Measured in InDesign 2026: the dialog `Print Presets`
//! lists `[Default]` and the saved presets with New / Edit / Delete / Load / Save; `[Default]`'s Delete cannot be focused (it is built in).
//! The settings a preset holds are in `settings.rs` (all of InDesign's `PrinterPreset` properties); this module keeps the list.
//! Decisions (not measured): `[Default]` is built in and cannot be changed, renamed or deleted; names are compared as written.
use folio_model::printing::PrintJob;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use super::settings;

pub const DEFAULT_NAME: &str = "[Default]";

/// A name and the settings of `settings::FIELDS` (flat, InDesign's property names). A saved preset may lack keys (older ones hold only
/// `spreads` and `printBlankPages`); `complete` fills them from the defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintPreset {
    pub name: String,
    #[serde(flatten)]
    pub values: Map<String, Value>,
}

impl PrintPreset {
    /// A preset with all settings present and consistent, or the reason it is not valid.
    pub fn complete(&self) -> Result<PrintPreset, String> {
        Ok(PrintPreset { name: self.name.clone(), values: settings::normalize(&self.values)? })
    }
    pub fn flag(&self, key: &str) -> bool {
        self.values.get(key).and_then(Value::as_bool).unwrap_or(false)
    }
    /// A preset of the defaults with `changes` on top.
    pub fn with(name: &str, changes: &[(&str, Value)]) -> Result<PrintPreset, String> {
        let mut values = Map::new();
        for (k, v) in changes { values.insert((*k).into(), v.clone()); }
        PrintPreset { name: name.into(), values }.complete().map_err(|message| format!("invalidPrintSettings at preset {name}: {message}"))
    }
}

/// The built-in `[Default]`: InDesign's defaults.
pub fn default_preset() -> PrintPreset {
    PrintPreset { name: DEFAULT_NAME.into(), values: settings::defaults() }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PresetOp {
    /// A new preset; the name must be free.
    Define { preset: PrintPreset },
    /// `name` becomes `preset` (values and, when it differs, the name).
    Update { name: String, preset: PrintPreset },
    /// A copy named `<name> <word> <n>`, `word` being the language's "Copy" (InDesign English: "Copy").
    Duplicate { name: String, word: String },
    Rename { name: String, to: String },
    Remove { name: String },
}

fn clean(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("preset name is empty".into());
    }
    if name == DEFAULT_NAME {
        return Err("the name [Default] is reserved".into());
    }
    Ok(name.to_string())
}

fn position(presets: &[PrintPreset], name: &str) -> Result<usize, String> {
    if name == DEFAULT_NAME {
        return Err("the built-in [Default] preset cannot be changed".into());
    }
    presets.iter().position(|p| p.name == name).ok_or_else(|| format!("no print preset named {name}"))
}

fn free(presets: &[PrintPreset], name: &str) -> Result<(), String> {
    if presets.iter().any(|p| p.name == name) { Err(format!("a print preset named {name} already exists")) } else { Ok(()) }
}

/// The list after one operation; `presets` itself is never changed (a failed operation changes nothing).
pub fn apply_op(presets: &[PrintPreset], op: &PresetOp) -> Result<Vec<PrintPreset>, String> {
    let mut out = presets.to_vec();
    match op {
        PresetOp::Define { preset } => {
            let name = clean(&preset.name)?;
            free(&out, &name)?;
            out.push(PrintPreset { name, ..preset.complete()? });
        }
        PresetOp::Update { name, preset } => {
            let at = position(&out, name)?;
            let new = clean(&preset.name)?;
            if new != *name {
                free(&out, &new)?;
            }
            out[at] = PrintPreset { name: new, ..preset.complete()? };
        }
        PresetOp::Duplicate { name, word } => {
            let source = find(&out, name)?;
            let word = word.trim();
            let word = if word.is_empty() { "Copy" } else { word };
            let n = (1..).find(|n| !out.iter().any(|p| p.name == format!("{name} {word} {n}"))).expect("a free number");
            out.push(PrintPreset { name: format!("{name} {word} {n}"), values: source.complete()?.values });
        }
        PresetOp::Rename { name, to } => {
            let at = position(&out, name)?;
            let to = clean(to)?;
            if to != *name {
                free(&out, &to)?;
            }
            out[at].name = to;
        }
        PresetOp::Remove { name } => {
            let at = position(&out, name)?;
            out.remove(at);
        }
    }
    Ok(out)
}

/// `[Default]` first, then the saved presets.
pub fn listed(presets: &[PrintPreset]) -> Result<Vec<PrintPreset>, String> {
    std::iter::once(Ok(default_preset())).chain(presets.iter().map(PrintPreset::complete)).collect()
}

/// The job with the preset's values; the page range (selection) stays the job's own.
pub fn apply_to_job(job: &PrintJob, preset: &PrintPreset) -> PrintJob {
    PrintJob { selection: job.selection.clone(), spreads: preset.flag("printSpreads"), print_blank_pages: preset.flag("printBlankPages") }
}

/// The preset by name (`[Default]` included).
pub fn find(presets: &[PrintPreset], name: &str) -> Result<PrintPreset, String> {
    if name == DEFAULT_NAME {
        return Ok(default_preset());
    }
    presets.iter().find(|p| p.name == name).ok_or_else(|| format!("no print preset named {name}"))?.complete()
}
