//! TOC commands receive page locations from this session's composed document.
use folio_edit::{Command, commands::RegisteredCommand};

pub fn story(req: &serde_json::Value, session: Option<&crate::Session>) -> Result<serde_json::Value,String> {
    let session=session.ok_or("문서가 없습니다")?;
    let id=req["story"].as_str().ok_or("목차 이야기가 없습니다")?;
    if !session.doc.toc_stories.contains_key(id) {return Err("목차 이야기가 아닙니다".into())}
    let story=session.doc.stories.get(id).ok_or("목차 이야기가 없습니다")?;
    let text=story.paragraphs.iter().map(|p|p.runs.iter().map(|r|r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\r");
    Ok(serde_json::json!({"text":text}))
}

pub fn import_styles(req: &serde_json::Value) -> Result<serde_json::Value, String> {
    let path = std::path::Path::new(req["path"].as_str().ok_or("파일 경로가 없습니다")?);
    if path.extension().and_then(|s| s.to_str()).is_none_or(|s| !s.eq_ignore_ascii_case("idml")) {
        return Err("IDML 파일에서 목차 스타일을 불러올 수 있습니다".into());
    }
    folio_idml::read_package(path, None).map_err(|e| e.to_string())?;
    let parts=crate::edit::package_parts(path)?.into_iter().filter(|(name,_)|matches!(name.as_str(),"Resources/Styles.xml"|"Resources/Graphic.xml")).collect::<std::collections::BTreeMap<_,_>>();
    Ok(serde_json::json!({"parts":parts}))
}

pub fn prepare(commands: &mut [Command], session: &crate::Session) -> Result<(), String> {
    let is_toc = |c: &Command| matches!(c, Command::Registered(RegisteredCommand::CreateTOC { .. } | RegisteredCommand::UpdateTOC { .. }));
    if !commands.iter().any(is_toc) { return Ok(()) }
    if commands.iter().any(|c| !is_toc(c) && !matches!(c, Command::Registered(RegisteredCommand::SaveTOCStyle { .. }))) {
        return Err("본문 편집 후 조판이 끝나면 목차를 업데이트하십시오".into());
    }
    let mut sources = folio_text::layout::toc::sources(&session.doc, &session.layout);
    if includes_book(commands, session) {
        sources = book_sources(session, sources)?;
    }
    for c in commands {
        if let Command::Registered(RegisteredCommand::CreateTOC {sources: target,..} | RegisteredCommand::UpdateTOC {sources:target,..})=c {
            *target=sources.clone();
        }
    }
    Ok(())
}

/// Whether a TOC of these commands includes the book's documents (its style, saved in the same edit or before).
fn includes_book(commands: &[Command], session: &crate::Session) -> bool {
    let saved = |id: &str| commands.iter().find_map(|c| match c {
        Command::Registered(RegisteredCommand::SaveTOCStyle { style }) if style.id == id || (style.id.is_empty() && id.is_empty()) => Some(style.include_book_documents),
        _ => None,
    }).or_else(|| session.doc.toc_styles.iter().find(|s| s.id == id).map(|s| s.include_book_documents)).unwrap_or(false);
    commands.iter().any(|c| match c {
        Command::Registered(RegisteredCommand::CreateTOC { style, .. }) => saved(style),
        Command::Registered(RegisteredCommand::UpdateTOC { story, .. }) => session.doc.toc_stories.get(story).is_some_and(|id| saved(id)),
        _ => false,
    })
}

/// The books of the project that hold `rel`, by path (`.foliobook` files; hidden folders are not looked in).
pub(crate) fn books_holding(root: &std::path::Path, rel: &str) -> Vec<(String, folio_model::book::Book)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, depth: usize, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') { continue; }
            if path.is_dir() {
                if depth < 8 { walk(&path, root, depth + 1, out); }
            } else if name.to_ascii_lowercase().ends_with(".foliobook")
                && let Ok(r) = path.strip_prefix(root)
            {
                out.push(r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, root, 0, &mut found);
    found.sort();
    found.into_iter().filter_map(|p| folio_project::book::read(root, &p).ok().map(|b| (p, b))).filter(|(_, b)| b.documents.iter().any(|d| d.path == rel)).collect()
}

/// A TOC that includes the book's documents: the paragraphs of every document of the book that holds this one, in the book's
/// order, each with its own page names (Book ▸ Repaginate numbers them; InDesign probe artifacts/build-b2-20261023/booktoc).
/// The other documents are read from their files (as Book ▸ Update All Cross-References reads them). A document in no book
/// lists its own paragraphs only.
fn book_sources(session: &crate::Session, own: Vec<folio_model::toc::TocSource>) -> Result<Vec<folio_model::toc::TocSource>, String> {
    use std::sync::{Arc, Mutex, OnceLock};
    let Some(folio) = session.folio.as_deref() else { return Ok(own) };
    let root = folio.opened.project.root.clone();
    let rel = folio.opened.rel.as_str().to_owned();
    let Some((_, book)) = books_holding(&root, &rel).into_iter().next() else { return Ok(own) };
    let fonts = crate::Fonts { startup: Mutex::new(None), spare: Mutex::new(None) };
    let profiles = Arc::new(crate::Profiles { startup: Mutex::new(None), index: OnceLock::new() });
    let mut out = Vec::new();
    let mut own = Some(own);
    for entry in &book.documents {
        if entry.path == rel {
            out.extend(own.take().unwrap_or_default());
            continue;
        }
        let file = folio_project::paths::checked_project_path(&root, &entry.path)?;
        let other = crate::load_session(file.to_str().ok_or("invalid document path")?, false, &fonts, &profiles, &|_, _| {})
            .map_err(|e| format!("책 문서를 읽지 못했습니다: {}: {e}", entry.path))?;
        for s in folio_text::layout::toc::sources(&other.doc, &other.layout) {
            let Some(p) = other.doc.stories.get(&s.story).and_then(|st| st.paragraphs.get(s.paragraph)) else { continue };
            let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
            out.push(folio_model::toc::TocSource { story: String::new(), paragraph: 0, page: s.page, page_index: s.page_index, hidden: s.hidden, number: s.number,
                book: Some(folio_model::toc::BookParagraph { document: entry.path.clone(), style: p.style.clone(), text }) });
        }
        if let Ok(store) = Arc::try_unwrap(other.fonts) {
            *fonts.spare.lock().expect("shared state must remain unpoisoned after each operation") = Some(store);
        }
    }
    Ok(out)
}
