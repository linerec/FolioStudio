//! Shape a read reply before it crosses the engine pipe. Offsets count Unicode scalar values.
use serde_json::{Value, json};

const CHUNK: usize = 32_768;
const INLINE_BYTES: usize = 96 * 1024;

pub struct ReadReply {
    image: bool,
    offset: Option<u64>,
    limit: Option<usize>,
}

fn private_key(key: &str) -> bool {
    crate::attached::is_secret_name(key)
        || matches!(
            key.to_ascii_lowercase().as_str(),
            "token" | "connectionstring" | "dsn" | "printer" | "printername" | "defaultprinter"
        )
}

fn local_address(text: &str) -> bool {
    let bytes = text.as_bytes();
    if text
        .to_ascii_lowercase()
        .match_indices("file:")
        .any(|(i, _)| i == 0 || !bytes[i - 1].is_ascii_alphabetic())
        || text.contains(r"\\")
    {
        return true;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if b == b':'
            && i > 0
            && bytes[i - 1].is_ascii_alphabetic()
            && (i == 1 || !bytes[i - 2].is_ascii_alphanumeric())
            && bytes.get(i + 1).is_some_and(|b| matches!(b, b'/' | b'\\'))
        {
            return true;
        }
        if b == b'/'
            && (i == 0 || bytes[i - 1].is_ascii_whitespace() || b"=\"'([{>".contains(&bytes[i - 1]))
            && bytes
                .get(i + 1)
                .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'>')
        {
            return true;
        }
    }
    false
}

fn scrub(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|k, _| !private_key(k) && !local_address(k));
            for child in map.values_mut() {
                scrub(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(scrub),
        Value::String(text) if local_address(text) => *text = "[local path]".into(),
        _ => {}
    }
}

impl ReadReply {
    pub fn take(req: &mut Value) -> Option<Self> {
        let read = crate::attached::reads::get(req["cmd"].as_str()?)?;
        let obj = req.as_object_mut()?;
        Some(Self {
            image: read.name == "render",
            offset: obj.remove("resultOffset").and_then(|v| v.as_u64()),
            limit: obj
                .remove("resultLimit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize),
        })
    }

    pub fn finish(&self, header: &mut Value) {
        if self.image && header["ok"] == true {
            return;
        }
        if let Some(map) = header.as_object_mut() {
            map.remove("id");
            map.remove("bodyBytes");
        }
        if header["type"] == "fileMetadata"
            && let Some(info) = header["info"].as_object_mut()
        {
            // Raw XMP can contain arbitrary application-private properties.
            info.remove("packet");
        }
        if header["type"] == "project"
            && let Some(tree) = header["tree"].as_object_mut()
        {
            // A later chunk must not shift when measuring the same directory takes longer.
            tree.remove("ms");
        }
        scrub(header);
        if header["ok"] == false {
            if let Some(message) = header.get_mut("error")
                && let Some(text) = message.as_str()
            {
                *message = Value::String(text.chars().take(2048).collect());
            }
            return;
        }
        let serialized = header.to_string();
        if serialized.len() <= INLINE_BYTES && self.offset.is_none() && self.limit.is_none() {
            return;
        }
        let total = serialized.chars().count();
        let offset = self.offset.unwrap_or(0).min(total as u64) as usize;
        let chunk: String = serialized
            .chars()
            .skip(offset)
            .take(self.limit.unwrap_or(CHUNK))
            .collect();
        let end = offset + chunk.chars().count();
        *header = json!({"ok":true,"type":"jsonChunk","offset":offset,"nextOffset":(end < total).then_some(end),"total":total,"chunk":chunk});
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_paths_and_secret_fields_never_reach_chunks() {
        let read = ReadReply {
            image: false,
            offset: Some(0),
            limit: Some(32768),
        };
        let mut v = json!({"hint":r"D:\home\file.pdf","error":"open /home/me/file",
            "manifest":[{"uri":"file:///Users/artist/file","access_token":"private"}],"project":"links/artwork.png"});
        read.finish(&mut v);
        let value: Value = serde_json::from_str(v["chunk"].as_str().unwrap()).unwrap();
        assert_eq!(value["project"], "links/artwork.png");
        assert_eq!(value["hint"], "[local path]");
        assert!(!value.to_string().contains("private"));
        assert!(!value.to_string().contains("/home"));
    }

    #[test]
    fn large_read_reassembles_without_splitting_unicode() {
        let source = json!({"ok":true,"text":"한🪶글".repeat(33_000)});
        let mut offset = None;
        let mut text = String::new();
        loop {
            let mut part = source.clone();
            ReadReply {
                image: false,
                offset,
                limit: None,
            }
            .finish(&mut part);
            assert!(part.to_string().len() < 512 * 1024);
            text.push_str(part["chunk"].as_str().unwrap());
            offset = part["nextOffset"].as_u64();
            if offset.is_none() {
                break;
            }
        }
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), source);
    }
}
