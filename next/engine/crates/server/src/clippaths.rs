//! Object ▸ Clipping Path ▸ Options makes a path from the picture's linked file: `prepare` gives each
//! `setClipping` that asks for Detect Edges, Alpha Channel or Photoshop Path the path made from the file
//! (`folio_render::clipextract`; one command per frame), and `sources` lists the file's alpha channels
//! and Photoshop paths for the dialog. The settings not given keep the picture's.
use folio_edit::{ClipPath, ClipSettings, Command, commands::RegisteredCommand};
use std::path::PathBuf;

fn frame<'d>(s: &'d crate::Session, id: &str) -> Option<(&'d folio_model::PageItem, &'d folio_model::Graphic)> {
    let mut found = None;
    for spread in s.doc.spreads.iter().chain(&s.doc.master_spreads) {
        for item in &spread.items {
            item.walk(&mut |i| {
                use folio_model::ItemKind::*;
                if found.is_none() && let Rectangle { content: Some(g) } | Oval { content: Some(g) } | Polygon { content: Some(g) } = &i.kind
                    && (i.id == id || g.id == id)
                {
                    found = Some((i, g));
                }
            });
        }
    }
    found
}

fn file(s: &crate::Session, es: &crate::edit::EditState, g: &folio_model::Graphic) -> Option<PathBuf> {
    let uri = g.link_uri.clone().or_else(|| g.link.as_ref().map(|l| l.uri.clone()));
    uri.and_then(|u| crate::clip::source_file(s, es, &u))
        .or_else(|| g.preview.as_deref().map(PathBuf::from).filter(|p| p.is_file()))
}

/// The alpha channels and Photoshop paths of the picture in frame `id` (none when its file cannot be read).
pub fn sources(s: &crate::Session, id: &str) -> folio_render::clipextract::Sources {
    let Some(shared) = s.edit.as_ref() else { return Default::default() };
    let es = shared.lock().expect("shared state must remain unpoisoned after each operation");
    frame(s, id).and_then(|(_, g)| file(s, &es, g)).and_then(|p| std::fs::read(p).ok())
        .map(|b| folio_render::clipextract::sources(&b)).unwrap_or_default()
}

fn made(s: &crate::Session, es: &crate::edit::EditState, id: &str, kind: &str, set: &ClipSettings) -> Result<Vec<ClipPath>, String> {
    let (item, g) = frame(s, id).ok_or_else(|| format!("그림 프레임이 아닙니다: {id}"))?;
    let now = g.clipping_options.clone();
    let path = file(s, es, g).ok_or("연결 그림 파일을 찾지 못해 클리핑 패스를 만들 수 없습니다")?;
    let bytes = std::fs::read(&path).map_err(|_| "연결 그림 파일을 찾지 못해 클리핑 패스를 만들 수 없습니다".to_owned())?;
    let get = |v: Option<f64>, f: fn(&folio_model::clipping::Options) -> f64, d: f64| v.or(now.as_ref().map(f)).unwrap_or(d);
    let restrict = set.restrict_to_frame.or(now.as_ref().map(|o| o.restrict_to_frame)).unwrap_or(false);
    let frame = restrict.then(|| folio_model::paths_bounds(&item.paths).zip(g.transform.invert()).map(|(b, inv)| inv.apply_rect(&b))).flatten();
    let req = folio_render::clipextract::Request {
        kind: kind.to_owned(),
        threshold: get(set.threshold, |o| o.threshold, 25.0),
        tolerance: get(set.tolerance, |o| o.tolerance, 2.0),
        inset: get(set.inset, |o| o.inset, 0.0),
        inside_edges: set.inside_edges.or(now.as_ref().map(|o| o.inside_edges)).unwrap_or(false),
        invert: set.invert.or(now.as_ref().map(|o| o.invert)).unwrap_or(false),
        path_name: set.path_name.clone().or(now.as_ref().map(|o| o.path_name.trim_start_matches("$ID/").to_owned())).unwrap_or_default(),
        index: set.index.or(now.as_ref().map(|o| o.index)).unwrap_or(-1),
        frame: frame.map(|r| [r.left, r.top, r.right, r.bottom]),
    };
    let b = &g.bounds;
    let subs = folio_render::clipextract::extract(&bytes, &req, [b.left, b.top, b.right, b.bottom])?;
    Ok(subs.into_iter().map(|s| ClipPath { open: s.open, points: s.points.iter().map(|p| p.map(|(x, y)| [x, y])).collect() }).collect())
}

/// Each `setClipping` that makes a path gets it (one command per frame); None and a given path are left as they are.
pub fn prepare(commands: &mut Vec<Command>, s: &crate::Session, es: &crate::edit::EditState) -> Result<(), String> {
    let mut out = Vec::with_capacity(commands.len());
    for command in std::mem::take(commands) {
        match command {
            Command::Registered(RegisteredCommand::SetClipping { ids, kind, settings, paths: None }) if kind != "None" => {
                let set = settings.clone().unwrap_or_default();
                for id in ids {
                    let paths = made(s, es, &id, &kind, &set)?;
                    // the alpha channel is found by its name: the position it has in the file goes with it
                    let mut settings = settings.clone().unwrap_or_default();
                    if kind == "AlphaChannel" && let Some((_, g)) = frame(s, &id)
                        && let Some(file) = file(s, es, g).and_then(|p| std::fs::read(p).ok())
                    {
                        let names = folio_render::clipextract::sources(&file).alpha_channels;
                        let wanted = settings.path_name.clone().unwrap_or_default();
                        let at = names.iter().position(|n| *n == wanted).unwrap_or(0);
                        settings.index = Some(at as i32);
                        settings.path_name = names.get(at).cloned();
                    }
                    if kind == "PhotoshopPath" && settings.path_name.is_none() && let Some((_, g)) = frame(s, &id)
                        && let Some(file) = file(s, es, g).and_then(|p| std::fs::read(p).ok())
                    {
                        settings.path_name = folio_render::clipextract::sources(&file).paths.first().cloned();
                    }
                    out.push(Command::Registered(RegisteredCommand::SetClipping { ids: vec![id], kind: kind.clone(), settings: Some(settings), paths: Some(paths) }));
                }
            }
            other => out.push(other),
        }
    }
    *commands = out;
    Ok(())
}
