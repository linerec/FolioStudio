//! Read face metadata, variation instances and names from font files.

use super::*;

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A font file opened for scanning. The first 4 KiB are read once; the header and table
/// directory of nearly every face fit in it.
struct Scan {
    file: File,
    head: Vec<u8>,
}

impl Scan {
    fn open(path: &Path) -> Option<Scan> {
        let mut file = File::open(path).ok()?;
        let mut head = vec![0u8; 4096];
        let mut n = 0;
        while n < head.len() {
            match file.read(&mut head[n..]) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(_) => return None,
            }
        }
        head.truncate(n);
        Some(Scan { file, head })
    }

    fn read(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let end = offset.checked_add(len as u64)?;
        if end <= self.head.len() as u64 {
            return Some(self.head[offset as usize..end as usize].to_vec());
        }
        if len > 64 * 1024 * 1024 {
            return None;
        }
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; len];
        self.file.read_exact(&mut buf).ok()?;
        Some(buf)
    }
}

fn is_sfnt(tag: &[u8]) -> bool {
    matches!(tag, [0, 1, 0, 0] | b"OTTO" | b"true")
}

pub(super) fn read_faces(path: &Path, source: FontSource) -> Vec<FaceInfo> {
    let Some(mut scan) = Scan::open(path) else { return Vec::new() };
    if scan.head.len() < 12 {
        return Vec::new();
    }
    let path = Arc::new(path.to_path_buf());
    if &scan.head[0..4] == b"ttcf" {
        let n = be32(&scan.head, 8).unwrap_or(0).min(256) as usize;
        let Some(offsets) = scan.read(12, n * 4) else { return Vec::new() };
        let mut out = Vec::new();
        for i in 0..n {
            if let Some(off) = be32(&offsets, i * 4) {
                out.extend(read_face(&mut scan, u64::from(off), i as u32, &path, source));
            }
        }
        out
    } else if is_sfnt(&scan.head[0..4]) {
        read_face(&mut scan, 0, 0, &path, source)
    } else {
        Vec::new()
    }
}

/// One `fvar` named instance.
struct Instance {
    subfamily_id: u16,
    postscript_id: Option<u16>,
    coords: Vec<([u8; 4], f32)>,
}

fn parse_fvar(d: &[u8]) -> Vec<Instance> {
    let parse = || -> Option<Vec<Instance>> {
        let axes_off = be16(d, 4)? as usize;
        let (axis_count, axis_size) = (be16(d, 8)? as usize, be16(d, 10)? as usize);
        let (count, size) = (be16(d, 12)? as usize, be16(d, 14)? as usize);
        let tags: Vec<[u8; 4]> = (0..axis_count)
            .map(|a| d.get(axes_off + a * axis_size..axes_off + a * axis_size + 4)?.try_into().ok())
            .collect::<Option<_>>()?;
        let base = axes_off + axis_count * axis_size;
        let mut out = Vec::new();
        for i in 0..count {
            let r = base + i * size;
            let subfamily_id = be16(d, r)?;
            let coords = (0..axis_count)
                .map(|a| Some((tags[a], be32(d, r + 4 + a * 4)? as i32 as f32 / 65536.0)))
                .collect::<Option<Vec<_>>>()?;
            let postscript_id = if size >= 4 + 4 * axis_count + 2 {
                be16(d, r + 4 + 4 * axis_count).filter(|&v| v != 0xFFFF)
            } else {
                None
            };
            out.push(Instance { subfamily_id, postscript_id, coords });
        }
        Some(out)
    };
    parse().unwrap_or_default()
}

fn read_face(scan: &mut Scan, offset: u64, index: u32, path: &Arc<PathBuf>, source: FontSource) -> Vec<FaceInfo> {
    read_face_inner(scan, offset, index, path, source).unwrap_or_default()
}

fn read_face_inner(
    scan: &mut Scan,
    offset: u64,
    index: u32,
    path: &Arc<PathBuf>,
    source: FontSource,
) -> Option<Vec<FaceInfo>> {
    let head = scan.read(offset, 12)?;
    if !is_sfnt(&head[0..4]) {
        return None;
    }
    let num_tables = be16(&head, 4)? as usize;
    let dir = scan.read(offset + 12, num_tables * 16)?;
    let (mut name, mut os2, mut fvar, mut cff) = (None, None, None, false);
    for i in 0..num_tables {
        let r = i * 16;
        let tag = dir.get(r..r + 4)?;
        // Table offsets are from the start of the file, also inside collections.
        let range = (u64::from(be32(&dir, r + 8)?), be32(&dir, r + 12)? as usize);
        match tag {
            b"name" => name = Some(range),
            b"OS/2" => os2 = Some(range),
            b"fvar" => fvar = Some(range),
            b"CFF " | b"CFF2" => cff = true,
            _ => {}
        }
    }
    let (noff, nlen) = name?;
    let name_data = scan.read(noff, nlen)?;
    let (mut weight, mut italic) = (400u16, false);
    if let Some((ooff, olen)) = os2
        && let Some(o) = scan.read(ooff, olen.min(96))
    {
        weight = be16(&o, 4).unwrap_or(400);
        italic = be16(&o, 62).is_some_and(|s| s & 1 != 0);
    }
    let instances = fvar.and_then(|(o, l)| scan.read(o, l.min(1 << 20))).map_or_else(Vec::new, |d| parse_fvar(&d));
    let mut wanted = vec![1u16, 2, 4, 5, 6, 16, 17, 25];
    for inst in &instances {
        wanted.push(inst.subfamily_id);
        wanted.extend(inst.postscript_id);
    }
    let names = parse_names(&name_data, &wanted)?;
    let family = names.pick(16).or_else(|| names.pick(1))?;
    let style = names.pick(17).or_else(|| names.pick(2)).unwrap_or_else(|| "Regular".into());
    let postscript = names.pick(6).unwrap_or_default();
    let full_name = names.pick(4).unwrap_or_else(|| format!("{family} {style}"));
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut families: Vec<String> = Vec::new();
    let add_pair = |pairs: &mut Vec<(String, String)>, f: &str, s: &str| {
        let p = (norm(f), norm(s));
        if !pairs.contains(&p) {
            pairs.push(p);
        }
    };
    for (fid, sid) in [(16u16, 17u16), (1, 2)] {
        for (lang, fam) in names.all(fid) {
            let nf = norm(&fam);
            if !families.contains(&nf) {
                families.push(nf);
            }
            if let Some(s) = names.in_lang(sid, lang).or_else(|| names.pick(sid)) {
                add_pair(&mut pairs, &fam, &s);
            }
        }
    }
    // A typographic family without its own subfamily uses ID 2.
    if names.all(17).is_empty()
        && let Some(s) = names.pick(2)
    {
        for (_, fam) in names.all(16) {
            add_pair(&mut pairs, &fam, &s);
        }
    }
    let mut fulls: Vec<String> = names.all(4).into_iter().map(|(_, s)| norm(&s)).collect();
    fulls.sort();
    fulls.dedup();
    if !instances.is_empty() {
        // A variable font's IDs 1/2/4 name its default coordinates loosely (NotoSansKR-VF
        // says "Regular" for its Thin default); its named instances answer instead.
        pairs.clear();
        fulls.clear();
    }
    let base = FaceInfo {
        path: path.clone(),
        index,
        source,
        postscript,
        family: family.clone(),
        style: style.clone(),
        full_name,
        version: names.pick(5).unwrap_or_default(),
        weight,
        italic: italic || style_weight(&style).1,
        cff,
        coords: Vec::new(),
        pairs,
        families: families.clone(),
        fulls,
    };
    let mut out = vec![base];
    // Variable fonts: every named instance is a face of its own.
    let prefix = names.pick(25).unwrap_or_else(|| family.chars().filter(char::is_ascii_alphanumeric).collect());
    for inst in instances {
        let Some(sub) = names.pick(inst.subfamily_id) else { continue };
        let ps = inst.postscript_id.and_then(|id| names.pick(id)).unwrap_or_else(|| {
            format!("{prefix}-{}", sub.chars().filter(char::is_ascii_alphanumeric).collect::<String>())
        });
        let wght = inst.coords.iter().find(|c| &c.0 == b"wght").map(|c| c.1);
        let slanted = inst.coords.iter().any(|c| (&c.0 == b"ital" && c.1 > 0.5) || (&c.0 == b"slnt" && c.1 != 0.0));
        let pairs = families.iter().map(|f| (f.clone(), norm(&sub))).collect();
        out.push(FaceInfo {
            path: path.clone(),
            index,
            source,
            postscript: ps,
            family: family.clone(),
            style: sub.clone(),
            full_name: format!("{family} {sub}"),
            version: names.pick(5).unwrap_or_default(),
            weight: wght.map_or_else(|| style_weight(&sub).0, |w| w.round().clamp(1.0, 1000.0) as u16),
            italic: slanted || style_weight(&sub).1,
            cff,
            coords: inst.coords,
            pairs,
            families: families.clone(),
            fulls: vec![norm(&format!("{family}{sub}"))],
        });
    }
    Some(out)
}

/// Name records grouped by ID: (language key, text). Language key: Windows language id,
/// or 0x10000 + Mac language id.
pub(super) struct Names {
    records: Vec<(u16, u32, String)>,
}

impl Names {
    fn all(&self, id: u16) -> Vec<(u32, String)> {
        self.records.iter().filter(|r| r.0 == id).map(|r| (r.1, r.2.clone())).collect()
    }
    pub(super) fn in_lang(&self, id: u16, lang: u32) -> Option<String> {
        self.records.iter().find(|r| r.0 == id && r.1 == lang).map(|r| r.2.clone())
    }
    /// English (Windows en-US, then Mac English, then any) value of a name ID.
    fn pick(&self, id: u16) -> Option<String> {
        self.in_lang(id, 0x409)
            .or_else(|| self.in_lang(id, 0x10000))
            .or_else(|| {
                self.records.iter().find(|r| r.0 == id && r.1 < 0x10000 && (r.1 & 0x3FF) == 0x09).map(|r| r.2.clone())
            })
            .or_else(|| self.records.iter().find(|r| r.0 == id).map(|r| r.2.clone()))
    }
}

pub(super) fn parse_names(data: &[u8], wanted: &[u16]) -> Option<Names> {
    let table = ttf_parser::name::Table::parse(data)?;
    let mut records = Vec::new();
    for n in table.names {
        if !wanted.contains(&n.name_id) {
            continue;
        }
        let (lang, text) = match n.platform_id {
            ttf_parser::PlatformId::Windows | ttf_parser::PlatformId::Unicode => {
                let Some(t) = n.to_string() else { continue };
                let lang =
                    if n.platform_id == ttf_parser::PlatformId::Unicode { 0x409 } else { u32::from(n.language_id) };
                (lang, t)
            }
            ttf_parser::PlatformId::Macintosh if n.encoding_id == 0 => {
                (0x10000 + u32::from(n.language_id), n.name.iter().map(|&b| mac_roman(b)).collect())
            }
            _ => continue,
        };
        let text = text.trim_matches(char::from(0)).trim().to_string();
        if text.is_empty() {
            continue;
        }
        if !records.iter().any(|r: &(u16, u32, String)| r.0 == n.name_id && r.1 == lang) {
            records.push((n.name_id, lang, text));
        }
    }
    Some(Names { records })
}

pub(super) fn mac_roman(b: u8) -> char {
    const HIGH: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»… ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{F8FF}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";
    if b < 0x80 { char::from(b) } else { HIGH.chars().nth(usize::from(b - 0x80)).unwrap_or('?') }
}
