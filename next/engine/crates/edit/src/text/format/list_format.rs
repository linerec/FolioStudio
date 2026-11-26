//! Toggle list formatting and numbering restarts.

use super::*;

/// Type ▸ Bulleted & Numbered Lists ▸ Apply/Remove Bullets and Apply/Remove Numbers on the paragraphs of the
/// range: when every one of them already is that list it is taken off, else all become that list (the
/// type of the others is replaced). A paragraph that was no list keeps where its first line starts and gets
/// the 18 pt hanging indent (left indent = left + first line + 18, first line −18); taking the list off gives
/// that back (left + first line, first line 0); a paragraph that already was a list keeps its indents. The
/// first numbered paragraph restarts the numbering unless a numbered paragraph precedes it (oracle:
/// text-format-lists).
pub(crate) fn toggle_list(w: &mut Work<'_>, id: &str, start: usize, end: usize, numbers: bool) -> Result<(), EditError> {
    let kind = if numbers { "NUMBERED_LIST" } else { "BULLET_LIST" };
    let (part, ix, i, sid) = locate(w, id)?;
    let paragraphs: Vec<(usize, usize)> = {
        let s = Story::read(w.text(&part)?, &ix, i)?;
        validate(&s, start, end)?;
        let range = paragraph_range(&s, start, end);
        let mut out = Vec::new();
        let mut at = range.start;
        for k in range.clone() {
            if s.atoms[k].ch == '\r' || k + 1 == range.end {
                out.push((at, (k + 1).min(s.atoms.len())));
                at = k + 1;
            }
        }
        if out.is_empty() {
            out.push((start, end));
        }
        out
    };
    let mut state = Vec::new();
    for &(a, b) in &paragraphs {
        let row = info(w, &sid, a, b.max(a + 1).min(a + 1))?;
        state.push((
            row["bulletsAndNumberingListType"].as_str().unwrap_or("").to_owned(),
            row["leftIndent"].as_f64().unwrap_or(0.0),
            row["firstLineIndent"].as_f64().unwrap_or(0.0),
        ));
    }
    let remove = state.iter().all(|(t, _, _)| t == kind);
    for (&(a, b), (t, left, first)) in paragraphs.iter().zip(&state) {
        let mut props: BTreeMap<String, Value> = BTreeMap::new();
        props.insert("bulletsAndNumberingListType".into(), if remove { "NO_LIST".into() } else { kind.into() });
        if remove {
            props.insert("leftIndent".into(), (left + first).into());
            props.insert("firstLineIndent".into(), 0.into());
        } else if t.is_empty() || t == "NO_LIST" {
            props.insert("leftIndent".into(), (left + first + 18.0).into());
            props.insert("firstLineIndent".into(), (-18).into());
        }
        format(w, &sid, a, b, &props)?;
    }
    if numbers && !remove {
        let first = paragraphs[0].0;
        let continues = first > 0 && info(w, &sid, first - 1, first)?["bulletsAndNumberingListType"].as_str() == Some("NUMBERED_LIST");
        if !continues {
            format(w, &sid, first, first + 1, &BTreeMap::from([("numberingContinue".to_owned(), Value::Bool(false))]))?;
        }
    }
    Ok(())
}

/// Type ▸ Bulleted & Numbered Lists ▸ Restart/Continue Numbering: toggles NumberingContinue of the one numbered
/// paragraph the selection is in (InDesign disables it for another list or several paragraphs; oracle:
/// text-format-lists).
pub(crate) fn toggle_numbering_restart(w: &mut Work<'_>, id: &str, start: usize, end: usize) -> Result<(), EditError> {
    let (part, ix, i, sid) = locate(w, id)?;
    let range = {
        let s = Story::read(w.text(&part)?, &ix, i)?;
        validate(&s, start, end)?;
        let range = paragraph_range(&s, start, end);
        if s.atoms[range.start..range.end.saturating_sub(1).max(range.start)].iter().any(|a| a.ch == '\r') {
            return Err(EditError::new("invalid", "Restart/Continue Numbering works on one paragraph"));
        }
        range
    };
    let row = info(w, &sid, range.start, range.start + 1)?;
    if row["bulletsAndNumberingListType"].as_str() != Some("NUMBERED_LIST") {
        return Err(EditError::new("invalid", "the paragraph is not numbered"));
    }
    let continuing = row["numberingContinue"].as_bool().unwrap_or(true);
    format(w, &sid, range.start, range.end, &BTreeMap::from([("numberingContinue".to_owned(), Value::Bool(!continuing))]))
}
