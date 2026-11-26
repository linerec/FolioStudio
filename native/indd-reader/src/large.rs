//! File-backed lean import. Page/text/geometry are never truncated for previews.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::Path,
};
type R<T> = Result<T, Box<dyn std::error::Error>>;
const MIB: usize = 1024 * 1024;
struct LimitedWriter<W> {
    inner: W,
    bytes: usize,
    limit: usize,
}
impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.saturating_add(bytes.len()) > self.limit {
            return Err(io::Error::other("lean IDML exceeds 32 MiB"));
        }
        let n = self.inner.write(bytes)?;
        self.bytes += n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub fn decode(input: &Path, folder: &Path, name: &str, embedded: bool) -> R<()> {
    let file = fs::File::open(input)?;
    let source_bytes = file.metadata()?.len();
    let (db, header, contig) = indd_large::Database::open_file(file)?;
    let reader = indd_large::model::Reader::new_lean(&db);
    let mut doc = reader.document(header.version)?;
    let omitted = reader.omitted_graphic_objects();
    let omitted_contents = reader.omitted_contents();
    drop(reader);
    let mut warnings = doc.warnings.clone();
    if let Ok(head) = db.read_range(contig, 32)
        && head[..16] == indd_large::container::CONTIG_HEADER_GUID
    {
        let len = u32::from_le_bytes(head[24..28].try_into()?) as usize;
        match db.read_range(contig + 32, len) {
            Ok(data) if data.len() >= 4 => {
                let n = header.byte_order.read_u32(&data, 0) as usize;
                if n <= data.len() - 4 {
                    doc.read_xmp(&data[4..4 + n]);
                }
            }
            Err(e) => warnings.push(format!("XMP metadata omitted: {e}")),
            _ => {}
        }
    }
    let mut preview_graphics: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut preview_colors: BTreeMap<u32, BTreeSet<Option<u32>>> = BTreeMap::new();
    let mut linked = BTreeSet::new();
    let mut graphic_ids = BTreeSet::new();
    let mut item_ids = BTreeSet::new();
    let mut items: Vec<&indd_large::model::PageItem> = doc
        .spreads
        .iter()
        .chain(&doc.master_spreads)
        .flat_map(|s| &s.items)
        .chain(
            doc.stories
                .iter()
                .flat_map(|s| s.anchors.values().flatten()),
        )
        .chain(
            doc.stories
                .iter()
                .flat_map(|s| s.tables.values())
                .flat_map(|t| &t.cells)
                .flat_map(|c| &c.items),
        )
        .collect();
    while let Some(item) = items.pop() {
        if !item_ids.insert(item.uid) {
            continue;
        }
        for g in item
            .graphics
            .iter()
            .chain(item.media.iter().filter_map(|m| m.poster.as_ref()))
        {
            graphic_ids.insert(g.uid);
            if let Some(link) = &g.link {
                linked.insert(link.uid);
            }
            if let Some(uid) = g.preview {
                preview_colors
                    .entry(uid)
                    .or_default()
                    .insert(g.image.as_ref().and_then(|i| i.color_space));
                preview_graphics
                    .entry(uid)
                    .or_default()
                    .push(format!("u{:x}", g.uid));
            }
        }
        items.extend(&item.children);
        if let indd_large::model::ItemKind::Form(f) = &item.kind {
            items.extend(f.states.iter().flat_map(|s| &s.items));
        }
    }
    let source_pages: usize = doc.spreads.iter().map(|s| s.pages.len()).sum();
    let master_pages: usize = doc.master_spreads.iter().map(|s| s.pages.len()).sum();
    let stories = doc.stories.len();
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(folder.join("document.idml"))?;
    let mut output = LimitedWriter {
        inner: file,
        bytes: 0,
        limit: 32 * MIB,
    };
    warnings.extend(indd_large::idml::write(&doc, name, &mut output)?);
    output.flush()?;
    output.inner.sync_all()?;
    let (lines_json, lines_summary) = indd_large::idml::lines::saved_lines(&doc);
    let lines: serde_json::Value = serde_json::from_str(&super::write_lines(folder, &lines_json, lines_summary, &mut warnings)?)?;
    drop(lines_json);
    let mut previews = Vec::new();
    // raw object UID → the file it was written to (the raw bytes, not a display wrapper)
    let mut preview_files: BTreeMap<u32, String> = BTreeMap::new();
    let mut preview_bytes = 0usize;
    let preview_candidates = preview_graphics.len();
    let mut omitted_previews = 0;
    let mut wrapped_eps_previews = 0;
    for (uid, graphics) in preview_graphics {
        let len = db.object_len(uid).ok_or("preview object missing")?;
        if previews.len() >= 1024
            || len > 64 * MIB
            || preview_bytes.saturating_add(len) > 512 * MIB
            || graphics.len() > 10000
        {
            omitted_previews += 1;
            warnings.push(format!("preview raw object {uid} ({len} bytes) omitted: bounded preview limit; page and placement retained"));
            continue;
        }
        let filename = format!("preview-{}.bin", previews.len());
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join(&filename))?;
        db.stream_object(uid, &mut target, 64 * MIB)?;
        target.flush()?;
        drop(target);
        preview_bytes += len;
        preview_files.insert(uid, filename.clone());
        let mut entry =
            serde_json::json!({"file":filename,"graphics":graphics,"rawObjectUid":uid,"bytes":len});
        let colors = preview_colors.get(&uid);
        let color_space = colors
            .filter(|c| c.len() == 1)
            .and_then(|c| c.first().copied().flatten());
        match super::epsimage::wrap(folder, &filename, color_space) {
            Ok(Some(wrapped)) => {
                entry["file"] = wrapped.file.into();
                entry["bytes"] = wrapped.bytes.into();
                entry["rawFile"] = filename.into();
                entry["rawBytes"] = len.into();
                entry["normalization"] = format!("EPSImage {}-channel to lossless uncompressed TIFF; no ICC profile",wrapped.channels).into();
                wrapped_eps_previews += 1;
            }
            Ok(None) => {},
            Err(error) => warnings.push(format!("Preview raw object {uid}: EPSImage display wrapper unavailable: {error}; raw bytes retained")),
        }
        previews.push(entry);
    }
    let embedded_entry = if embedded {
        write_embedded(&db, folder, &omitted_contents, &preview_files, &mut warnings)?
    } else {
        serde_json::Value::Null
    };
    if wrapped_eps_previews > 0 {
        warnings.push(format!("{wrapped_eps_previews} EPSImage previews wrapped as gray/RGB/CMYK TIFF without changing pixel bytes, checked against source image color metadata. Preview color is approximate because no ICC profile is attached."));
    }
    warnings.push(format!("File-backed lean import: {source_pages} pages, {stories} stories, {} graphic placements; {omitted} embedded pixel payloads omitted from IDML and {} previews streamed separately. Preview resolution is limited by the host; original linked file URIs are retained.",graphic_ids.len(),previews.len()));
    let (read_bytes, cache_misses) = db.storage_stats();
    let coverage = serde_json::json!({"pages":source_pages,"masterPages":master_pages,"spreads":doc.spreads.len(),"masterSpreads":doc.master_spreads.len(),"stories":stories,"links":linked.len(),"graphics":graphic_ids.len(),"pageItems":item_ids.len(),"previewCandidates":preview_candidates,"previewCount":previews.len(),"previewBytes":preview_bytes,"omittedPreviews":omitted_previews,"wrappedEpsPreviews":wrapped_eps_previews,"embeddedContentsOmitted":true,"omittedEmbeddedObjects":omitted,"sourceBytes":source_bytes,"storageReadBytes":read_bytes,"cacheMisses":cache_misses,"cacheBytes":8*MIB,"idmlBytes":output.bytes});
    let warning_count = warnings.len();
    let mut warnings: Vec<String> = warnings
        .into_iter()
        .take(900)
        .map(|w| w.chars().take(3000).collect())
        .collect();
    if warning_count > 900 {
        warnings.push(format!(
            "{} additional decoder warnings omitted from inline list",
            warning_count - 900
        ));
    }
    let manifest = serde_json::json!({"ok":true,"parser":"indd-utils@7f3060e5+folio.7","mode":"file-backed-lean","warnings":warnings,"previews":previews,"coverage":coverage,"savedLines":lines,"embedded":embedded_entry});
    let bytes = serde_json::to_vec(&manifest)?;
    if bytes.len() > MIB {
        return Err("lean manifest exceeds 1 MiB".into());
    }
    super::write_new(&folder.join("result.json"), &bytes)?;
    Ok(())
}

/// Format of the embedded-file sidecar (`embedded.json`).
pub const EMBEDDED_FORMAT: &str = "folio-embedded";
pub const EMBEDDED_VERSION: u32 = 1;
/// Bounds of the selective extraction: per object, in total, and the number of objects.
const EMBEDDED_OBJECT_LIMIT: usize = 1024 * MIB;
const EMBEDDED_TOTAL_LIMIT: u64 = 16 * 1024 * MIB as u64;
const EMBEDDED_COUNT_LIMIT: usize = 8192;

/// Writes the embedded files the lean IDML leaves out (embedded links, pasted graphics,
/// images whose file is their preview): one `embedded-<n>.bin` per raw data object, the raw
/// bytes InDesign stores (what full IDML writes as `Contents`), and `embedded.json` naming
/// each with the graphics (`u<hex UID>`) that use it. An object that is also a preview is
/// not written twice: its entry names the preview file instead. Returns the result.json
/// entry.
fn write_embedded(
    db: &indd_large::Database<'_>,
    folder: &Path,
    pairs: &[(u32, u32)],
    preview_files: &BTreeMap<u32, String>,
    warnings: &mut Vec<String>,
) -> R<serde_json::Value> {
    let mut graphics: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut order: Vec<u32> = Vec::new();
    for &(g, raw) in pairs {
        let list = graphics.entry(raw).or_insert_with(|| {
            order.push(raw);
            Vec::new()
        });
        let id = format!("u{g:x}");
        if !list.contains(&id) {
            list.push(id);
        }
    }
    let mut items = Vec::new();
    let (mut written, mut bytes, mut shared, mut omitted) = (0usize, 0u64, 0usize, 0usize);
    for raw in order {
        let users = graphics.remove(&raw).unwrap_or_default();
        let len = db.object_len(raw).ok_or("embedded object missing")?;
        if let Some(file) = preview_files.get(&raw) {
            shared += 1;
            items.push(serde_json::json!({"previewFile": file, "rawObjectUid": raw, "bytes": len, "graphics": users}));
            continue;
        }
        if written >= EMBEDDED_COUNT_LIMIT || len > EMBEDDED_OBJECT_LIMIT || bytes + len as u64 > EMBEDDED_TOTAL_LIMIT {
            omitted += 1;
            warnings.push(format!("embedded raw object {raw} ({len} bytes) not extracted: extraction limit; its preview remains"));
            continue;
        }
        let filename = format!("embedded-{written}.bin");
        let mut target = fs::OpenOptions::new().write(true).create_new(true).open(folder.join(&filename))?;
        let n = db.stream_object(raw, &mut target, EMBEDDED_OBJECT_LIMIT)?;
        target.flush()?;
        drop(target);
        written += 1;
        bytes += n as u64;
        items.push(serde_json::json!({"file": filename, "rawObjectUid": raw, "bytes": n, "graphics": users}));
    }
    let sidecar = serde_json::json!({"format": EMBEDDED_FORMAT, "version": EMBEDDED_VERSION, "items": items});
    super::write_new(&folder.join("embedded.json"), &serde_json::to_vec(&sidecar)?)?;
    Ok(serde_json::json!({"file": "embedded.json", "format": EMBEDDED_FORMAT, "version": EMBEDDED_VERSION,
        "objects": written + shared, "files": written, "bytes": bytes, "sharedWithPreviews": shared, "notExtracted": omitted}))
}
