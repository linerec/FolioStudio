use super::*;

pub(crate) fn run(w: &mut Work<'_>, story: &str, start: &usize, end: &usize) -> Result<(), EditError> {
    let (start, end) = (*start, *end);
    let (part, ix, i, _) = locate(w, story)?;
    let s = Story::read(w.text(&part)?, &ix, i)?;
    validate(&s, start, end)?;
    let mut paragraphs = Vec::new();
    let range = paragraph_range(&s, start, end);
    let mut at = range.start;
    while at < range.end {
        let next = s.atoms[at..range.end].iter().position(|a| a.ch == '\r')
            .map_or(range.end, |i| at + i + 1);
        let attrs = style_attrs(&s.paragraphs[s.atoms[at].p]);
        paragraphs.push((at, next, val(&attrs, "AppliedParagraphStyle").to_owned()));
        at = next;
    }
    if paragraphs.is_empty() {
        let (p, _) = inherit(&s, start);
        paragraphs.push((start, end, val(&style_attrs(&s.paragraphs[p]), "AppliedParagraphStyle").to_owned()));
    }
    let mut characters = Vec::new();
    if start == end {
        let (_, c) = inherit(&s, start);
        characters.push((start, end, val(&style_attrs(&s.characters[c]), "AppliedCharacterStyle").to_owned()));
    } else {
        let mut at = start;
        while at < end {
            let c = s.atoms[at].c;
            let next = (at + 1..end).find(|&i| s.atoms[i].c != c).unwrap_or(end);
            characters.push((at, next, val(&style_attrs(&s.characters[c]), "AppliedCharacterStyle").to_owned()));
            at = next;
        }
    }
    for (from, to, style) in paragraphs {
        apply_style(w, story, from, to, &style, true, false)?;
    }
    for (from, to, style) in characters {
        apply_style(w, story, from, to, &style, false, true)?;
    }
    Ok(())
}
