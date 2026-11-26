//! The print dialogs' queries: the plan, the PDF proof file, the booklet, the presets, and sending the print to a printer
//! (`listPrinters`, `printToDevice`, `cancelPrintJob`; the device layer is `folio_pdf::printing::device`).
use serde_json::{Value, json};
use folio_model::printing::PrintJob;
use std::path::Path;

pub fn job(req: &Value) -> Result<PrintJob, String> {
    serde_json::from_value(req["job"].clone()).map_err(|e| format!("invalid print job: {e}"))
}

/// What the print dialog's settings add to a print job: the marks they set (with their offset and weight) and the bleed.
/// The PDF proof and the print to a device both take it from here, so both draw the same marks. `values` are complete (`settings::normalize`).
/// "Use Document Bleed Settings" prints the document's bleed with or without marks (measured in InDesign's PostScript file output: a 9 pt bleed makes
/// the 240 x 300 pt sheet 258 x 318 pt even with no mark on). With it off, the dialog's own four bleed values are the bleed.
fn marks_export(job_value: &Value, values: &serde_json::Map<String, Value>) -> Value {
    let flag = |k: &str| values.get(k).and_then(Value::as_bool).unwrap_or(false);
    let marks = json!({"crop": flag("cropMarks"), "bleed": flag("bleedMarks"), "registration": flag("registrationMarks"), "pageInfo": flag("pageInformationMarks")});
    // "Use Document Bleed Settings" off: the four bleed boxes of the dialog (top, bottom, inside, outside) are the bleed
    let number = |k: &str| values.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let custom = (!flag("useDocumentBleedToPrint")).then(|| [number("bleedTop"), number("bleedBottom"), number("bleedInside"), number("bleedOutside")]);
    json!({"printingJob": job_value, "marks": marks, "bleed": flag("useDocumentBleedToPrint"), "customBleed": custom,
        "markOffset": values.get("markOffset").and_then(Value::as_f64),
        "markWeight": values.get("markLineWeight").and_then(Value::as_str).and_then(folio_pdf::printing::settings::mark_weight_points)})
}

/// The export of a request that has a `job` and may have the dialog's `settings` (a print preset's values: marks, offset, bleed); without settings, no marks.
fn export_for(req: &Value) -> Result<Value, String> {
    let job_value = req["job"].clone();
    match req.get("settings") {
        Some(Value::Object(m)) => Ok(marks_export(&job_value, &folio_pdf::printing::settings::normalize(m)?)),
        None | Some(Value::Null) => Ok(json!({"printingJob": job_value})),
        Some(_) => Err("settings must be an object".into()),
    }
}

pub fn plan(req: &Value, s: Option<&crate::Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let job = job(req)?;
    let mut opts = crate::print::export_options(&export_for(req)?, folio_pdf::ExportOptions::default())?;
    folio_pdf::printing::configure(&s.doc, &job, &mut opts)?;
    Ok(json!({"sheets":folio_pdf::printing::sheets(&s.doc, &opts),"destination":"pdfProof","printerTransmission":false}))
}

pub fn file(req: &Value, s: Option<&crate::Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let plan = plan(req, Some(s))?;
    let path = Path::new(req["path"].as_str().ok_or("PDF proof path required")?);
    if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) { return Err("PDF proof path required".into()); }
    let export = export_for(req)?;
    let (report, _, ms) = crate::export_pdf(s, &export, path, &|_| {})?;
    let count = plan["sheets"].as_array().map_or(0, Vec::len);
    Ok(json!({"plan":plan,"result":crate::print::export_view(&report,path,count,ms)}))
}

/// The selection and options of a Print Booklet request (`selection` as in a print job, `options` as `BookletOptions`; defaults when absent).
fn booklet_request(req: &Value) -> Result<(folio_model::printing::PrintSelection, folio_pdf::printing::booklet::BookletOptions), String> {
    use folio_pdf::printing::booklet::BookletOptions;
    let selection = match req.get("selection") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone()).map_err(|e| format!("invalid print selection: {e}"))?,
        _ => folio_model::printing::PrintSelection::All,
    };
    let options: BookletOptions = match req.get("options") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone()).map_err(|e| format!("invalid booklet options: {e}"))?,
        _ => BookletOptions::default(),
    };
    Ok((selection, options))
}

/// The sheets of a Print Booklet request in terms of the document's pages, for `ExportOptions::booklet`.
pub fn booklet_sheets(doc: &folio_model::Document, req: &Value) -> Result<Vec<folio_pdf::printing::booklet::LaidSheet>, String> {
    let (selection, options) = booklet_request(req)?;
    folio_pdf::printing::booklet::layout(doc, &selection, &options)
}

/// Print Booklet plan: which selected pages sit where on which sheet. No output is made and the document is not reordered.
pub fn booklet(req: &Value, s: Option<&crate::Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let sheets = booklet_sheets(&s.doc, req)?;
    let page = |index: usize| s.doc.spreads.iter().flat_map(|sp| &sp.pages).find(|p| p.index == index);
    let rows: Vec<Value> = sheets.iter().enumerate().map(|(n, sheet)| json!({
        "label": sheet.cells.iter().flatten().filter_map(|c| page(c.page).map(|p| p.name.clone())).collect::<Vec<_>>().join(", "),
        "face": n + 1,
        "size": sheet.size,
        "cells": sheet.cells.iter().map(|c| match c.as_ref().and_then(|c| page(c.page).map(|p| (p, c.origin))) {
            Some((p, origin)) => json!({"pageId": p.id, "name": p.name, "origin": origin}),
            None => json!({"pageId": null, "name": null, "origin": null}),
        }).collect::<Vec<_>>(),
    })).collect();
    Ok(json!({"sheets": rows, "output": true}))
}

/// Print Booklet output: the sheets drawn into one PDF/X-4 (every page at its place, clipped to the page). `path` is a `.pdf` file.
/// The answer holds the same plan as `booklet` and the export's result.
pub fn booklet_file(req: &Value, s: Option<&crate::Session>) -> Result<Value, String> {
    let s = s.ok_or("no document open")?;
    let plan = booklet(req, Some(s))?;
    let path = Path::new(req["path"].as_str().ok_or("PDF path required")?);
    if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) { return Err("PDF path required".into()); }
    let export = json!({"booklet": {"selection": req.get("selection").cloned().unwrap_or(Value::Null), "options": req.get("options").cloned().unwrap_or(Value::Null)}});
    let (report, _, ms) = crate::export_pdf(s, &export, path, &|_| {})?;
    let count = plan["sheets"].as_array().map_or(0, Vec::len);
    Ok(json!({"plan": plan, "result": crate::print::export_view(&report, path, count, ms)}))
}

/// Print presets (application user settings, never the document): the list after one operation, or a job with a preset applied.
/// `presets` is the saved list the caller keeps; nothing is stored here.
pub fn presets(req: &Value) -> Result<Value, String> {
    use folio_pdf::printing::presets::{PresetOp, PrintPreset, apply_op, apply_to_job, find, listed};
    use folio_pdf::printing::settings;
    let saved: Vec<PrintPreset> = serde_json::from_value(req["presets"].clone()).map_err(|e| format!("invalid print presets: {e}"))?;
    match req["op"].as_str().ok_or("op required")? {
        "list" => Ok(json!({"presets": listed(&saved)?})),
        // the dialog's fields and what does not apply: for a saved preset (`name`) or for `values` (a draft; missing keys are defaults)
        "schema" => {
            let values = match req.get("values") {
                Some(Value::Object(m)) => settings::normalize(m)?,
                _ => find(&saved, req["name"].as_str().ok_or("name or values required")?)?.values,
            };
            Ok(settings::schema(&values))
        }
        // one setting changed in a draft; the answer is the same as `schema` for the result
        "set" => {
            let Some(Value::Object(m)) = req.get("values") else { return Err("values required".into()) };
            let next = settings::set(m, req["key"].as_str().ok_or("key required")?, &req["value"])?;
            Ok(settings::schema(&next))
        }
        // Save... / Load... of the Print Presets dialog: a Folio file of presets (InDesign's own .prst format is not read). `names` picks which to save.
        "exportFile" => {
            let path = std::path::Path::new(req["path"].as_str().ok_or("path required")?);
            let names: Vec<String> = serde_json::from_value(req["names"].clone()).map_err(|e| format!("invalid names: {e}"))?;
            let picked: Vec<PrintPreset> = names.iter().map(|n| find(&saved, n)).collect::<Result<_, _>>()?;
            let text = serde_json::to_string_pretty(&json!({"format": "folio-print-presets", "version": 1, "presets": picked})).map_err(|e| e.to_string())?;
            std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            Ok(json!({"written": picked.len()}))
        }
        "importFile" => {
            let path = std::path::Path::new(req["path"].as_str().ok_or("path required")?);
            let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let file: Value = serde_json::from_str(&text).map_err(|e| format!("not a print preset file: {e}"))?;
            if file["format"] != "folio-print-presets" { return Err("not a Folio print preset file".into()); }
            let incoming: Vec<PrintPreset> = serde_json::from_value(file["presets"].clone()).map_err(|e| format!("invalid presets in file: {e}"))?;
            let mut next = saved.clone();
            for p in incoming {
                let p = p.complete()?;
                let taken = |n: &str| n == folio_pdf::printing::presets::DEFAULT_NAME || next.iter().any(|q| q.name == n);
                let name = if taken(&p.name) { (1..).map(|i| format!("{} Copy {i}", p.name)).find(|n| !taken(n)).expect("a free name") } else { p.name.clone() };
                next.push(PrintPreset { name, values: p.values });
            }
            Ok(json!({"saved": next, "presets": listed(&next)?}))
        }
        // InDesign's "Preset Settings" text of the Print Presets dialog (English)
        "summary" => {
            let Some(Value::Object(m)) = req.get("values") else { return Err("values required".into()) };
            let values = settings::normalize(m)?;
            let text = settings::summary_en(req["name"].as_str().unwrap_or(""), &values, req["documentProfile"].as_str().unwrap_or("sRGB IEC61966-2.1"), req["defaultPrinter"].as_str().unwrap_or(""));
            Ok(json!({"text": text}))
        }
        "apply" => {
            let preset = find(&saved, req["name"].as_str().ok_or("preset name required")?)?;
            Ok(json!({"job": apply_to_job(&job(req)?, &preset), "preset": preset}))
        }
        _ => {
            let op: PresetOp = serde_json::from_value(req.clone()).map_err(|e| format!("invalid preset operation: {e}"))?;
            let next = apply_op(&saved, &op)?;
            Ok(json!({"saved": next, "presets": listed(&next)?}))
        }
    }
}

// ---- sending to a printer ----------------------------------------------------------------------------------------------------------------

use folio_pdf::printing::device::{self, DeviceRequest, ErrorKind, PrintError, SheetRef};

/// A failure the caller can branch on: `ok:false`, the text, and `errorKind` (camelCase, see `device::ErrorKind`).
fn failure(e: &PrintError) -> Value { json!({"ok": false, "error": e.message, "errorKind": e.kind}) }
fn invalid(message: impl Into<String>) -> Value { failure(&PrintError::new(ErrorKind::InvalidRequest, message)) }

/// `{"cmd":"listPrinters"}`: the printers of this computer (name, default, PostScript, virtual PDF, papers). `device` says which backend answered.
pub fn list_printers() -> Value {
    let backend = device::backend();
    match backend.printers() {
        Ok(printers) => json!({"printers": printers, "defaultPrinter": printers.iter().find(|p| p.is_default).map(|p| p.name.clone()), "device": backend.kind()}),
        Err(e) => failure(&e),
    }
}

fn temp_pdf() -> std::path::PathBuf {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!("folio-print-{}-{}.pdf", std::process::id(), COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)))
}

/// `{"cmd":"printToDevice","printer":"…","settings":{…print preset values…},"job":{selection,spreads,printBlankPages} | "booklet":{selection,options},"outputFile":"x.pdf"}`
/// Writes the print PDF the dialog's "save as PDF" writes (same range, spreads, blank pages, marks; a booklet's sheets), hands it to the printer and
/// answers `{job:{id,printer,state,warnings}, sheets, request, ignored, device}`. `settings` are the keys of `printPresets` (copies, collating, reverseOrder,
/// printPageOrientation, paperSize, scaleMode, scaleWidth, colorOutput, the marks…); `ignored` lists the changed settings a printer print does not use.
/// `outputFile` is for a virtual PDF printer only. No project or document file is written; the temporary PDF is removed.
pub fn print_to_device(req: &Value, s: Option<&crate::Session>) -> Value {
    let Some(s) = s else { return invalid("no document open") };
    let Some(printer) = req["printer"].as_str().filter(|p| !p.is_empty()) else { return invalid("printer required") };
    let values = match req.get("settings") {
        Some(Value::Object(m)) => match folio_pdf::printing::settings::normalize(m) { Ok(v) => v, Err(e) => return invalid(e) },
        None | Some(Value::Null) => folio_pdf::printing::settings::defaults(),
        Some(_) => return invalid("settings must be an object"),
    };
    let flag = |k: &str| values.get(k).and_then(Value::as_bool).unwrap_or(false);
    let booklet_job = req.get("booklet").filter(|v| !v.is_null());
    if booklet_job.is_some() && req.get("job").is_some_and(|v| !v.is_null()) { return invalid("give a job or a booklet, not both"); }
    let (export, sheets) = if let Some(b) = booklet_job {
        let rows = match booklet_sheets(&s.doc, b).and_then(|_| booklet(b, Some(s))) { Ok(v) => v, Err(e) => return failure(&PrintError::new(ErrorKind::InvalidRange, e)) };
        let sheets = rows["sheets"].as_array().map(|a| a.iter().map(|row| SheetRef {
            page_ids: row["cells"].as_array().map(|c| c.iter().filter_map(|c| c["pageId"].as_str().map(str::to_owned)).collect()).unwrap_or_default(),
            label: row["label"].as_str().unwrap_or_default().to_owned(),
            size: [row["size"][0].as_f64().unwrap_or(0.0), row["size"][1].as_f64().unwrap_or(0.0)],
        }).collect::<Vec<_>>()).unwrap_or_default();
        (json!({"booklet": b}), sheets)
    } else {
        let job_value = match req.get("job") {
            Some(v) if !v.is_null() => v.clone(),
            _ => json!({"selection": {"kind": "all"}, "spreads": flag("printSpreads"), "printBlankPages": flag("printBlankPages")}),
        };
        let job = match serde_json::from_value::<folio_model::printing::PrintJob>(job_value.clone()) { Ok(j) => j, Err(e) => return invalid(format!("invalid print job: {e}")) };
        // the marks the dialog set (a booklet has none); the bleed is printed with them
        let export = marks_export(&job_value, &values);
        let mut opts = match crate::print::export_options(&export, folio_pdf::ExportOptions::default()) { Ok(o) => o, Err(e) => return invalid(e) };
        if let Err(e) = folio_pdf::printing::configure(&s.doc, &job, &mut opts) { return failure(&PrintError::new(ErrorKind::InvalidRange, e)); }
        let sheets = folio_pdf::printing::sheets(&s.doc, &opts).into_iter().map(|x| SheetRef { page_ids: x.page_ids, label: x.label, size: x.size }).collect();
        (export, sheets)
    };
    let file = temp_pdf();
    let output_file = req["outputFile"].as_str().filter(|p| !p.is_empty()).map(std::path::PathBuf::from);
    let name = format!("Folio {}", s.source_name);
    let request: Result<DeviceRequest, PrintError> = device::request_from_settings(&values, printer, name.trim(), sheets, file.clone(), output_file);
    let mut request = match request { Ok(r) => r, Err(e) => return failure(&e) };
    // a booklet sheet is as wide as two pages: unless the caller names an orientation, the paper turns with the sheet
    if booklet_job.is_some() && !req["settings"].as_object().is_some_and(|m| m.contains_key("printPageOrientation"))
        && let Some(sheet) = request.sheets.first() { request.orientation = if sheet.size[0] > sheet.size[1] { device::Orientation::Landscape } else { device::Orientation::Portrait }; }
    // the checks that need no PDF run first: a refused request never writes one
    let backend = device::backend();
    let result = device::prepare(backend.as_ref(), &request).and_then(|(prepared, warnings)| {
        crate::export_pdf(s, &export, &file, &|_| {}).map_err(|e| PrintError::new(ErrorKind::ExportFailed, e))?;
        let mut ticket = backend.submit(&prepared)?;
        ticket.warnings.extend(warnings);
        Ok((ticket, prepared))
    });
    let _ = std::fs::remove_file(&file);
    match result {
        // `request` is what the printer was given (a paper it lacks is already the driver's own)
        Ok((ticket, given)) => json!({"job": ticket, "sheets": given.sheets.len(), "copies": given.copies, "request": given, "ignored": device::ignored_settings(&values), "device": backend.kind()}),
        Err(e) => failure(&e),
    }
}

/// `{"cmd":"cancelPrintJob","printer":"…","job":"<id printToDevice gave>"}`
pub fn cancel_print_job(req: &Value) -> Value {
    let (Some(printer), Some(job)) = (req["printer"].as_str(), req["job"].as_str()) else { return invalid("printer and job required") };
    match device::backend().cancel(printer, job) {
        Ok(()) => json!({"canceled": true, "printer": printer, "job": job}),
        Err(e) => failure(&e),
    }
}
