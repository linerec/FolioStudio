//! Cross references of a topic: the "See Target" and "See also Target" lines (index-c6, measured in InDesign 2025, scenarios
//! `index6-see-*`, rules `next/oracle/rules/index-c6.ko.md`). A cross reference is a `CrossReference` element inside its `Topic`
//! with the kind (`CrossReferenceType`), the words of a custom kind (`CustomTypeString`) and the topic it points to
//! (`ReferencedTopic`). InDesign's DOM keeps `CustomCrossReference` as `CustomCrossReferenceBefore`.
use super::invalid;
use crate::{EditError, Work, xml::escape};

/// The kinds InDesign writes for `CrossReferenceType`.
pub(super) const KINDS: &[&str] = &["See", "SeeAlso", "SeeHerein", "SeeAlsoHerein", "SeeOrAlsoBracket", "CustomCrossReferenceBefore", "CustomCrossReferenceAfter"];

/// Adds a cross reference of `kind` to `topic`, pointing at the topic `referenced`; `custom` is the text of the two custom kinds
/// (`CustomCrossReference` is the same as `CustomCrossReferenceBefore`). The new cross reference's id is returned in `created`.
pub(crate) fn add(w: &mut Work<'_>, topic: &String, referenced: &String, kind: &String, custom: &str) -> Result<(), EditError> {
    let kind = if kind == "CustomCrossReference" { "CustomCrossReferenceBefore" } else { kind.as_str() };
    if !KINDS.contains(&kind) {
        return Err(EditError::new("unsupported", format!("cross reference kind {kind}")));
    }
    let custom_kind = kind.starts_with("Custom");
    if custom_kind && custom.trim().is_empty() {
        return Err(invalid("a custom cross reference needs its text"));
    }
    if custom.chars().any(char::is_control) {
        return Err(invalid("the text of a cross reference is one line"));
    }
    let (part, ix, i) = w.find(topic)?;
    let text = w.text(&part)?.to_owned();
    let t = text.as_str();
    if part != "designmap.xml" || ix.local(t, i) != "Topic" {
        return Err(invalid(format!("{topic} is not a topic")));
    }
    let (rp, rix, ri) = w.find(referenced)?;
    if rp != "designmap.xml" || rix.local(w.text(&rp)?, ri) != "Topic" {
        return Err(invalid(format!("{referenced} is not a topic")));
    }
    if topic == referenced {
        return Err(invalid("a topic cannot point to itself"));
    }
    // InDesign refuses a second cross reference of the same kind to the same topic (measured: index6-see-many); another kind is fine
    let wanted_custom = if custom_kind { custom } else { "" };
    if ix.children(i).iter().any(|&c| ix.local(t, c) == "CrossReference" && ix.value(t, c, "ReferencedTopic").as_deref() == Some(referenced.as_str())
        && ix.value(t, c, "CrossReferenceType").as_deref() == Some(kind) && ix.value(t, c, "CustomTypeString").unwrap_or_default() == wanted_custom) {
        return Err(invalid("이 주제에 같은 상호 참조가 이미 있습니다"));
    }
    let id = w.new_id();
    let raw = format!("<CrossReference Self=\"{id}\" ReferencedTopic=\"{}\" CrossReferenceType=\"{kind}\" CustomTypeString=\"{}\" />",
        escape(referenced), escape(if custom_kind { custom } else { "" }));
    let n = &ix.nodes[i];
    let line = t[..n.start].rfind('\n').map_or(0, |p| p + 1);
    let indent = if t[line..n.start].chars().all(char::is_whitespace) { &t[line..n.start] } else { "" };
    let edit = if n.empty {
        let tag = ix.local(t, i);
        (n.start..n.end, format!("{}>\n{indent}\t{raw}\n{indent}</{tag}>", t[n.start..n.open_end - 2].trim_end()))
    } else {
        // after the last cross reference or, when there is none, at the end of the topic (InDesign: measured, index6-see-nested)
        let last = ix.children(i).iter().copied().rfind(|&c| ix.local(t, c) == "CrossReference");
        match last {
            Some(c) => {
                let at = ix.nodes[c].end;
                (at..at, format!("\n{indent}\t{raw}"))
            }
            None => (n.close_start..n.close_start, format!("\t{raw}\n{indent}")),
        }
    };
    w.splice("designmap.xml", vec![edit])?;
    w.created.push(id);
    Ok(())
}

/// Removes one cross reference (by its id).
pub(crate) fn delete(w: &mut Work<'_>, id: &String) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    if part != "designmap.xml" || ix.local(t, i) != "CrossReference" {
        return Err(invalid(format!("{id} is not a cross reference")));
    }
    let n = &ix.nodes[i];
    let line = t[..n.start].rfind('\n').map_or(0, |p| p + 1);
    let from = if t[line..n.start].chars().all(char::is_whitespace) { line } else { n.start };
    let to = if t[n.end..].starts_with('\n') { n.end + 1 } else { n.end };
    w.splice(&part, vec![(from..to, String::new())])
}
