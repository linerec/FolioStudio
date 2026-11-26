//! The package reports: `package-report.json` holds the facts; `Instructions.txt` is the
//! same facts written for a print-house operator (Korean, UTF-8 with BOM, CRLF), rendered
//! from the JSON so the two can never disagree.

use crate::{Failure, Inputs, LOW_PPI, Options, PdfOutcome, Plan, Timings, Written};
use folio_preflight::Severity;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

const SEVERITY: [&str; 3] = ["error", "warning", "info"];

pub struct Facts<'a> {
    pub plan: &'a Plan,
    pub inp: &'a Inputs<'a>,
    pub opts: &'a Options,
    pub dest: &'a Path,
    pub written: &'a [Written],
    pub failures: &'a [Failure],
    pub copied_links: &'a [bool],
    pub copied_fonts: &'a [bool],
    pub pdf: Option<&'a Result<PdfOutcome, String>>,
    pub document_sha256: Option<&'a str>,
    pub created: crate::time::Stamp,
    pub timings: &'a Timings,
}

fn mm(pt: f64) -> f64 {
    (pt * 25.4 / 72.0 * 10.0).round() / 10.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Page names of document page indexes, in page order, de-duplicated.
fn page_list(indexes: &[usize], names: &HashMap<usize, String>) -> Vec<String> {
    let mut v: Vec<usize> = indexes.to_vec();
    v.sort_unstable();
    v.dedup();
    v.iter().map(|i| names.get(i).cloned().unwrap_or_else(|| (i + 1).to_string())).collect()
}

pub fn json(f: &Facts) -> Value {
    let (plan, inp, doc) = (f.plan, f.inp, f.inp.doc);
    let names = crate::page_names(doc);

    // document
    let mut sizes: Vec<(f64, f64, Vec<usize>)> = Vec::new();
    let mut pages: Vec<(usize, f64, f64)> =
        doc.spreads.iter().flat_map(|s| s.pages.iter()).map(|p| (p.index, p.transform.apply_rect(&p.bounds))).map(|(i, r)| (i, r.width(), r.height())).collect();
    pages.sort_by_key(|p| p.0);
    for (i, w, h) in &pages {
        match sizes.iter_mut().find(|s| (s.0 - mm(*w)).abs() < 0.05 && (s.1 - mm(*h)).abs() < 0.05) {
            Some(s) => s.2.push(*i),
            None => sizes.push((mm(*w), mm(*h), vec![*i])),
        }
    }
    let b = doc.print.bleed;
    let sl = doc.print.slug;
    let profile = |p: &Option<crate::Profile>| {
        p.as_ref().map(|p| json!({"name": p.name, "status": p.status, "file": p.file.as_ref().map(|f| f.display().to_string())}))
    };
    let document = json!({
        "name": plan.document.dest,
        "source": plan.document.source.display().to_string(),
        "bytes": plan.document.bytes,
        "sha256": f.document_sha256,
        "pages": pages.len(),
        "pageSizes": sizes.iter().map(|(w, h, p)| json!({"widthMm": w, "heightMm": h, "count": p.len(), "pages": page_list(p, &names)})).collect::<Vec<_>>(),
        "facingPages": doc.print.facing_pages,
        "bleedMm": {"top": mm(b[0]), "bottom": mm(b[1]), "inside": mm(b[2]), "outside": mm(b[3])},
        "slugMm": {"top": mm(sl[0]), "bottom": mm(sl[1]), "inside": mm(sl[2]), "outside": mm(sl[3])},
        "profiles": {"cmyk": profile(&inp.profiles.cmyk), "rgb": profile(&inp.profiles.rgb),
                     "documentCmyk": doc.print.cmyk_profile, "documentRgb": doc.print.rgb_profile},
        "stories": doc.stories.len(),
        "oversetStories": inp.layout.overset.len(),
    });

    // inks
    let process: Vec<Value> = inp.inks.plates.iter().filter(|p| !p.spot).map(|p| json!({"name": p.name, "pages": p.pages.len()})).collect();
    let spots: Vec<Value> = inp
        .inks
        .plates
        .iter()
        .filter(|p| p.spot)
        .map(|p| json!({"name": p.name, "inkType": p.ink_type, "pages": page_list(&p.pages, &names), "aliases": p.aliases}))
        .collect();
    let manager: Vec<Value> = doc
        .print
        .inks
        .iter()
        .map(|i| json!({"name": i.name, "process": i.process, "convertToProcess": i.convert_to_process, "alias": i.alias, "print": i.print, "inkType": i.ink_type}))
        .collect();
    let inks = json!({"process": process, "spots": spots, "convertedToProcess": inp.inks.converted, "inkManager": manager, "overprintBlack": doc.print.overprint_black});

    // fonts
    let count = |s: &str| plan.font_rows.iter().filter(|r| r.status == s).count();
    let font_files: Vec<Value> = plan
        .fonts
        .iter()
        .zip(f.copied_fonts)
        .map(|(ff, &ok)| json!({"path": ff.copy.dest, "source": ff.copy.source.display().to_string(), "bytes": ff.copy.bytes, "faces": ff.faces, "origin": ff.origin, "copied": ok}))
        .collect();
    let fonts = json!({
        "summary": {"used": plan.font_rows.len(), "copied": count("copied"), "notCopied": count("notCopied"), "notSelected": count("notSelected"),
                    "missing": count("missing"), "substituted": count("substituted"), "files": plan.fonts.len(),
                    "cjkFontsCopied": f.opts.cjk_fonts},
        "rows": plan.font_rows,
        "files": font_files,
        "placedNotEmbedded": plan.placed_fonts,
    });

    // links
    let failed: std::collections::HashSet<&str> = f.failures.iter().map(|x| x.dest.as_str()).collect();
    let mut pages_of: Vec<Vec<usize>> = vec![Vec::new(); plan.links.len()];
    for r in &plan.link_rows {
        if let (Some(i), Some(p)) = (r.file, r.page) {
            pages_of[i].push(p);
        }
    }
    let link_files: Vec<Value> = plan
        .links
        .iter()
        .enumerate()
        .map(|(i, l)| {
            json!({"path": l.copy.dest, "originalName": l.original_name, "renamed": l.renamed, "source": l.copy.source.display().to_string(),
                   "sameAs": l.same_as.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(), "bytes": l.copy.bytes,
                   "placements": l.owners.len(), "pages": page_list(&pages_of[i], &names),
                   "copied": f.copied_links.get(i).copied().unwrap_or(false) && !failed.contains(l.copy.dest.as_str())})
        })
        .collect();
    let rows_with = |pred: &dyn Fn(&crate::LinkRow) -> bool| -> Vec<Value> {
        // one line per recorded file; pages of every placement
        let mut by: BTreeMap<(String, String), (Vec<usize>, &crate::LinkRow, usize)> = BTreeMap::new();
        for r in plan.link_rows.iter().filter(|r| pred(r)) {
            let e = by.entry((r.name.clone(), r.recorded.clone())).or_insert((Vec::new(), r, 0));
            e.2 += 1;
            if let Some(p) = r.page {
                e.0.push(p);
            }
        }
        let mut out: Vec<(usize, Value)> = by
            .into_values()
            .map(|(p, r, placements)| {
                let first = p.iter().min().copied().unwrap_or(usize::MAX);
                (
                    first,
                    json!({"name": r.name, "recorded": r.recorded, "pages": page_list(&p, &names), "packaged": r.packaged, "placements": placements,
                           "source": r.source.as_ref().map(|s| s.display().to_string()), "ppi": r.ppi.map(round1), "space": r.space,
                           "candidates": r.candidates.iter().map(|c| c.display().to_string()).collect::<Vec<_>>(), "how": r.how}),
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1["name"].as_str().cmp(&b.1["name"].as_str())));
        out.into_iter().map(|x| x.1).collect()
    };
    let lc = |s: &str| plan.link_rows.iter().filter(|r| r.status == s).count();
    let renamed: Vec<Value> = plan
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| l.renamed)
        .map(|(i, l)| json!({"originalName": l.original_name, "packaged": l.copy.dest, "source": l.copy.source.display().to_string(), "pages": page_list(&pages_of[i], &names)}))
        .collect();
    let links = json!({
        "summary": {"placed": plan.link_rows.len(), "files": plan.links.len(), "copied": link_files.iter().filter(|l| l["copied"] == true).count(),
                    "found": lc("found"), "modified": lc("modified"), "missing": lc("missing"), "ambiguous": lc("ambiguous"), "embedded": lc("embedded"),
                    "renamed": renamed.len(), "sameContent": plan.links.iter().map(|l| l.same_as.len()).sum::<usize>(), "failed": f.failures.iter().filter(|x| x.dest.starts_with(crate::LINKS_DIR)).count(),
                    "bytes": plan.links.iter().map(|l| l.copy.bytes).sum::<u64>(), "collected": f.opts.links},
        "missing": rows_with(&|r| r.status == "missing"),
        "ambiguous": rows_with(&|r| r.status == "ambiguous"),
        "modified": rows_with(&|r| r.status == "modified"),
        "embedded": rows_with(&|r| r.status == "embedded"),
        "nameOnly": rows_with(&|r| r.how == Some("name")),
        "lowResolution": rows_with(&|r| r.ppi.is_some_and(|p| p < LOW_PPI)),
        "rgb": rows_with(&|r| r.space.as_deref() == Some("Rgb")),
        "nfdNames": rows_with(&|r| r.nfd_name),
        "renamed": renamed,
        "nested": plan.nested,
        "files": link_files,
        "placed": plan.link_rows,
        "searchFolders": inp.links.search.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(),
        "failed": f.failures,
    });

    // preflight
    let order: HashMap<&str, usize> = folio_preflight::rules::ALL.iter().enumerate().map(|(i, r)| (*r, i)).collect();
    let mut by_rule: BTreeMap<&str, [usize; 3]> = BTreeMap::new();
    for x in &inp.preflight.findings {
        let c = by_rule.entry(x.rule.as_str()).or_default();
        c[match x.severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        }] += 1;
    }
    let mut rules: Vec<(&str, [usize; 3])> = by_rule.into_iter().collect();
    let worst = |c: &[usize; 3]| c.iter().position(|&n| n > 0).unwrap_or(2);
    rules.sort_by_key(|(r, c)| (worst(c), order.get(r).copied().unwrap_or(usize::MAX)));
    let s = &inp.preflight.summary;
    let preflight = json!({
        "errors": s.errors, "warnings": s.warnings, "infos": s.infos,
        "rules": rules.iter().map(|(r, c)| json!({"rule": r, "title": folio_preflight::rules::title(r).unwrap_or(r),
                                                    "severity": SEVERITY[worst(c)],
                                                    "errors": c[0], "warnings": c[1], "infos": c[2]})).collect::<Vec<_>>(),
        "pagesWithErrors": page_list(&s.pages_with_errors, &names),
        "limitations": inp.preflight.limitations,
    });

    let pdf = match f.pdf {
        Some(Ok(p)) => json!({"file": plan.pdf_name, "bytes": p.bytes, "pages": p.pages, "standard": p.standard, "preset": p.preset, "flatten": p.flatten, "outputIntent": p.output_intent,
                              "fontsEmbedded": p.fonts_embedded, "fontsSubstituted": p.fonts_substituted, "warnings": p.warnings, "ms": p.ms.round(),
                              "settings": f.opts.pdf_settings}),
        Some(Err(e)) => json!({"file": Value::Null, "error": e, "settings": f.opts.pdf_settings}),
        None => Value::Null,
    };

    let package_bytes: u64 = f.written.iter().map(|w| w.bytes).sum();
    // the two reports are in the folder too (their sizes are not known while writing them)
    let mut files: Vec<Value> = f.written.iter().map(|w| json!(w)).collect();
    for name in [crate::INSTRUCTIONS, crate::REPORT_JSON] {
        files.push(json!({"path": name, "bytes": Value::Null, "sha256": Value::Null}));
    }
    let ins = &f.opts.instructions;
    json!({
        "format": "folio-package-report",
        "version": 1,
        "created": f.created.iso,
        "createdText": f.created.text,
        "folio": {"app": f.opts.app_version, "engine": f.opts.engine_version},
        "instructions": {"contact": ins.contact, "company": ins.company, "phone": ins.phone, "email": ins.email, "notes": ins.notes},
        "options": {"links": f.opts.links, "fonts": f.opts.fonts, "pdf": f.opts.pdf, "cjkFonts": f.opts.cjk_fonts, "copy": f.opts.copy, "jobs": f.opts.jobs},
        "document": document,
        "inks": inks,
        "fonts": fonts,
        "links": links,
        "preflight": preflight,
        "pdf": pdf,
        "package": {"folder": f.dest.display().to_string(), "files": files, "fileCount": files.len(), "bytes": package_bytes,
                    "verify": match f.opts.copy {
                        crate::CopyMethod::Os => "size + unchanged source; SHA-256 of the document copy",
                        crate::CopyMethod::Stream => "size + unchanged source; SHA-256 of every copied file",
                    },
                    "reports": [crate::INSTRUCTIONS, crate::REPORT_JSON]},
        "timings": f.timings,
    })
}

// ---------------------------------------------------------------------------------------
// Instructions.txt

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn n(v: &Value) -> u64 {
    v.as_u64().unwrap_or(0)
}

fn mb(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.2} GB", bytes as f64 / (1u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", (bytes as f64 / 1024.0).ceil())
    }
}

fn grouped(v: u64) -> String {
    let d = v.to_string();
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// "3, 5, 12쪽" (at most `max` names, then "외 n쪽"); "쪽 밖(대지·마스터)" when empty.
fn pages_text(v: &Value, max: usize) -> String {
    let list: Vec<&str> = v.as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if list.is_empty() {
        return "쪽 밖(대지·마스터)".into();
    }
    let shown = list.iter().take(max).copied().collect::<Vec<_>>().join(", ");
    if list.len() > max { format!("{shown}쪽 외 {}쪽", list.len() - max) } else { format!("{shown}쪽") }
}

fn mm_text(v: &Value) -> String {
    let f = |k: &str| v[k].as_f64().unwrap_or(0.0);
    let all = [f("top"), f("bottom"), f("inside"), f("outside")];
    if all.iter().all(|x| *x == 0.0) {
        return "없음".into();
    }
    if all.iter().all(|x| *x == all[0]) {
        return format!("{} mm (네 방향)", all[0]);
    }
    format!("위 {} · 아래 {} · 안쪽 {} · 바깥쪽 {} mm", all[0], all[1], all[2], all[3])
}

fn profile_text(v: &Value, fallback: &Value) -> String {
    if v.is_null() {
        return match fallback.as_str() {
            Some(name) => format!("{name} (상태 모름)"),
            None => "문서에 없음".into(),
        };
    }
    let name = v["name"].as_str().or(fallback.as_str()).unwrap_or("이름 없음");
    let state = match s(&v["status"]) {
        "used" => "이 컴퓨터에 있음".to_string(),
        "display" => "화면 색 공간과 같아 그대로 씀".to_string(),
        "missing" => "이 컴퓨터에 없음".to_string(),
        "notNamed" => "문서에 이름 없음".to_string(),
        other => other.to_string(),
    };
    format!("{name} ({state})")
}

pub fn text(r: &Value) -> String {
    let mut o = String::new();
    macro_rules! ln { ($($t:tt)*) => { let _ = writeln!(o, $($t)*); } }
    let doc = &r["document"];
    let links = &r["links"];
    let ls = &links["summary"];
    let fonts = &r["fonts"];
    let fs = &fonts["summary"];
    let pf = &r["preflight"];
    let pdf = &r["pdf"];

    ln!("Folio Studio 패키지 보고서");
    ln!("==========================");
    ln!("문서: {}", s(&doc["name"]));
    ln!("만든 때: {}", s(&r["createdText"]));
    ln!("만든 프로그램: Folio Studio {} (엔진 {})", s(&r["folio"]["app"]), s(&r["folio"]["engine"]));
    ln!("");

    let ins = &r["instructions"];
    ln!("[인쇄 지시 사항]");
    for (k, label) in [("contact", "담당자"), ("company", "회사"), ("phone", "전화"), ("email", "전자우편")] {
        ln!("{label}: {}", if s(&ins[k]).is_empty() { "-" } else { s(&ins[k]) });
    }
    if s(&ins["notes"]).trim().is_empty() {
        ln!("메모: -");
    } else {
        ln!("메모:");
        for l in s(&ins["notes"]).lines() {
            ln!("  {l}");
        }
    }
    ln!("");

    // what needs attention first
    ln!("[먼저 확인할 것]");
    let mut alerts = Vec::new();
    if n(&ls["missing"]) + n(&ls["ambiguous"]) > 0 {
        let files = |k: &str| links[k].as_array().map_or(0, Vec::len);
        alerts.push(format!("없는 링크 파일 {}개(배치 {}곳){} — 패키지에 들어 있지 않습니다. 원본 파일을 따로 받아야 합니다(아래 [링크]).", files("missing"), n(&ls["missing"]),
            if n(&ls["ambiguous"]) > 0 { format!(", 후보가 여러 개라 고르지 않은 링크 파일 {}개(배치 {}곳)", files("ambiguous"), n(&ls["ambiguous"])) } else { String::new() }));
    }
    if n(&ls["failed"]) > 0 {
        alerts.push(format!("복사하지 못한 링크 {}개 (원본을 읽을 수 없음).", n(&ls["failed"])));
    }
    if n(&ls["renamed"]) > 0 {
        alerts.push(format!("이름이 겹쳐 이름을 바꿔 넣은 링크 {}개 — InDesign에서 해당 링크를 Links 폴더의 새 이름 파일로 다시 연결하세요.", n(&ls["renamed"])));
    }
    if n(&ls["modified"]) > 0 {
        alerts.push(format!("수정된 링크 {}곳 — 문서에 기록된 뒤 파일이 바뀌었습니다. 패키지에는 지금 파일이 들어 있습니다.", n(&ls["modified"])));
    }
    if n(&fs["missing"]) + n(&fs["substituted"]) > 0 {
        alerts.push(format!("없는 글꼴 {}개, 다른 굵기로 대체된 글꼴 {}개 — 인쇄소에 해당 글꼴이 있어야 합니다.", n(&fs["missing"]), n(&fs["substituted"])));
    }
    if n(&fs["notCopied"]) > 0 {
        alerts.push(format!("사용권 때문에 복사하지 않은 글꼴 {}개 — 인쇄소에서 활성화·설치해야 합니다(아래 [글꼴]).", n(&fs["notCopied"])));
    }
    if n(&pf["errors"]) > 0 {
        alerts.push(format!("사전 검사 오류 {}개 (아래 [사전 검사]).", n(&pf["errors"])));
    }
    if pdf.get("error").is_some() {
        alerts.push(format!("PDF를 만들지 못했습니다: {}", s(&pdf["error"])));
    }
    if alerts.is_empty() {
        ln!("- 없음: 링크와 글꼴이 모두 들어 있고 사전 검사 오류가 없습니다.");
    }
    for a in alerts {
        ln!("- {a}");
    }
    ln!("");

    ln!("[문서]");
    ln!("파일: {} ({} 바이트, {})", s(&doc["name"]), grouped(n(&doc["bytes"])), mb(n(&doc["bytes"])));
    if let Some(h) = doc["sha256"].as_str() {
        ln!("SHA-256: {h}");
    }
    ln!("원래 위치: {}", s(&doc["source"]));
    ln!("쪽: {}쪽{}", n(&doc["pages"]), if doc["facingPages"] == true { " (펼침면)" } else { "" });
    for sz in doc["pageSizes"].as_array().into_iter().flatten() {
        let list = sz["pages"].as_array().map(|a| a.len()).unwrap_or(0);
        let range = sz["pages"].as_array().map(|a| {
            let first = a.first().and_then(Value::as_str).unwrap_or("");
            let last = a.last().and_then(Value::as_str).unwrap_or("");
            if list > 1 { format!("{first}–{last}쪽") } else { format!("{first}쪽") }
        });
        let f = |v: &Value| v.as_f64().unwrap_or(0.0);
        ln!("크기: {} × {} mm — {}쪽 ({})", f(&sz["widthMm"]), f(&sz["heightMm"]), n(&sz["count"]), range.unwrap_or_default());
    }
    ln!("도련: {}", mm_text(&doc["bleedMm"]));
    ln!("슬러그: {}", mm_text(&doc["slugMm"]));
    ln!("CMYK 프로파일: {}", profile_text(&doc["profiles"]["cmyk"], &doc["profiles"]["documentCmyk"]));
    ln!("RGB 프로파일: {}", profile_text(&doc["profiles"]["rgb"], &doc["profiles"]["documentRgb"]));
    ln!("넘친 기사: {}개", n(&doc["oversetStories"]));
    ln!("");

    let inks = &r["inks"];
    ln!("[잉크·별색]");
    let process: Vec<&str> = inks["process"].as_array().into_iter().flatten().filter(|p| n(&p["pages"]) > 0).filter_map(|p| p["name"].as_str()).collect();
    ln!("원색 판: {}", if process.is_empty() { "쓰이지 않음".to_string() } else { process.join(", ") });
    let spots = inks["spots"].as_array().cloned().unwrap_or_default();
    if spots.is_empty() {
        ln!("별색 판: 없음");
    } else {
        ln!("별색 판 {}개:", spots.len());
        for sp in &spots {
            let used = sp["pages"].as_array().is_some_and(|a| !a.is_empty());
            let alias = sp["aliases"].as_array().filter(|a| !a.is_empty()).map(|a| format!(" · 별칭으로 이 판에: {}", a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))).unwrap_or_default();
            ln!("  {} — {}{}", s(&sp["name"]), if used { pages_text(&sp["pages"], 20) } else { "쓰이지 않음".into() }, alias);
        }
    }
    let conv: Vec<&str> = inks["convertedToProcess"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !conv.is_empty() {
        ln!("원색으로 바뀌는 별색(잉크 관리자): {}", conv.join(", "));
    }
    ln!("[검정] 100% 오버프린트: {}", if inks["overprintBlack"] == true { "켬" } else { "끔" });
    ln!("");

    ln!("[글꼴] 쓰인 글꼴 {}개 — 복사 {} · 복사하지 않음 {} · 없음 {} · 대체됨 {}{}", n(&fs["used"]), n(&fs["copied"]), n(&fs["notCopied"]), n(&fs["missing"]), n(&fs["substituted"]),
        if n(&fs["notSelected"]) > 0 { format!(" · 고르지 않아 넣지 않음 {}", n(&fs["notSelected"])) } else { String::new() });
    let rows = fonts["rows"].as_array().cloned().unwrap_or_default();
    let section = |o: &mut String, title: &str, status: &str, detail: &dyn Fn(&Value) -> String| {
        let list: Vec<&Value> = rows.iter().filter(|x| x["status"] == status).collect();
        if list.is_empty() {
            return;
        }
        let _ = writeln!(o, "{title} {}개:", list.len());
        for x in list {
            let _ = writeln!(o, "  {} {} — {}", s(&x["family"]), s(&x["style"]), detail(x));
        }
    };
    section(&mut o, "복사한 글꼴(Document fonts 폴더)", "copied", &|x| s(&x["file"]).trim_start_matches("Document fonts/").to_string());
    section(&mut o, "복사하지 않은 글꼴", "notCopied", &|x| format!("{} [{}]", s(&x["reason"]), s(&x["file"])));
    section(&mut o, "없는 글꼴(이 컴퓨터에 없음 — Folio는 대체 글꼴로 그림)", "missing", &|x| format!("대체: {}", s(&x["substitute"])));
    section(&mut o, "대체된 글꼴(같은 글꼴의 다른 굵기·스타일로 그림)", "substituted", &|x| format!("대체: {}", s(&x["substitute"])));
    section(&mut o, "넣지 않은 글꼴(글꼴 복사를 고르지 않음)", "notSelected", &|x| s(&x["file"]).to_string());
    let placed = fonts["placedNotEmbedded"].as_array().cloned().unwrap_or_default();
    if !placed.is_empty() {
        ln!("배치한 PDF·EPS 안에 포함되지 않은 글꼴 {}개 (복사하지 않음 — 그 파일을 만든 쪽에서 글꼴을 포함해야 함):", placed.len());
        for p in &placed {
            ln!("  {} ({}) — {}", s(&p["font"]), s(&p["kind"]), s(&p["link"]));
        }
    }
    ln!("");

    ln!("[링크] 배치 {}개 · 파일 {}개 복사 ({}) · 없음 {} · 후보 여러 개 {} · 수정됨 {} · 포함됨 {}", n(&ls["placed"]), n(&ls["copied"]), mb(n(&ls["bytes"])),
        n(&ls["missing"]), n(&ls["ambiguous"]), n(&ls["modified"]), n(&ls["embedded"]));
    if ls["collected"] == false {
        ln!("링크 복사를 고르지 않아 Links 폴더를 만들지 않았습니다.");
    }
    let folders: Vec<&str> = links["searchFolders"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !folders.is_empty() {
        ln!("찾아본 검색 폴더: {}", folders.join(" ; "));
    }
    let list = |o: &mut String, key: &str, title: &str, detail: &dyn Fn(&Value) -> String| {
        let items = links[key].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let placed: u64 = items.iter().map(|x| n(&x["placements"])).sum();
        let extra = if placed as usize != items.len() { format!(" (배치 {placed}곳)") } else { String::new() };
        let _ = writeln!(o, "{title} {}개{extra}:", items.len());
        for x in &items {
            let _ = writeln!(o, "  {} — {}{}", s(&x["name"]), pages_text(&x["pages"], 12), detail(x));
        }
    };
    list(&mut o, "missing", "없는 링크(패키지에 없음)", &|x| format!(" — 기록된 경로: {}", s(&x["recorded"])));
    list(&mut o, "ambiguous", "같은 이름 파일이 여러 개라 고르지 않은 링크(패키지에 없음)", &|x| {
        let c: Vec<&str> = x["candidates"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        format!(" — 후보: {}", c.join(" ; "))
    });
    let renamed = links["renamed"].as_array().cloned().unwrap_or_default();
    if !renamed.is_empty() {
        ln!("이름을 바꿔 넣은 링크 {}개(같은 이름의 다른 파일 — InDesign에서 다시 연결):", renamed.len());
        for x in &renamed {
            ln!("  {} → {} — {} — 원본: {}", s(&x["originalName"]), s(&x["packaged"]), pages_text(&x["pages"], 12), s(&x["source"]));
        }
    }
    if n(&ls["sameContent"]) > 0 {
        ln!("같은 이름·같은 내용이라 하나만 넣은 파일: {}개 (Links 목록의 sameAs 참고)", n(&ls["sameContent"]));
    }
    list(&mut o, "modified", "수정된 링크(기록과 크기·시간이 다름, 지금 파일을 넣음)", &|x| format!(" — {}", s(&x["packaged"])));
    list(&mut o, "nameOnly", "검색 폴더에서 이름만으로 찾은 링크(맞는 파일인지 확인)", &|x| format!(" — {}", s(&x["source"])));
    list(&mut o, "lowResolution", &format!("유효 해상도 {} ppi 미만 이미지", LOW_PPI as u32), &|x| format!(" — {} ppi", x["ppi"].as_f64().map_or(0, |v| v.round() as i64)));
    list(&mut o, "rgb", "RGB 이미지(출력 때 CMYK로 바뀜)", &|_| String::new());
    list(&mut o, "nfdNames", "이름이 맥 방식(NFD)으로 저장된 파일(Windows InDesign에서 이름으로 다시 찾지 못할 수 있음)", &|x| format!(" — {}", s(&x["packaged"])));
    let nested: Vec<&str> = links["nested"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !nested.is_empty() {
        ln!("배치한 InDesign·Illustrator 파일 {}개 안의 링크는 모으지 않았습니다: {}", nested.len(), nested.join(", "));
    }
    let failed = links["failed"].as_array().cloned().unwrap_or_default();
    if !failed.is_empty() {
        ln!("복사하지 못한 파일 {}개:", failed.len());
        for x in &failed {
            ln!("  {} — {} ({})", s(&x["dest"]), s(&x["error"]), s(&x["source"]));
        }
    }
    ln!("");

    ln!("[사전 검사] 오류 {} · 경고 {} · 정보 {}", n(&pf["errors"]), n(&pf["warnings"]), n(&pf["infos"]));
    for x in pf["rules"].as_array().into_iter().flatten() {
        let sev = match s(&x["severity"]) {
            "error" => "오류",
            "warning" => "경고",
            _ => "정보",
        };
        let total = n(&x["errors"]) + n(&x["warnings"]) + n(&x["infos"]);
        ln!("  {sev}  {} ({}) {}개", s(&x["title"]), s(&x["rule"]), total);
    }
    let err_pages = &pf["pagesWithErrors"];
    if err_pages.as_array().is_some_and(|a| !a.is_empty()) {
        ln!("오류가 있는 쪽: {}", pages_text(err_pages, 30));
    }
    ln!("");

    ln!("[PDF]");
    if pdf.is_null() {
        ln!("넣지 않음");
    } else if let Some(e) = pdf["error"].as_str() {
        ln!("만들지 못함: {e}");
    } else {
        ln!("{} — {} ({}쪽, {})", s(&pdf["file"]), s(&pdf["standard"]), n(&pdf["pages"]), mb(n(&pdf["bytes"])));
        if !s(&pdf["preset"]).is_empty() {
            ln!("프리셋: {} (InDesign 기본 프리셋과 같은 설정)", s(&pdf["preset"]));
        }
        let fl = &pdf["flatten"];
        if fl.is_object() {
            ln!(
                "투명도 병합: {}쪽 (선화·텍스트 {} ppi, 그라디언트·효과 {} ppi, 나머지는 벡터 그대로)",
                n(&fl["pages"]),
                fl["line_art_ppi"].as_f64().unwrap_or(0.0).round(),
                fl["smooth_ppi"].as_f64().unwrap_or(0.0).round()
            );
            if fl["preview_graphics"].as_u64().unwrap_or(0) > 0 {
                ln!("  주의: 투명도가 있는 배치 PDF {}개를 저해상도 미리보기로 합침", n(&fl["preview_graphics"]));
            }
        }
        ln!("출력 의도: {}", s(&pdf["outputIntent"]));
        ln!("포함한 글꼴: {}개", n(&pdf["fontsEmbedded"]));
        let subs: Vec<&str> = pdf["fontsSubstituted"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if !subs.is_empty() {
            ln!("대체 글꼴로 넣은 글꼴: {}", subs.join(", "));
        }
        let st = &pdf["settings"];
        ln!("시간: {:.1}초", pdf["ms"].as_f64().unwrap_or(0.0) / 1000.0);
        if st.is_object() {
            let m = &st["marks"];
            let marks: Vec<&str> = [("crop", "재단선"), ("bleed", "도련 표시"), ("registration", "맞춰찍기 표시"), ("pageInfo", "페이지 정보")]
                .iter()
                .filter(|(k, _)| m[*k] == true)
                .map(|(_, t)| *t)
                .collect();
            ln!("설정: 도련 {} · 펼침면 {} · 표시 {}", if st["bleed"] == true { "사용" } else { "안 씀" }, if st["spreads"] == true { "예" } else { "아니요" },
                if marks.is_empty() { "없음".to_string() } else { marks.join(", ") });
        }
        let warnings = pdf["warnings"].as_array().cloned().unwrap_or_default();
        // what reading the document could not model is for Folio's makers, not the operator
        let (reading, printing): (Vec<&Value>, Vec<&Value>) = warnings.iter().partition(|w| w["kind"] == "document");
        for w in &printing {
            ln!("  경고: {}", s(&w["message"]));
        }
        if !reading.is_empty() {
            ln!("  (문서 읽기 참고 {}개는 package-report.json의 pdf.warnings에 있음)", reading.len());
        }
    }
    ln!("");

    let pk = &r["package"];
    ln!("[패키지 내용] 파일 {}개, {}", n(&pk["fileCount"]), mb(n(&pk["bytes"])));
    ln!("확인: 복사한 파일마다 크기를 원본과 비교하고, 복사하는 동안 원본이 바뀌지 않았는지 확인함{}",
        if s(&pk["verify"]).contains("every") { " · 모든 파일 SHA-256은 package-report.json" } else { " · 문서 SHA-256은 위 [문서]" });
    let files = pk["files"].as_array().cloned().unwrap_or_default();
    let in_dir = |d: &str| files.iter().filter(|f| s(&f["path"]).starts_with(&format!("{d}/"))).collect::<Vec<_>>();
    for f in files.iter().filter(|f| !s(&f["path"]).contains('/')) {
        ln!("  {} ({})", s(&f["path"]), mb(n(&f["bytes"])));
    }
    for d in [crate::LINKS_DIR, crate::FONTS_DIR] {
        let list = in_dir(d);
        if !list.is_empty() {
            ln!("  {d}\\ — 파일 {}개, {}", list.len(), mb(list.iter().map(|f| n(&f["bytes"])).sum()));
        }
    }
    ln!("");

    // full list, last: every packaged link with its pages
    let lf = links["files"].as_array().cloned().unwrap_or_default();
    if !lf.is_empty() {
        ln!("[링크 파일 전체 목록]");
        for x in &lf {
            let flag = if x["copied"] == false { " (복사 실패)" } else if x["renamed"] == true { " (이름 바꿈)" } else { "" };
            ln!("  {}{} — {} — {}", s(&x["path"]).trim_start_matches("Links/"), flag, pages_text(&x["pages"], 8), mb(n(&x["bytes"])));
        }
        ln!("");
    }
    ln!("이 보고서와 같은 내용이 package-report.json에 있습니다(기계용).");
    o
}

/// UTF-8 with a byte order mark and CRLF line ends: Notepad and TextEdit both read it.
pub fn text_file_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 64);
    out.extend_from_slice(b"\xEF\xBB\xBF");
    for line in text.split('\n') {
        out.extend_from_slice(line.trim_end_matches('\r').as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    // `split` yields one empty piece after the final newline
    if text.ends_with('\n') {
        out.truncate(out.len() - 2);
    }
    out
}
