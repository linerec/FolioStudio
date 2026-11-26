//! IDML export of a `.folio`: the stored IDML parts with every Folio reference turned back
//! into what InDesign reads, plus a report of what InDesign will not get.
//!
//! * `folio:ref/<id>` → the absolute `file:` URI of the file on this machine (InDesign's
//!   `file:C:/…` form); `folio:url-ref/<id>` (XMP) → `file:///…`. References not in the
//!   project get their original URI (the hint).
//! * Links whose content is unchanged since import (same SHA-256) get a `LinkImportStamp`
//!   and `LinkResourceSize` of the file as it is now, so InDesign does not report them as
//!   modified merely because the project was copied; changed files keep the old stamp.
//! * Embedded and pasted files that the INDD reader extracted are written back as
//!   `Properties/Contents`, as InDesign's own IDML holds them.
//! * `folio.*` labels stay (they survive InDesign: interop-labels evidence).

use crate::format::{self, PartAction, RefKind};
use crate::idmlparts::{self, Edits};
use crate::open::Opened;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

/// FILETIME ticks (100 ns since 1601) of a Unix time.
fn filetime(unix_secs: f64) -> u64 {
    ((unix_secs + 11_644_473_600.0) * 1e7).round().max(0.0) as u64
}

fn hex_size(n: u64) -> String {
    format!("{:x}~{:x}", n >> 32, n & 0xFFFF_FFFF)
}

/// Writes the IDML of `o` to `dest` (atomically). Returns the export report.
/// What to write besides the IDML parts.
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// Write extracted embedded files back as `Contents` (default). Off: a lean IDML whose
    /// embedded and pasted graphics InDesign shows without their data.
    pub contents: bool,
    /// Only `Contents` of these formats (by first bytes): `None` = all.
    pub contents_raster_only: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { contents: true, contents_raster_only: false }
    }
}

pub fn export_idml(o: &Opened, dest: &Path, saved_line_stories: usize) -> Result<Value, format::Error> {
    export_idml_with(o, dest, saved_line_stories, ExportOptions::default(), None)
}

/// [`export_idml`] with options.
/// `edited`: parts an editor changed (their text, or None when removed), written instead of
/// the stored ones.
pub fn export_idml_with(o: &Opened, dest: &Path, saved_line_stories: usize, opts: ExportOptions, edited: Option<&std::collections::BTreeMap<String, Option<String>>>) -> Result<Value, format::Error> {
    let started = Instant::now();
    let m = &o.manifest;
    let state: HashMap<&str, &crate::open::RefState> = o.refs.iter().map(|r| (r.id.as_str(), r)).collect();
    // reference id → URI written to IDML
    // reference id → (InDesign `file:` URI, XMP `file:///` URL, native path)
    let mut uri_of: HashMap<String, (String, String, String)> = HashMap::new();
    let (mut found, mut hinted, mut unresolved, mut modified) = (0usize, 0usize, 0usize, 0usize);
    for r in &m.references {
        let st = state.get(r.id.as_str());
        match st.and_then(|s| s.file.as_ref()) {
            // a content link to a document imported from an INDD goes back to InDesign's own address of that file (InDesign cannot open a
            // `.folio`): the hint, as it was found
            Some(_) if r.extra.contains_key("document") && r.hint.is_some() => {
                let h = r.hint.clone().unwrap_or_default();
                uri_of.insert(r.id.clone(), (h.clone(), h.clone(), h));
            }
            Some(f) => {
                uri_of.insert(r.id.clone(), (crate::paths::file_uri(f), crate::paths::file_url(f), f.display().to_string()));
            }
            None => {
                // a hint is written back exactly as it was found (Link or XMP form)
                if let Some(h) = &r.hint {
                    uri_of.insert(r.id.clone(), (h.clone(), h.clone(), h.clone()));
                }
            }
        }
    }
    let mut link_uris: HashMap<String, String> = HashMap::new();
    let mut link_attrs: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut contents_for: HashSet<String> = HashSet::new();
    let mut contents_file: HashMap<String, std::path::PathBuf> = HashMap::new();
    for g in &m.graphics {
        if let (Some(ls), Some(rid)) = (&g.link_self, &g.link) {
            match uri_of.get(rid) {
                Some((uri, _, _)) => {
                    link_uris.insert(ls.clone(), uri.clone());
                }
                None => unresolved += 1,
            }
            let modified_at_import = m.reference(rid).is_some_and(|r| r.status.as_deref() == Some("modified"));
            if let Some(st) = state.get(rid.as_str()) {
                match (st.kind, st.state, st.size, st.mtime) {
                    (RefKind::Link, "ok" | "moved", _, _) if modified_at_import => modified += 1,
                    (RefKind::Link, "ok" | "moved", Some(size), Some(t)) => {
                        found += 1;
                        link_attrs.insert(
                            ls.clone(),
                            vec![
                                ("LinkImportStamp".into(), format!("file {} {size}", filetime(t))),
                                ("LinkResourceSize".into(), hex_size(size)),
                                ("LinkResourceModified".into(), "false".into()),
                            ],
                        );
                    }
                    (RefKind::Link, _, _, _) if st.file.is_none() => hinted += 1,
                    _ => {}
                }
            }
        }
        if let Some(eid) = &g.embedded
            && opts.contents
            && let Some(f) = state.get(eid.as_str()).and_then(|s| s.file.clone())
            && (!opts.contents_raster_only || is_raster_file(&f))
        {
            contents_for.insert(g.self_id.clone());
            contents_file.insert(g.self_id.clone(), f);
        }
    }
    let read_contents = |id: &str| contents_file.get(id).and_then(|f| std::fs::read(f).ok());
    let mut other = |u: &str| {
        let (form, id) = if let Some(id) = u.strip_prefix(format::URL_REF_SCHEME) {
            (1, id)
        } else if let Some(id) = u.strip_prefix(format::PATH_REF_SCHEME) {
            (2, id)
        } else {
            (0, u.strip_prefix(format::REF_SCHEME)?)
        };
        let (uri, url, native) = uri_of.get(id)?;
        Some(match form {
            1 => url.clone(),
            2 => native.clone(),
            _ => uri.clone(),
        })
    };
    let mut edits = Edits {
        link_uris,
        link_attrs,
        contents: Some(&read_contents),
        contents_for,
        other_uri: Some(&mut other),
        prefixes: vec![format::URL_REF_SCHEME, format::PATH_REF_SCHEME, format::REF_SCHEME],
        ..Default::default()
    };
    let mut counts = idmlparts::Counts::default();
    let mut left: Vec<String> = Vec::new();
    let src = std::fs::File::open(&o.file)?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(src))?;
    let added: Vec<_> = edited.into_iter().flat_map(|e| e.iter())
        .filter_map(|(name, value)| value.as_deref().map(|t| (name.as_str(), t))).collect();
    let mut failure: Option<format::Error> = None;
    // principle 2, item 3: which of the known differences from InDesign this document uses (the parts are read anyway)
    let mut limits = crate::limits::Scanner::new();
    crate::atomic::write_file_with(dest, &mut |file| {
        let w = std::io::BufWriter::with_capacity(1 << 20, file);
        let r = format::write_package_with_parts(
            w,
            &mut archive,
            &mut |name, text| {
                let edit = edited.and_then(|e| e.get(name));
                if let Some(None) = edit {
                    return Ok(PartAction::Drop);
                }
                if !name.ends_with(".xml") {
                    return Ok(PartAction::Keep);
                }
                let t = match edit {
                    Some(Some(t)) => t.clone(),
                    _ => text()?,
                };
                limits.feed(name, &t);
                if !idmlparts::worth_scanning(name, &t) {
                    return Ok(if edit.is_some() { PartAction::Replace(t) } else { PartAction::Keep });
                }
                let (out, c) = idmlparts::rewrite(name, &t, &mut edits);
                counts.add(&c);
                let final_text = out.as_deref().unwrap_or(&t);
                if final_text.contains(format::REF_SCHEME) || final_text.contains(format::URL_REF_SCHEME) || final_text.contains(format::PATH_REF_SCHEME) {
                    left.push(name.to_string());
                }
                Ok(match out {
                    Some(s) => PartAction::Replace(s),
                    None if edit.is_some() => PartAction::Replace(t),
                    None => PartAction::Keep,
                })
            },
            &[],
            &added,
        );
        match r {
            Ok(mut w) => std::io::Write::flush(&mut w),
            Err(e) => {
                let msg = e.to_string();
                failure = Some(e);
                Err(std::io::Error::other(msg))
            }
        }
    })
    .map_err(|e| failure.take().unwrap_or(format::Error::Io(e)))?;
    let bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    let missing_previews = m.graphics.iter().filter(|g| g.preview.is_some() && g.link.as_ref().and_then(|l| state.get(l.as_str())).is_some_and(|s| s.file.is_none() && s.kind == RefKind::Link)).count();
    let pasted_without_file = m.graphics.iter().filter(|g| g.link.is_none() && g.embedded.is_none() && g.preview.is_some()).count();
    Ok(json!({
        "format": "folio-idml-export-report", "version": 1,
        "document": o.rel, "idml": dest.file_name().map(|n| n.to_string_lossy().into_owned()), "bytes": bytes,
        "ms": started.elapsed().as_secs_f64() * 1000.0,
        "links": {"found": found, "modifiedAtImport": modified, "notInProject": hinted, "unresolved": unresolved, "uris": counts.link_uris, "stamps": counts.link_attrs},
        "contents": {"written": counts.contents},
        "otherUris": counts.other_uris,
        "labels": "kept (folio.ref on graphics, folio.document on the document)",
        "placeholdersLeft": left,
        "limits": crate::limits::report(&limits.hits()),
        // principle 2: what InDesign will not get, and how it differs
        "differences": [
            {"what": "savedLines", "count": saved_line_stories,
             "effect": "IDML에는 InDesign이 저장한 줄(조판 결과)이 없습니다. InDesign은 IDML을 열 때 모든 기사를 다시 조판하므로, 이 기사들의 줄바꿈·넘침은 INDD와 달라질 수 있습니다."},
            {"what": "missingLinkPreviews", "count": missing_previews,
             "effect": "가져올 때 없던 링크는 원래 주소만 적힙니다. IDML에는 미리보기가 없으므로 InDesign에서 빈 프레임(링크 없음)으로 보이고 인쇄되지 않습니다."},
            {"what": "modifiedLinks", "count": modified,
             "effect": "가져올 때 찾은 파일이 InDesign이 배치한 파일과 크기가 달랐던 링크는 InDesign의 기록을 그대로 두어 '수정됨'으로 보입니다. Folio 화면은 INDD에 저장된 미리보기를, PDF는 찾은 파일을 씁니다."},
            {"what": "pastedWithoutFile", "count": pasted_without_file,
             "effect": "붙여 넣은 그림 중 원본 데이터를 꺼내지 못한 것은 InDesign에 내용이 전달되지 않습니다."},
            {"what": "projectData", "count": m.references.len(),
             "effect": "프로젝트 기준 경로·해시·고정(pins)은 folio.* 스크립트 레이블로만 전달됩니다(InDesign 화면에는 보이지 않음)."},
            {"what": "indd", "count": 1,
             "effect": "Folio는 INDD를 직접 쓰지 않습니다. INDD가 필요하면 이 IDML을 InDesign에서 열어 INDD로 저장하세요."}
        ],
    }))
}

/// A raster image file InDesign places (JPEG, PNG, TIFF, PSD, GIF, BMP) by its first bytes.
fn is_raster_file(p: &Path) -> bool {
    let mut b = [0u8; 8];
    let n = std::fs::File::open(p).and_then(|mut f| std::io::Read::read(&mut f, &mut b)).unwrap_or(0);
    let h = &b[..n];
    h.starts_with(&[0xFF, 0xD8])
        || h.starts_with(&[0x89, b'P', b'N', b'G'])
        || h.starts_with(&[b'I', b'I', b'*', 0])
        || h.starts_with(&[b'M', b'M', 0, b'*'])
        || h.starts_with(b"8BPS")
        || h.starts_with(b"GIF8")
        || h.starts_with(b"BM")
}
