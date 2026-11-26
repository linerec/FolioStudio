//! Reads DOCX package parts and resolves the media relationships.

use crate::xmltree::parse;
use std::collections::HashMap;
use std::io::Read;

/// A part may inflate to this many bytes (a zip bomb must not fill the memory).
pub(super) const MAX_PART: u64 = 128 << 20;

pub(super) fn part(zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Result<Option<String>, String> {
    let file = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format!("Word 파일 안의 {name} 을(를) 읽을 수 없습니다: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_PART + 1).read_to_end(&mut bytes).map_err(|e| format!("Word 파일 안의 {name} 을(를) 읽을 수 없습니다: {e}"))?;
    if bytes.len() as u64 > MAX_PART {
        return Err(format!("Word 파일 안의 {name} 이(가) 너무 큽니다"));
    }
    String::from_utf8(bytes).map(Some).map_err(|_| format!("Word 파일 안의 {name} 이(가) UTF-8 이 아닙니다"))
}

fn part_bytes(zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let file = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format!("Word 파일 안의 {name} 을(를) 읽을 수 없습니다: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_PART + 1).read_to_end(&mut bytes).map_err(|e| format!("Word 파일 안의 {name} 을(를) 읽을 수 없습니다: {e}"))?;
    if bytes.len() as u64 > MAX_PART {
        return Err(format!("Word 파일 안의 {name} 이(가) 너무 큽니다"));
    }
    Ok(Some(bytes))
}

/// The pictures the document's relationships name (`word/_rels/document.xml.rels`, type `…/image`), by relationship id.
pub(super) fn read_pictures(zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut out = HashMap::new();
    let Some(rels) = part(zip, "word/_rels/document.xml.rels")? else { return Ok(out) };
    for rel in parse(&rels)?.elements().filter(|e| e.name == "Relationship") {
        let (Some(id), Some(target)) = (rel.attr("Id"), rel.attr("Target")) else { continue };
        if !rel.attr("Type").is_some_and(|t| t.ends_with("/image")) || rel.attr("TargetMode") == Some("External") {
            continue;
        }
        // a target is relative to `word/`, or from the package root when it starts with a slash
        let mut path: Vec<&str> = if target.starts_with('/') { Vec::new() } else { vec!["word"] };
        for piece in target.split('/') {
            match piece {
                "" | "." => {}
                ".." => {
                    path.pop();
                }
                other => path.push(other),
            }
        }
        if let Some(bytes) = part_bytes(zip, &path.join("/"))? {
            out.insert(id.to_owned(), bytes);
        }
    }
    Ok(out)
}
