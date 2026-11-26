//! Editorial metadata in the containing story's character space.
use crate::xml::{Element, Node};
use folio_model::{EditorialNote, TrackedChange};

fn text(e: &Element<'_>) -> String {
    match e.name {
        "Content" => e.children.iter().map(|n| match n {
            Node::Text(t) => t.to_string(),
            Node::Pi("ACE", _) => "\u{fffc}".into(),
            _ => String::new(),
        }).collect(),
        "Br" => "\r".into(),
        "Note" => "\u{feff}".into(),
        "PageReference" if e.elements().all(|c| c.name == "Properties") => "\u{feff}".into(),
        "Table" => "\u{16}".into(),
        "Footnote" | "Endnote" => "\u{4}".into(),
        "TextVariableInstance" => "\u{18}".into(),
        "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "Group" | "TextFrame" => "\u{fffc}".into(),
        "Properties" | "HiddenText" => String::new(),
        "Change" if e.attr("ChangeType") == Some("DeletedText") => String::new(),
        _ => e.elements().map(text).collect(),
    }
}

pub(crate) fn read(story: &Element<'_>) -> (Vec<EditorialNote>, Vec<TrackedChange>) {
    fn visit(e: &Element<'_>, at: &mut usize, notes: &mut Vec<EditorialNote>, changes: &mut Vec<TrackedChange>) {
        let attr = |key| e.attr(key).unwrap_or_default().to_owned();
        match e.name {
            "Note" => {
                notes.push(EditorialNote {
                    at: *at, text: e.elements().map(text).collect(),
                    collapsed: e.attr("Collapsed") == Some("true"),
                    user_name: attr("UserName"), creation_date: attr("CreationDate"),
                    modification_date: attr("ModificationDate"), name: e.child("Properties")
                        .and_then(|p| p.child("Label")).and_then(|p| p.elements().find(|v| v.attr("Key") == Some("Label")))
                        .and_then(|v| v.attr("Value")).map(str::to_owned),
                });
                *at += 1;
            }
            "Change" => {
                let start = *at;
                let body: String = e.elements().map(text).collect();
                let deleted = e.attr("ChangeType") == Some("DeletedText");
                if !deleted { for child in e.elements() { visit(child, at, notes, changes); } }
                changes.push(TrackedChange {
                    start, end: *at, change_type: attr("ChangeType"), user_name: attr("UserName"),
                    date: attr("Date"), text: body,
                });
            }
            "PageReference" if e.elements().all(|c| c.name == "Properties") => *at += 1,
            "Content" | "Br" | "Table" | "Footnote" | "Endnote" | "TextVariableInstance" |
            "Rectangle" | "Oval" | "Polygon" | "GraphicLine" | "Group" | "TextFrame" => *at += text(e).chars().count(),
            "Properties" | "HiddenText" => {},
            _ => for child in e.elements() { visit(child, at, notes, changes); },
        }
    }
    let (mut notes, mut changes, mut at) = (Vec::new(), Vec::new(), 0);
    visit(story, &mut at, &mut notes, &mut changes);
    (notes, changes)
}

/// Read metadata without rewriting or normalizing the source part.
pub fn contents(source: &str, id: &str) -> Result<String, String> {
    fn find(e: &Element<'_>, id: &str) -> Option<String> {
        if e.attr("Self") == Some(id) { return Some(text(e)); }
        e.elements().find_map(|c| find(c,id))
    }
    let root=crate::xml::parse(source).map_err(|e|e.0)?;
    find(&root,id).ok_or_else(||format!("missing story {id}"))
}

pub fn metadata(source: &str, id: &str) -> Result<(Vec<EditorialNote>, Vec<TrackedChange>, bool), String> {
    fn find<'a, 'b>(e: &'b Element<'a>, id: &str) -> Option<&'b Element<'a>> {
        if e.name == "Story" && e.attr("Self") == Some(id) { return Some(e); }
        e.elements().find_map(|c| find(c, id))
    }
    let root = crate::xml::parse(source).map_err(|e| e.0)?;
    let story = find(&root, id).ok_or_else(|| format!("missing story {id}"))?;
    let (notes, changes) = read(story);
    Ok((notes, changes, story.attr("TrackChanges") == Some("true")))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn measured(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/notes/structure").join(format!("{name}.xml"));
        std::fs::read_to_string(path).unwrap()
    }
    #[test]
    fn measured_notes_and_changes_use_visible_character_offsets() {
        let (notes, changes, tracking) = metadata(&measured("01-note"), "u117").unwrap();
        assert!(!tracking); assert!(changes.is_empty());
        assert_eq!(notes.len(), 1);
        let note = &notes[0];
        assert_eq!((note.at, note.text.as_str(), note.collapsed), (2, "Memo", false));
        assert_eq!(note.user_name, "Unknown User Name");
        assert_eq!(note.creation_date, note.modification_date);
        assert!(note.creation_date.starts_with("2026-10-22T"));
        assert_eq!(note.name, None);
        assert!(metadata(&measured("02-collapsed"), "u117").unwrap().0[0].collapsed);
        let (notes, changes, tracking) = metadata(&measured("05-delete"), "u117").unwrap();
        assert!(tracking); assert!(notes.is_empty());
        assert_eq!(changes.len(), 2);
        assert_eq!((changes[0].start, changes[0].end, changes[0].text.as_str(), changes[0].change_type.as_str()), (2, 4, "xy", "InsertedText"));
        assert_eq!((changes[1].start, changes[1].end, changes[1].text.as_str(), changes[1].change_type.as_str()), (4, 4, "CD", "DeletedText"));
        assert_eq!(changes[1].user_name, "$ID/Unknown User Name");
        assert!(!changes[1].date.is_empty());
    }
}
