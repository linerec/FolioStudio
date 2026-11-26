//! A document's display conversions: profile resolution, the cache and the fallback.

use crate::cache::{self, ALGORITHM, Tables};
use crate::grid::{CMYK_N, Cmyk4, RGB_CMYK_N, RGB_N, Rgb3, RgbCmyk, RgbDisplay, Shaper3};
use crate::icc::{self, Bpc, Profile, Space, Srgb};
use crate::profiles::{ProfileIndex, read_profile};
use crate::Colors;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// The description of sRGB IEC 61966-2.1, the display space (no conversion needed).
pub const SRGB_NAME: &str = "sRGB IEC61966-2.1";

/// What became of one of the document's profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileStatus {
    /// Converted through the file in [`ProfileUse::path`].
    Used,
    /// sRGB, the display space itself.
    Display,
    /// The document names no profile.
    NotNamed,
    /// No profile with this description was found.
    Missing,
    /// Found but not usable (why).
    Unusable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileUse {
    pub name: Option<String>,
    pub path: Option<PathBuf>,
    pub status: ProfileStatus,
}

/// How the conversion was obtained.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorReport {
    /// Loaded from the on-disk cache.
    pub cached: bool,
    /// Time spent building tables (cache miss), ms.
    pub build_ms: Option<f64>,
    /// Time of the whole resolution (lookup, hashing, cache or build), ms.
    pub total_ms: f64,
    pub cache_file: Option<PathBuf>,
    /// Why the cache could not be written, if it could not.
    pub cache_error: Option<String>,
}

pub struct DocumentColors {
    pub colors: Colors,
    pub cmyk: ProfileUse,
    pub rgb: ProfileUse,
    /// Document summary warnings (missing or unusable profiles).
    pub warnings: Vec<String>,
    pub report: ColorReport,
}

/// Finds the profile `name` and reads it. `None` bytes when it cannot be used.
fn resolve(name: Option<&str>, space: Space, index: &ProfileIndex, dir: Option<&Path>) -> (ProfileUse, Option<Vec<u8>>) {
    let Some(name) = name else { return (ProfileUse { name: None, path: None, status: ProfileStatus::NotNamed }, None) };
    let mut path = index.find_for_document(name, dir);
    if path.is_none() {
        // Installed since the index was built?
        path = ProfileIndex::scan(index.roots()).find(name);
    }
    let named = |path: Option<PathBuf>, status| ProfileUse { name: Some(name.to_owned()), path, status };
    let Some(path) = path else { return (named(None, ProfileStatus::Missing), None) };
    match read_profile(&path) {
        Ok(bytes) => {
            let want: &[u8; 4] = match space {
                Space::Cmyk => b"CMYK",
                Space::Rgb => b"RGB ",
                Space::Gray => b"GRAY",
            };
            if &bytes[16..20] != want {
                let msg = format!("its colour space is {:?}", String::from_utf8_lossy(&bytes[16..20]).trim());
                return (named(Some(path), ProfileStatus::Unusable(msg)), None);
            }
            (named(Some(path), ProfileStatus::Used), Some(bytes))
        }
        Err(e) => (named(Some(path), ProfileStatus::Unusable(e.to_string())), None),
    }
}

fn warning(kind: &str, u: &ProfileUse, fallback: &str) -> Option<String> {
    let name = u.name.as_deref().unwrap_or_default();
    match &u.status {
        ProfileStatus::Used | ProfileStatus::Display => None,
        ProfileStatus::NotNamed => Some(format!("colour: the document names no {kind} profile; {fallback}")),
        ProfileStatus::Missing => Some(format!("colour: {kind} profile \"{name}\" is not installed; {fallback}")),
        ProfileStatus::Unusable(why) => Some(format!(
            "colour: {kind} profile \"{name}\" ({}) cannot be used: {why}; {fallback}",
            u.path.as_deref().map(|p| p.display().to_string()).unwrap_or_default()
        )),
    }
}

/// Resolves a document's CMYK and RGB profiles (by description, see [`ProfileIndex`]) and
/// returns its display conversions, from `cache_dir` when built before. `document_dir`
/// is searched after the installed folders.
pub fn document_colors(
    cmyk_name: Option<&str>,
    rgb_name: Option<&str>,
    index: &ProfileIndex,
    document_dir: Option<&Path>,
    cache_dir: Option<&Path>,
) -> DocumentColors {
    let started = Instant::now();
    let (mut cmyk, mut cmyk_bytes) = resolve(cmyk_name, Space::Cmyk, index, document_dir);
    let (mut rgb, mut rgb_bytes) = match rgb_name {
        Some(n) if n.trim() == SRGB_NAME => (ProfileUse { name: Some(n.to_owned()), path: None, status: ProfileStatus::Display }, None),
        n => resolve(n, Space::Rgb, index, document_dir),
    };
    let mut report = ColorReport::default();
    let colors = loop {
        if cmyk_bytes.is_none() && rgb_bytes.is_none() {
            break Colors::unmanaged();
        }
        let key = format!(
            "folio-color {ALGORITHM}|relative colorimetric + black point compensation|grids {CMYK_N} {RGB_CMYK_N} {RGB_N}|cmyk {}|rgb {}",
            cmyk_bytes.as_deref().map_or("none".into(), cache::content_id),
            rgb_bytes.as_deref().map_or("sRGB".into(), cache::content_id),
        );
        let id = cache::hash64(key.as_bytes()) | 1;
        if let Some((t, path)) = cache_dir.and_then(|d| cache::load(d, &key)) {
            report.cached = true;
            report.cache_file = Some(path);
            break Colors::from_tables(t, id);
        }
        let parse = |b: &Option<Vec<u8>>, space: Space| -> Option<Result<Profile, String>> {
            b.as_deref().map(|b| {
                Profile::parse(b)
                    .map_err(|e| e.to_string())
                    .and_then(|p| if p.space == space { Ok(p) } else { Err(format!("not a {space:?} profile")) })
            })
        };
        let pc = parse(&cmyk_bytes, Space::Cmyk);
        let pr = parse(&rgb_bytes, Space::Rgb);
        if let Some(Err(e)) = &pc {
            cmyk.status = ProfileStatus::Unusable(e.clone());
            cmyk_bytes = None;
            continue;
        }
        if let Some(Err(e)) = &pr {
            rgb.status = ProfileStatus::Unusable(e.clone());
            rgb_bytes = None;
            continue;
        }
        let t = Instant::now();
        let tables = build_tables(pc.and_then(Result::ok).as_ref(), pr.and_then(Result::ok).as_ref());
        report.build_ms = Some(t.elapsed().as_secs_f64() * 1000.0);
        if let Some(d) = cache_dir {
            match cache::store(d, &key, &tables) {
                Ok(p) => report.cache_file = Some(p),
                Err(e) => report.cache_error = Some(e.to_string()),
            }
        }
        break Colors::from_tables(tables, id);
    };
    let warnings = [
        warning("CMYK", &cmyk, "CMYK shown without colour management"),
        warning("RGB", &rgb, "RGB shown as sRGB"),
    ]
    .into_iter()
    .flatten()
    .collect();
    report.total_ms = started.elapsed().as_secs_f64() * 1000.0;
    DocumentColors { colors, cmyk, rgb, warnings, report }
}

/// Samples the conversions of a CMYK and an RGB profile (`None`: unmanaged CMYK, sRGB).
pub(crate) fn build_tables(cmyk: Option<&Profile>, rgb: Option<&Profile>) -> Tables {
    let srgb = Srgb::new();
    // Display (sRGB) black is zero: a source black point maps to it.
    let to_display = |p: &Profile| Bpc::new(icc::source_black_point(p), [0.0; 3]);
    let unit = |v: [u8; 3]| v.map(|c| c as f64 / 255.0);
    let mut t = Tables::default();
    let link = cmyk.map(|c| {
        let bpc = to_display(c);
        Cmyk4::sample_cmyk(&|ink| srgb.from_xyz(bpc.apply(c.to_xyz(&ink))))
    });
    if let Some(r) = rgb {
        let bpc = to_display(r);
        match r.matrix_shaper() {
            Some(icc::MatrixShaper { lin, matrix: m }) => {
                // linear RGB → XYZ → black point compensation → linear sRGB is affine
                let f = |l: [f64; 3]| srgb.linear_from_xyz(bpc.apply([0, 1, 2].map(|i| m[i][0] * l[0] + m[i][1] * l[1] + m[i][2] * l[2])));
                let o = f([0.0; 3]);
                let cols = [f([1.0, 0.0, 0.0]), f([0.0, 1.0, 0.0]), f([0.0, 0.0, 1.0])];
                let total = [0, 1, 2].map(|row| [0, 1, 2].map(|c| cols[c][row] - o[row]));
                t.rgb_shaper = Some(Shaper3::new(&lin, total, o).v);
            }
            None => t.rgb = Some(Rgb3::sample(&|v| srgb.from_xyz8(bpc.apply(r.to_xyz(&unit(v))))).rgb),
        }
    }
    if let Some(c) = cmyk.filter(|c| c.can_convert_from_pcs()) {
        let dst = icc::destination_black_point(c);
        // Content composited in the CMYK blend space becomes 8-bit CMYK pixels, as the
        // reference renderer composites a CMYK transparency group: source → CMYK through a
        // precalculated RGB → CMYK link, then shown through the CMYK link.
        let to_cmyk = |to_xyz: &(dyn Fn([f64; 3]) -> [f64; 3] + Sync), src_bp: [f64; 3]| {
            let to_ink = Bpc::new(src_bp, dst);
            RgbCmyk::sample_rgb(&|v| c.from_xyz(to_ink.apply(to_xyz(v))).unwrap_or([0.0; 4])).data
        };
        let srgb_link = || to_cmyk(&|v| srgb.to_xyz(v), [0.0; 3]);
        match rgb {
            Some(r) => {
                t.rgb_cmyk = Some(to_cmyk(&|v| r.to_xyz(&v), icc::source_black_point(r)));
                t.srgb_cmyk = Some(srgb_link());
            }
            None => t.rgb_cmyk = Some(srgb_link()),
        }
    }
    t.cmyk = link.map(|l| l.data);
    t
}

impl Colors {
    pub(crate) fn from_tables(t: Tables, key: u64) -> Colors {
        let mut c = Colors {
            cmyk: t.cmyk.and_then(Cmyk4::from_box).map(Arc::new),
            rgb: match (t.rgb_shaper.and_then(Shaper3::from_box), t.rgb.and_then(Rgb3::from_box)) {
                (Some(s), _) => Some(Arc::new(RgbDisplay::Shaper(s))),
                (None, Some(g)) => Some(Arc::new(RgbDisplay::Grid(g))),
                (None, None) => None,
            },
            rgb_cmyk: t.rgb_cmyk.and_then(RgbCmyk::from_box).map(Arc::new),
            rgb_cmyk_px: None,
            srgb_cmyk: t.srgb_cmyk.and_then(RgbCmyk::from_box).map(Arc::new),
            proof_paper: None,
            key,
        };
        // Every node of the pixel grid is an 8-bit input: it holds the exact result.
        if c.rgb_cmyk.is_some() && c.cmyk.is_some() {
            let exact = c.clone();
            c.rgb_cmyk_px = Some(Arc::new(Rgb3::sample(&|v| exact.rgb_through_cmyk(v))));
        }
        c
    }

    /// Builds the conversions of profile files directly (no index, no cache), e.g. for tests
    /// and tools. `None` for CMYK: unmanaged; `None` for RGB: sRGB.
    pub fn from_profiles(cmyk: Option<&[u8]>, rgb: Option<&[u8]>) -> Result<Colors, icc::IccError> {
        let c = cmyk.map(Profile::parse).transpose()?;
        let r = rgb.map(Profile::parse).transpose()?;
        // a picture's embedded CMYK or gray profile is not an RGB profile (placeimg-c1: it used to panic in the conversion)
        if c.as_ref().is_some_and(|p| p.space != icc::Space::Cmyk) || r.as_ref().is_some_and(|p| p.space != icc::Space::Rgb) {
            return Err(icc::IccError::Unsupported("the profile is not of the colour space it was given for".into()));
        }
        let key = cache::hash64(format!("{:?}|{:?}", cmyk.map(cache::content_id), rgb.map(cache::content_id)).as_bytes()) | 1;
        Ok(Colors::from_tables(build_tables(c.as_ref(), r.as_ref()), key))
    }
}
