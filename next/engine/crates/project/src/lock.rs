//! Document locks for shared (NAS) projects.
//!
//! Opening a document writes `.folio/locks/<document id>.json` with who holds it: user,
//! machine, process id (and the process's start time, so a reused id is not mistaken for the
//! holder) and when. A second opener gets the holder and opens read-only. A lock is stale
//! when it was taken on this machine by a process that no longer runs; a stale lock is
//! replaced. Locks from other machines are never judged stale automatically: the window
//! shows their age and offers an explicit override.
//!
//! Lock files are transient: removed when the document closes, never part of the project's
//! content, and the only place a user or machine name is written inside a project.

use crate::util::{now_rfc3339, now_secs, parse_rfc3339, short_id};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockInfo {
    pub user: String,
    pub machine: String,
    pub pid: u32,
    /// Start time of the holding process (OS ticks; 0 when unknown).
    #[serde(default)]
    pub process_start: u64,
    /// When the lock was taken (RFC 3339 UTC).
    pub time: String,
    /// The document, root-relative.
    pub document: String,
    #[serde(default)]
    pub app: String,
    /// Random id of this lock, so a holder removes only its own file.
    pub session: String,
}

/// How a lock request ended.
#[derive(Debug)]
pub enum Acquired {
    /// This process holds the lock (`replaced`: a stale lock of this machine was removed).
    Held { guard: LockGuard, replaced: Option<LockInfo> },
    /// Someone else holds it: open read-only.
    HeldBy { info: LockInfo, age_secs: Option<i64>, same_machine: bool },
}

/// Removes the lock file when dropped, if the file is still this holder's.
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    pub info: LockInfo,
}

impl LockGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if read_file(&self.path).is_some_and(|i| i.session == self.info.session) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn user_name() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "unknown".into())
}

pub fn machine_name() -> String {
    if let Ok(n) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        return n;
    }
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::process::Command::new("hostname").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()))
        .unwrap_or_else(|| "unknown".into())
}

fn lock_path(root: &Path, doc_id: &str) -> PathBuf {
    let safe: String = doc_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    root.join(".folio").join("locks").join(format!("{safe}.json"))
}

fn read_file(path: &Path) -> Option<LockInfo> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The current holder of a document's lock, if any.
pub fn holder(root: &Path, doc_id: &str) -> Option<LockInfo> {
    read_file(&lock_path(root, doc_id))
}

fn mine(doc_rel: &str) -> LockInfo {
    let pid = std::process::id();
    LockInfo {
        user: user_name(),
        machine: machine_name(),
        pid,
        process_start: process::start_time(pid).unwrap_or(0),
        time: now_rfc3339(),
        document: doc_rel.to_string(),
        app: format!("Folio Studio engine {}", env!("CARGO_PKG_VERSION")),
        session: short_id(),
    }
}

/// Whether a lock is stale: taken on this machine by a process that is gone (or whose id now
/// belongs to another process). Locks of other machines are never stale here, and neither is
/// a process this user may not inspect.
pub fn is_stale(info: &LockInfo) -> bool {
    if !info.machine.eq_ignore_ascii_case(&machine_name()) {
        return false;
    }
    match process::liveness(info.pid) {
        process::Liveness::Gone => true,
        process::Liveness::Running(start) => info.process_start != 0 && start != 0 && start != info.process_start,
        process::Liveness::Unknown => false,
    }
}

fn create_new(path: &Path, info: &LockInfo) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    let r = f.write_all(&serde_json::to_vec_pretty(info).map_err(std::io::Error::other)?).and_then(|_| f.sync_all());
    if r.is_err() {
        drop(f);
        let _ = std::fs::remove_file(path);
    }
    r
}

/// Takes the lock of document `doc_id` (at `doc_rel`) in the project at `root`.
pub fn acquire(root: &Path, doc_id: &str, doc_rel: &str) -> std::io::Result<Acquired> {
    let path = lock_path(root, doc_id);
    std::fs::create_dir_all(path.parent().unwrap_or(root))?;
    let info = mine(doc_rel);
    for _ in 0..3 {
        match create_new(&path, &info) {
            Ok(()) => return Ok(Acquired::Held { guard: LockGuard { path, info }, replaced: None }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let Some(existing) = read_file(&path) else {
            // being written by someone right now, or damaged: look again shortly
            std::thread::sleep(std::time::Duration::from_millis(150));
            if read_file(&path).is_none() && std::fs::metadata(&path).is_ok_and(|m| m.modified().is_ok_and(|t| t.elapsed().is_ok_and(|d| d.as_secs() > 10))) {
                // a damaged lock older than 10 s is removed like a stale one
                let _ = std::fs::remove_file(&path);
            }
            continue;
        };
        if is_stale(&existing) {
            replace(&path, &info)?;
            return Ok(Acquired::Held { guard: LockGuard { path, info }, replaced: Some(existing) });
        }
        let same_machine = existing.machine.eq_ignore_ascii_case(&info.machine);
        let age_secs = parse_rfc3339(&existing.time).map(|t| now_secs() - t);
        return Ok(Acquired::HeldBy { info: existing, age_secs, same_machine });
    }
    Err(std::io::Error::other("잠금 파일을 읽을 수 없습니다"))
}

fn replace(path: &Path, info: &LockInfo) -> std::io::Result<()> {
    crate::atomic::write_bytes(path, &serde_json::to_vec_pretty(info).map_err(std::io::Error::other)?)
}

/// Takes the lock over explicitly (the user chose to override another holder).
pub fn take_over(root: &Path, doc_id: &str, doc_rel: &str) -> std::io::Result<(LockGuard, Option<LockInfo>)> {
    let path = lock_path(root, doc_id);
    std::fs::create_dir_all(path.parent().unwrap_or(root))?;
    let previous = read_file(&path);
    let info = mine(doc_rel);
    replace(&path, &info)?;
    Ok((LockGuard { path, info }, previous))
}

/// Process liveness and start time, without extra crates.
pub mod process {
    #[cfg(windows)]
    mod win {
        use std::ffi::c_void;
        pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        pub const STILL_ACTIVE: u32 = 259;
        pub const ERROR_INVALID_PARAMETER: i32 = 87;
        #[repr(C)]
        #[derive(Default, Clone, Copy)]
        pub struct FileTime {
            pub low: u32,
            pub high: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            pub fn CloseHandle(h: *mut c_void) -> i32;
            pub fn GetExitCodeProcess(h: *mut c_void, code: *mut u32) -> i32;
            pub fn GetProcessTimes(h: *mut c_void, creation: *mut FileTime, exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        }
    }

    /// What is known about a process id.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Liveness {
        /// Running, with its start time (OS ticks; 0 when not available).
        Running(u64),
        /// No process has this id.
        Gone,
        /// It may run but cannot be inspected (another user's process, no permission).
        Unknown,
    }

    pub fn liveness(pid: u32) -> Liveness {
        #[cfg(windows)]
        {
            // SAFETY: plain Win32 calls on a handle opened and closed here.
            unsafe {
                let h = win::OpenProcess(win::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if h.is_null() {
                    return if std::io::Error::last_os_error().raw_os_error() == Some(win::ERROR_INVALID_PARAMETER) { Liveness::Gone } else { Liveness::Unknown };
                }
                let mut code = 0u32;
                let exited = win::GetExitCodeProcess(h, &mut code) != 0 && code != win::STILL_ACTIVE;
                let (mut c, mut e, mut k, mut u) = (win::FileTime::default(), win::FileTime::default(), win::FileTime::default(), win::FileTime::default());
                let ok = win::GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0;
                win::CloseHandle(h);
                if exited {
                    Liveness::Gone
                } else {
                    Liveness::Running(if ok { (u64::from(c.high) << 32) | u64::from(c.low) } else { 0 })
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                // field 22: start time in clock ticks after boot
                Ok(stat) => Liveness::Running(stat.rsplit_once(')').and_then(|(_, a)| a.split_whitespace().nth(19)?.parse().ok()).unwrap_or(0)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Liveness::Gone,
                Err(_) => Liveness::Unknown,
            }
        }
        #[cfg(target_os = "macos")]
        {
            // kill -0's exit status cannot distinguish ESRCH from EPERM. A live process
            // owned by another user must never be mistaken for a stale NAS lock.
            if pid == 0 || pid > i32::MAX as u32 {
                return Liveness::Unknown;
            }
            if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
                Liveness::Running(0)
            } else {
                match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::ESRCH) => Liveness::Gone,
                    Some(libc::EPERM) => Liveness::Running(0),
                    _ => Liveness::Unknown,
                }
            }
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            match std::process::Command::new("kill").args(["-0", &pid.to_string()]).stderr(std::process::Stdio::null()).status() {
                Ok(s) if s.success() => Liveness::Running(0),
                Ok(_) => Liveness::Gone,
                Err(_) => Liveness::Unknown,
            }
        }
    }

    /// Start time of a running process, when known.
    pub fn start_time(pid: u32) -> Option<u64> {
        match liveness(pid) {
            Liveness::Running(t) if t != 0 => Some(t),
            _ => None,
        }
    }

    pub fn alive(pid: u32) -> bool {
        matches!(liveness(pid), Liveness::Running(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        let root = std::env::temp_dir().join(format!("folio-lock-{}-{}", std::process::id(), short_id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn second_opener_is_read_only_and_release_removes_the_file() {
        let root = project();
        let Acquired::Held { guard, replaced: None } = acquire(&root, "doc-1", "a.folio").unwrap() else { panic!("first open must hold") };
        assert!(guard.path().is_file());
        match acquire(&root, "doc-1", "a.folio").unwrap() {
            Acquired::HeldBy { info, same_machine, age_secs } => {
                assert_eq!(info.pid, std::process::id());
                assert!(same_machine);
                assert!(age_secs.is_some_and(|a| a < 60));
            }
            other => panic!("{other:?}"),
        }
        assert!(holder(&root, "doc-1").is_some());
        drop(guard);
        assert!(holder(&root, "doc-1").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_and_foreign_locks() {
        let root = project();
        // a process of this machine that has exited
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" }).args(if cfg!(windows) { vec!["/C", "exit"] } else { vec![] }).spawn().unwrap();
        let dead = child.id();
        child.wait().unwrap();
        let stale = LockInfo { user: "kim".into(), machine: machine_name(), pid: dead, process_start: 1, time: now_rfc3339(), document: "a.folio".into(), app: String::new(), session: "old".into() };
        std::fs::create_dir_all(root.join(".folio/locks")).unwrap();
        std::fs::write(lock_path(&root, "doc-2"), serde_json::to_vec(&stale).unwrap()).unwrap();
        assert!(is_stale(&stale));
        match acquire(&root, "doc-2", "a.folio").unwrap() {
            Acquired::Held { replaced: Some(old), .. } => assert_eq!(old.session, "old"),
            other => panic!("{other:?}"),
        }
        // a lock from another machine is never stale here, however old
        let foreign = LockInfo { user: "lee".into(), machine: "OTHER-MAC".into(), pid: dead, process_start: 0, time: "2026-01-01T00:00:00Z".into(), document: "b.folio".into(), app: String::new(), session: "f".into() };
        std::fs::write(lock_path(&root, "doc-3"), serde_json::to_vec(&foreign).unwrap()).unwrap();
        assert!(!is_stale(&foreign));
        match acquire(&root, "doc-3", "b.folio").unwrap() {
            Acquired::HeldBy { info, same_machine: false, age_secs: Some(age) } => {
                assert_eq!(info.user, "lee");
                assert!(age > 86_400);
            }
            other => panic!("{other:?}"),
        }
        // explicit override
        let (guard, previous) = take_over(&root, "doc-3", "b.folio").unwrap();
        assert_eq!(previous.unwrap().user, "lee");
        assert_eq!(holder(&root, "doc-3").unwrap().session, guard.info.session);
        drop(guard);
        assert!(holder(&root, "doc-3").is_none());
        // the running process is alive with a known start time
        assert!(process::alive(std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
