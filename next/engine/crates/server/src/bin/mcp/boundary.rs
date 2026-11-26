use std::path::{Path, PathBuf};

pub struct Boundary {
    root: PathBuf,
}

/// Folio's own folders of a project (`.folio/…`): a tool call of the project server reads and writes there without naming them
/// (the hash index and caches, import staging, locks). With the project's assets and fonts folders (its settings) they are
/// checked like a requested path before every call (`own_folders`).
const OWN_FOLDERS: &[&str] = &[".folio", ".folio/cache", ".folio/tmp", ".folio/locks"];

impl Boundary {
    /// The boundary of the project server (`folio-mcp PROJECT`, mcp-c6): `components`, and Folio's own folders (`own_folders`).
    /// No walk over the whole tree any more (mcp-c5 did the same for an attached connection): a link elsewhere in the tree is
    /// not reached, a path through one is refused.
    pub fn new(root: &Path) -> Result<Self, String> {
        let result = Self::components(root)?;
        result.own_folders()?;
        Ok(result)
    }

    /// The boundary of a connection attached to the running app (the engine's `attach_paths.rs`): no walk over the whole tree
    /// (a large NAS project made every path request slow), only the names of each requested path, root to target (`path`):
    /// a link, junction or other reparse point on the way is refused, and every existing step's real path must be below the root.
    /// `path` again after the command used the file catches a folder swapped for a link in between. The project's own walks
    /// (hash index, documents, tree) never follow links, so a link elsewhere in the tree is not reached through a path tool.
    pub fn components(root: &Path) -> Result<Self, String> {
        let result = Self::folder(root)?;
        result.existing("folio.project")?;
        Ok(result)
    }

    /// `components` for a folder that need not be a project (the folder a person chose for new projects, mcp-c6).
    #[allow(dead_code)] // the engine's attach_paths.rs; folio-mcp itself only opens projects
    pub fn folder(root: &Path) -> Result<Self, String> {
        let meta = std::fs::symlink_metadata(root).map_err(|e| e.to_string())?;
        if linked(&meta) { return Err("the project folder itself is a link".into()); }
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("project folder required".into());
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path { &self.root }

    /// The folders the project server uses without a tool naming them: Folio's own (`OWN_FOLDERS`: hash index, caches, import
    /// staging, locks) and the project's assets and fonts folders (its settings; a setting that leaves the project is refused by
    /// `manifest::load`). Each is checked like a requested path (no link on the way, below the root); a missing one is fine.
    /// An import's folders below the assets folder are checked when it is planned (`import.rs`).
    pub fn own_folders(&self) -> Result<(), String> {
        let project = folio_project::manifest::load(&self.root).map_err(|e| e.to_string())?;
        let settings = &project.file.settings;
        for folder in OWN_FOLDERS.iter().copied().chain([settings.assets_folder.as_str(), settings.fonts_folder.as_str()]) {
            self.path(folder).map_err(|e| format!("{folder}: {e}"))?;
        }
        Ok(())
    }

    pub fn path(&self, raw: &str) -> Result<PathBuf, String> {
        let rel = folio_project::RelPath::parse(raw).map_err(|e| e.to_string())?;
        let mut path = self.root.clone();
        for part in rel.as_str().split('/') {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(meta) => {
                    if linked(&meta) { return Err("project links are not allowed".into()); }
                    if !path.canonicalize().map_err(|e| e.to_string())?.starts_with(&self.root) {
                        return Err("outside project".into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(path)
    }

    pub fn existing(&self, raw: &str) -> Result<PathBuf, String> {
        let path = self.path(raw)?;
        if !path.is_file() { return Err("project file not found".into()); }
        Ok(path)
    }

    pub fn output(&self, raw: &str, extension: &str) -> Result<PathBuf, String> {
        let path = self.path(raw)?;
        if path.extension().and_then(|s| s.to_str()) != Some(extension) {
            return Err(format!("output must have .{extension} extension"));
        }
        if path.exists() { return Err("overwriting existing files is not allowed".into()); }
        if !path.parent().is_some_and(Path::is_dir) { return Err("output folder must exist".into()); }
        Ok(path)
    }
}

fn linked(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 { return true; }
    }
    meta.file_type().is_symlink()
}
