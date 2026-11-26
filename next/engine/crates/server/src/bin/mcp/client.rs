//! `folio-mcp --attach`: the connection to the running Folio app's engine (the engine side is `src/attach.rs`).
//! The app writes `mcp-attach.json` (endpoint and a new token) in its user data folder while the user allows it.
use super::attached;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

pub struct Client {
    stream: Box<dyn Stream>,
    next: u64,
}

/// Why the app could not be reached, for the client to show (`appConnection`).
#[derive(Clone, Debug)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

/// Electron's user data folder name (`next/app/package.json` "name"); `FOLIO_USER_DATA` overrides the folder.
const APP_FOLDER: &str = "folio-studio-next";

pub fn default_file() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FOLIO_USER_DATA") { return Some(PathBuf::from(dir).join(attached::CONNECTION_FILE)); }
    app_data().map(|dir| dir.join(APP_FOLDER).join(attached::CONNECTION_FILE))
}

#[cfg(windows)]
fn app_data() -> Option<PathBuf> { std::env::var_os("APPDATA").map(PathBuf::from) }
#[cfg(target_os = "macos")]
fn app_data() -> Option<PathBuf> { std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library").join("Application Support")) }
#[cfg(all(unix, not(target_os = "macos")))]
fn app_data() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}
#[cfg(not(any(windows, unix)))]
fn app_data() -> Option<PathBuf> { None }

#[cfg(windows)]
fn open(endpoint: &str) -> std::io::Result<Box<dyn Stream>> {
    Ok(Box::new(std::fs::OpenOptions::new().read(true).write(true).open(endpoint)?))
}
#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
fn open(endpoint: &str) -> std::io::Result<Box<dyn Stream>> {
    Ok(Box::new(std::os::unix::net::UnixStream::connect(endpoint)?))
}
#[cfg(not(any(windows, all(unix, not(any(target_os = "ios", target_os = "android"))))))]
fn open(_endpoint: &str) -> std::io::Result<Box<dyn Stream>> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no MCP endpoint on this platform"))
}

pub const HOW_TO: &str = "Open Folio, turn on Preferences ▸ General ▸ Allow LLM connection (MCP), keep the app running, then restart this MCP server.";

pub fn connect(file: &Path) -> Result<Client, Failure> {
    let not_running = |message: String| Failure { code: "notRunning", message };
    let text = std::fs::read_to_string(file)
        .map_err(|_| not_running("Folio is not running with LLM connection (MCP) allowed: no connection file".into()))?;
    let info: Value = serde_json::from_str(&text).map_err(|_| Failure { code: "badFile", message: "the connection file is not valid JSON".into() })?;
    let (Some(endpoint), Some(token)) = (info["endpoint"].as_str(), info["token"].as_str()) else {
        return Err(Failure { code: "badFile", message: "the connection file has no endpoint or token".into() });
    };
    let stream = open(endpoint).map_err(|e| not_running(format!("the Folio app does not answer ({e}); it may have ended")))?;
    let mut client = Client { stream, next: 0 };
    let hello = format!("{}\n", json!({"attach": attached::PROTOCOL, "token": token}));
    client.stream.write_all(hello.as_bytes()).and_then(|_| client.stream.flush())
        .map_err(|e| not_running(format!("the Folio app closed the connection ({e})")))?;
    let (reply, _) = client.read_frame().map_err(|e| not_running(format!("the Folio app closed the connection ({e})")))?;
    if reply["event"] != "attached" {
        return Err(Failure { code: "refused", message: reply["error"].as_str().unwrap_or("refused").into() });
    }
    Ok(client)
}

impl Client {
    fn read_frame(&mut self) -> std::io::Result<(Value, Vec<u8>)> {
        let mut size = [0u8; 4];
        self.stream.read_exact(&mut size)?;
        let mut header = vec![0u8; u32::from_le_bytes(size) as usize];
        self.stream.read_exact(&mut header)?;
        let header: Value = serde_json::from_slice(&header).map_err(std::io::Error::other)?;
        let mut body = vec![0u8; header["bodyBytes"].as_u64().unwrap_or(0) as usize];
        self.stream.read_exact(&mut body)?;
        Ok((header, body))
    }

    /// One engine request (`{"cmd":…}`) and its reply header (without `id` and `bodyBytes`).
    pub fn request(&mut self, mut request: Value) -> Result<Value, String> {
        self.next += 1;
        request["id"] = json!(self.next);
        let line = format!("{request}\n");
        let closed = |e: std::io::Error| format!("the Folio app closed the connection ({e})");
        self.stream.write_all(line.as_bytes()).and_then(|_| self.stream.flush()).map_err(closed)?;
        loop {
            let (mut header, _) = self.read_frame().map_err(closed)?;
            if header["id"] != json!(self.next) && !header["id"].is_null() { continue; }
            if let Some(object) = header.as_object_mut() {
                object.remove("id");
                object.remove("bodyBytes");
            }
            return Ok(header);
        }
    }
}
