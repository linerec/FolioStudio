use super::Reply;
use crate::transport::{self, Cancel};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};

pub trait Decoder {
    fn event(&mut self, data: &str) -> Result<bool, String>;
    fn preview(&self) -> Option<(usize, &str, &str)>;
    fn finish(self) -> Result<Reply, String>;
}
pub fn decode(
    reader: impl Read,
    cancel: &Cancel,
    mut decoder: impl Decoder,
    preview: &mut impl FnMut(usize, &str, &str),
) -> Result<Reply, String> {
    let mut r = BufReader::new(reader.take(32 * 1024 * 1024));
    let mut line = Vec::new();
    let mut data = String::new();
    loop {
        cancel.check()?;
        line.clear();
        let n = (&mut r)
            .take(8 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(transport::read_error)?;
        if n == 0 {
            return Err("AI stream ended before its stop event".into());
        }
        if line.len() + data.len() > 8 * 1024 * 1024 {
            return Err("AI event too large".into());
        }
        let s = std::str::from_utf8(&line)
            .map_err(|_| "invalid SSE text")?
            .trim_end_matches(['\n', '\r']);
        if s.is_empty() {
            if !data.is_empty() {
                if decoder.event(data.trim_end_matches('\n'))? {
                    return decoder.finish();
                }
                if let Some((index, kind, text)) = decoder.preview() {
                    preview(index, kind, text)
                }
            }
            data.clear()
        } else if let Some(s) = s.strip_prefix("data:") {
            data.push_str(s.strip_prefix(' ').unwrap_or(s));
            data.push('\n')
        }
    }
}
pub fn push(v: &mut Value, field: &str, part: &Value) -> Result<(), String> {
    let s = part.as_str().ok_or("invalid stream fragment")?;
    if v[field].is_null() {
        v[field] = Value::String(String::new())
    }
    match &mut v[field] {
        Value::String(text) => {
            text.push_str(s);
            Ok(())
        }
        _ => Err("invalid stream field".into()),
    }
}
