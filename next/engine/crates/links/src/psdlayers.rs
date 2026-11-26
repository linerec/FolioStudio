//! Photoshop layers of a placed PSD: the list Object Layer Options shows (`read`) and the
//! picture drawn from the layers that are shown (`flatten`). Layer `Id` is the record index
//! (bottom layer 0), the list is top layer first; both measured against InDesign 2025
//! (objlayer-c4, psdlayers-c1). A layer group is one entry of the list (its folder record);
//! the layers inside it and the group's divider records are not listed. A PSD without layer
//! records has no list.
//!
//! `flatten` composes 8-bit RGB or grayscale pixel layers: opacity, fill, pixel layer masks,
//! blend modes, clipping masks and layer groups (`compose`). A file whose shown layers use
//! anything else (vector masks, effects, adjustments, text, smart objects, 16-bit, CMYK) is
//! refused (the caller keeps the saved merged image and says so). The result is a PSD whose
//! merged image is the composition, so every reader that already draws a flat PSD (screen,
//! PDF, colour profile) works unchanged.
use folio_model::graphic_layers::{Comp, Layer, Options};
use std::ops::Range;

mod adjust;
mod blend;
mod compose;
mod descriptor;
mod vector;

/// A colour value, 0..1 per channel (a grayscale image repeats its one channel).
pub(crate) type Rgb = [f32; 3];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Layer,
    /// The folder record of a layer group (last record of the group).
    Folder,
    /// The "bounding section divider" record that opens a group (first record of the group).
    Divider,
}

struct MaskRec {
    rect: [i32; 4],
    default: u8,
    flags: u8,
}

struct Rec {
    id: u32,
    name: String,
    rect: [i32; 4],
    opacity: u8,
    /// Fill opacity (`iOpa`), 255 when the block is absent.
    fill: u8,
    visible: bool,
    clip: bool,
    blend: [u8; 4],
    mask: Option<MaskRec>,
    section: Section,
    /// Why `flatten` cannot compose this layer (None: it can).
    unsupported: Option<String>,
    channels: Vec<(i16, Range<usize>)>,
    /// Layer comp states saved with the layer (`shmd`/`cmls`): comp id and whether the comp shows the layer.
    comps: Vec<(i32, bool)>,
    /// The vector mask block (`vmsk`) as saved.
    vmask: Option<Vec<u8>>,
    /// An adjustment or fill layer: no pixels of its own.
    adjust: Option<adjust::Adjust>,
}

struct Psd<'a> {
    b: &'a [u8],
    w: usize,
    h: usize,
    depth: u16,
    mode: u16,
    head_end: usize,
    resources: Range<usize>,
    layers: Vec<Rec>,
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn bei32(b: &[u8], at: usize) -> Option<i32> {
    be32(b, at).map(|v| v as i32)
}

/// Layer blocks that do not change the pixels a layer contributes.
const HARMLESS: [&[u8; 4]; 11] = [b"luni", b"lyid", b"lnsr", b"lyvr", b"shmd", b"infx", b"lspf", b"lclr", b"fxrp", b"brst", b"lsdk"];

fn block_reason(key: &[u8]) -> String {
    let name = String::from_utf8_lossy(key).into_owned();
    match key {
        b"vmsk" | b"vsms" => "a vector mask".into(),
        b"lfx2" | b"lrFX" | b"lfxs" => "layer effects".into(),
        b"TySh" | b"tySh" => "a text layer".into(),
        b"SoLd" | b"PlLd" | b"plLd" | b"lnkD" | b"lnk2" | b"lnk3" | b"lnkE" => "a smart object".into(),
        b"SoCo" | b"GdFl" | b"PtFl" | b"brit" | b"levl" | b"curv" | b"expA" | b"hue " | b"hue2" | b"blnc" | b"blwh" | b"phfl" | b"mixr" | b"clrL" | b"nvrt" | b"post" | b"thrs" | b"grdm" | b"selc" | b"vibA" => "an adjustment or fill layer".into(),
        _ => format!("the \"{name}\" block"),
    }
}

fn parse(b: &[u8]) -> Result<Psd<'_>, String> {
    let bad = || "truncated or unsupported PSD".to_string();
    if !b.starts_with(b"8BPS") || be16(b, 4) != Some(1) {
        return Err("not a PSD (or PSB not supported)".into());
    }
    let h = be32(b, 14).ok_or_else(bad)? as usize;
    let w = be32(b, 18).ok_or_else(bad)? as usize;
    let depth = be16(b, 22).ok_or_else(bad)?;
    let mode = be16(b, 24).ok_or_else(bad)?;
    let mut at = 26usize;
    at = at.checked_add(4 + be32(b, at).ok_or_else(bad)? as usize).ok_or_else(bad)?;
    let res_len = be32(b, at).ok_or_else(bad)? as usize;
    let resources = at + 4..at + 4 + res_len;
    at = resources.end;
    let lm_len = be32(b, at).ok_or_else(bad)? as usize;
    let lm = at + 4;
    let head_end = lm + lm_len;
    if head_end > b.len() {
        return Err(bad());
    }
    let mut layers = Vec::new();
    let info_len = if lm_len >= 4 { be32(b, lm).ok_or_else(bad)? as usize } else { 0 };
    if info_len >= 2 {
        let count = be16(b, lm + 4).ok_or_else(bad)? as i16;
        let count = count.unsigned_abs() as usize;
        let info_end = lm + 4 + info_len;
        let mut p = lm + 6;
        let mut pending = Vec::new();
        for index in 0..count {
            let rect = [bei32(b, p).ok_or_else(bad)?, bei32(b, p + 4).ok_or_else(bad)?, bei32(b, p + 8).ok_or_else(bad)?, bei32(b, p + 12).ok_or_else(bad)?];
            let n = be16(b, p + 16).ok_or_else(bad)? as usize;
            p += 18;
            let mut chans = Vec::new();
            for _ in 0..n {
                chans.push((be16(b, p).ok_or_else(bad)? as i16, be32(b, p + 2).ok_or_else(bad)? as usize));
                p += 6;
            }
            if b.get(p..p + 4) != Some(b"8BIM") {
                return Err(bad());
            }
            let blend: [u8; 4] = b.get(p + 4..p + 8).ok_or_else(bad)?.try_into().map_err(|_| bad())?;
            let (opacity, clipping, flags) = (*b.get(p + 8).ok_or_else(bad)?, *b.get(p + 9).ok_or_else(bad)?, *b.get(p + 10).ok_or_else(bad)?);
            let extra = be32(b, p + 12).ok_or_else(bad)? as usize;
            p += 16;
            let extra_end = p + extra;
            if extra_end > info_end {
                return Err(bad());
            }
            let mask_len = be32(b, p).ok_or_else(bad)? as usize;
            let mut unsupported = None;
            let mut mask = None;
            if mask_len == 20 {
                mask = Some(MaskRec {
                    rect: [bei32(b, p + 4).ok_or_else(bad)?, bei32(b, p + 8).ok_or_else(bad)?, bei32(b, p + 12).ok_or_else(bad)?, bei32(b, p + 16).ok_or_else(bad)?],
                    default: *b.get(p + 20).ok_or_else(bad)?,
                    flags: *b.get(p + 21).ok_or_else(bad)?,
                });
            } else if mask_len != 0 {
                unsupported = Some("a layer mask with parameters or a second mask".to_string());
            }
            let mut q = p + 4 + mask_len;
            q += 4 + be32(b, q).ok_or_else(bad)? as usize;
            let name_len = *b.get(q).ok_or_else(bad)? as usize;
            let pascal = b.get(q + 1..q + 1 + name_len).ok_or_else(bad)?;
            q += (1 + name_len).div_ceil(4) * 4;
            let mut name = String::from_utf8_lossy(pascal).into_owned();
            let (mut fill, mut section) = (255u8, Section::Layer);
            let mut comps = Vec::new();
            let (mut vmask, mut adjust) = (None, None);
            while q + 12 <= extra_end {
                let key = &b[q + 4..q + 8];
                let len = be32(b, q + 8).ok_or_else(bad)? as usize;
                let data = b.get(q + 12..q + 12 + len).ok_or_else(bad)?;
                if key == b"luni" {
                    let n = be32(data, 0).unwrap_or(0) as usize;
                    let units: Vec<u16> = (0..n).filter_map(|i| be16(data, 4 + 2 * i)).collect();
                    name = String::from_utf16_lossy(&units);
                } else if key == b"iOpa" {
                    fill = data.first().copied().unwrap_or(255);
                } else if key == b"lsct" {
                    section = match be32(data, 0) {
                        Some(1 | 2) => Section::Folder,
                        Some(3) => Section::Divider,
                        _ => Section::Layer,
                    };
                } else if key == b"clbl" {
                    // "blend clipped elements as group" switched off changes how clipped layers blend
                    if data.first() == Some(&0) && unsupported.is_none() {
                        unsupported = Some("clipped layers that are not blended as a group".into());
                    }
                } else if key == b"shmd" {
                    comps = comp_states(data);
                } else if key == b"vmsk" {
                    vmask = Some(data.to_vec());
                } else if let Some(found) = adjust::from_block(key, data) {
                    match found {
                        Ok(a) => adjust = Some(a),
                        Err(why) => {
                            if unsupported.is_none() {
                                unsupported = Some(why);
                            }
                        }
                    }
                } else if key == b"knko" {
                    if data.first().is_some_and(|v| *v != 0) && unsupported.is_none() {
                        unsupported = Some("knockout".into());
                    }
                } else if !HARMLESS.iter().any(|k| key == &k[..]) && unsupported.is_none() {
                    unsupported = Some(block_reason(key));
                }
                q += 12 + len;
            }
            p = extra_end;
            layers.push(Rec { id: index as u32, name, rect, opacity, fill, visible: flags & 2 == 0, clip: clipping != 0, blend, mask, section, unsupported, channels: Vec::new(), comps, vmask, adjust });
            pending.push(chans);
        }
        // the channel image data of every layer follows the records, in order
        for (rec, chans) in layers.iter_mut().zip(pending) {
            for (id, len) in chans {
                rec.channels.push((id, p..p + len));
                p += len;
            }
        }
        if p > info_end.max(head_end) {
            return Err(bad());
        }
    }
    Ok(Psd { b, w, h, depth, mode, head_end, resources, layers })
}

/// A layer or a layer group (with the records inside it), in file order (bottom first).
enum Node {
    Layer(usize),
    Group(usize, Vec<Node>),
}

impl Node {
    fn rec(&self) -> usize {
        match self {
            Node::Layer(i) | Node::Group(i, _) => *i,
        }
    }
}

/// The records as nested layers: a divider opens a group, the next folder record closes it.
fn tree(psd: &Psd) -> Result<Vec<Node>, String> {
    let bad = || "PSD layer groups are not balanced".to_string();
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    for (i, rec) in psd.layers.iter().enumerate() {
        match rec.section {
            Section::Layer => stack.last_mut().ok_or_else(bad)?.push(Node::Layer(i)),
            Section::Divider => stack.push(Vec::new()),
            Section::Folder => {
                let kids = stack.pop().ok_or_else(bad)?;
                stack.last_mut().ok_or_else(bad)?.push(Node::Group(i, kids));
            }
        }
    }
    if stack.len() != 1 {
        return Err(bad());
    }
    stack.pop().ok_or_else(bad)
}

/// The layers in the order InDesign lists them (top first); each is shown as the file saved it.
/// `None` when the file has no layer records.
pub fn read(bytes: &[u8]) -> Result<Option<Options>, String> {
    let psd = parse(bytes)?;
    if psd.layers.is_empty() {
        return Ok(None);
    }
    let nodes = tree(&psd)?;
    Ok(Some(Options { update_link: "KeepOverrides".into(), layers: listed_layers(&psd, &nodes), comps: comp_list(&psd), ..Default::default() }))
}

/// The comp states of one layer: `shmd` metadata item `cmls`, whose `layerSettings` list gives one
/// entry per comp (`compList`: the comp ids it holds for, `enab`: whether the layer is shown).
fn comp_states(shmd: &[u8]) -> Vec<(i32, bool)> {
    let Some(settings) = descriptor::metadata(shmd, b"cmls").and_then(descriptor::parse_versioned) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in settings.get("layerSettings").map(|v| v.list()).unwrap_or(&[]) {
        let Some(descriptor::Value::Bool(enab)) = entry.get("enab") else { continue };
        for id in entry.get("compList").map(|v| v.list()).unwrap_or(&[]) {
            if let Some(id) = id.int() {
                out.push((id, *enab));
            }
        }
    }
    out
}

/// The layer comps of the file (image resource 1065) with the layers each one shows (ids of
/// the layer list). A comp a layer holds no state for shows the layer as the file saved it.
fn comp_list(psd: &Psd) -> Vec<Comp> {
    let Some(list) = psd.b.get(psd.resources.clone()).and_then(|r| descriptor::resource(r, 1065)).and_then(descriptor::parse_versioned) else { return Vec::new() };
    list.get("list").map(|v| v.list()).unwrap_or(&[]).iter().filter_map(|c| {
        let id = c.get("compID")?.int()?;
        let name = match c.get("Nm  ")? {
            descriptor::Value::Text(t) => t.clone(),
            _ => return None,
        };
        let shown = psd.layers.iter().filter(|l| l.section != Section::Divider && l.comps.iter().find(|(i, _)| *i == id).map_or(l.visible, |(_, v)| *v)).map(|l| l.id).collect();
        Some(Comp { id, name, shown })
    }).collect()
}

/// Top first; a group carries the layers inside it (InDesign writes them nested in the IDML).
fn listed_layers(psd: &Psd, nodes: &[Node]) -> Vec<Layer> {
    nodes.iter().rev().map(|n| {
        let l = &psd.layers[n.rec()];
        let children = match n {
            Node::Group(_, kids) => listed_layers(psd, kids),
            Node::Layer(_) => Vec::new(),
        };
        Layer { id: l.id, name: l.name.clone(), original: l.visible, visible: l.visible, children, adjustment: l.adjust.as_ref().is_some_and(|a| !a.is_fill()) }
    }).collect()
}

/// Whether `options` shows a different set of layers than the file saved.
pub fn overrides(options: &Options) -> bool {
    options.layers.iter().any(|l| l.visible != l.original)
}

fn unpack(src: &[u8], out: &mut [u8]) -> Option<()> {
    let (mut i, mut o) = (0, 0);
    while o < out.len() {
        let n = *src.get(i)? as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            out.get_mut(o..o + k)?.copy_from_slice(src.get(i..i + k)?);
            i += k;
            o += k;
        } else if n != -128 {
            let k = (1 - n as isize) as usize;
            let v = *src.get(i)?;
            i += 1;
            out.get_mut(o..o + k)?.fill(v);
            o += k;
        }
    }
    Some(())
}

/// One channel of a `w` × `h` layer, every sample as 0..65535 (an 8-bit sample is spread with × 257).
fn plane(data: &[u8], w: usize, h: usize, depth: u16) -> Result<Vec<u16>, String> {
    let bad = || "truncated PSD layer channel".to_string();
    let bpp = (depth / 8) as usize;
    let mut raw = vec![0u8; w * h * bpp];
    if w * h == 0 {
        return Ok(Vec::new());
    }
    match be16(data, 0).ok_or_else(bad)? {
        0 => raw.copy_from_slice(data.get(2..2 + w * h * bpp).ok_or_else(bad)?),
        1 => {
            let mut src = 2 + h * 2;
            for y in 0..h {
                let n = be16(data, 2 + y * 2).ok_or_else(bad)? as usize;
                unpack(data.get(src..src + n).ok_or_else(bad)?, &mut raw[y * w * bpp..(y + 1) * w * bpp]).ok_or_else(bad)?;
                src += n;
            }
        }
        _ => return Err("PSD layer compression is not supported yet".into()),
    }
    Ok(if bpp == 2 { raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect() } else { raw.iter().map(|v| *v as u16 * 257).collect() })
}

/// The file with its merged image replaced by the composition of the layers `options` shows;
/// `None` when that is the picture the file already holds (nothing overridden).
pub fn flatten(bytes: &[u8], options: &Options) -> Result<Option<Vec<u8>>, String> {
    let psd = parse(bytes)?;
    let nodes = tree(&psd)?;
    let mismatch = || "PSD layer source no longer matches placement".to_string();
    if options.layers.iter().any(|o| !nodes.iter().any(|n| psd.layers[n.rec()].id == o.id && psd.layers[n.rec()].name == o.name)) {
        return Err(mismatch());
    }
    // listed layers follow `options`; layers inside a group keep what the file saved
    let shown: Vec<bool> = psd.layers.iter().map(|l| options.layers.iter().find(|o| o.id == l.id).map_or(l.visible, |o| o.visible)).collect();
    if nodes.iter().all(|n| psd.layers[n.rec()].visible == shown[n.rec()]) {
        return Ok(None);
    }
    let canvas = compose::compose(&psd, &nodes, &shown)?;
    let (w, h, nc) = (canvas.w, canvas.h, canvas.nc);
    // a transparent composition keeps its alpha as the merged transparency (negative layer count)
    let transparent = canvas.alpha.iter().any(|a| *a != 255);
    let mut out = Vec::with_capacity(bytes.len().min(psd.head_end) + w * h * (nc + 1));
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&((nc + usize::from(transparent)) as u16).to_be_bytes());
    out.extend_from_slice(&(h as u32).to_be_bytes());
    out.extend_from_slice(&(w as u32).to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes());
    out.extend_from_slice(&psd.mode.to_be_bytes());
    out.extend_from_slice(&bytes[26..psd.resources.start - 4]); // colour mode data
    out.extend_from_slice(&bytes[psd.resources.start - 4..psd.resources.end]); // image resources, kept as saved
    if transparent {
        out.extend_from_slice(&10u32.to_be_bytes());
        out.extend_from_slice(&2u32.to_be_bytes());
        out.extend_from_slice(&(-1i16).to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
    } else {
        out.extend_from_slice(&0u32.to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes());
    for c in 0..nc {
        out.extend((0..w * h).map(|p| canvas.color[p * nc + c]));
    }
    if transparent {
        out.extend_from_slice(&canvas.alpha);
    }
    Ok(Some(out))
}

/// What was read from a file, kept for this run only (a file that changes is read again).
type Listed = std::collections::HashMap<std::path::PathBuf, ((u64, Option<std::time::SystemTime>), Option<Options>)>;
static LISTED: std::sync::Mutex<Option<Listed>> = std::sync::Mutex::new(None);

fn is_psd(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd"))
}

/// The layer list of the PSD at `path` (None: no layers, or not a readable PSD).
pub fn listed(path: &std::path::Path) -> Option<Options> {
    let meta = std::fs::metadata(path).ok()?;
    let stamp = (meta.len(), meta.modified().ok());
    if let Some(hit) = LISTED.lock().ok()?.get_or_insert_with(Default::default).get(path).filter(|(s, _)| *s == stamp) {
        return hit.1.clone();
    }
    let read = std::fs::read(path).ok().and_then(|b| read(&b).ok().flatten());
    LISTED.lock().ok()?.get_or_insert_with(Default::default).insert(path.to_path_buf(), (stamp, read.clone()));
    read
}

/// The composition of `options` as a PSD file in `cache`, named by the content hash of the
/// source and the layers shown. `Ok(None)`: nothing is overridden, draw the file itself.
pub fn cached(path: &std::path::Path, options: &Options, cache: &std::path::Path) -> Result<Option<std::path::PathBuf>, String> {
    use sha2::{Digest, Sha256};
    if !overrides(options) {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    hash.update(&bytes);
    hash.update(serde_json::to_vec(&options.layers.iter().map(|l| (l.id, &l.name, l.visible)).collect::<Vec<_>>()).map_err(|e| e.to_string())?);
    let digest = hash.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
    let output = cache.join(format!("psd-v1-{digest}.psd"));
    if output.is_file() {
        return Ok(Some(output));
    }
    let Some(flat) = flatten(&bytes, options)? else { return Ok(None) };
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let temporary = output.with_extension(format!("{}-tmp", std::process::id()));
    std::fs::write(&temporary, flat).map_err(|e| e.to_string())?;
    if output.is_file() {
        let _ = std::fs::remove_file(&temporary);
    } else if let Err(e) = std::fs::rename(&temporary, &output) {
        let _ = std::fs::remove_file(&temporary);
        return Err(e.to_string());
    }
    Ok(Some(output))
}

/// Gives every placed layered PSD its layer list (the saved options, else the file's) and,
/// where layers are switched, the composition as its picture (`preview`). A file that cannot
/// be composed keeps the picture it saved; a warning says why.
pub fn attach(doc: &mut folio_model::Document, cache: &std::path::Path, resolve: &dyn Fn(&folio_model::Graphic) -> Option<std::path::PathBuf>) {
    use folio_model::{ItemKind, PageItem};
    fn walk(items: &mut [PageItem], cache: &std::path::Path, resolve: &dyn Fn(&folio_model::Graphic) -> Option<std::path::PathBuf>, warnings: &mut Vec<String>) {
        for item in items {
            match &mut item.kind {
                ItemKind::Group { children } => walk(children, cache, resolve, warnings),
                ItemKind::Rectangle { content } | ItemKind::Oval { content } | ItemKind::Polygon { content } => {
                    if let Some(g) = content.as_mut().filter(|g| g.element == "Image")
                        && let Some(path) = resolve(g).filter(|p| is_psd(p))
                        && let Some(file) = listed(&path)
                    {
                        let comps = file.comps.clone();
                        let options = g.layer_options.get_or_insert(file);
                        if options.comps.is_empty() {
                            options.comps = comps;
                        }
                        let options = options.clone();
                        match cached(&path, &options, cache) {
                            Ok(Some(flat)) => g.preview = Some(flat.to_string_lossy().into_owned()),
                            Ok(None) => {}
                            Err(e) => warnings.push(format!("PSD layers: {e}")),
                        }
                    }
                }
                _ => {}
            }
            walk(&mut item.clipped, cache, resolve, warnings);
        }
    }
    let mut warnings = Vec::new();
    for spread in doc.spreads.iter_mut().chain(&mut doc.master_spreads) {
        walk(&mut spread.items, cache, resolve, &mut warnings);
    }
    warnings.sort();
    warnings.dedup();
    doc.warnings.extend(warnings);
}

#[cfg(test)]
mod tests;
