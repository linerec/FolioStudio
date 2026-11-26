//! File system work of a package: copies (the OS copy call, or a large-buffer stream copy
//! that hashes on the way), the final rename that never replaces an existing folder, and
//! free-space checks.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Buffer of one stream-copy worker.
pub const STREAM_BUFFER: usize = 4 << 20;

pub fn cancelled() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled")
}

pub fn is_cancelled(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::Interrupted
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        s.push(char::from_digit((b & 15) as u32, 16).unwrap_or('0'));
    }
    s
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    pub type ProgressRoutine = unsafe extern "system" fn(i64, i64, i64, i64, u32, u32, *mut c_void, *mut c_void, *const c_void) -> u32;
    pub const COPY_FILE_FAIL_IF_EXISTS: u32 = 0x0000_0001;
    pub const PROGRESS_CONTINUE: u32 = 0;
    pub const PROGRESS_CANCEL: u32 = 1;
    pub const ERROR_REQUEST_ABORTED: i32 = 1235;
    pub const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn CopyFileExW(existing: *const u16, new: *const u16, progress: Option<ProgressRoutine>, data: *const c_void, cancel: *mut i32, flags: u32) -> i32;
        pub fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        pub fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        pub fn GetCurrentThread() -> *mut c_void;
        pub fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
    }

    /// NUL-terminated wide path in verbatim form (`\\?\`), so paths past MAX_PATH work.
    pub fn wide(p: &std::path::Path) -> std::io::Result<Vec<u16>> {
        use std::os::windows::ffi::OsStrExt;
        let abs = std::path::absolute(p)?;
        let raw: Vec<u16> = abs.as_os_str().encode_wide().collect();
        let bs = b'\\' as u16;
        let mut v: Vec<u16> = Vec::with_capacity(raw.len() + 8);
        if raw.starts_with(&[bs, bs, b'?' as u16, bs]) || raw.starts_with(&[bs, bs, b'.' as u16, bs]) {
            v.extend_from_slice(&raw);
        } else if raw.starts_with(&[bs, bs]) {
            v.extend(r"\\?\UNC\".encode_utf16());
            v.extend_from_slice(&raw[2..]);
        } else {
            v.extend(r"\\?\".encode_utf16());
            v.extend_from_slice(&raw);
        }
        v.push(0);
        Ok(v)
    }
}

/// Lowers the calling thread's CPU priority (copy workers: renders stay responsive).
pub fn lower_thread_priority() {
    #[cfg(windows)]
    unsafe {
        win::SetThreadPriority(win::GetCurrentThread(), win::THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// Bytes the caller may still write on the volume holding `dir`.
pub fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        let w = win::wide(dir).ok()?;
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        let ok = unsafe { win::GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) };
        (ok != 0).then_some(avail)
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return None;
        }
        let stat = unsafe { stat.assume_init() };
        (stat.f_bavail as u64).checked_mul(stat.f_frsize as u64)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = dir;
        None
    }
}

#[cfg(windows)]
struct CopyCtx<'a> {
    last: std::cell::Cell<i64>,
    done: &'a AtomicU64,
    cancel: &'a AtomicBool,
}

#[cfg(windows)]
unsafe extern "system" fn copy_progress(
    _total: i64,
    transferred: i64,
    _stream_size: i64,
    _stream_transferred: i64,
    _stream: u32,
    _reason: u32,
    _src: *mut std::ffi::c_void,
    _dst: *mut std::ffi::c_void,
    data: *const std::ffi::c_void,
) -> u32 {
    // SAFETY: `data` is the `CopyCtx` that `copy_os` passes and keeps alive for the call.
    let ctx = unsafe { &*(data as *const CopyCtx) };
    let delta = transferred - ctx.last.get();
    if delta > 0 {
        ctx.done.fetch_add(delta as u64, Ordering::Relaxed);
        ctx.last.set(transferred);
    }
    if ctx.cancel.load(Ordering::Relaxed) { win::PROGRESS_CANCEL } else { win::PROGRESS_CONTINUE }
}

/// Copies with the OS copy call (Windows `CopyFileExW`: keeps the modification time, uses
/// server-side copy on SMB shares and block cloning on ReFS). Never replaces `dst`. Adds
/// copied bytes to `done` as they go; stops when `cancel` is set. Returns the bytes copied.
pub fn copy_os(src: &Path, dst: &Path, done: &AtomicU64, cancel: &AtomicBool) -> std::io::Result<u64> {
    #[cfg(windows)]
    {
        let (s, d) = (win::wide(src)?, win::wide(dst)?);
        let ctx = CopyCtx { last: std::cell::Cell::new(0), done, cancel };
        let ok = unsafe {
            win::CopyFileExW(s.as_ptr(), d.as_ptr(), Some(copy_progress), &ctx as *const CopyCtx as *const std::ffi::c_void, std::ptr::null_mut(), win::COPY_FILE_FAIL_IF_EXISTS)
        };
        if ok == 0 {
            let e = std::io::Error::last_os_error();
            // what the callback already counted is not in the package
            done.fetch_sub(ctx.last.get().max(0) as u64, Ordering::Relaxed);
            return Err(if e.raw_os_error() == Some(win::ERROR_REQUEST_ABORTED) { cancelled() } else { e });
        }
        // count what the last callback did not report
        let len = std::fs::metadata(dst)?.len();
        let counted = ctx.last.get().max(0) as u64;
        if len > counted {
            done.fetch_add(len - counted, Ordering::Relaxed);
        }
        Ok(len)
    }
    #[cfg(not(windows))]
    {
        // create_new is atomic; std::fs::copy after exists() could overwrite a file
        // created by another NAS client. The stream path also preserves mtime/cancellation.
        copy_stream(src, dst, &mut vec![0; STREAM_BUFFER], done, cancel).map(|(n, _)| n)
    }
}

/// Copies through `buf` (large reads and writes), hashing the bytes on the way; keeps the
/// modification time; never replaces `dst`. Returns (bytes, SHA-256 hex).
pub fn copy_stream(src: &Path, dst: &Path, buf: &mut [u8], done: &AtomicU64, cancel: &AtomicBool) -> std::io::Result<(u64, String)> {
    let mut input = std::fs::File::open(src)?;
    let meta = input.metadata()?;
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
    // one allocation for the whole file
    let _ = out.set_len(meta.len());
    let mut h = Sha256::new();
    let mut total = 0u64;
    let result = (|| -> std::io::Result<()> {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            let n = match input.read(buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted && !cancel.load(Ordering::Relaxed) => continue,
                Err(e) => return Err(e),
            };
            h.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            total += n as u64;
            done.fetch_add(n as u64, Ordering::Relaxed);
        }
        if total != meta.len() {
            out.set_len(total)?;
        }
        if let Ok(t) = meta.modified() {
            out.set_modified(t)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        done.fetch_sub(total, Ordering::Relaxed);
        drop(out);
        let _ = std::fs::remove_file(dst);
        return Err(e);
    }
    Ok((total, hex(&h.finalize())))
}

/// SHA-256 of a file (hex).
pub fn sha256_file(path: &Path, buf: &mut [u8], cancel: &AtomicBool) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let n = f.read(buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Whether two files have the same bytes (sizes first, then content in chunks).
pub fn same_content(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (ma, mb) = (std::fs::metadata(a)?, std::fs::metadata(b)?);
    if ma.len() != mb.len() {
        return Ok(false);
    }
    let (mut fa, mut fb) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = read_full(&mut fa, &mut ba)?;
        let m = read_full(&mut fb, &mut bb)?;
        if n != m || ba[..n] != bb[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Copied files must stay deletable by the user (sources on a NAS are often read-only).
// Retain the existing cross-platform copy permissions, including writable NAS copies.
#[allow(clippy::permissions_set_readonly_false)]
pub fn make_writable(path: &Path) {
    if let Ok(m) = std::fs::metadata(path) {
        let mut p = m.permissions();
        if p.readonly() {
            p.set_readonly(false);
            let _ = std::fs::set_permissions(path, p);
        }
    }
}

/// Renames folder `from` to `to`; fails when `to` exists (never replaces it). Retries a few
/// times while another program (an antivirus scanner) still has a new file open.
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut wait = std::time::Duration::from_millis(25);
    for attempt in 0.. {
        match move_no_replace(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                let busy = matches!(e.raw_os_error(), Some(5 | 32)) && std::fs::symlink_metadata(to).is_err();
                if !busy || attempt >= 7 {
                    return Err(e);
                }
                std::thread::sleep(wait);
                wait *= 2;
            }
        }
    }
    // The retry loop returns by attempt seven, before its unbounded range can end.
    unreachable!()
}

fn move_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let (f, t) = (win::wide(from)?, win::wide(to)?);
        // no MOVEFILE_REPLACE_EXISTING: an existing `to` makes this fail
        if unsafe { win::MoveFileExW(f.as_ptr(), t.as_ptr(), 0) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let f = std::ffi::CString::new(from.as_os_str().as_bytes())?;
        let t = std::ffi::CString::new(to.as_os_str().as_bytes())?;
        #[cfg(target_os = "macos")]
        let result = unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(target_os = "linux")]
        let result = unsafe { libc::renameat2(libc::AT_FDCWD, f.as_ptr(), libc::AT_FDCWD, t.as_ptr(), libc::RENAME_NOREPLACE) };
        if result == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        if std::fs::symlink_metadata(to).is_ok() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "destination exists"));
        }
        std::fs::rename(from, to)
    }
}

/// Whether an error means the target already exists.
pub fn is_exists(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::AlreadyExists || matches!(e.raw_os_error(), Some(80 | 183))
}

/// A new, empty folder beside `dest` for building the package; unique per process and call.
pub fn create_staging(dest: &Path) -> std::io::Result<PathBuf> {
    use std::sync::atomic::AtomicUsize;
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    let parent = dest.parent().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent folder"))?;
    let name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    loop {
        let n = SERIAL.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(".{name}.folio-partial-{}-{n}", std::process::id()));
        match std::fs::create_dir(&tmp) {
            Ok(()) => return Ok(tmp),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("folio-package-fsops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copies_never_replace_and_keep_times() {
        let d = dir("copy");
        let src = d.join("a.bin");
        let data: Vec<u8> = (0..(5 << 20) + 17).map(|i| (i * 31 % 251) as u8).collect();
        std::fs::write(&src, &data).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::File::options().write(true).open(&src).unwrap().set_modified(old).unwrap();
        let (done, cancel) = (AtomicU64::new(0), AtomicBool::new(false));

        let os = d.join("os.bin");
        assert_eq!(copy_os(&src, &os, &done, &cancel).unwrap(), data.len() as u64);
        assert_eq!(std::fs::read(&os).unwrap(), data);
        assert_eq!(std::fs::metadata(&os).unwrap().modified().unwrap(), old);
        assert!(copy_os(&src, &os, &done, &cancel).is_err(), "existing file is not replaced");

        let st = d.join("stream.bin");
        let mut buf = vec![0u8; 1 << 20];
        let (n, sha) = copy_stream(&src, &st, &mut buf, &done, &cancel).unwrap();
        assert_eq!(n, data.len() as u64);
        assert_eq!(std::fs::read(&st).unwrap(), data);
        assert_eq!(std::fs::metadata(&st).unwrap().modified().unwrap(), old);
        assert_eq!(sha, sha256_file(&src, &mut buf, &cancel).unwrap());
        assert_eq!(sha, hex(&Sha256::digest(&data)));
        assert!(copy_stream(&src, &st, &mut buf, &done, &cancel).is_err());
        assert_eq!(done.load(Ordering::Relaxed), 2 * data.len() as u64);
        assert!(same_content(&src, &st).unwrap());
        std::fs::write(d.join("other.bin"), [1u8, 2, 3]).unwrap();
        assert!(!same_content(&src, &d.join("other.bin")).unwrap());

        // cancelled copies leave nothing and count nothing
        cancel.store(true, Ordering::Relaxed);
        let before = done.load(Ordering::Relaxed);
        assert!(is_cancelled(&copy_os(&src, &d.join("c1.bin"), &done, &cancel).unwrap_err()));
        assert!(is_cancelled(&copy_stream(&src, &d.join("c2.bin"), &mut buf, &done, &cancel).unwrap_err()));
        assert!(!d.join("c1.bin").exists() && !d.join("c2.bin").exists());
        assert_eq!(done.load(Ordering::Relaxed), before);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_never_replaces_a_folder() {
        let d = dir("rename");
        let a = d.join("a");
        std::fs::create_dir(&a).unwrap();
        std::fs::write(a.join("x.txt"), b"new").unwrap();
        let existing = d.join("taken");
        std::fs::create_dir(&existing).unwrap();
        std::fs::write(existing.join("keep.txt"), b"old").unwrap();
        let e = rename_no_replace(&a, &existing).unwrap_err();
        assert!(is_exists(&e), "{e:?}");
        assert_eq!(std::fs::read(existing.join("keep.txt")).unwrap(), b"old");
        // an empty existing folder is not replaced either
        let empty = d.join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert!(rename_no_replace(&a, &empty).is_err());
        rename_no_replace(&a, &d.join("b")).unwrap();
        assert_eq!(std::fs::read(d.join("b").join("x.txt")).unwrap(), b"new");
        assert!(!a.exists());
        let s1 = create_staging(&d.join("pkg")).unwrap();
        let s2 = create_staging(&d.join("pkg")).unwrap();
        assert_ne!(s1, s2);
        assert!(free_space(&d).is_none_or(|v| v > 0));
        let _ = std::fs::remove_dir_all(&d);
    }
}
