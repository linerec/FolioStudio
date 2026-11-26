//! Object Layer Options of a placed PSD. The layers come from the file the picture links to:
//! `attach` gives each layered PSD its list and, where layers are switched, the composed
//! picture (in the project's disposable cache); `prepare` hands a visibility command the list
//! for a picture that has no saved layer options yet. The composing and the rules are
//! `folio_links::psdlayers`.
use folio_edit::{Command, commands::RegisteredCommand};
use std::path::{Path, PathBuf};

fn cache(folio: Option<&crate::project::FolioSession>) -> PathBuf {
    folio.map(|f| f.opened.project.root.join(".folio/cache/psdlayers"))
        .unwrap_or_else(|| crate::app_cache("psdlayers"))
}

fn is_psd(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd")) && p.is_file()
}

pub fn attach(doc: &mut folio_model::Document, folio: Option<&crate::project::FolioSession>, source_dir: Option<&Path>) {
    folio_links::psdlayers::attach(doc, &cache(folio), &|g| {
        // a project keeps the file as the graphic's preview; other documents link by URI
        if let Some(p) = g.preview.as_deref().map(PathBuf::from).filter(|p| is_psd(p)) {
            return Some(p);
        }
        let uri = g.link_uri.as_deref()?;
        if let Some(path) = folio_links::parse_uri(uri).and_then(|p| p.local_path()).filter(|p| is_psd(p)) {
            return Some(path);
        }
        let name = folio_links::parse_uri(uri)?.file_name()?.to_owned();
        let dir = source_dir?;
        [dir.join(&name), dir.join("Links").join(&name)].into_iter().find(|p| is_psd(p))
    });
}

fn link_uri(doc: &folio_model::Document, id: &str) -> Option<String> {
    let mut found = None;
    folio_idml::for_each_item(doc, &mut |item| {
        use folio_model::ItemKind::*;
        if let Rectangle { content: Some(g) } | Oval { content: Some(g) } | Polygon { content: Some(g) } = &item.kind
            && (item.id == id || g.id == id)
        {
            found = g.link_uri.clone().or_else(|| g.link.as_ref().map(|l| l.uri.clone()));
        }
    });
    found
}

/// The layer commands get what the PSD the picture links to says: its layer list (a picture that has no saved
/// layer options yet is made from it), its layer comps and whether it has any.
pub fn prepare(commands: &mut [Command], s: &crate::Session, es: &crate::edit::EditState) -> Result<(), String> {
    let listed = |ids: &[String]| {
        let uri = ids.first().and_then(|id| link_uri(&s.doc, id))?;
        let file = crate::clip::source_file(s, es, &uri).filter(|p| is_psd(p))?;
        folio_links::psdlayers::listed(&file)
    };
    for command in commands.iter_mut() {
        let Command::Registered(command) = command else { continue };
        match command {
            RegisteredCommand::SetGraphicLayerVisibility { ids, layers, has_comps, .. } if layers.is_none() || has_comps.is_none() => {
                if let Some(file) = listed(ids) {
                    has_comps.get_or_insert(!file.comps.is_empty());
                    if layers.is_none() {
                        *layers = Some(file.layers);
                    }
                }
            }
            RegisteredCommand::SetGraphicLayerComp { ids, comps, layers, .. } if layers.is_none() || comps.is_none() => {
                if let Some(file) = listed(ids) {
                    if comps.is_none() {
                        *comps = Some(file.comps);
                    }
                    if layers.is_none() {
                        *layers = Some(file.layers);
                    }
                }
            }
            RegisteredCommand::SetGraphicLayerUpdateLink { ids, layers, .. } if layers.is_none() => {
                *layers = listed(ids).map(|o| o.layers);
            }
            _ => {}
        }
    }
    Ok(())
}
