//! Text style identifiers and reference attributes shared by readers and editors.

pub fn renamed_id(id: &str, name: &str) -> Option<String> {
    if name.trim().is_empty() || name.chars().any(|c| c.is_control() || c == '/') {
        return None;
    }
    let (group, _) = id.rsplit_once('/')?;
    Some(format!("{group}/{name}"))
}

pub fn is_reference_attribute(key: &str) -> bool {
    key == "NextStyle" || key.ends_with("ParagraphStyle") || key.ends_with("CharacterStyle")
}
