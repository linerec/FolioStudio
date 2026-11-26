//! Atomic saves: write a temporary file in the same folder, flush it to disk, then rename it
//! over the target. A failure at any point leaves the previous file as it was and removes
//! the temporary file; readers (another user on the NAS, a sync client) never see a half
//! written file under the real name.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Prefix of temporary files (also how leftovers of a crash are recognised).
pub const TEMP_PREFIX: &str = ".folio-save-";

/// Where a save failed, for tests that inject failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    /// The writer fails after this many bytes.
    WriteAfter(u64),
    /// Everything is written; the final rename fails.
    Rename,
}

struct Faulty<W> {
    inner: W,
    written: u64,
    limit: Option<u64>,
}

impl<W: Write> Write for Faulty<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(limit) = self.limit
            && self.written + buf.len() as u64 > limit
        {
            return Err(std::io::Error::other("injected write failure"));
        }
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn temp_path(target: &Path) -> std::io::Result<PathBuf> {
    let dir = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = target.file_name().ok_or_else(|| std::io::Error::other("no file name"))?.to_string_lossy();
    Ok(dir.join(format!("{TEMP_PREFIX}{}-{}-{name}", std::process::id(), crate::util::short_id())))
}

/// Writes `target` atomically with `fill`, which receives a buffered writer.
pub fn write_with(target: &Path, fill: &mut dyn FnMut(&mut dyn Write) -> std::io::Result<()>) -> std::io::Result<()> {
    write_with_fault(target, fill, Fault::None)
}

/// [`write_with`] with an injected failure (tests).
pub fn write_with_fault(target: &Path, fill: &mut dyn FnMut(&mut dyn Write) -> std::io::Result<()>, fault: Fault) -> std::io::Result<()> {
    let tmp = temp_path(target)?;
    let result = (|| -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let limit = match fault {
            Fault::WriteAfter(n) => Some(n),
            _ => None,
        };
        let mut w = Faulty { inner: BufWriter::with_capacity(1 << 20, file), written: 0, limit };
        fill(&mut w)?;
        w.flush()?;
        let file = w.inner.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        drop(file);
        if fault == Fault::Rename {
            return Err(std::io::Error::other("injected rename failure"));
        }
        std::fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes `target` atomically through the temporary file itself (seekable, for ZIP writers).
pub fn write_file_with(target: &Path, fill: &mut dyn FnMut(&mut std::fs::File) -> std::io::Result<()>) -> std::io::Result<()> {
    let tmp = temp_path(target)?;
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&tmp)?;
        fill(&mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes `bytes` to `target` atomically.
pub fn write_bytes(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_with(target, &mut |w| w.write_all(bytes))
}

/// Leftover temporary files of interrupted saves in `dir` (they are never the real file).
pub fn leftovers(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().starts_with(TEMP_PREFIX)).map(|e| e.path()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_failures_keep_the_old_file() {
        let dir = std::env::temp_dir().join(format!("folio-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("문서.folio");
        write_bytes(&target, b"version 1").unwrap();
        let big = vec![7u8; 3 << 20];
        for fault in [Fault::WriteAfter(0), Fault::WriteAfter(1 << 20), Fault::Rename] {
            let e = write_with_fault(&target, &mut |w| w.write_all(&big), fault).unwrap_err();
            assert!(e.to_string().contains("injected"), "{e}");
            assert_eq!(std::fs::read(&target).unwrap(), b"version 1", "{fault:?}");
            assert!(leftovers(&dir).is_empty(), "{fault:?}: {:?}", leftovers(&dir));
        }
        // an error from the caller's own writer behaves the same
        let e = write_with(&target, &mut |w| {
            w.write_all(b"partial")?;
            Err(std::io::Error::other("caller failed"))
        })
        .unwrap_err();
        assert_eq!(e.to_string(), "caller failed");
        assert_eq!(std::fs::read(&target).unwrap(), b"version 1");
        write_with(&target, &mut |w| w.write_all(&big)).unwrap();
        assert_eq!(std::fs::read(&target).unwrap().len(), big.len());
        assert!(leftovers(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
