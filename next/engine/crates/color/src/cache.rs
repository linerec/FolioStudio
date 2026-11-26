//! On-disk cache of sampled conversions, one file per conversion key.
//!
//! The key names everything the tables depend on: the content hash of each profile, the
//! intent, black point compensation, the grids and [`ALGORITHM`] (bump it whenever the
//! conversion changes). File layout: magic, key length (u32 LE), key (UTF-8), table mask
//! (u8), the present tables at their fixed sizes, then a 64-bit hash of everything before
//! it. A file whose key, size or hash does not match is ignored (and rewritten).
//! Files are written to a temporary name and renamed; only the newest [`KEEP`] are kept.

use crate::grid::{CMYK_LEN, RGB_CMYK_LEN, RGB_LEN, SHAPER_LEN};
use std::path::{Path, PathBuf};

/// Version of the conversion algorithm; part of every key.
pub const ALGORITHM: u32 = 1;
/// Cache files kept (most recently used).
pub const KEEP: usize = 16;
const MAGIC: &[u8; 8] = b"FOLIOCLR";
const EXT: &str = "fct";

/// The tables of one conversion (`None` = not needed). 16-bit tables are stored
/// little-endian.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tables {
    /// CMYK → display link, `CMYK_LEN` values.
    pub cmyk: Option<Box<[u16]>>,
    /// Document RGB → display as a grid, `RGB_LEN` bytes (table-based RGB profiles).
    pub rgb: Option<Box<[u8]>>,
    /// Document RGB → display through a matrix/TRC model, `SHAPER_LEN` f32 values.
    pub rgb_shaper: Option<Box<[f32]>>,
    /// Document RGB → CMYK link, `RGB_CMYK_LEN` values.
    pub rgb_cmyk: Option<Box<[u16]>>,
    /// sRGB → CMYK link when the document RGB is not sRGB, `RGB_CMYK_LEN` values.
    pub srgb_cmyk: Option<Box<[u16]>>,
}

fn u16s(raw: &[u8]) -> Box<[u16]> {
    raw.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect()
}

/// A stable 64-bit hash (not cryptographic) of `bytes`, eight bytes per step.
pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0x243F_6A88_85A3_08D3 ^ (bytes.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut chunks = bytes.chunks_exact(8);
    for c in &mut chunks {
        let w = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        h = (h ^ w.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)).rotate_left(31).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    let mut tail = [0u8; 8];
    tail[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
    h = (h ^ u64::from_le_bytes(tail).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)).rotate_left(31).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // final avalanche (SplitMix64)
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

/// Content identity of a profile file: two independent 64-bit hashes and the length.
pub fn content_id(bytes: &[u8]) -> String {
    // A second, differently mixed pass over the words in reverse order.
    let mut h: u64 = 0x1319_8A2E_0370_7344;
    for c in bytes.chunks(8).rev() {
        let mut w = [0u8; 8];
        w[..c.len()].copy_from_slice(c);
        h = h.wrapping_add(u64::from_le_bytes(w)).wrapping_mul(0xFF51_AFD7_ED55_8CCD).rotate_left(29);
    }
    format!("{:016x}{:016x}:{}", hash64(bytes), h ^ (h >> 33), bytes.len())
}

fn file_for(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{:016x}.{EXT}", hash64(key.as_bytes())))
}

/// The cached tables for `key`, if a valid file exists.
pub fn load(dir: &Path, key: &str) -> Option<(Tables, PathBuf)> {
    let path = file_for(dir, key);
    let bytes = std::fs::read(&path).ok()?;
    let body_len = bytes.len().checked_sub(8)?;
    let (body, sum) = bytes.split_at(body_len);
    if u64::from_le_bytes(sum.try_into().ok()?) != hash64(body) || !body.starts_with(MAGIC) {
        return None;
    }
    let mut at = MAGIC.len();
    let klen = u32::from_le_bytes(body.get(at..at + 4)?.try_into().ok()?) as usize;
    at += 4;
    if body.get(at..at + klen)? != key.as_bytes() {
        return None;
    }
    at += klen;
    let mask = *body.get(at)?;
    at += 1;
    let mut take = |bit: u8, bytes: usize| -> Option<Option<&[u8]>> {
        if mask >> bit & 1 == 0 {
            return Some(None);
        }
        let raw = body.get(at..at + bytes)?;
        at += bytes;
        Some(Some(raw))
    };
    let t = Tables {
        cmyk: take(0, 2 * CMYK_LEN)?.map(u16s),
        rgb: take(1, RGB_LEN)?.map(Into::into),
        rgb_cmyk: take(2, 2 * RGB_CMYK_LEN)?.map(u16s),
        srgb_cmyk: take(3, 2 * RGB_CMYK_LEN)?.map(u16s),
        rgb_shaper: take(4, 4 * SHAPER_LEN)?.map(|raw| raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()),
    };
    if at != body.len() {
        return None;
    }
    // touch: recently used files survive eviction
    if let Ok(f) = std::fs::File::options().append(true).open(&path) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
    Some((t, path))
}

/// Writes the tables for `key` (atomically: temporary file, then rename) and evicts old
/// files. Only files of this cache's own naming are touched.
pub fn store(dir: &Path, key: &str, t: &Tables) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut body = Vec::with_capacity(2 * CMYK_LEN + RGB_LEN + 4 * RGB_CMYK_LEN + key.len() + 32);
    body.extend_from_slice(MAGIC);
    body.extend_from_slice(&(key.len() as u32).to_le_bytes());
    body.extend_from_slice(key.as_bytes());
    let sizes = [
        t.cmyk.as_ref().map(|v| (v.len(), CMYK_LEN)),
        t.rgb.as_ref().map(|v| (v.len(), RGB_LEN)),
        t.rgb_cmyk.as_ref().map(|v| (v.len(), RGB_CMYK_LEN)),
        t.srgb_cmyk.as_ref().map(|v| (v.len(), RGB_CMYK_LEN)),
        t.rgb_shaper.as_ref().map(|v| (v.len(), SHAPER_LEN)),
    ];
    let mut mask = 0u8;
    for (bit, size) in sizes.iter().enumerate() {
        if let Some((have, want)) = size {
            if have != want {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "table of the wrong size"));
            }
            mask |= 1 << bit;
        }
    }
    body.push(mask);
    let put16 = |body: &mut Vec<u8>, v: &Option<Box<[u16]>>| {
        if let Some(v) = v {
            body.extend(v.iter().flat_map(|x| x.to_le_bytes()));
        }
    };
    put16(&mut body, &t.cmyk);
    if let Some(v) = &t.rgb {
        body.extend_from_slice(v);
    }
    put16(&mut body, &t.rgb_cmyk);
    put16(&mut body, &t.srgb_cmyk);
    if let Some(v) = &t.rgb_shaper {
        body.extend(v.iter().flat_map(|x| x.to_le_bytes()));
    }
    let sum = hash64(&body);
    body.extend_from_slice(&sum.to_le_bytes());
    let path = file_for(dir, key);
    let tmp = dir.join(format!("tmp-{}-{:016x}.{EXT}", std::process::id(), hash64(key.as_bytes())));
    std::fs::write(&tmp, &body)?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        // Another process may have written the same entry meanwhile.
        if !path.is_file() {
            return Err(e);
        }
    }
    evict(dir);
    Ok(path)
}

/// Keeps the newest `KEEP` cache files; removes older ones and stale temporary files.
fn evict(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files = Vec::new();
    let now = std::time::SystemTime::now();
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.ends_with(&format!(".{EXT}")) {
            continue;
        }
        let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(now);
        if name.starts_with("tmp-") {
            // a writer still running would have finished long ago
            if now.duration_since(modified).is_ok_and(|d| d.as_secs() > 600) {
                let _ = std::fs::remove_file(&p);
            }
            continue;
        }
        files.push((modified, p));
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_rejects_other_keys() {
        let dir = std::env::temp_dir().join(format!("folio-color-cache-test-{}", std::process::id()));
        let cmyk: Box<[u16]> = (0..CMYK_LEN).map(|i| (i * 7) as u16).collect();
        let link: Box<[u16]> = (0..RGB_CMYK_LEN).map(|i| (i * 3) as u16).collect();
        let shaper: Box<[f32]> = (0..SHAPER_LEN).map(|i| i as f32 * 0.5).collect();
        let t = Tables { cmyk: Some(cmyk), rgb: Some(vec![7u8; RGB_LEN].into()), rgb_cmyk: Some(link), srgb_cmyk: None, rgb_shaper: Some(shaper) };
        let path = store(&dir, "key A", &t).unwrap();
        assert_eq!(load(&dir, "key A").map(|x| x.0), Some(t.clone()));
        assert!(load(&dir, "key B").is_none());
        // A damaged file is ignored.
        let mut bytes = std::fs::read(&path).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        assert!(load(&dir, "key A").is_none());
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(content_id(b"abc"), content_id(b"abd"));
        assert_ne!(hash64(b""), hash64(&[0]));
    }
}
