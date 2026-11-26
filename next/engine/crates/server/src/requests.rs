//! Request handlers for document lifecycle, queries, rendering and file operations.

use crate::{
    Fonts, Out, Profiles, Session, app_cache, attach_paths, attached, conveyor, datamerge,
    dialog_state, dialog_then_edit, edit, history_doc, inspect, open, package, print_command,
    project, reader_exe, registered_queries, send, send_text, text_edit, user_settings, wire,
};
use folio_model::Rect;
use folio_render::{DisplayOptions, ViewDisplay};
use folio_text::FontStore;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, mpsc};

pub(super) type RenderJob = (Value, Arc<Session>, usize, f64, Option<Rect>, Option<Value>, DisplayOptions);

pub(super) struct Context<'a> {
    pub(super) out: &'a Out,
    pub(super) session: &'a Arc<Mutex<Option<Arc<Session>>>>,
    pub(super) font_pool: &'a Arc<Fonts>,
    pub(super) profiles: &'a Arc<Profiles>,
    pub(super) tx: &'a mpsc::Sender<RenderJob>,
}

pub(super) struct Request {
    pub(super) id: Value,
    pub(super) req: Value,
    pub(super) mcp: bool,
    pub(super) dialog_values: Option<Value>,
    pub(super) dialog_then: Option<Value>,
}

pub(super) fn self_check(context: &Context<'_>, request: Request) {
    let out = context.out;
    let font_pool = context.font_pool;
    let profiles = context.profiles;
    let Request { id, .. } = request;
    // No project or document is opened. Reuse the startup scan afterwards.
    let fonts = font_pool.take();
    let faces = fonts.faces().len();
    *font_pool.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(fonts);
    let cache = app_cache("diagnostics");
    let probe = cache.join(format!("write-test-{}", std::process::id()));
    let writable = std::fs::create_dir_all(&cache)
        .and_then(|_| std::fs::write(&probe, b"Folio"))
        .and_then(|_| std::fs::remove_file(&probe));
    send(out, json!({"id": id, "ok": true, "type": "selfCheck",
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "version": env!("CARGO_PKG_VERSION"), "fontFaces": faces,
        "fontDirs": FontStore::system_dirs().into_iter().map(|(p, _)| p).collect::<Vec<_>>(),
        "profiles": profiles.get().entries().len(), "reader": reader_exe(),
        "cache": cache, "cacheWritable": writable.is_ok(),
        "cacheError": writable.err().map(|e| e.to_string())}), &[]);
}

pub(super) fn open_document(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let font_pool = context.font_pool;
    let profiles = context.profiles;
    let Request { id, req, mcp, .. } = request;
    let path = req["path"].as_str().unwrap_or("").to_string();
    let editable = req["edit"].as_bool().unwrap_or(false);
    let out2 = out.clone();
    let slot = session.clone();
    let pool = font_pool.clone();
    let profiles = profiles.clone();
    // the previous document's font store is reused when nothing renders from it any more
    if let Some(previous) = slot.lock().expect("shared state must remain unpoisoned after each operation").take()
        && let Ok(old) = Arc::try_unwrap(previous)
        && let Ok(fonts) = Arc::try_unwrap(old.fonts)
    {
        *pool.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(fonts);
    }
    // open on its own thread so stdin keeps flowing (progress events)
    std::thread::spawn(move || match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| open(&id, &path, editable, &out2, &pool, &profiles))) {
        Ok(Ok(s)) => {
            let summary = s.summary.clone();
            let opened = s.source_path.clone();
            *slot.lock().expect("shared state must remain unpoisoned after each operation") = Some(Arc::new(s));
            // the screen shows the document an attached client opened, as after its own File ▸ Open
            if mcp { send(&out2, json!({"event": "external-open", "path": opened, "detail": {"document": summary, "source": "mcp"}}), &[]); }
            send(&out2, json!({"id": id, "ok": true, "type": "opened", "document": summary}), &[]);
        }
        Ok(Err(e)) => send(&out2, wire::error_reply(&id, e), &[]),
        Err(_) => send(&out2, wire::error_reply(&id, "engine panicked while opening"), &[]),
    });
}

pub(super) fn render(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let tx = context.tx;
    let Request { id, req, mcp, .. } = request;
    let current = session.lock().expect("shared state must remain unpoisoned after each operation").clone();
    match current {
        Some(s) if mcp => {
            let out = out.clone();
            let profiles = context.profiles.clone();
            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::attach_image::render(&req, &s, &profiles)));
                let header = match result {
                    Ok(Ok(mut image)) => { image["id"] = id; image }
                    Ok(Err(error)) => wire::error_reply(&id, error),
                    Err(_) => wire::error_reply(&id, "page image failed"),
                };
                send(&out, header, &[]);
            });
        }
        Some(s) => {
            let page = req["page"].as_u64().unwrap_or(0) as usize;
            let scale = req["scale"].as_f64().unwrap_or(1.0).clamp(0.05, 32.0);
            let region = req["region"].as_array().and_then(|a| {
                let v: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
                (v.len() == 4).then(|| Rect { left: v[0], top: v[1], right: v[2], bottom: v[3] })
            });
            let view = req.get("view").filter(|v| v.is_object()).cloned();
            // Display Performance of the window: {"view":"OPTIMIZED"|"TYPICAL"|"HIGH_QUALITY","ignoreLocal":bool}
            let display = DisplayOptions {
                view: req["display"]["view"].as_str().and_then(ViewDisplay::from_name).unwrap_or_default(),
                ignore_local: req["display"]["ignoreLocal"].as_bool().unwrap_or(false),
            };
            let _ = tx.send((id, s, page, scale, region, view, display));
        }
        None => send(out, wire::error_reply(&id, "no document open"), &[]),
    }
}

pub(super) fn info(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, .. } = request;
    let current = session.lock().expect("shared state must remain unpoisoned after each operation").clone();
    let summary = current.map(|s| s.summary.clone()).unwrap_or(Value::Null);
    send(out, json!({"id": id, "ok": true, "type": "info", "document": summary}), &[]);
}

pub(super) fn print(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, req, mcp, .. } = request;
    let Some(mut s) = session.lock().expect("shared state must remain unpoisoned after each operation").clone() else {
        send(out, wire::error_reply(&id, "no document open"), &[]);
        return;
    };
    // an export is never of an open preview (edit/preview.rs)
    if req["cmd"] == "export" && let Some(n) = edit::settle(&s) {
        s = Arc::new(n);
        *session.lock().expect("shared state must remain unpoisoned after each operation") = Some(s.clone());
    }
    let out2 = out.clone();
    // off the request loop: renders keep flowing while these run
    std::thread::spawn(move || {
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| print_command(&req, &s, &id, &out2)));
        let header = match reply {
            Ok(Ok(mut v)) => {
                v["id"] = id.clone();
                v["ok"] = json!(true);
                v
            }
            Ok(Err(e)) => wire::error_reply(&id, e),
            Err(_) => wire::error_reply(&id, "engine panicked"),
        };
        if req["cmd"] == "export" && header["ok"] == true {
            let written = header["result"]["path"].as_str().unwrap_or("").to_owned();
            if mcp { send(&out2, json!({"event": "external-file", "path": s.source_path, "detail": {"kind": "export", "file": written, "source": "mcp"}}), &[]); }
            else { attach_paths::person_exported(&written); }
        }
        send(&out2, header, &[]);
    });
}

pub(super) fn text(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, req, .. } = request;
    let current = session.lock().expect("shared state must remain unpoisoned after each operation").clone();
    let reply = current.as_ref().ok_or_else(|| "no document".to_string())
        .and_then(|s| text_edit::command(&req, s));
    let header = match reply {
        Ok(mut v) => { v["id"]=id.clone(); v["ok"]=json!(true); v }
        Err(e) => wire::error_reply(&id, e),
    };
    send(out,header,&[]);
}

pub(super) fn inspect(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, req, .. } = request;
    let Some(s) = session.lock().expect("shared state must remain unpoisoned after each operation").clone() else {
        send(out, wire::error_reply(&id, "no document"), &[]);
        return;
    };
    let out2 = out.clone();
    // read-only and quick; off the request loop like the other panel commands
    std::thread::spawn(move || {
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let text = inspect::command(&req, &s.doc, &s.layout, &s.colors, &s.inspect)?;
            if req["cmd"] == "documentInfo" {
                let mut info: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                info["info"]["activeLayer"] = json!(edit::drawing_layer(&s));
                registered_queries::document_fields(&s, &mut info["info"]);
                Ok::<_, String>(info.to_string())
            } else { Ok(text) }
        }));
        match reply {
            // written as text by inspect (large pages); framed as `send` does
            Ok(Ok(text)) => send_text(&out2, &id, &text),
            Ok(Err(e)) => send(&out2, wire::error_reply(&id, e), &[]),
            Err(_) => send(&out2, wire::error_reply(&id, "engine panicked"), &[]),
        }
    });
}

pub(super) fn package(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, req, mcp, .. } = request;
    let Some(mut s) = session.lock().expect("shared state must remain unpoisoned after each operation").clone() else {
        send(out, wire::error_reply(&id, "no document open"), &[]);
        return;
    };
    if let Some(n) = edit::settle(&s) {
        s = Arc::new(n);
        *session.lock().expect("shared state must remain unpoisoned after each operation") = Some(s.clone());
    }
    let out2 = out.clone();
    // copies run on their own threads; renders and other commands keep flowing
    std::thread::spawn(move || {
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if req["cmd"] == "package" { package::package_command(&req, &s, Some(&out2)) } else { package::plan_command(&req, &s) }
        }));
        let header = reply.unwrap_or_else(|_| json!({"id": id, "ok": false, "code": "io", "error": "engine panicked while packaging"}));
        // the screen's project tree shows the package an attached client made (as after its own saves)
        if mcp && req["cmd"] == "package" && header["ok"] == true {
            send(&out2, json!({"event": "external-file", "path": s.source_path, "detail": {"kind": "package", "file": header["result"]["dir"], "source": "mcp"}}), &[]);
        }
        send(&out2, header, &[]);
    });
}

pub(super) fn project(context: &Context<'_>, request: Request) {
    let out = context.out;
    let Request { id, req, mcp, .. } = request;
    let reply = if req["cmd"] == "projectCreate" { project::create(&req) } else { project::info(&req) };
    // the project the screen shows: an attached client's paths are relative to it while no document is open
    if !mcp && req["cmd"] == "projectInfo" && reply["ok"] == true { attach_paths::screen_project(reply["project"]["root"].as_str().unwrap_or("")); }
    let out2 = out.clone();
    let mut h = reply;
    h["id"] = id;
    send(&out2, h, &[]);
}

pub(super) fn new_document_fonts(context: &Context<'_>, request: Request) {
    let out = context.out;
    let font_pool = context.font_pool;
    let Request { id, .. } = request;
    let mut h = project::new_document_fonts(font_pool);
    h["id"] = id;
    send(out, h, &[]);
}

pub(super) fn new_document(context: &Context<'_>, request: Request) {
    let out = context.out;
    let font_pool = context.font_pool;
    let Request { id, req, .. } = request;
    let out2 = out.clone();
    let pool = font_pool.clone();
    std::thread::spawn(move || {
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| project::new_document(&req, &out2, &pool)));
        let mut header = reply.unwrap_or_else(|_| json!({"ok": false, "code": "io", "error": "engine panicked while creating the document"}));
        header["id"] = id;
        send(&out2, header, &[]);
    });
}

pub(super) fn import(context: &Context<'_>, request: Request) {
    let out = context.out;
    let font_pool = context.font_pool;
    let Request { id, req, mcp, .. } = request;
    let out2 = out.clone();
    let pool = font_pool.clone();
    // the reader, copies and hashing run on their own thread
    std::thread::spawn(move || {
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| project::import_command(&req, &out2, &pool)));
        let mut header = reply.unwrap_or_else(|_| json!({"ok": false, "code": "io", "error": "engine panicked while importing"}));
        header["id"] = id;
        if mcp && req["cmd"] == "importRun" && header["ok"] == true {
            let made = std::path::Path::new(req["project"].as_str().unwrap_or("")).join(req["document"].as_str().unwrap_or(""));
            send(&out2, json!({"event": "external-file", "path": Value::Null, "detail": {"kind": "importRun", "file": made, "source": "mcp"}}), &[]);
        }
        send(&out2, header, &[]);
    });
}

pub(super) fn import_cancel(context: &Context<'_>, request: Request) {
    let out = context.out;
    let Request { id, .. } = request;
    let mut header = project::cancel();
    header["id"] = id;
    send(out, header, &[]);
}

pub(super) fn edit(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, mut req, mcp, dialog_values, mut dialog_then } = request;
    // in order, on this thread: a render asked after an edit draws the edited document
    let Some(s) = session.lock().expect("shared state must remain unpoisoned after each operation").clone() else {
        send(out, wire::error_reply(&id, "no document open"), &[]);
        return;
    };
    // what an attached LLM client edits says so in the history (Edit ▸ Undo "LLM: …")
    if mcp && req["cmd"] == "edit" {
        let label = req["label"].as_str().filter(|l| !l.is_empty()).map(str::to_owned)
            .unwrap_or_else(|| req["commands"][0]["op"].as_str().unwrap_or("edit").to_owned());
        req["label"] = json!(attached::llm_label(&label));
    }
    let mut created_resource = None;
    let mut header = if req["cmd"] == "selectionSize" {
        edit::selection_size(&s, &req)
    } else if req["cmd"] == "editState" {
        json!({"ok": true, "type": "editState", "state": edit::state(&s)})
    } else if req["cmd"] == "debugDigest" {
        edit::digest(&s)
    } else if req["cmd"] == "effects" {
        edit::effects(&s, &req)
    } else if req["cmd"] == "copy" {
        edit::copy(&s, &req)
    } else if req["cmd"] == "labels" {
        let ids: Option<Vec<String>> = req["ids"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect());
        edit::labels(&s, req["key"].as_str().unwrap_or(""), ids.as_deref())
    } else {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| edit::run(&req, &s))) {
            Ok(Ok((next, reply))) => {
                if let Some(n) = next {
                    if mcp && req["commands"].as_array().is_some_and(|commands| commands.len() == 1)
                        && reply["created"].as_array().is_some_and(Vec::is_empty) {
                        created_resource = crate::attach_created::resource_id(&req["commands"][0]["op"], &s.doc, &n.doc);
                    }
                    *session.lock().expect("shared state must remain unpoisoned after each operation") = Some(Arc::new(n));
                }
                reply
            }
            Ok(Err(e)) => e,
            Err(_) => json!({"ok": false, "error": "engine panicked while editing"}),
        }
    };
    // a dialog whose next commands take what these made (Table of Contents: the id a new style got), in the same undo step
    if header["ok"] == true && let Some(then) = dialog_then.take() { header = dialog_then_edit(session, &then, header); }
    if header["ok"] == true {
        if let Some(values) = dialog_values {
            send(out, dialog_state::applied(&s, &values, &header), &[]);
            if let Ok(settings) = user_settings::run(&json!({})) {
                send(out, user_settings::event(&settings["settings"], &Value::Null), &[]);
            }
        } else if mcp && matches!(req["cmd"].as_str(), Some("edit" | "undo" | "redo")) {
            // the screen draws what an attached client changed, as after its own edit (useEditingCommands)
            send(out, json!({"event": "external-edit", "path": s.source_path, "detail": {"edit": header, "source": "mcp"}}), &[]);
            if let Some(ids) = created_resource { header["created"] = ids; }
        }
    }
    header["id"] = id;
    send(out, header, &[]);
}

pub(super) fn conveyor(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, req, .. } = request;
    // the Content Collector / Placer conveyor (conveyor.rs): state of this process, not of a document
    let s = session.lock().expect("shared state must remain unpoisoned after each operation").clone();
    let mut header = conveyor::run(s.as_deref(), &req);
    header["id"] = id;
    send(out, header, &[]);
}

pub(super) fn save(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let profiles = context.profiles;
    let Request { id, req, mcp, .. } = request;
    let Some(mut s) = session.lock().expect("shared state must remain unpoisoned after each operation").clone() else {
        send(out, wire::error_reply(&id, "no document open"), &[]);
        return;
    };
    // what is written is never an open preview (edit/preview.rs)
    if !(mcp && req["cmd"] == "projectStatus") && let Some(n) = edit::settle(&s) {
        s = Arc::new(n);
        *session.lock().expect("shared state must remain unpoisoned after each operation") = Some(s.clone());
    }
    let out2 = out.clone();
    let profiles = profiles.clone();
    std::thread::spawn(move || {
        let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match req["cmd"].as_str() {
            Some("save") => project::save(&s),
            // Data Merge ▸ Export to PDF (build-c): merged in the cache, written as a PDF
            Some("dataMergeExportPdf") => datamerge::export_pdf(&req, &s, &profiles),
            // History ▸ New Document (build-c): the document as it was at a step, as a new .folio
            Some("historyNewDocument") => history_doc::new_document(&req, &s),
            Some("saveCopy") => project::save_copy(&req, &s),
            Some("dataMergeCreate") => datamerge::create(&req, &s),
            Some("exportIdml") => project::export_idml(&req, &s),
            Some("lockOverride") => project::lock_override(&s),
            _ => project::status(&s),
        }));
        let mut header = reply.unwrap_or_else(|_| json!({"ok": false, "error": "engine panicked"}));
        let kind = req["cmd"].as_str().unwrap_or("");
        if header["ok"] == true && matches!(kind, "save" | "saveCopy" | "dataMergeCreate" | "exportIdml") {
            let written = header["path"].as_str().unwrap_or("").to_owned();
            // the screen hears what an attached client wrote (a save clears its unsaved mark); what a person exported may be exported to again
            if mcp { send(&out2, json!({"event": "external-file", "path": s.source_path, "detail": {"kind": kind, "file": written, "source": "mcp"}}), &[]); }
            else if kind == "exportIdml" { attach_paths::person_exported(&written); }
        }
        header["id"] = id;
        send(&out2, header, &[]);
    });
}

pub(super) fn close(context: &Context<'_>, request: Request) {
    let out = context.out;
    let session = context.session;
    let Request { id, .. } = request;
    // the document (and its lock) is released once no render uses it
    let previous = session.lock().expect("shared state must remain unpoisoned after each operation").take();
    drop(previous);
    send(out, json!({"id": id, "ok": true, "type": "closed"}), &[]);
}

pub(super) fn package_cancel(context: &Context<'_>, request: Request) {
    let out = context.out;
    let Request { id, .. } = request;
    let mut header = package::cancel();
    header["id"] = id;
    send(out, header, &[]);
}
