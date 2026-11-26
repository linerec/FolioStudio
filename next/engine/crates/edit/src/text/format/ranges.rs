use super::*;

pub(super) fn has_override(attrs: &Attrs, applied: &str) -> bool {
    attrs.keys().any(|key| key != applied)
}

impl crate::Editor {
    pub fn style_override_ranges(&mut self, id: &str) -> Result<Vec<(usize, usize, bool, bool)>, EditError> {
        let mut w = Work { ed: self, splices: vec![], recompose: vec![], created: vec![] };
        let (part, ix, i, _) = locate(&mut w, id)?;
        let s = changes::visible(Story::read_mode(w.text(&part)?, &ix, i, true)?)?;
        let paragraphs: Vec<_> = s.paragraphs.iter().map(|p| has_override(&style_attrs(p), "AppliedParagraphStyle")).collect();
        let characters: Vec<_> = s.characters.iter().map(|c| has_override(&style_attrs(c), "AppliedCharacterStyle")).collect();
        let mut ranges: Vec<(usize, usize, bool, bool)> = vec![];
        for (at, a) in s.atoms.iter().enumerate() {
            let flags = (paragraphs[a.p], characters[a.c]);
            if !flags.0 && !flags.1 { continue; }
            if let Some(last) = ranges.last_mut().filter(|last| last.1 == at && (last.2,last.3) == flags) {
                last.1 = at + 1;
            } else { ranges.push((at, at + 1, flags.0, flags.1)); }
        }
        Ok(ranges)
    }

    pub(crate) fn range_has_variable(&mut self, id: &str, start: usize, end: usize, variables: &[String]) -> Result<bool, EditError> {
        let mut w = Work { ed: self, splices: vec![], recompose: vec![], created: vec![] };
        let (part, ix, i, _) = locate(&mut w, id)?;
        let s = changes::visible(Story::read_mode(w.text(&part)?, &ix, i, true)?)?;
        validate(&s, start, end)?;
        for a in &s.atoms[start..end] {
            if a.content || !a.raw.starts_with("<TextVariableInstance") { continue; }
            let ix = Index::scan(&a.raw).map_err(|e| EditError::new("xml", e))?;
            if ix.value(&a.raw, 0, "AssociatedTextVariable").is_some_and(|v| variables.contains(&v)) { return Ok(true); }
        }
        Ok(false)
    }
}
