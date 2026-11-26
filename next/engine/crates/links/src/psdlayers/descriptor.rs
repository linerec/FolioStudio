//! Photoshop "descriptor" structures (key/value trees) as far as layer comps need them: the
//! comp list of image resource 1065 and the per-layer comp settings of the `shmd`/`cmls`
//! metadata block. A value of a kind that is not understood stops the read of that descriptor.
use super::{be16, be32};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Value {
    Long(i32),
    Double(f64),
    Bool(bool),
    Text(String),
    Enum(String, String),
    Object(String, Vec<(String, Value)>),
    List(Vec<Value>),
    Unit(f64),
}

impl Value {
    pub(super) fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(_, items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub(super) fn int(&self) -> Option<i32> {
        match self {
            Value::Long(v) => Some(*v),
            Value::Double(v) => Some(*v as i32),
            _ => None,
        }
    }
    pub(super) fn list(&self) -> &[Value] {
        match self {
            Value::List(v) => v,
            _ => &[],
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> Option<u32> {
        let v = be32(self.b, self.at)?;
        self.at += 4;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn unicode(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        let raw = self.take(n.checked_mul(2)?)?;
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
    }
    /// A key: a length (0 means four bytes) and that many bytes.
    fn key(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        let n = if n == 0 { 4 } else { n };
        Some(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > 16 {
            return None;
        }
        let kind: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(match &kind {
            b"long" => Value::Long(self.u32()? as i32),
            b"doub" => Value::Double(f64::from_be_bytes(self.take(8)?.try_into().ok()?)),
            b"bool" => Value::Bool(self.take(1)?[0] != 0),
            b"TEXT" => Value::Text(self.unicode()?),
            b"enum" => {
                let (t, v) = (self.key()?, self.key()?);
                Value::Enum(t, v)
            }
            b"UntF" => {
                self.take(4)?;
                Value::Unit(f64::from_be_bytes(self.take(8)?.try_into().ok()?))
            }
            b"Objc" | b"GlbO" => self.object(depth)?,
            b"VlLs" => {
                let n = self.u32()? as usize;
                let mut items = Vec::new();
                for _ in 0..n.min(4096) {
                    items.push(self.value(depth + 1)?);
                }
                Value::List(items)
            }
            _ => return None,
        })
    }
    fn object(&mut self, depth: usize) -> Option<Value> {
        self.unicode()?;
        let class = self.key()?;
        let n = self.u32()? as usize;
        let mut items = Vec::new();
        for _ in 0..n.min(4096) {
            let k = self.key()?;
            items.push((k, self.value(depth + 1)?));
        }
        Some(Value::Object(class, items))
    }
}

/// A descriptor with the 4-byte version in front (`16`), as image resources and metadata hold it.
pub(super) fn parse_versioned(b: &[u8]) -> Option<Value> {
    if be32(b, 0)? != 16 {
        return None;
    }
    let mut r = Reader { b, at: 4 };
    r.object(0)
}

/// The image resource `id` of the resource block `res` (8BIM, id, pascal name, size, data).
pub(super) fn resource(res: &[u8], id: u16) -> Option<&[u8]> {
    let mut p = 0;
    while p + 12 <= res.len() {
        if res.get(p..p + 4)? != b"8BIM" {
            return None;
        }
        let rid = be16(res, p + 4)?;
        let name = *res.get(p + 6)? as usize;
        let mut q = p + 6 + (1 + name).div_ceil(2) * 2;
        let size = be32(res, q)? as usize;
        q += 4;
        if rid == id {
            return res.get(q..q + size);
        }
        p = q + size.checked_add(size & 1)?;
    }
    None
}

/// The data of metadata item `key` in an `shmd` block (count, then 8BIM key copy pad len data).
pub(super) fn metadata<'a>(shmd: &'a [u8], key: &[u8; 4]) -> Option<&'a [u8]> {
    let count = be32(shmd, 0)? as usize;
    let mut p = 4;
    for _ in 0..count.min(64) {
        if shmd.get(p..p + 4)? != b"8BIM" {
            return None;
        }
        let k = shmd.get(p + 4..p + 8)?;
        let len = be32(shmd, p + 12)? as usize;
        let data = shmd.get(p + 16..p + 16 + len)?;
        if k == key {
            return Some(data);
        }
        p += 16 + len + (len & 1);
    }
    None
}
