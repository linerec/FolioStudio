use crate::connection::{Connection, Kind};
use serde_json::Value;
use std::{
    io::{self, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::Acquire) {
            Err("stopped".into())
        } else {
            Ok(())
        }
    }
}
pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Value>,
}
impl Request {
    pub fn new(c: &Connection, key: &str, path: &str, body: Option<Value>) -> Self {
        let mut headers = vec![];
        if c.kind == Kind::Anthropic {
            headers.push(("anthropic-version", "2026-10-21".into()));
            headers.push(("x-api-key", key.into()));
        } else if !key.is_empty() {
            headers.push(("authorization", format!("Bearer {key}")))
        }
        if body.is_some() {
            headers.push(("content-type", "application/json".into()))
        }
        Self {
            url: format!("{}/{}", c.base_url, path),
            headers,
            body,
        }
    }
}
pub struct Response {
    pub status: u16,
    pub retry_after: Option<u64>,
    pub body: Box<dyn Read + Send>,
}
pub trait Transport: Send + Sync {
    fn send(&self, req: &Request, cancel: &Cancel) -> Result<Response, String>;
}
pub struct Desktop(ureq::Agent);
impl Default for Desktop {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_per_read(Some(Duration::from_secs(30)))
            .build();
        Self(config.new_agent())
    }
}
impl Transport for Desktop {
    fn send(&self, r: &Request, cancel: &Cancel) -> Result<Response, String> {
        cancel.check()?;
        let response = if let Some(body) = &r.body {
            let mut req = self.0.post(&r.url);
            for (k, v) in &r.headers {
                req = req.header(*k, v)
            }
            req.send(body.to_string().as_bytes())
        } else {
            let mut req = self.0.get(&r.url);
            for (k, v) in &r.headers {
                req = req.header(*k, v)
            }
            req.call()
        }
        .map_err(|_| "AI network request failed or timed out")?;
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()?.parse().ok());
        Ok(Response {
            status: response.status().as_u16(),
            retry_after,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}
pub fn bytes(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|_| "AI response read failed")?;
    if out.len() as u64 > limit {
        return Err("AI response too large".into());
    }
    Ok(out)
}
pub fn retry_wait(seconds: u64, cancel: &Cancel) -> Result<(), String> {
    if seconds > 120 {
        return Err(format!("{seconds}초 뒤에 다시 시도하세요"));
    }
    let end = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < end {
        cancel.check()?;
        std::thread::sleep(Duration::from_millis(50))
    }
    cancel.check()
}
pub fn response(
    t: &dyn Transport,
    r: &Request,
    cancel: &Cancel,
) -> Result<Box<dyn Read + Send>, String> {
    for attempt in 0..3 {
        let res = t.send(r, cancel)?;
        if (200..300).contains(&res.status) {
            return Ok(res.body);
        }
        if matches!(res.status, 401 | 403) {
            return Err("키를 확인하세요".into());
        }
        let error: Value = serde_json::from_slice(&bytes(res.body, 65536)?).unwrap_or_default();
        let message = error["error"]["message"]
            .as_str()
            .unwrap_or("AI request failed");
        if message.to_ascii_lowercase().contains("credit")
            || error["error"]["type"] == "insufficient_quota"
        {
            return Err("크레디트가 부족합니다".into());
        }
        if matches!(res.status, 429 | 529) || error["error"]["type"] == "overloaded_error" {
            if attempt < 2 {
                retry_wait(res.retry_after.unwrap_or(1 << attempt), cancel)?;
                continue;
            }
        }
        let mut safe = crate::session::Secrets::default();
        for (name, value) in &r.headers {
            if *name == "x-api-key" {
                safe.remember(value)
            }
            if *name == "authorization"
                && let Some(key) = value.strip_prefix("Bearer ")
            {
                safe.remember(key)
            }
        }
        return Err(if res.status == 400 {
            safe.text(message)
        } else {
            format!("AI HTTP {}", res.status)
        });
    }
    unreachable!()
}
pub(crate) fn read_error(_: io::Error) -> String {
    "AI stream interrupted".into()
}
