//! Read-only glyph catalogues of resolved faces. Unicode mappings are aliases, not text sequences.
mod alternates;
mod feature_view;
mod outline;
mod show;
mod unicode_order;
mod wire;
pub use alternates::Alternate;
pub use wire::{Request, Filter, Page, Entry, ShowItem, typescript};
use crate::FontStore;
use folio_model::{FontInfo, layout::FontId};
use std::{collections::{BTreeSet, HashMap}, sync::{Arc, Mutex}};
use ttf_parser::{Face, GlyphId};

pub(crate) type CatalogCache = Mutex<HashMap<FontId, Arc<Vec<Mapping>>>>;
pub(crate) struct Mapping { id: u16, unicode: Vec<u32>, name: Option<String> }

fn catalog(face: &Face<'_>) -> Vec<Mapping> {
    let mut mappings: Vec<BTreeSet<u32>> = (0..face.number_of_glyphs()).map(|_| BTreeSet::new()).collect();
    if let Some(cmap) = face.tables().cmap {
        for sub in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
            sub.codepoints(|cp| {
                if let Some(id) = char::from_u32(cp).and_then(|c| face.glyph_index(c)) {
                    if let Some(row) = mappings.get_mut(usize::from(id.0)) { row.insert(cp); }
                }
            });
        }
    }
    mappings.into_iter().enumerate().map(|(id, unicode)| Mapping {
        id: id as u16, unicode: unicode.into_iter().collect(),
        name: face.glyph_name(GlyphId(id as u16)).map(str::to_owned),
    }).collect()
}

pub fn query(store: &FontStore, table: &[FontInfo], req: &Request) -> Result<Page, String> {
    if req.family.trim().is_empty() || req.style.trim().is_empty() { return Err("font family and style required".into()); }
    if !(1..=128).contains(&req.limit) { return Err("glyph page limit must be 1..128".into()); }
    if !matches!(req.order.as_str(), "" | "gid" | "unicode") { return Err("glyph order must be gid or unicode".into()); }
    // InDesign opens the panel with By Unicode checked (measured, glyphs.sort): "" is By Unicode, "gid" is By CID / GID
    let by_gid = req.order == "gid";
    let resolved = store.resolve(&req.family, &req.style, table);
    if !resolved.exact { return Err("font face not found".into()); }
    let info = store.face_info(resolved.font).ok_or("font face not found")?;
    let cached = store.glyph_catalogs.lock().map_err(|_| "glyph cache unavailable")?.get(&resolved.font).cloned();
    let rows = match cached {
        Some(rows) => rows,
        None => {
            let rows = Arc::new(store.with_face(resolved.font, catalog).ok_or("font face unreadable")?);
            let mut cache = store.glyph_catalogs.lock().map_err(|_| "glyph cache unavailable")?;
            if cache.len() >= 8 { cache.clear(); }
            cache.insert(resolved.font, rows.clone()); rows
        }
    };
    let search = req.search.trim().to_lowercase();
    // Alternates for Selection: the glyphs the face gives the selected text, in the order of `alternates::list`
    let edges = if by_gid && !matches!(req.filter, Filter::Alternates) && req.show.is_empty() { None } else { Some(store.with_face(resolved.font, unicode_order::edges).ok_or("font face unreadable")?) };
    let alternates: Option<Vec<Alternate>> = match req.filter {
        Filter::Alternates => {
            let mut list = store.with_shaping_face(resolved.font, |face| alternates::list(face, &req.text)).ok_or("font face unreadable")?;
            alternates::arrange(&mut list, &rows, edges.as_ref().ok_or("font face unreadable")?, &req.text);
            Some(list)
        }
        _ => None,
    };
    let shown = |row: &Mapping| {
        let visible = match req.filter { Filter::All | Filter::Alternates => true, Filter::Mapped => !row.unicode.is_empty(), Filter::Unmapped => row.unicode.is_empty() };
        visible && (search.is_empty() || row.id.to_string() == search
            || row.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&search))
            || row.unicode.iter().any(|cp| format!("u+{cp:04x}").contains(&search)
                || char::from_u32(*cp).is_some_and(|c| c.to_string().to_lowercase() == search)))
    };
    let mut matches: Vec<_> = match &alternates {
        Some(list) => list.iter().filter_map(|a| rows.get(usize::from(a.glyph))).filter(|row| shown(row)).collect(),
        None => rows.iter().filter(|row| shown(row)).collect(),
    };
    // By Unicode (the default): InDesign's order of "Entire Font", the alternates of a character right after it (unicode_order)
    if let (None, Some(edges), false) = (&alternates, &edges, by_gid) {
        let mut rank = vec![u32::MAX; rows.len()];
        for (i, id) in unicode_order::order(&rows, edges).into_iter().enumerate() { rank[usize::from(id)] = i as u32; }
        matches.sort_by_key(|row| rank[usize::from(row.id)]);
    }
    // a Show item of the panel that is a range or a feature view (rules/glyphs-c3): the glyphs of that range / that feature's substitutions
    if let Some(keep) = show::view(&req.show) { matches.retain(|row| keep(row)); }
    if let Some(tag) = req.show.strip_prefix("feature:").and_then(|t| <[u8; 4]>::try_from(t.as_bytes()).ok()) {
        let given = store.with_face(resolved.font, |face| feature_view::targets(face, tag)).ok_or("font face unreadable")?;
        matches.retain(|row| given.contains(&row.id));
    }
    // Show "ids:<glyph id>,<glyph id>...": those glyphs of the face in that order (the panel's Recent Glyphs asks for the entries it has remembered)
    if let Some(list) = req.show.strip_prefix("ids:") {
        matches = list.split(',').filter_map(|s| s.trim().parse::<usize>().ok()).filter_map(|id| rows.get(id)).collect();
    }
    store.with_face(resolved.font, |face| {
        let show_items = show::items(&rows, &show::features(face)).into_iter().map(|(group, id, name)| ShowItem { id, name, group: u16::from(group) }).collect();
        let entries = matches.iter().skip(req.offset).take(req.limit).map(|row| {
            let gid = GlyphId(row.id);
            let (path, bounds) = outline::read(face, gid);
            Entry { id: row.id, unicode: row.unicode.clone(), name: row.name.clone(),
                insertable: row.unicode.iter().copied().filter(|cp| char::from_u32(*cp).is_some_and(|c| !c.is_control())).collect(),
                path, bounds, advance: face.glyph_hor_advance(gid).unwrap_or(0),
                features: alternates.iter().flatten().find(|a| a.glyph == row.id).map(|a| a.features.clone()).unwrap_or_default() }
        }).collect();
        Page { family: info.family.clone(), style: info.style.clone(), postscript: info.postscript.clone(),
            glyph_count: rows.len(), total: matches.len(), offset: req.offset, limit: req.limit,
            units_per_em: face.units_per_em(), entries, show_items }
    }).ok_or_else(|| "font face unreadable".into())
}

/// The glyph table of one resolved face: glyph count and, per glyph id, its Unicode scalars (the cmap, reversed).
pub fn table(store: &FontStore, font_table: &[FontInfo], family: &str, style: &str) -> Result<(usize, Vec<(u16, Vec<u32>)>), String> {
    let resolved = store.resolve(family, style, font_table);
    if !resolved.exact { return Err("font face not found".into()); }
    let cached = store.glyph_catalogs.lock().map_err(|_| "glyph cache unavailable")?.get(&resolved.font).cloned();
    let rows = match cached {
        Some(rows) => rows,
        None => {
            let rows = Arc::new(store.with_face(resolved.font, catalog).ok_or("font face unreadable")?);
            let mut cache = store.glyph_catalogs.lock().map_err(|_| "glyph cache unavailable")?;
            if cache.len() >= 8 { cache.clear(); }
            cache.insert(resolved.font, rows.clone()); rows
        }
    };
    Ok((rows.len(), rows.iter().filter(|row| !row.unicode.is_empty()).map(|row| (row.id, row.unicode.clone())).collect()))
}
