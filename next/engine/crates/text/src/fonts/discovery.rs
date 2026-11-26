//! Find installed and document fonts and add their faces to the name index.

use super::*;

impl FontStore {
    /// The system and user font folders: `%LOCALAPPDATA%\Microsoft\Windows\Fonts`,
    /// `%APPDATA%\Adobe\CoreSync\plugins\livetype\{r,w,t,u,x}` (Adobe Fonts: OpenType
    /// files without extension; `e` is encrypted and skipped) and `%WINDIR%\Fonts`, plus
    /// the macOS folders when present.
    pub fn system_dirs() -> Vec<(PathBuf, FontSource)> {
        let mut dirs = Vec::new();
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
        if let Some(local) = env("LOCALAPPDATA") {
            dirs.push((local.join("Microsoft").join("Windows").join("Fonts"), FontSource::User));
        }
        if let Some(roaming) = env("APPDATA") {
            let livetype = roaming.join("Adobe").join("CoreSync").join("plugins").join("livetype");
            for sub in ["r", "w", "t", "u", "x"] {
                dirs.push((livetype.join(sub), FontSource::Adobe));
            }
        }
        if let Some(windir) = env("WINDIR") {
            dirs.push((windir.join("Fonts"), FontSource::System));
        }
        if let Some(home) = env("HOME") {
            dirs.push((home.join("Library/Fonts"), FontSource::User));
        }
        dirs.push((PathBuf::from("/Library/Fonts"), FontSource::System));
        dirs.push((PathBuf::from("/System/Library/Fonts"), FontSource::System));
        // Supplemental is traversed by collect_files (depth 3), including .ttc/.otc
        // collections and .otf. Do not scan it twice: duplicate faces alter fallback order.
        #[cfg(target_os = "linux")]
        {
            if let Some(home) = env("HOME") {
                dirs.push((home.join(".local/share/fonts"), FontSource::User));
                dirs.push((home.join(".fonts"), FontSource::User));
            }
            dirs.push((PathBuf::from("/usr/local/share/fonts"), FontSource::System));
            dirs.push((PathBuf::from("/usr/share/fonts"), FontSource::System));
        }
        dirs
    }

    /// Scans the system folders (`system_dirs`) and `extra_dirs` (e.g. a "Document fonts"
    /// folder, searched first).
    pub fn discover(extra_dirs: &[PathBuf]) -> FontStore {
        let mut dirs: Vec<(PathBuf, FontSource)> = extra_dirs.iter().map(|d| (d.clone(), FontSource::Extra)).collect();
        dirs.extend(FontStore::system_dirs());
        FontStore::from_dirs(&dirs)
    }

    /// Scans exactly these folders (recursively, three levels deep).
    pub fn from_dirs(dirs: &[(PathBuf, FontSource)]) -> FontStore {
        let mut store = FontStore::empty();
        store.scan(dirs);
        store
    }

    /// Scans further folders (e.g. a document's "Document fonts"), skipping folders already
    /// scanned. Their fonts rank before system fonts. Resolutions made afterwards see the
    /// new fonts; substitutes chosen before are reconsidered, exact matches kept.
    pub fn add_dirs(&mut self, dirs: &[PathBuf]) {
        let dirs: Vec<(PathBuf, FontSource)> = dirs.iter().map(|d| (d.clone(), FontSource::Extra)).collect();
        self.scan(&dirs);
        if let Ok(state) = self.state.get_mut() {
            state.resolved.retain(|_, r| r.exact);
            state.substitutes.clear();
            state.char_fallback.clear();
        }
    }

    fn scan(&mut self, dirs: &[(PathBuf, FontSource)]) {
        let started = std::time::Instant::now();
        let mut files: Vec<(PathBuf, FontSource)> = Vec::new();
        for (dir, source) in dirs {
            let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
            if self.scanned.contains(&canonical) {
                continue;
            }
            self.scanned.push(canonical);
            collect_files(dir, *source, 3, &mut files);
        }
        let known: std::collections::HashSet<PathBuf> = self.faces.iter().map(|f| (*f.path).clone()).collect();
        let mut seen = std::collections::HashSet::new();
        files.retain(|(p, _)| !known.contains(p) && seen.insert(p.clone()));

        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
        let chunk = files.len().div_ceil(threads).max(1);
        let mut found: Vec<FaceInfo> = std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        let mut out = Vec::new();
                        for (path, source) in part {
                            out.extend(read_faces(path, *source));
                        }
                        out
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        // Existing entries keep their index (loaded faces refer to them); new ones are
        // appended in a deterministic order, and lookups rank by source.
        found.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.path.cmp(&b.path)).then(a.index.cmp(&b.index)));
        let base = self.faces.len();
        self.faces.extend(found);
        for i in base..self.faces.len() {
            let f = &self.faces[i];
            self.by_postscript.entry(f.postscript.to_ascii_lowercase()).or_default().push(i);
            for fam in &f.families {
                let list = self.by_family.entry(fam.clone()).or_default();
                if list.last() != Some(&i) {
                    list.push(i);
                }
            }
            for full in &f.fulls {
                let list = self.by_full.entry(full.clone()).or_default();
                if list.last() != Some(&i) {
                    list.push(i);
                }
            }
        }
        self.scan_ms += started.elapsed().as_millis() as u64;
    }
}

fn collect_files(dir: &Path, source: FontSource, depth: u32, out: &mut Vec<(PathBuf, FontSource)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_dir() {
            if depth > 0 {
                collect_files(&path, source, depth - 1, out);
            }
            continue;
        }
        let ext = path.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("ttf" | "otf" | "ttc" | "otc" | "tte") => out.push((path, source)),
            // Adobe Fonts are stored without extension; the magic number decides.
            None => out.push((path, source)),
            _ => {}
        }
    }
}
