//! Executes the import plan and rolls back staged project changes on failure.

use super::files::{Cleanup, Staged, stage};
use super::names::file_name;
use super::references::{References, assemble};
use super::report::import_report;
use super::{Error, Outcome, Plan, Prepared, Progress, Src, io};
use crate::format::{self, DocManifest, PartAction, Pin, SourceInfo};
use crate::hash;
use crate::idmlparts::{self, Edits};
use crate::manifest::Project;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Carries out `plan`. `progress` is called as files are placed; `cancel` stops it (nothing
/// is left in the project). `report_extra` goes into the import report (the document's
/// unsupported features, reader warnings).
pub fn run(project: &Project, p: &Prepared<'_>, plan: &Plan, report_extra: Value, cancel: &AtomicBool, progress: &dyn Fn(Progress)) -> Result<Outcome, Error> {
    let t0 = Instant::now();
    let root = &project.root;
    let target = plan.document.on_disk(root);
    if target.exists() {
        return Err(Error { code: "exists", message: format!("같은 이름의 문서가 이미 있습니다: {}", plan.document) });
    }
    let staging = project.data_dir(crate::manifest::TMP_DIR).join(format!("import-{}", crate::util::short_id()));
    std::fs::create_dir_all(&staging).map_err(io)?;
    let mut cleanup = Cleanup::new(staging);
    let total: u64 = plan.items.iter().filter(|i| !matches!(i.src, Src::None)).map(|i| i.bytes).sum::<u64>()
        + plan.items.iter().filter(|i| i.hash_only.is_some()).map(|i| i.bytes).sum::<u64>();
    let files_total = plan.items.iter().filter(|i| !matches!(i.src, Src::None) || i.hash_only.is_some()).count();
    let done = std::sync::atomic::AtomicU64::new(0);
    let files = std::sync::atomic::AtomicUsize::new(0);
    let tick = |stage: &'static str| progress(Progress { stage, done: done.load(Ordering::Relaxed), total, files: files.load(Ordering::Relaxed), files_total });
    let last = std::sync::Mutex::new(Instant::now());
    let add = |n: u64| {
        done.fetch_add(n, Ordering::Relaxed);
        let mut l = last.lock().expect("progress timestamp is only updated while holding this lock");
        if l.elapsed().as_millis() > 150 {
            *l = Instant::now();
            drop(l);
            tick("copy");
        }
    };
    folio_package::fsops::lower_thread_priority();
    tick("copy");

    // 1. stage every file and learn its content hash
    let Staged { placed, mut index, dedup, copy_ms } = stage(root, &cleanup.staging, plan, cancel, &add, &files)?;

    // 2. references and graphics
    let References { references, doc_id, gtable, edits_labels, edits_links, uri_refs } = assemble(p.doc, plan, &placed);

    // 3. place staged files, then write the .folio
    tick("place");
    cleanup.place(root, &placed, &mut index)?;
    let pins: Vec<Pin> = references
        .iter()
        .filter(|r| r.path.as_ref().is_some_and(|p| !p.as_str().starts_with(&format!("{}/", plan.assets))))
        .filter_map(|r| Some(Pin { reference: r.id.clone(), path: r.path.clone()?, sha256: r.sha256.clone()?, bytes: r.bytes?, kind: r.kind }))
        .collect();
    let now = crate::util::now_rfc3339();
    let saved_lines = match (p.reader_dir, p.reader_result) {
        (Some(d), Some(r)) => folio_idml::saved::sidecar_bytes(r, d),
        _ => None,
    };
    let report = import_report(plan, p.doc, &now, dedup, report_extra);
    let manifest = DocManifest {
        format: format::FORMAT.into(),
        version: format::VERSION,
        id: doc_id.clone(),
        project: project.file.id.clone(),
        created: now.clone(),
        saved: now,
        engine: env!("CARGO_PKG_VERSION").into(),
        source: Some(SourceInfo { name: file_name(p.source), kind: p.kind.into(), sha256: p.source_sha256.into(), bytes: p.source_bytes, reader: p.reader_result.and_then(|r| r.get("parser")).and_then(Value::as_str).unwrap_or("").into() }),
        link_uris: "folio-ref".into(),
        references,
        graphics: gtable,
        pins,
        saved_lines: saved_lines.as_ref().map(|_| format::PartRef { part: format::SAVED_LINES_PART.into(), format: folio_idml::saved::FORMAT.into(), version: folio_idml::saved::VERSION as u32 }),
        import_report: Some(format::PartRef { part: format::REPORT_PART.into(), format: "folio-import-report".into(), version: 1 }),
        extra: Default::default(),
    };
    tick("write");
    if let Some(parent) = target.parent() {
        cleanup.create_dirs(root, parent)?;
    }
    let doc_label = vec![(format::LABEL_DOCUMENT.to_string(), json!({"id": doc_id, "project": project.file.id, "format": format::VERSION}).to_string())];
    let report_bytes = serde_json::to_vec_pretty(&report).unwrap_or_default();
    let mut counts = idmlparts::Counts::default();
    let t_write = Instant::now();
    let mut other = |u: &str| {
        let id = uri_refs.get(u)?;
        let scheme = if u.starts_with("file:///") || u.starts_with("file://localhost/") {
            format::URL_REF_SCHEME
        } else if u.starts_with("file:") {
            format::REF_SCHEME
        } else {
            format::PATH_REF_SCHEME
        };
        Some(format!("{scheme}{id}"))
    };
    let mut edits = Edits { link_uris: edits_links, labels: edits_labels, document_labels: doc_label, other_uri: Some(&mut other), native_paths: true, ..Default::default() };
    let folio_bytes = format::write_folio(
        &target,
        p.idml,
        &mut |name, text| {
            if !name.ends_with(".xml") {
                return Ok(PartAction::Keep);
            }
            let t = text()?;
            if !idmlparts::worth_scanning(name, &t) {
                return Ok(PartAction::Keep);
            }
            let (out, c) = idmlparts::rewrite(name, &t, &mut edits);
            counts.add(&c);
            Ok(match out {
                Some(s) => PartAction::Replace(s),
                None => PartAction::Keep,
            })
        },
        &manifest,
        saved_lines.as_deref(),
        Some(&report_bytes),
    )
    .map_err(|e| Error::new("io", e.to_string()))?;
    let write_ms = t_write.elapsed().as_secs_f64() * 1000.0;
    cleanup.done = true;
    if let Ok(m) = std::fs::metadata(&target) {
        index.record(&plan.document, &target, &m, &hash::sha256_file(&target).map(|h| h.0).unwrap_or_default());
    }
    let _ = index.save();
    drop(cleanup);
    tick("done");
    let files_written = placed.iter().filter(|p| p.staged.is_some()).count();
    let bytes_written = placed.iter().filter(|p| p.staged.is_some()).filter_map(|p| p.bytes).sum();
    Ok(Outcome {
        document: plan.document.clone(),
        path: target,
        id: doc_id,
        folio_bytes,
        references: manifest.references.len(),
        files_written,
        bytes_written,
        deduplicated: dedup,
        links: json!({"uris": counts.link_uris, "labels": counts.labels, "otherUris": counts.other_uris}),
        timings: json!({"copyMs": copy_ms, "writeMs": write_ms, "totalMs": t0.elapsed().as_secs_f64() * 1000.0}),
        report,
    })
}
