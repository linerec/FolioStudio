//! File ▸ Package commands:
//!   {"cmd":"packagePlan", "links":true, "fonts":true, "cjkFonts":false, "dir":"…"?}
//!       what the package would contain (the dialog), and whether `dir` can be used
//!   {"cmd":"package", "dir":"…\\issue 폴더", "pdf":true, "fonts":true, "links":true,
//!    "export":{bleed, marks, spreads}, "roots":[…]?, "instructions":{…}, "cjkFonts":false,
//!    "copy":"os"|"stream", "jobs":4, "appVersion":"…", "utcOffsetMinutes":540}
//!       makes the folder; progress events {job:"package", stage, done, total, files,
//!       filesTotal, pdf}; errors carry `code` (exists, protected, noParent, space,
//!       cancelled, source, io, busy) and, for `exists`, `suggestion`
//!   {"cmd":"packageCancel"}
//!
//! The facts come from what the session already holds: the current link report (search
//! folders, index and probe caches of `print.rs`), a fresh preflight, the separations' ink
//! list, the font store the text was set with. `folio_package` decides and copies.

use crate::{Out, Session, send};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// The running package's cancel flag (one package at a time).
fn current() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    static C: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// The tests that make a package take turns (one package at a time in a process: another one is refused `busy`).
#[cfg(test)]
pub static TEST_SLOT: Mutex<()> = Mutex::new(());

/// `packageCancel`: asks the running package to stop.
pub fn cancel() -> Value {
    let flag = current().lock().expect("shared state must remain unpoisoned after each operation").clone();
    if let Some(f) = &flag {
        f.store(true, Ordering::Relaxed);
    }
    json!({"ok": true, "type": "packageCancel", "cancelling": flag.is_some()})
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn protected_roots(req: &Value) -> Vec<PathBuf> {
    let mut roots = folio_package::protect::default_protected_roots();
    if let Some(env) = std::env::var_os("FOLIO_PROTECTED_ROOTS") {
        roots.extend(std::env::split_paths(&env).filter(|p| !p.as_os_str().is_empty()));
    }
    if let Some(a) = req["protectedRoots"].as_array() {
        roots.extend(a.iter().filter_map(Value::as_str).map(PathBuf::from));
    }
    roots
}

fn options(req: &Value) -> folio_package::Options {
    let flag = |k: &str, d: bool| req[k].as_bool().unwrap_or(d);
    let ins = &req["instructions"];
    folio_package::Options {
        links: flag("links", true),
        fonts: flag("fonts", true),
        pdf: flag("pdf", false),
        cjk_fonts: flag("cjkFonts", false),
        jobs: req["jobs"].as_u64().map_or(4, |j| j.clamp(1, 16) as usize),
        copy: if req["copy"].as_str() == Some("stream") { folio_package::CopyMethod::Stream } else { folio_package::CopyMethod::Os },
        protected_roots: protected_roots(req),
        instructions: folio_package::Instructions {
            contact: text(&ins["contact"]),
            company: text(&ins["company"]),
            phone: text(&ins["phone"]),
            email: text(&ins["email"]),
            notes: text(&ins["notes"]),
        },
        app_version: req["appVersion"].as_str().unwrap_or("").to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        utc_offset_minutes: req["utcOffsetMinutes"].as_i64().map(|v| v as i32),
        pdf_settings: req["export"].is_object().then(|| req["export"].clone()),
    }
}

fn profile(v: &Value) -> Option<folio_package::Profile> {
    v.is_object().then(|| folio_package::Profile {
        name: v["name"].as_str().map(str::to_owned),
        file: v["file"].as_str().map(PathBuf::from),
        status: text(&v["status"]),
    })
}

fn ink_facts(s: &Session) -> folio_package::InkFacts {
    let inks = s.doc_inks();
    let plates = inks
        .inks
        .iter()
        .enumerate()
        .map(|(i, k)| folio_package::Plate {
            name: k.name.clone(),
            spot: k.kind == folio_render::sep::InkKind::Spot,
            ink_type: k.ink_type.clone(),
            pages: inks.pages.iter().enumerate().filter(|(_, p)| p.contains(&i)).map(|(page, _)| page).collect(),
            aliases: k.aliases.clone(),
        })
        .collect();
    folio_package::InkFacts { plates, converted: inks.converted.clone() }
}

/// Replaces the link report when the request names other search folders.
fn use_roots(req: &Value, s: &Session) {
    let Some(a) = req["roots"].as_array() else { return };
    let roots: Vec<PathBuf> = a.iter().filter_map(Value::as_str).map(PathBuf::from).filter(|r| r.is_dir()).collect();
    let same = s.print.lock().expect("shared state must remain unpoisoned after each operation").as_ref().is_some_and(|p| p.roots == roots);
    if !same {
        let state = crate::print::build_links(&s.doc, s.source_dir.as_deref(), &roots);
        *s.print.lock().expect("shared state must remain unpoisoned after each operation") = Some(state);
    }
}

fn inputs<'a>(s: &'a Session, links: &'a folio_links::LinkReport, preflight: &'a folio_preflight::Report) -> folio_package::Inputs<'a> {
    let color = &s.summary["color"];
    folio_package::Inputs {
        doc: &s.doc,
        layout: &s.layout,
        fonts: &s.fonts,
        links,
        preflight,
        locations: s.locations(),
        source: &s.source_path,
        inks: ink_facts(s),
        profiles: folio_package::ProfileFacts { cmyk: profile(&color["cmyk"]), rgb: profile(&color["rgb"]) },
    }
}

fn error(id: &Value, e: &folio_package::Error) -> Value {
    json!({"id": id, "ok": false, "error": e.message, "code": e.code, "suggestion": e.suggestion,
           "messageId": e.message_id, "messageArgs": e.message_args,
           "leftover": e.leftover.as_ref().map(|p| p.display().to_string())})
}

/// Lists for the dialog (short: the full lists are in the report).
fn preview(plan: &folio_package::Plan, preflight: &folio_preflight::Report) -> Value {
    let limit = 40;
    let missing: Vec<Value> = plan
        .link_rows
        .iter()
        .filter(|r| r.status == "missing" || r.status == "ambiguous")
        .take(limit)
        .map(|r| json!({"name": r.name, "status": r.status, "pageName": r.page_name}))
        .collect();
    let fonts: Vec<Value> = plan
        .font_rows
        .iter()
        .filter(|r| r.status != "copied")
        .take(limit)
        .map(|r| json!({"family": r.family, "style": r.style, "status": r.status, "code": r.code, "reason": r.reason, "substitute": r.substitute}))
        .collect();
    let mut not_copied: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for r in plan.font_rows.iter().filter(|r| r.status == "notCopied") {
        *not_copied.entry(r.code.unwrap_or("other")).or_default() += 1;
    }
    let s = &preflight.summary;
    let mut errors: Vec<(String, usize)> = Vec::new();
    for f in preflight.findings.iter().filter(|f| f.severity == folio_preflight::Severity::Error) {
        match errors.iter_mut().find(|e| e.0 == f.rule) {
            Some(e) => e.1 += 1,
            None => errors.push((f.rule.clone(), 1)),
        }
    }
    json!({
        "missingLinks": missing,
        "fonts": fonts,
        "notCopiedByCode": not_copied,
        "renamed": plan.links.iter().filter(|l| l.renamed).map(|l| json!({"from": l.original_name, "to": l.copy.dest})).take(limit).collect::<Vec<_>>(),
        "preflight": {"errors": s.errors, "warnings": s.warnings,
                      "errorRules": errors.iter().map(|(r, n)| json!({"rule": r, "title": folio_preflight::rules::title(r).unwrap_or(r), "count": n})).collect::<Vec<_>>()},
    })
}

/// `packagePlan`: what the dialog shows before anything is written.
pub fn plan_command(req: &Value, s: &Session) -> Value {
    let id = &req["id"];
    use_roots(req, s);
    let opts = options(req);
    let links = s.links();
    // the same checks as the preflight panel (image ink samples are cached per file)
    let preflight = s.preflight(true);
    let inp = inputs(s, &links, &preflight);
    let plan = match folio_package::plan(&inp, &opts, false) {
        Ok(p) => p,
        Err(e) => return error(id, &e),
    };
    let base = Path::new(&plan.document.dest).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let dir = req["dir"].as_str().filter(|d| !d.is_empty()).map(PathBuf::from);
    let dir_state = dir.as_ref().map(|d| match folio_package::check_destination(d, &inp, &opts) {
        Ok(()) => json!({"ok": true, "warnings": folio_package::destination_cautions(d, &inp, &opts)}),
        Err(e) => json!({"ok": false, "code": e.code, "error": e.message, "suggestion": e.suggestion,
                         "messageId": e.message_id, "messageArgs": e.message_args}),
    });
    json!({"id": id, "ok": true, "type": "packagePlan",
           "plan": {"counts": plan.counts(), "preview": preview(&plan, &preflight),
                    "folderName": format!("{base} 폴더"), "pdfPossible": s.cmyk_profile.is_some(),
                    "searchFolders": links.search.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
                    "dir": dir_state}})
}

/// Writes the PDF/X of the export settings in `export` (its preset; pages: all) to `path`.
fn pdf_maker<'a>(s: &'a Session, export: &'a Value) -> impl Fn(&Path) -> Result<folio_package::PdfOutcome, String> + Sync + 'a {
    move |path: &Path| {
        let (report, pages, ms) = crate::export_pdf(s, export, path, &|_| {})?;
        Ok(folio_package::PdfOutcome {
            bytes: report.bytes,
            pages,
            ms,
            standard: report.standard.clone(),
            preset: folio_pdf::Preset::parse(&report.preset).unwrap_or_default().indesign_preset().to_string(),
            flatten: report.flatten.as_ref().map(|f| folio_package::PdfFlatten {
                pages: f.pages,
                line_art_ppi: f.line_art_ppi,
                smooth_ppi: f.smooth_ppi,
                preview_graphics: f.preview_graphics,
            }),
            output_intent: report.output_intent.description.clone(),
            fonts_embedded: report.fonts_embedded.len(),
            fonts_substituted: report
                .fonts_missing
                .iter()
                .map(|f| format!("{} {} → {}", f.family, f.style, f.embedded.as_deref().unwrap_or("그리지 않음")))
                .collect(),
            warnings: report
                .warnings
                .iter()
                .map(|w| folio_package::PdfWarning {
                    kind: w.kind.clone(),
                    message: if w.count > 1 { format!("{} ({})", w.message, w.count) } else { w.message.clone() },
                })
                .collect(),
        })
    }
}

/// `package`: makes the folder. Returns the reply header.
pub fn package_command(req: &Value, s: &Session, out: Option<&Out>) -> Value {
    package_with(req, s, out, &|_| {})
}

/// `package` with each progress event also handed to `told` (File ▸ Package keeps it in the dialog's values).
pub fn package_with(req: &Value, s: &Session, out: Option<&Out>, told: &(dyn Fn(&Value) + Sync)) -> Value {
    let id = req["id"].clone();
    let started = Instant::now();
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut cur = current().lock().expect("shared state must remain unpoisoned after each operation");
        if cur.is_some() {
            return json!({"id": id, "ok": false, "code": "busy", "error": "이미 패키지를 만드는 중입니다"});
        }
        *cur = Some(flag.clone());
    }
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            *current().lock().expect("shared state must remain unpoisoned after each operation") = None;
        }
    }
    let _clear = Clear;

    let dir = PathBuf::from(req["dir"].as_str().unwrap_or(""));
    if dir.as_os_str().is_empty() {
        return json!({"id": id, "ok": false, "code": "noParent", "error": "패키지 폴더를 고르세요"});
    }
    let first = json!({"id": id, "event": "progress", "job": "package", "stage": "plan", "done": 0, "total": 0});
    if let Some(out) = out { send(out, first.clone(), &[]); }
    told(&first);
    use_roots(req, s);
    let opts = options(req);
    let links = s.links();
    let preflight = s.preflight(true);
    let inp = inputs(s, &links, &preflight);
    let plan = match folio_package::plan(&inp, &opts, true) {
        Ok(p) => p,
        Err(e) => return error(&id, &e),
    };
    let export = if req["export"].is_object() { req["export"].clone() } else { json!({"bleed": true, "marks": {"crop": true}}) };
    let maker = pdf_maker(s, &export);
    let last = Mutex::new(Instant::now() - std::time::Duration::from_secs(1));
    let progress = |p: &folio_package::Progress| {
        // the coordinator reports ~10 per second; stage changes always go out
        let mut l = last.lock().expect("shared state must remain unpoisoned after each operation");
        if p.stage == "copy" && l.elapsed().as_millis() < 90 {
            return;
        }
        *l = Instant::now();
        let event = json!({"id": id, "event": "progress", "job": "package", "stage": p.stage, "done": p.done, "total": p.total,
                           "files": p.files, "filesTotal": p.files_total, "pdf": p.pdf});
        if let Some(out) = out { send(out, event.clone(), &[]); }
        told(&event);
    };
    let pdf: Option<folio_package::PdfMaker> = if opts.pdf { Some(&maker) } else { None };
    match folio_package::run(&plan, &inp, &opts, &dir, pdf, &progress, &flag) {
        Ok(o) => {
            let r = &o.report;
            let pdf = match &o.pdf {
                Some(Ok(p)) => json!({"file": dir.join(&plan.pdf_name).display().to_string(), "bytes": p.bytes, "pages": p.pages, "ms": p.ms.round(),
                                      "fontsSubstituted": p.fonts_substituted.len(), "warnings": p.warnings.len(),
                                      "standard": p.standard, "preset": p.preset,
                                      "flatten": p.flatten.as_ref().map(|f| json!({"pages": f.pages, "lineArtPpi": f.line_art_ppi, "smoothPpi": f.smooth_ppi}))}),
                Some(Err(e)) => json!({"error": e}),
                None => Value::Null,
            };
            json!({"id": id, "ok": true, "type": "packaged", "result": {
                "dir": o.dir.display().to_string(),
                "report": o.dir.join(folio_package::INSTRUCTIONS).display().to_string(),
                "json": o.dir.join(folio_package::REPORT_JSON).display().to_string(),
                "files": o.files.len(),
                "bytes": o.bytes,
                "ms": (started.elapsed().as_secs_f64() * 1000.0).round(),
                "timings": o.timings,
                "document": {"name": plan.document.dest, "bytes": plan.document.bytes, "sha256": o.document_sha256},
                "links": r["links"]["summary"],
                "fonts": r["fonts"]["summary"],
                "preflight": {"errors": r["preflight"]["errors"], "warnings": r["preflight"]["warnings"]},
                "pdf": pdf,
                "failures": o.failures,
                "missingLinks": r["links"]["missing"].as_array().map(|a| a.iter().take(40).map(|x| x["name"].clone()).collect::<Vec<_>>()),
                "fontsNotCopied": plan.font_rows.iter().filter(|f| f.status == "notCopied").take(40)
                    .map(|f| json!({"family": f.family, "style": f.style, "code": f.code})).collect::<Vec<_>>(),
            }})
        }
        Err(e) => error(&id, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_error_keeps_legacy_text_and_message_metadata() {
        let e = folio_package::Error {
            code: "noParent",
            message: "저장할 폴더가 없습니다: 한글 {path}".into(),
            message_id: Some("package.destination.missingParent"),
            message_args: json!({"path": "한글 {path}"}),
            suggestion: None,
            leftover: None,
        };
        let response = error(&json!(7), &e);
        assert_eq!(response["id"], 7);
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"], e.message);
        assert_eq!(response["messageId"], "package.destination.missingParent");
        assert_eq!(response["messageArgs"]["path"], "한글 {path}");
    }
}
