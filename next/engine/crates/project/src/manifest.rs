//! `folio.project`: the file that makes a folder a Folio project.
//!
//! ```json
//! {
//!   "format": "folio.project",
//!   "version": 1,
//!   "id": "3f1c…-…",                 // stable project UUID (machine settings are keyed by it)
//!   "name": "월간 부동산뱅크",
//!   "created": "2026-10-19T03:12:45Z",
//!   "settings": { "linkPolicy": "copy", "assetsFolder": "assets", "fontsFolder": "fonts" },
//!   "locations": [ { "name": "사진창고", "description": "공용 사진 NAS" } ]
//! }
//! ```
//!
//! Only portable facts live here. The real folders of named locations, window state, recent
//! lists and user names are machine settings of the app, keyed by `id`. Fields this version
//! does not know are kept and written back unchanged.

use crate::atomic;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "folio.project";
pub const FORMAT: &str = "folio.project";
pub const VERSION: u32 = 1;
/// The project's own data folder (cache, locks, staging); never holds user content.
pub const DATA_DIR: &str = ".folio";
pub const CACHE_DIR: &str = ".folio/cache";
pub const LOCKS_DIR: &str = ".folio/locks";
pub const TMP_DIR: &str = ".folio/tmp";

/// What happens to a file outside the project when a document is imported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkPolicy {
    /// Copied into the project (the default; content hashes decide duplicates).
    #[default]
    Copy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub link_policy: LinkPolicy,
    /// Folder for imported assets: `<assetsFolder>/<document name>/…`.
    #[serde(default = "default_assets", deserialize_with = "folder_reference")]
    pub assets_folder: String,
    /// Folder for font files the licences allow to copy.
    #[serde(default = "default_fonts", deserialize_with = "folder_reference")]
    pub fonts_folder: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn folder_reference<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw = String::deserialize(d)?;
    crate::RelPath::parse(&raw).map(|r| r.to_string()).map_err(serde::de::Error::custom)
}

fn default_assets() -> String {
    "assets".into()
}
fn default_fonts() -> String {
    "fonts".into()
}

impl Default for Settings {
    fn default() -> Self {
        Settings { link_policy: LinkPolicy::Copy, assets_folder: default_assets(), fonts_folder: default_fonts(), extra: Map::new() }
    }
}

/// A named location (`@사진창고/…`): the name and what it is. Its real folder on each
/// machine is an app setting, never stored in the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedLocation {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub created: String,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub locations: Vec<NamedLocation>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Invalid(String),
    /// The folder is already a project, or inside one.
    AlreadyProject(PathBuf),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Invalid(m) => write!(f, "folio.project: {m}"),
            Error::AlreadyProject(p) => write!(f, "이미 프로젝트 폴더입니다: {}", p.display()),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

/// An opened project: its root folder (as this machine reaches it) and `folio.project`.
#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub file: ProjectFile,
}

impl Project {
    pub fn id(&self) -> &str {
        &self.file.id
    }

    pub fn path(&self, rel: &crate::RelPath) -> PathBuf {
        rel.on_disk(&self.root)
    }

    pub fn data_dir(&self, sub: &str) -> PathBuf {
        let mut p = self.root.clone();
        for n in sub.split('/') {
            p.push(n);
        }
        p
    }
}

/// The nearest folder at or above `start` (a file or folder) holding `folio.project`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let abs = std::path::absolute(start).ok()?;
    let abs = folio_links::uri::resolve_native(&abs).unwrap_or(abs);
    abs.ancestors().find(|a| a.join(PROJECT_FILE).is_file()).map(Path::to_path_buf)
}

/// Reads `folio.project` in `root`.
pub fn load(root: &Path) -> Result<Project, Error> {
    let actual = folio_links::uri::resolve_native(root).unwrap_or_else(|| root.to_path_buf());
    let root = actual.as_path();
    let text = std::fs::read_to_string(root.join(PROJECT_FILE))?;
    let file: ProjectFile = serde_json::from_str(text.trim_start_matches('\u{FEFF}')).map_err(|e| Error::Invalid(e.to_string()))?;
    if file.format != FORMAT {
        return Err(Error::Invalid(format!("format is {:?}, not {FORMAT}", file.format)));
    }
    if file.id.trim().is_empty() {
        return Err(Error::Invalid("no project id".into()));
    }
    let root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    Ok(Project { root, file })
}

/// Opens the project that holds `path` (a document, a folder inside the project, or the
/// root itself).
pub fn open(path: &Path) -> Result<Project, Error> {
    let root = find_root(path).ok_or_else(|| Error::Invalid(format!("{} 위에 folio.project가 없습니다", path.display())))?;
    load(&root)
}

/// Makes `dir` a project (creating the folder when needed). Refuses a folder that already is,
/// or is inside, a project.
pub fn create(dir: &Path, name: &str) -> Result<Project, Error> {
    if let Some(existing) = find_root(dir) {
        return Err(Error::AlreadyProject(existing));
    }
    std::fs::create_dir_all(dir)?;
    let name = if name.trim().is_empty() { dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Folio 프로젝트".into()) } else { name.trim().to_string() };
    let file = ProjectFile {
        format: FORMAT.into(),
        version: VERSION,
        id: crate::util::uuid_v4(),
        name,
        created: crate::util::now_rfc3339(),
        settings: Settings::default(),
        locations: Vec::new(),
        extra: Map::new(),
    };
    let project = Project { root: std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf()), file };
    save(&project)?;
    Ok(project)
}

/// Writes `folio.project` atomically (pretty JSON, UTF-8, LF).
pub fn save(project: &Project) -> Result<(), Error> {
    let mut text = serde_json::to_string_pretty(&project.file).map_err(|e| Error::Invalid(e.to_string()))?;
    text.push('\n');
    atomic::write_bytes(&project.root.join(PROJECT_FILE), text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_find_load_round_trip() {
        let base = std::env::temp_dir().join(format!("folio-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let p = create(&base.join("월간지"), "").unwrap();
        assert_eq!(p.file.name, "월간지");
        assert_eq!(p.file.settings.link_policy, LinkPolicy::Copy);
        let deep = p.root.join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(find_root(&deep.join("x.folio")).unwrap(), p.root);
        let again = open(&deep).unwrap();
        assert_eq!(again.file, p.file);
        assert!(matches!(create(&deep, "x"), Err(Error::AlreadyProject(_))));
        // unknown fields of a newer version survive a save
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(p.root.join(PROJECT_FILE)).unwrap()).unwrap();
        v["future"] = serde_json::json!({"a": 1});
        v["settings"]["futureSetting"] = serde_json::json!(true);
        v["locations"] = serde_json::json!([{"name": "사진창고", "description": "NAS", "color": "red"}]);
        std::fs::write(p.root.join(PROJECT_FILE), serde_json::to_string(&v).unwrap()).unwrap();
        let loaded = load(&p.root).unwrap();
        save(&loaded).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(p.root.join(PROJECT_FILE)).unwrap()).unwrap();
        assert_eq!(back["future"], serde_json::json!({"a": 1}));
        assert_eq!(back["settings"]["futureSetting"], serde_json::json!(true));
        assert_eq!(back["locations"][0]["color"], "red");
        // no absolute path, user or machine name in the file
        let text = std::fs::read_to_string(p.root.join(PROJECT_FILE)).unwrap();
        assert!(!text.contains(&*base.to_string_lossy()));
        if let Ok(user) = std::env::var("USERNAME").or_else(|_| std::env::var("USER")) {
            assert!(!text.contains(&format!("\"{user}\"")));
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
