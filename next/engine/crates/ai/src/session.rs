use folio_project::{atomic, hash::sha256_bytes, util::uuid_v4};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Credentials are supplied only by the shell. Apply at every conversation boundary,
/// including model text, object keys and errors before they become events or JSONL.
#[derive(Default)]
pub struct Secrets {
    values: Vec<String>,
}

impl Secrets {
    pub fn remember(&mut self, value: &str) {
        if !value.is_empty() && !self.values.iter().any(|v| v == value) {
            self.values.push(value.into());
            self.values.sort_by_key(|s| std::cmp::Reverse(s.len()));
        }
    }
    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for value in &self.values {
            out = out.replace(value, "[redacted]");
        }
        for prefix in [
            "test-key-",
            "sk-",
            "ghp_",
            "github_pat_",
            "xoxb-",
            "Bearer ",
            "bearer ",
        ] {
            let mut at = 0;
            while let Some(offset) = out[at..].find(prefix) {
                let start = at + offset;
                let tail = &out[start + prefix.len()..];
                let length = tail
                    .bytes()
                    .take_while(|c| c.is_ascii_alphanumeric() || b"_.-/+=".contains(c))
                    .count();
                if length == 0 {
                    at = start + prefix.len();
                    continue;
                }
                out.replace_range(start..start + prefix.len() + length, "[redacted]");
                at = start + "[redacted]".len();
            }
        }
        out
    }
    pub fn clean(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => json!(self.text(s)),
            Value::Array(a) => Value::Array(a.iter().map(|v| self.clean(v)).collect()),
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| {
                        let field = k.to_ascii_lowercase().replace(['-', '_'], "");
                        let value = if matches!(
                            field.as_str(),
                            "apikey"
                                | "credential"
                                | "password"
                                | "authorization"
                                | "accesstoken"
                                | "refreshtoken"
                                | "token"
                        ) {
                            json!("[redacted]")
                        } else {
                            self.clean(v)
                        };
                        (self.text(k), value)
                    })
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
}

type Result<T> = std::result::Result<T, String>;
fn io(_: std::io::Error) -> String {
    "could not access AI session storage".into()
}
fn id(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid identifier".into());
    }
    let upper = value.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "INDEX")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit())
    {
        return Err("reserved identifier".into());
    }
    Ok(value)
}
fn safe(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io(e)),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("linked session path".into());
        }
    }
    if meta.file_type().is_symlink() {
        return Err("linked session path".into());
    }
    Ok(())
}

pub struct Store {
    dir: PathBuf,
}
impl Store {
    pub fn open(user_data: &Path, project: &str) -> Result<Self> {
        fs::create_dir_all(user_data).map_err(io)?;
        let dir = user_data.join("ai");
        for path in [&dir, &dir.join(id(project)?)] {
            safe(path)?;
            fs::create_dir_all(path).map_err(io)?;
        }
        Ok(Self {
            dir: dir.join(project),
        })
    }
    fn path(&self, name: &str) -> Result<PathBuf> {
        let p = self.dir.join(name);
        safe(&p)?;
        Ok(p)
    }
    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path("sessions.lock")?)
            .map_err(io)?;
        file.lock().map_err(io)?;
        Ok(file)
    }
    fn file(&self, session: &str) -> Result<PathBuf> {
        self.path(&format!("{}.jsonl", id(session)?))
    }
    fn records_unlocked(&self, session: &str) -> Result<Vec<Value>> {
        let f = File::open(self.file(session)?).map_err(io)?;
        let mut records = Vec::new();
        let mut reader = BufReader::new(f);
        let mut line = String::new();
        while reader.read_line(&mut line).map_err(io)? != 0 {
            if !line.ends_with('\n') {
                return Err("incomplete AI session record".into());
            }
            records.push(serde_json::from_str(&line).map_err(|_| "invalid AI session record")?);
            line.clear();
        }
        Ok(records)
    }
    pub fn records(&self, session: &str) -> Result<Vec<Value>> {
        let _lock = self.lock()?;
        self.records_unlocked(session)
    }
    fn write_record(mut file: File, value: &Value, secrets: &Secrets) -> Result<()> {
        let end = file.seek(SeekFrom::End(0)).map_err(io)?;
        if end != 0 {
            file.seek(SeekFrom::End(-1)).map_err(io)?;
            let mut last = [0];
            file.read_exact(&mut last).map_err(io)?;
            if last != [b'\n'] {
                return Err("incomplete AI session record".into());
            }
        }
        let mut bytes = serde_json::to_vec(&secrets.clean(value)).map_err(|_| "invalid record")?;
        bytes.push(b'\n');
        if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_data()) {
            file.set_len(end).map_err(io)?;
            return Err(io(e));
        }
        Ok(())
    }
    fn append_unlocked(&self, session: &str, v: &Value, secrets: &Secrets) -> Result<()> {
        Self::write_record(
            OpenOptions::new()
                .read(true)
                .append(true)
                .open(self.file(session)?)
                .map_err(io)?,
            v,
            secrets,
        )
    }
    pub fn append(&self, session: &str, value: &Value, secrets: &Secrets) -> Result<()> {
        let _lock = self.lock()?;
        self.append_unlocked(session, value, secrets)
    }
    // JSONL is authoritative. An interrupted rename/create/delete is reflected on the next list.
    fn rebuild(&self) -> Result<Vec<Value>> {
        let mut list = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(io)? {
            let path = entry.map_err(io)?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let session = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("invalid filename")?;
            let records = self.records_unlocked(session)?;
            let first = records.first().ok_or("empty session")?;
            let name = records
                .iter()
                .rev()
                .find(|v| v["kind"] == "rename")
                .unwrap_or(first)["name"]
                .clone();
            list.push(json!({"sessionId":session, "name":name, "language":first["language"], "mode":first["mode"],"connectionId":first["connectionId"],"model":first["model"],"provider":first["provider"]}));
        }
        list.sort_by(|a, b| a["sessionId"].as_str().cmp(&b["sessionId"].as_str()));
        let bytes = serde_json::to_vec(&list).map_err(|_| "invalid session index")?;
        let path = self.path("index.json")?;
        if fs::read(&path).ok().as_ref() != Some(&bytes) {
            atomic::write_bytes(&path, &bytes).map_err(io)?;
        }
        Ok(list)
    }
    pub fn list(&self) -> Result<Vec<Value>> {
        let _lock = self.lock()?;
        self.rebuild()
    }
    pub fn create(&self, header: &Value, secrets: &Secrets) -> Result<String> {
        let _lock = self.lock()?;
        let session = uuid_v4();
        let path = self.file(&session)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io)?;
        if let Err(error) = Self::write_record(file, header, secrets) {
            fs::remove_file(path).map_err(io)?;
            return Err(error);
        }
        self.rebuild()?;
        Ok(session)
    }
    pub fn rename(&self, session: &str, name: &str, secrets: &Secrets) -> Result<()> {
        let _lock = self.lock()?;
        self.append_unlocked(session, &json!({"kind":"rename","name":name}), secrets)?;
        self.rebuild()?;
        Ok(())
    }
    pub fn delete(&self, session: &str) -> Result<()> {
        let _lock = self.lock()?;
        let file = self.file(session)?;
        let images = self.images_dir(session)?;
        if images.exists() {
            fs::remove_dir_all(images).map_err(io)?;
        }
        fs::remove_file(file).map_err(io)?;
        self.rebuild()?;
        Ok(())
    }
    pub fn markdown(&self, session: &str) -> Result<String> {
        let records = self.records(session)?;
        let name = records
            .iter()
            .rev()
            .find(|v| matches!(v["kind"].as_str(), Some("rename" | "session")))
            .and_then(|v| v["name"].as_str())
            .unwrap_or("Folio");
        let mut text = format!("# {name}\n");
        for v in records {
            if let Some(t) = v["text"]
                .as_str()
                .filter(|_| matches!(v["kind"].as_str(), Some("user" | "text")))
            {
                text.push_str(&format!(
                    "\n## {}\n\n{t}\n",
                    if v["kind"] == "user" { "User" } else { "Folio" }
                ));
            }
        }
        Ok(text)
    }
    fn images_dir(&self, session: &str) -> Result<PathBuf> {
        self.path(&format!("{}.images", id(session)?))
    }
    fn images(record: &mut Value, mut visit: impl FnMut(&mut Value) -> Result<()>) -> Result<()> {
        match record["kind"].as_str() {
            Some("user") => {
                for attachment in record["attachments"].as_array_mut().into_iter().flatten() {
                    if let Some(image) = attachment.get_mut("image") {
                        visit(image)?;
                    }
                }
            }
            Some("tool_result") if record["content"]["type"] == "pageImage" => {
                visit(&mut record["content"])?
            }
            _ => {}
        }
        Ok(())
    }
    pub fn persist_images(&self, session: &str, record: &mut Value) -> Result<()> {
        Self::images(record, |image| {
            let bytes = folio_project::idmlparts::decode_base64(
                image["png"].as_str().ok_or("missing image")?,
            )
            .ok_or("invalid PNG")?;
            let hash = sha256_bytes(&bytes);
            let _lock = self.lock()?;
            let dir = self.images_dir(session)?;
            fs::create_dir_all(&dir).map_err(io)?;
            let path = dir.join(format!("{hash}.png"));
            safe(&path)?;
            if fs::read(&path).ok().as_deref() != Some(&bytes) {
                atomic::write_bytes(&path, &bytes).map_err(io)?;
            }
            let fields = image.as_object_mut().ok_or("invalid image")?;
            fields.remove("png");
            fields.insert("sha256".into(), json!(hash));
            fields.insert("pngRef".into(), json!(hash));
            Ok(())
        })
    }
    pub fn hydrate(&self, session: &str, record: &mut Value) -> Result<()> {
        Self::images(record, |image| {
            let hash = image["pngRef"]
                .as_str()
                .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or("invalid image reference")?;
            if image["sha256"] != hash {
                return Err("invalid image reference".into());
            }
            let path = self.images_dir(session)?.join(format!("{hash}.png"));
            safe(&path)?;
            let bytes = fs::read(path).map_err(io)?;
            if sha256_bytes(&bytes) != hash {
                return Err("changed attachment".into());
            }
            image
                .as_object_mut()
                .ok_or("invalid image")?
                .remove("pngRef");
            image["png"] = json!(
                folio_project::idmlparts::base64_lines(&bytes)
                    .split_whitespace()
                    .collect::<String>()
            );
            Ok(())
        })
    }
}
