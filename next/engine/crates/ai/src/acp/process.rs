use crate::connection::Connection;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Stdio},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

pub(super) struct Process {
    child: Child,
    writer: Option<mpsc::SyncSender<Value>>,
    pub inbox: mpsc::Receiver<Result<Value, String>>,
}
impl Process {
    pub fn start(
        c: &Connection,
        key: &str,
        cwd: &Path,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let mut cmd = super::executable(&c.command)?;
        cmd.args(&c.args).current_dir(cwd).env_clear();
        for var in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "COMSPEC",
            "PATHEXT",
            "TEMP",
            "TMP",
            "HOME",
            "USERPROFILE",
            "LOCALAPPDATA",
            "APPDATA",
            "TMPDIR",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(v) = std::env::var_os(var) {
                cmd.env(var, v);
            }
        }
        for (k, v) in &c.env {
            cmd.env(k, if v == "${credential}" { key } else { v });
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "could not start ACP executable")?;
        let mut stdin = child.stdin.take().unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let (writer, writes) = mpsc::sync_channel::<Value>(16);
        let (events, inbox) = mpsc::sync_channel(32);
        let process = Self {
            child,
            writer: Some(writer),
            inbox,
        };
        let signal = wake.clone();
        let failed = events.clone();
        std::thread::Builder::new()
            .name("acp-writer".into())
            .spawn(move || {
                for v in writes {
                    if serde_json::to_writer(&mut stdin, &v).is_err()
                        || stdin.write_all(b"\n").and_then(|_| stdin.flush()).is_err()
                    {
                        let _ = failed.send(Err("ACP input closed".into()));
                        signal();
                        break;
                    }
                }
            })
            .map_err(|_| "could not start ACP writer")?;
        std::thread::Builder::new()
            .name("acp-reader".into())
            .spawn(move || {
                loop {
                    let mut line = Vec::new();
                    let message = match reader
                        .by_ref()
                        .take(4 * 1024 * 1024 + 1)
                        .read_until(b'\n', &mut line)
                    {
                        Ok(n) if n > 0 && n <= 4 * 1024 * 1024 => {
                            serde_json::from_slice::<Value>(&line)
                                .map_err(|_| "invalid ACP message".to_string())
                        }
                        _ => Err("ACP output closed or exceeded message limit".into()),
                    };
                    let end = message.is_err();
                    if events.send(message).is_err() {
                        break;
                    }
                    wake();
                    if end {
                        break;
                    }
                }
            })
            .map_err(|_| "could not start ACP reader")?;
        Ok(process)
    }
    pub fn send(&self, v: Value) -> Result<(), String> {
        self.writer
            .as_ref()
            .unwrap()
            .try_send(v)
            .map_err(|_| "ACP input unavailable".into())
    }
    pub fn request(&self, id: u32, method: &str, params: Value) -> Result<(), String> {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.writer.take();
        let deadline = Instant::now() + Duration::from_millis(200);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
