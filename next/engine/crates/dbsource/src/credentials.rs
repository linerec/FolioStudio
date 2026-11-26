//! Passwords in the OS credential store (design 0, "계정 정보"): Windows Credential Manager, the macOS keychain (`security`),
//! the Linux Secret Service (`secret-tool`), the iOS keychain (Security.framework). Android keeps none (the Android Keystore is reached
//! from the app's Java side, which Folio does not have yet): there a password is kept for the session only (limits.toml).
//! A password never goes into the project, the user settings, a reply or a log.
//!
//! `store_info` says whether this device can keep passwords and why not (`dbCapabilities`); `self_test` keeps, reads and removes a
//! throwaway entry under its own service name (`dbCredentialStoreSelfTest`), never a user's connection.
//!
//! `FOLIO_TEST_CREDENTIALS=memory` keeps them in this process only (tests); `FOLIO_TEST_CREDENTIALS=none` acts as a device without a
//! store (the screen checks of the greyed "save the password": nothing is kept, `store_info` says `platformNoStore`).
use crate::error::{DbError, Kind, Result};
use crate::model::Secret;
use serde::Serialize;

const SERVICE: &str = "FolioStudio.db";
/// The self test's entries live under this service: no connection's password can be touched by it.
const SELF_TEST_SERVICE: &str = "FolioStudio.db.selftest";

fn memory() -> Option<&'static std::sync::Mutex<std::collections::BTreeMap<String, String>>> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, String>>> = std::sync::OnceLock::new();
    (std::env::var("FOLIO_TEST_CREDENTIALS").as_deref() == Ok("memory")).then(|| STORE.get_or_init(Default::default))
}

fn failed(what: &str) -> DbError { DbError::new(Kind::Credentials, format!("the credential store could not {what} the password")) }

/// What this device's credential store is and whether passwords can be kept there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreInfo {
    pub available: bool,
    /// `windowsCredentialManager`, `macosKeychain`, `secretService`, `iosKeychain`, `memory` (tests), `none`.
    pub kind: &'static str,
    /// Why not (`available` false): `secretToolMissing`, `androidNoStore`, `platformNoStore`. The screen words it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    /// The same in English, for a log or an LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'static str>,
}

/// Whether a program is on `PATH` (Linux: `secret-tool`).
#[cfg(target_os = "linux")]
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(program).is_file()))
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "ios"))]
fn yes(kind: &'static str) -> StoreInfo { StoreInfo { available: true, kind, reason: None, message: None } }
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios")))]
fn no(reason: &'static str, message: &'static str) -> StoreInfo { StoreInfo { available: false, kind: "none", reason: Some(reason), message: Some(message) } }

#[cfg(target_os = "windows")]
fn platform_store() -> StoreInfo { yes("windowsCredentialManager") }
#[cfg(target_os = "macos")]
fn platform_store() -> StoreInfo { yes("macosKeychain") }
#[cfg(target_os = "ios")]
fn platform_store() -> StoreInfo { yes("iosKeychain") }
#[cfg(target_os = "linux")]
fn platform_store() -> StoreInfo {
    if on_path("secret-tool") { return yes("secretService"); }
    StoreInfo { kind: "secretService", ..no("secretToolMissing", "the Secret Service tool secret-tool (package libsecret-tools) is not installed: a password is kept for this session only") }
}
#[cfg(target_os = "android")]
fn platform_store() -> StoreInfo {
    no("androidNoStore", "Android keeps no passwords for Folio yet (the Android Keystore needs the app's Java side): a password is kept for this session only")
}
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "ios", target_os = "android")))]
fn platform_store() -> StoreInfo { no("platformNoStore", "this platform keeps no passwords: a password is kept for this session only") }

/// This device's credential store (`dbCapabilities`).
pub fn store_info() -> StoreInfo {
    if memory().is_some() { return StoreInfo { available: true, kind: "memory", reason: None, message: None }; }
    if no_store() { return NO_STORE; }
    platform_store()
}

/// `FOLIO_TEST_CREDENTIALS=none`: this process acts as a device without a credential store.
fn no_store() -> bool { std::env::var("FOLIO_TEST_CREDENTIALS").as_deref() == Ok("none") }

const NO_STORE: StoreInfo = StoreInfo { available: false, kind: "none", reason: Some("platformNoStore"),
    message: Some("this platform keeps no passwords: a password is kept for this session only") };

/// Whether this platform can keep passwords.
pub fn available() -> bool { store_info().available }

/// The test store of this process (`FOLIO_TEST_CREDENTIALS=memory`), unless `os` asks for the real one.
fn test_store(os: bool) -> Option<&'static std::sync::Mutex<std::collections::BTreeMap<String, String>>> { if os { None } else { memory() } }

fn save_in(os: bool, service: &str, name: &str, secret: &Secret) -> Result<()> {
    if let Some(m) = test_store(os) { m.lock().map_err(|_| failed("keep"))?.insert(format!("{service}/{name}"), secret.expose().into()); return Ok(()); }
    if !os && no_store() { return Err(DbError::unsupported(NO_STORE.message.unwrap_or_default())); }
    os::save(service, name, secret)
}

fn load_in(os: bool, service: &str, name: &str) -> Result<Option<Secret>> {
    if let Some(m) = test_store(os) { return Ok(m.lock().map_err(|_| failed("read"))?.get(&format!("{service}/{name}")).map(Secret::new)); }
    if !os && no_store() { return Ok(None); }
    os::load(service, name)
}

fn remove_in(os: bool, service: &str, name: &str) -> Result<()> {
    if let Some(m) = test_store(os) { m.lock().map_err(|_| failed("remove"))?.remove(&format!("{service}/{name}")); return Ok(()); }
    if !os && no_store() { return Ok(()); }
    os::remove(service, name)
}

pub fn save(name: &str, secret: &Secret) -> Result<()> { save_in(false, SERVICE, name, secret) }

pub fn load(name: &str) -> Result<Option<Secret>> { load_in(false, SERVICE, name) }

pub fn remove(name: &str) -> Result<()> { remove_in(false, SERVICE, name) }

/// One step of the self test.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// `save`, `read` (the value read back is the one kept), `remove`, `gone` (nothing is found after removing).
    pub step: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The result of `self_test` (`dbCredentialStoreSelfTest`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfTest {
    pub ok: bool,
    pub store: StoreInfo,
    pub steps: Vec<Step>,
    pub ms: u64,
}

/// Keep a throwaway password under `SELF_TEST_SERVICE`, read it back, remove it and check it is gone: proves on this device (a Mac,
/// a Linux desktop) that the credential store works, without touching any connection. The value is not a secret and is not returned.
/// A failed step stops the test; the entry is removed in any case.
pub fn self_test() -> SelfTest { self_test_on(memory().is_none() && !no_store()) }

/// `os`: the real store even when this process tests with the memory one.
fn self_test_on(os: bool) -> SelfTest {
    let t = std::time::Instant::now();
    let store = if os { platform_store() } else { store_info() };
    let mut steps = Vec::new();
    if store.available {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let account = format!("selftest-{}-{nanos}", std::process::id());
        let value = Secret::new(format!("folio self test 비밀 \"{nanos}\""));
        let mut step = |step: &'static str, r: Result<bool>| -> bool {
            let ok = matches!(r, Ok(true));
            let (error_kind, error) = match r {
                Ok(true) => (None, None),
                Ok(false) => (Some(Kind::Credentials), Some(format!("the credential store gave a wrong answer at '{step}'"))),
                Err(e) => (Some(e.kind), Some(e.message)),
            };
            steps.push(Step { step, ok, error_kind, error });
            ok
        };
        let _ = step("save", save_in(os, SELF_TEST_SERVICE, &account, &value).map(|_| true))
            && step("read", load_in(os, SELF_TEST_SERVICE, &account).map(|got| got.as_ref() == Some(&value)))
            && step("remove", remove_in(os, SELF_TEST_SERVICE, &account).map(|_| true))
            && step("gone", load_in(os, SELF_TEST_SERVICE, &account).map(|got| got.is_none()));
        let _ = remove_in(os, SELF_TEST_SERVICE, &account);
    }
    let ok = store.available && steps.len() == 4 && steps.iter().all(|s| s.ok);
    SelfTest { ok, store, steps, ms: t.elapsed().as_millis() as u64 }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The session passwords: a connection the user unlocked for this run of the engine (`dbUnlockConnection`). They live in this process's
// memory only: never the disk, the settings, a log, a reply, the undo history or the project. Quitting the engine forgets them
// (`dbLockConnection` earlier); each `Secret` overwrites its memory when it is dropped.

fn sessions() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<String, Secret>> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, Secret>>> = std::sync::OnceLock::new();
    STORE.get_or_init(Default::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keep `secret` for connection `name` until the engine quits or `session_clear`.
pub fn session_set(name: &str, secret: Secret) { sessions().insert(name.to_owned(), secret); }

/// Forget the session password of `name` (its memory is overwritten).
pub fn session_clear(name: &str) { sessions().remove(name); }

/// Whether `name` is unlocked for this session.
pub fn session_unlocked(name: &str) -> bool { sessions().contains_key(name) }

/// The password a driver uses for connection `name`: this session's first, else the one in the OS credential store, else none
/// (the connection is tried without; a server that wants one answers `Kind::NeedsPassword`).
pub fn resolve(name: &str) -> Option<Secret> {
    if let Some(s) = sessions().get(name).cloned() { return Some(s); }
    load(name).unwrap_or(None)
}

#[cfg(target_os = "windows")]
mod os {
    use super::*;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW};

    fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
    fn target(service: &str, name: &str) -> Vec<u16> { wide(&format!("{service}/{name}")) }

    pub fn save(service: &str, name: &str, secret: &Secret) -> Result<()> {
        let mut t = target(service, name);
        let mut user = wide(name);
        let mut blob = secret.expose().as_bytes().to_vec();
        // SAFETY: every pointer is to a buffer alive for the call; the API copies what it keeps
        let ok = unsafe {
            let mut c: CREDENTIALW = std::mem::zeroed();
            c.Type = CRED_TYPE_GENERIC;
            c.TargetName = t.as_mut_ptr();
            c.UserName = user.as_mut_ptr();
            c.CredentialBlobSize = blob.len() as u32;
            c.CredentialBlob = blob.as_mut_ptr();
            c.Persist = CRED_PERSIST_LOCAL_MACHINE;
            CredWriteW(&c, 0)
        };
        blob.iter_mut().for_each(|b| *b = 0);
        if ok == 0 { Err(failed("keep")) } else { Ok(()) }
    }

    pub fn load(service: &str, name: &str) -> Result<Option<Secret>> {
        let t = target(service, name);
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: CredReadW gives a buffer the API owns, read then freed with CredFree
        unsafe {
            if CredReadW(t.as_ptr(), CRED_TYPE_GENERIC, 0, &mut p) == 0 {
                return if GetLastError() == ERROR_NOT_FOUND { Ok(None) } else { Err(failed("read")) };
            }
            let c = &*p;
            let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let secret = String::from_utf8(bytes.to_vec()).map(Secret::new).map_err(|_| failed("read"));
            CredFree(p as *const _);
            secret.map(Some)
        }
    }

    pub fn remove(service: &str, name: &str) -> Result<()> {
        let t = target(service, name);
        // SAFETY: a NUL-terminated name alive for the call
        unsafe {
            if CredDeleteW(t.as_ptr(), CRED_TYPE_GENERIC, 0) == 0 && GetLastError() != ERROR_NOT_FOUND { return Err(failed("remove")); }
        }
        Ok(())
    }
}

/// macOS (`/usr/bin/security`) and Linux (`secret-tool`): the password goes through standard input, never the command line.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod os {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn run(mut cmd: Command, input: Option<&str>) -> Result<(bool, String)> {
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_| failed("reach"))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) { stdin.write_all(input.as_bytes()).map_err(|_| failed("reach"))?; }
        let out = child.wait_with_output().map_err(|_| failed("reach"))?;
        Ok((out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned()))
    }

    #[cfg(target_os = "macos")]
    fn quoted(s: &str) -> String { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")) }

    pub fn save(service: &str, name: &str, secret: &Secret) -> Result<()> {
        #[cfg(target_os = "macos")]
        let (cmd, input) = {
            let mut c = Command::new("/usr/bin/security");
            c.arg("-i");
            (c, format!("add-generic-password -U -s {} -a {} -w {}\n", quoted(service), quoted(name), quoted(secret.expose())))
        };
        #[cfg(target_os = "linux")]
        let (cmd, input) = {
            let mut c = Command::new("secret-tool");
            c.args(["store", "--label", &format!("Folio Studio {name}"), "service", service, "account", name]);
            (c, secret.expose().to_owned())
        };
        match run(cmd, Some(&input))? { (true, _) => Ok(()), _ => Err(failed("keep")) }
    }

    pub fn load(service: &str, name: &str) -> Result<Option<Secret>> {
        #[cfg(target_os = "macos")]
        let cmd = { let mut c = Command::new("/usr/bin/security"); c.args(["find-generic-password", "-s", service, "-a", name, "-w"]); c };
        #[cfg(target_os = "linux")]
        let cmd = { let mut c = Command::new("secret-tool"); c.args(["lookup", "service", service, "account", name]); c };
        let (ok, out) = run(cmd, None)?;
        let value = out.strip_suffix('\n').unwrap_or(&out).to_owned();
        Ok((ok && !value.is_empty()).then(|| Secret::new(value)))
    }

    pub fn remove(service: &str, name: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        let cmd = { let mut c = Command::new("/usr/bin/security"); c.args(["delete-generic-password", "-s", service, "-a", name]); c };
        #[cfg(target_os = "linux")]
        let cmd = { let mut c = Command::new("secret-tool"); c.args(["clear", "service", service, "account", name]); c };
        run(cmd, None).map(|_| ())
    }
}

/// iOS: the app's keychain through Security.framework (an app cannot start `security`; no child processes on iOS). Compiled by the
/// portability check; not run on a device yet (limits.toml).
#[cfg(target_os = "ios")]
mod os {
    use super::*;
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
    /// `errSecItemNotFound`
    const NOT_FOUND: i32 = -25300;

    pub fn save(service: &str, name: &str, secret: &Secret) -> Result<()> {
        set_generic_password(service, name, secret.expose().as_bytes()).map_err(|_| failed("keep"))
    }

    pub fn load(service: &str, name: &str) -> Result<Option<Secret>> {
        match get_generic_password(service, name) {
            Ok(bytes) => String::from_utf8(bytes).map(|s| Some(Secret::new(s))).map_err(|e| { let mut b = e.into_bytes(); b.iter_mut().for_each(|x| *x = 0); failed("read") }),
            Err(e) if e.code() == NOT_FOUND => Ok(None),
            Err(_) => Err(failed("read")),
        }
    }

    pub fn remove(service: &str, name: &str) -> Result<()> {
        match delete_generic_password(service, name) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == NOT_FOUND => Ok(()),
            Err(_) => Err(failed("remove")),
        }
    }
}

/// Android and the rest: nothing is kept (`store_info` says why); the session password still works.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "ios")))]
mod os {
    use super::*;
    pub fn save(_: &str, _: &str, _: &Secret) -> Result<()> { Err(DbError::unsupported(store_info().message.unwrap_or("this platform keeps no passwords"))) }
    pub fn load(_: &str, _: &str) -> Result<Option<Secret>> { Ok(None) }
    pub fn remove(_: &str, _: &str) -> Result<()> { Ok(()) }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    #[test]
    fn windows_credential_manager_keeps_and_forgets() {
        let name = format!("folio-test-{}", std::process::id());
        let secret = crate::Secret::new("p@ss 비밀 \"x\"");
        super::os::save(super::SELF_TEST_SERVICE, &name, &secret).unwrap();
        assert_eq!(super::os::load(super::SELF_TEST_SERVICE, &name).unwrap(), Some(secret));
        super::os::remove(super::SELF_TEST_SERVICE, &name).unwrap();
        assert_eq!(super::os::load(super::SELF_TEST_SERVICE, &name).unwrap(), None);
        super::os::remove(super::SELF_TEST_SERVICE, &name).unwrap();
    }

    #[test]
    fn the_self_test_passes_with_the_windows_credential_manager() {
        let r = super::self_test_on(true);
        assert!(r.ok, "{r:?}");
        assert_eq!(r.store.kind, "windowsCredentialManager");
        assert_eq!(r.steps.iter().map(|s| s.step).collect::<Vec<_>>(), ["save", "read", "remove", "gone"]);
    }
}

/// The real store of a Mac or a Linux desktop (Secret Service): run with `FOLIO_TEST_OS_CREDENTIALS=1` where one is unlocked
/// (artifacts/dbplatform-c1-20261023/wsl-secret-service/run.sh starts a D-Bus session and an unlocked gnome-keyring in WSL).
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod os_tests {
    fn wanted() -> bool {
        let yes = std::env::var("FOLIO_TEST_OS_CREDENTIALS").as_deref() == Ok("1");
        if !yes { eprintln!("skipped: FOLIO_TEST_OS_CREDENTIALS=1 runs the real credential store test"); }
        yes
    }

    #[test]
    fn the_os_store_keeps_reads_and_forgets() {
        if !wanted() { return; }
        let name = format!("folio-test-{}", std::process::id());
        let secret = crate::Secret::new("p@ss 비밀 \"x\" \\ $HOME");
        assert_eq!(super::os::load(super::SELF_TEST_SERVICE, &name).unwrap(), None, "nothing before");
        super::os::save(super::SELF_TEST_SERVICE, &name, &secret).unwrap();
        assert_eq!(super::os::load(super::SELF_TEST_SERVICE, &name).unwrap(), Some(secret.clone()));
        let other = crate::Secret::new("second value");
        super::os::save(super::SELF_TEST_SERVICE, &name, &other).unwrap();
        assert_eq!(super::os::load(super::SELF_TEST_SERVICE, &name).unwrap(), Some(other), "saving again replaces");
        super::os::remove(super::SELF_TEST_SERVICE, &name).unwrap();
        assert_eq!(super::os::load(super::SELF_TEST_SERVICE, &name).unwrap(), None);
        super::os::remove(super::SELF_TEST_SERVICE, &name).unwrap();
    }

    #[test]
    fn the_self_test_passes_with_the_os_store() {
        if !wanted() { return; }
        let r = super::self_test_on(true);
        eprintln!("self test: {}", serde_json::to_string(&r).unwrap());
        assert!(r.ok, "{r:?}");
        assert!(matches!(r.store.kind, "secretService" | "macosKeychain"), "{r:?}");
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn a_session_password_is_found_first_and_forgotten_on_request() {
        // SAFETY: only this module's tests read this process variable, and each uses its own connection name
        unsafe { std::env::set_var("FOLIO_TEST_CREDENTIALS", "memory") };
        let name = "session-test-잡지";
        assert!(!session_unlocked(name) && resolve(name).is_none());
        save(name, &Secret::new("from-the-store")).unwrap();
        assert_eq!(resolve(name).unwrap().expose(), "from-the-store");
        session_set(name, Secret::new("from-the-session"));
        assert!(session_unlocked(name));
        assert_eq!(resolve(name).unwrap().expose(), "from-the-session", "the session wins over the store");
        session_clear(name);
        assert!(!session_unlocked(name));
        assert_eq!(resolve(name).unwrap().expose(), "from-the-store", "then the store");
        remove(name).unwrap();
        assert!(resolve(name).is_none());
        session_clear(name);
    }

    #[test]
    fn the_self_test_keeps_reads_and_removes_a_throwaway_entry_only() {
        // SAFETY: as above
        unsafe { std::env::set_var("FOLIO_TEST_CREDENTIALS", "memory") };
        save("잡지DB-selftest-neighbour", &Secret::new("kept")).unwrap();
        let r = self_test();
        assert!(r.ok, "{r:?}");
        assert_eq!((r.store.kind, r.store.available), ("memory", true));
        assert_eq!(r.steps.iter().map(|s| (s.step, s.ok)).collect::<Vec<_>>(), [("save", true), ("read", true), ("remove", true), ("gone", true)]);
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("folio self test"), "the throwaway value is not returned: {json}");
        assert_eq!(load("잡지DB-selftest-neighbour").unwrap().unwrap().expose(), "kept", "a connection's password is not touched");
        assert!(memory().unwrap().lock().unwrap().keys().all(|k| !k.starts_with(SELF_TEST_SERVICE)), "nothing is left behind");
        remove("잡지DB-selftest-neighbour").unwrap();
    }

    #[test]
    fn a_secret_never_prints() {
        let s = Secret::new("hunter2-비밀");
        assert!(!format!("{s:?}").contains("hunter2"));
        assert_eq!(s.clone(), s);
    }
}
