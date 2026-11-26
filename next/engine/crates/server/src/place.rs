//! Import placement sources before editing. Project assets are copied by content hash;
//! a refused transaction removes only the files this request created.
use crate::{Session, edit::EditState};
use folio_project::{
    RelPath,
    format::{RefKind, Reference},
    paths,
};
use serde_json::{Value, json};
use std::{collections::HashMap, path::{Path, PathBuf}};

#[derive(Default)]
pub struct Prepared {
    pub(crate) created: Vec<PathBuf>,
    pub(crate) refs: Vec<Reference>,
    sources: HashMap<String, folio_idml::LinkOverride>,
    committed: bool,
    /// What a Paste in this edit pastes (the engine's clipboard, its links in this project).
    pub clipboard: Option<folio_edit::Clipboard>,
    /// The edit places the Place and Link cursor (conveyor `loaded`): it is emptied once the edit went through.
    pub(crate) unload_after: bool,
    /// The edit links what it places (`placeAndLink`, `linkPlaced`): objects of another document need the link numbers from it (xlink.rs).
    pub(crate) wants_link: bool,
    /// The source document of such links and the numbers of its objects, for the editor of this edit.
    pub(crate) foreign: Option<folio_edit::ForeignSource>,
    /// The source documents of the links an update of this edit takes from (by the reference the `Link` writes), read from their files.
    pub(crate) foreign_docs: std::collections::BTreeMap<String, std::sync::Arc<std::collections::BTreeMap<String, String>>>,
    /// Placed text whose characters the font lacks takes installed fonts that have them (`options.fillMissingGlyphs`, on by default; glyphfill.rs).
    pub(crate) fill_glyphs: bool,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if !self.committed {
            for path in &self.created {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
impl Prepared {
    /// The references of the files this request brought into the project, kept (the document the edit is for
    /// is not an open one: its manifest takes them).
    pub(crate) fn take_references(mut self) -> Vec<Reference> {
        self.committed = true;
        std::mem::take(&mut self.refs)
    }

    pub fn commit(mut self, es: &mut EditState) {
        if let Some(g) = &mut es.graphics {
            g.references.append(&mut self.refs);
            g.placed_sources.extend(self.sources.drain());
        }
        self.committed = true;
        if self.unload_after {
            crate::conveyor::unload();
        }
    }
}

pub fn prepare(req: &Value, cur: &Session, es: &EditState) -> Result<(Value, Prepared), String> {
    let mut commands = req["commands"].clone();
    let mut prepared = Prepared::default();
    let Some(list) = commands.as_array_mut() else {
        return Err("편집 명령이 없습니다".into());
    };
    prepared.wants_link = list.iter().any(|c| c["op"] == "placeAndLink" || c["op"] == "linkPlaced");
    for cmd in list {
        if cmd["op"] == "loadParentPages" {
            crate::parent_links::prepare(cur, es, cmd, &mut prepared)?;
            continue;
        }
        if cmd["op"] == "placeLibraryAsset" || cmd["op"] == "placeLibraryAssetInText" {
            crate::library::links::prepare(cur, es, cmd, &mut prepared)?;
            continue;
        }
        if cmd["op"] == "reconnectContentLinks" {
            // content links InDesign made to another document: joined to the project's copy of it (xlink.rs)
            crate::xlink::prepare_reconnect(cur, es, cmd, &mut prepared)?;
            continue;
        }
        if cmd["op"] == "updateContentLink" {
            // a link to another document takes its source from that document's file (xlink.rs)
            crate::xlink::prepare_update(cur, es, cmd, &mut prepared);
            continue;
        }
        if cmd["op"] == "placeText" {
            prepared.fill_glyphs |= prepare_text(cmd, cur)?;
            continue;
        }
        if (cmd["op"] == "paste" || cmd["op"] == "placeAndLink") && prepared.clipboard.is_none() {
            // placing from the conveyor (Content Placer) pastes its item; else the clipboard
            prepared.clipboard = match req.get("conveyor").filter(|c| c.is_object()) {
                Some(spec) => crate::conveyor::for_place(cur, es, &mut prepared, spec)?,
                None => crate::clip::for_paste(cur, es, &mut prepared)?,
            };
            continue;
        }
        if cmd["op"] == "unembedLink" && cmd["uri"].is_string() {
            continue; // the address is given (a document without a project, the checks)
        }
        if cmd["op"] == "unembedLink" {
            // the embedded bytes become a file of the project (a project's pictures are files, as for any link)
            let id = cmd["id"].as_str().ok_or("링크 포함 해제에는 그림이 있어야 합니다")?.to_owned();
            let f = cur.folio.as_ref().ok_or("링크 포함 해제는 프로젝트에서만 할 수 있습니다")?;
            let data = es.editor.embedded_data(&id).ok_or("이 그림은 포함되어 있지 않습니다")?;
            let bytes = folio_project::idmlparts::decode_base64(&data).ok_or("포함된 그림 내용을 읽을 수 없습니다")?;
            let tmp = std::env::temp_dir().join(format!("folio-unembed-{}.bin", folio_project::util::short_id()));
            std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
            let result = (|| {
                let source = folio_edit::GraphicSource::read(&tmp, String::new())?;
                let ext = source.extension();
                import_link(&tmp, ext, f, es, &mut prepared)
            })();
            let _ = std::fs::remove_file(&tmp);
            let (reference, _) = result?;
            cmd["uri"] = json!(format!("{}{}", folio_project::format::REF_SCHEME, reference.id));
            continue;
        }
        // Relink brings its file into the project as a placement does
        if cmd["op"] == "embedLink" {
            // the file's bytes, written as InDesign writes embedded data
            let file = cmd["file"].as_str().ok_or("링크를 포함하려면 원본 파일이 있어야 합니다")?;
            // the file goes into the document as text: a very large one is refused
            const MAX_EMBED: u64 = 128 << 20;
            if std::fs::metadata(file).map_err(|e| format!("{file}: {e}"))?.len() > MAX_EMBED {
                return Err("128 MB보다 큰 파일은 포함할 수 없습니다".into());
            }
            let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
            cmd["data"] = json!(folio_project::idmlparts::base64_lines(&bytes));
            cmd.as_object_mut().expect("embedLink has a string op and file field").remove("file");
            continue;
        }
        if cmd["op"] != "placeGraphic" && cmd["op"] != "relinkGraphic" {
            continue;
        }
        let Some(file) = cmd["file"].as_str() else {
            if cur.folio.is_some() {
                return Err("프로젝트 배치는 원본 파일을 지정해야 합니다".into());
            }
            continue;
        };
        let file = PathBuf::from(file);
        let info = folio_links::probe::probe(&file).map_err(|e| probe_message(&e))?;
        let pdf = cmd.get("pdf").map(|v| serde_json::from_value::<folio_links::pdfplace::Options>(v.clone())).transpose().map_err(|e| format!("PDF options: {e}"))?
            .or_else(|| (cmd["op"] == "relinkGraphic" && !info.page_boxes.is_empty()).then(|| crate::pdfplace::placed_options(&cur.doc, cmd["id"].as_str().unwrap_or(""))).flatten());
        let mut source = folio_edit::GraphicSource::read_with_pdf(&file, paths::file_uri(&file), pdf.clone())?;
        crate::pdfplace::prepare(&file, &source, cur.folio.as_deref())?;
        if let Some(profile) = info.icc {
            folio_color::icc::Profile::parse(&profile)
                .map_err(|e| format!("읽을 수 없는 그림 색 프로필: {e}"))?;
        }
        if let Some(f) = &cur.folio {
            let ext = source.extension();
            let (reference, path) = import_link(&file, ext, f, es, &mut prepared)?;
            source = folio_edit::GraphicSource::read_with_pdf(
                &path,
                format!("{}{}", folio_project::format::REF_SCHEME, reference.id),
                pdf,
            )?;
        }
        // import options of a picture (`options.renderingIntent`: the colour intent the Image is placed with)
        if let Some(intent) = cmd.get("options").and_then(|o| o.get("renderingIntent")).and_then(Value::as_str) {
            if !folio_model::image_color::INTENTS.contains(&intent) {
                return Err("알 수 없는 렌더링 의도입니다".into());
            }
            source.rendering_intent = (intent != "UseColorSettings").then(|| intent.to_owned());
        }
        let object = cmd.as_object_mut().expect("graphic command has a string op and file field");
        object.remove("file");
        object.remove("pdf");
        object.remove("options");
        cmd["source"] = json!(source);
    }
    Ok((commands, prepared))
}


/// What a person is told when a picture file cannot be placed: the format by name, and what can be (`이 형식은 아직 지원하지 않습니다`).
/// The same sentence comes from the place query (the screen refuses a file before it is loaded) and from the command.
pub fn probe_message(e: &folio_links::probe::ProbeError) -> String {
    match e {
        // a metafile whose header (signature, size) cannot be read (build-a: EMF and WMF are placed when they can)
        folio_links::probe::ProbeError::Unsupported(what) if matches!(what.as_str(), ".emf" | ".wmf") => {
            "이 메타파일(EMF·WMF)은 읽을 수 없습니다: 머리(서명·크기)가 올바르지 않습니다. 다시 저장하거나 PDF나 PNG로 바꿔서 놓으세요".into()
        }
        folio_links::probe::ProbeError::Unsupported(what) => format!("이 형식은 아직 지원하지 않습니다: {what}. JPEG·PNG·TIFF·PSD·GIF·BMP·PDF를 놓을 수 있습니다"),
        other => other.to_string(),
    }
}

/// A file a graphic of the document will link to, brought into the project (principle 1):
/// a reference already holding the same content is used again; a file inside the project is
/// referred to where it is; another is found in the project by its hash or else copied to
/// `assets/<document>/<hash>.<ext>`. Returns the reference and the file in the project; the
/// reference and the file's location are kept in `prepared` (given to the document when the
/// edit is applied; a file copied here is removed when it is not).
pub fn import_link(
    file: &Path,
    ext: &str,
    f: &crate::project::FolioSession,
    es: &EditState,
    prepared: &mut Prepared,
) -> Result<(Reference, PathBuf), String> {
    let root = &f.opened.project.root;
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    let hash = folio_project::hash::sha256_bytes(&bytes);
    let size = bytes.len() as u64;
    let existing = es
        .graphics
        .as_ref()
        .into_iter()
        .flat_map(|g| &g.references)
        .chain(&prepared.refs)
        .find(|r| {
            r.kind == RefKind::Link
                && r.sha256.as_deref() == Some(&hash)
                && r.bytes == Some(size)
                && r.path.as_ref().is_some_and(|p| {
                    folio_project::hash::sha256_file(&p.to_native(root)).ok()
                        == Some((hash.clone(), size))
                })
        });
    let (reference, path) = if let Some(r) = existing {
        (r.clone(), r.path.as_ref().expect("matching reference has a path whose file hash was checked").to_native(root))
    } else {
        let mut index = folio_project::hash::HashIndex::load(root);
        let rel = if let Ok(r) = paths::relative_to_root(root, file) {
            if paths::link_inside(root, file).is_some() {
                return Err("프로젝트의 심볼릭 링크에는 그림을 놓을 수 없습니다".into());
            }
            r
        } else if let Some(r) = index.find_by_hash(&hash, size) {
            r
        } else {
            let r = RelPath::parse(&format!("assets/{}/{}.{ext}", f.opened.manifest.id, hash))
                .map_err(|e| e.to_string())?;
            let dest = r.to_native(root);
            if paths::link_inside(root, &dest).is_some() {
                return Err("그림 폴더에 심볼릭 링크가 있습니다".into());
            }
            std::fs::create_dir_all(dest.parent().expect("asset destination contains its assets directory")).map_err(|e| e.to_string())?;
            use std::io::Write;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
            {
                Ok(mut out) => {
                    prepared.created.push(dest);
                    out.write_all(&bytes).map_err(|e| e.to_string())?;
                    out.sync_all().map_err(|e| e.to_string())?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if folio_project::hash::sha256_file(&dest).ok()
                        != Some((hash.clone(), size))
                    {
                        return Err("같은 이름의 그림 내용이 다릅니다".into());
                    }
                }
                Err(e) => return Err(e.to_string()),
            }
            r
        };
        let path = rel.to_native(root);
        let r = Reference {
            id: format!("r{}", folio_project::util::short_id()),
            kind: RefKind::Link,
            path: Some(rel),
            sha256: Some(hash),
            bytes: Some(size),
            hint: None,
            original_name: file.file_name().map(|s| s.to_string_lossy().into_owned()),
            status: Some("found".into()),
            font: None,
            extra: Default::default(),
        };
        prepared.refs.push(r.clone());
        (r, path)
    };
    let m = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let time = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64());
    prepared.sources.insert(
        reference.id.clone(),
        folio_idml::LinkOverride {
            uri: paths::file_uri(&path),
            stamp: Some((Some(size), time)),
        },
    );
    Ok((reference, path))
}

/// Export uses the live references too, even before the first save.
pub fn snapshot(
    opened: &folio_project::open::Opened,
    es: &EditState,
) -> folio_project::open::Opened {
    let mut o = opened.clone();
    if let Some(g) = &es.graphics {
        o.manifest.graphics = g.synced.clone();
        for r in &g.references {
            if !o.manifest.references.iter().any(|old| old.id == r.id) {
                o.manifest.references.push(r.clone());
            }
            if o.refs.iter().any(|old| old.id == r.id) {
                continue;
            }
            let file = r
                .path
                .as_ref()
                .and_then(|p| paths::resolve_on_disk(&o.project.root, p));
            let m = file.as_ref().and_then(|p| std::fs::metadata(p).ok());
            let mtime = m
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64());
            let valid = file
                .as_ref()
                .and_then(|p| folio_project::hash::sha256_file(p).ok())
                .is_some_and(|(h, b)| r.sha256.as_deref() == Some(&h) && r.bytes == Some(b));
            o.refs.push(folio_project::open::RefState {
                id: r.id.clone(),
                kind: r.kind,
                path: r.path.clone(),
                state: if file.is_none() {
                    "missing"
                } else if valid {
                    "ok"
                } else {
                    "changed"
                },
                found_at: None,
                file,
                size: m.map(|m| m.len()),
                mtime,
            });
        }
    }
    o
}

/// InDesign previews have already been converted to document colour. Newly placed RGB
/// originals have not. Keep their embedded profile local to that source's image cache.
pub fn configure_previews(s: &Session) {
    fn walk(items: &[folio_model::PageItem], files: &mut std::collections::HashSet<String>) {
        for item in items {
            if let folio_model::ItemKind::Group { children } = &item.kind { walk(children, files) }
            // A source raster is used as preview only when both paths identify it.
            let graphic = match &item.kind {
                folio_model::ItemKind::Rectangle { content }
                | folio_model::ItemKind::Oval { content }
                | folio_model::ItemKind::Polygon { content } => content.as_ref(),
                _ => None,
            };
            if let Some(g) = graphic
                && let Some(path) = &g.preview
            {
                // a layered PSD's composed picture lives in the cache but keeps the file's own profile
                if g.link_uri.as_deref() == Some(&paths::file_uri(std::path::Path::new(path))) || g.layer_options.is_some() && g.element == "Image" {
                    files.insert(path.clone());
                }
            }
        }
    }
    let mut files = std::collections::HashSet::new();
    for spread in &s.doc.spreads {
        walk(&spread.items, &mut files);
    }
    let cmyk = s.cmyk_profile.as_ref().and_then(|p| std::fs::read(p).ok());
    for path in files {
        if let Ok(info) = folio_links::probe::probe(std::path::Path::new(&path))
            && let Some(profile) = info.icc
            && let Ok(colors) = folio_color::Colors::from_profiles(cmyk.as_deref(), Some(&profile))
        {
            s.images.register_source_colors(path.clone(), colors.clone());
            s.ink_images.register_source_colors(path, colors);
        }
    }
}

/// The file of a text placement: a path relative to the project root (not leaving it) or an absolute path the user picked.
fn text_path(file: &str, cur: Option<&Session>) -> Result<PathBuf, String> {
    let given = Path::new(file);
    if given.is_absolute() {
        return Ok(given.to_path_buf());
    }
    let f = cur.and_then(|c| c.folio.as_ref()).ok_or("프로젝트 없이는 프로젝트 기준 상대 경로로 글 파일을 놓을 수 없습니다")?;
    Ok(RelPath::parse(&file.replace('\\', "/")).map_err(|e| e.to_string())?.to_native(&f.opened.project.root))
}

/// Place of a text file: the file is read here (outside the editor) and the command carries what it holds. The text goes
/// into the document itself, so nothing of the file stays in the project (no link, no copy; principle 1). The file is only read.
/// Returns whether the characters its fonts lack take installed fonts (`fillMissingGlyphs`, on by default).
fn prepare_text(cmd: &mut Value, cur: &Session) -> Result<bool, String> {
    let Some(file) = cmd["file"].as_str().map(str::to_owned) else {
        return if cmd.get("source").is_some() {
            Ok(cmd.as_object_mut().and_then(|o| o.remove("fillMissingGlyphs")).and_then(|v| v.as_bool()).unwrap_or(true))
        } else {
            Err("글 가져와 놓기에는 원본 파일(file)이 있어야 합니다".into())
        };
    };
    let path = text_path(&file, Some(cur))?;
    let options = folio_textimport::Options::from_dom(cmd.get("options").unwrap_or(&Value::Null))?;
    let mut source = folio_textimport::read(&path, &options)?;
    folio_edit::prune_unreadable_pictures(&mut source);
    let object = cmd.as_object_mut().expect("text command has a string file field");
    object.remove("file");
    object.remove("options");
    object.insert("source".into(), json!(source));
    Ok(options.fill_missing_glyphs)
}

/// `{"cmd":"textImportInfo","file":…,"options":{…}}`: what Place would put into a story, without placing it (for a dialog or an
/// LLM that chooses the options): the format, the paragraphs, the characters, the styles and swatches a Word file brings, and what
/// was not carried over.
pub fn text_info(req: &Value, cur: Option<&Session>) -> Result<Value, String> {
    let file = req["file"].as_str().ok_or("file required")?;
    let options = folio_textimport::Options::from_dom(req.get("options").unwrap_or(&Value::Null))?;
    let path = text_path(file, cur)?;
    let mut source = folio_textimport::read(&path, &options)?;
    folio_edit::prune_unreadable_pictures(&mut source);
    let text = source.story_text();
    // the character set a plain text file is read with (AUTO: the one its bytes look like) and the one its bytes look like
    let (character_set, detected) = if source.format == "txt" {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let detected = folio_textimport::detect_character_set(&bytes);
        (json!(options.character_set.resolve(&bytes)), json!(detected))
    } else {
        (Value::Null, Value::Null)
    };
    Ok(json!({
        "info": {"format": source.format, "paragraphs": source.paragraph_count(), "characters": text.chars().count(),
                 "characterSet": character_set, "detectedCharacterSet": detected,
                 "endsWithReturn": source.ends_with_return, "warnings": source.warnings, "warningMessages": source.warning_messages, "preview": text.chars().take(200).collect::<String>(),
                 "paragraphStyles": source.paragraph_styles.iter().map(|s| &s.name).collect::<Vec<_>>(),
                 "characterStyles": source.character_styles.iter().map(|s| &s.name).collect::<Vec<_>>(),
                 "colors": source.colors.iter().map(|c| &c.name).collect::<Vec<_>>()}
    }))
}
