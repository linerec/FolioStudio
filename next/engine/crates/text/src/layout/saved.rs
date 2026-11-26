//! Laying a story out with the lines InDesign saved (`Story::saved`, INDD documents): each
//! paragraph is broken exactly where InDesign broke it and each line goes to the frame and
//! column InDesign put it in; justification and glyph positions within the line stay this
//! layout's own. A story whose saved lines do not fit its text or its frames is composed as
//! before and counted as a fallback.
//!
//! InDesign's baselines and line areas (x and width of each segment in the frame's text
//! area) are kept too, so a saved line sits where InDesign set it whatever this layout's
//! vertical spacing, wraps and vertical justification would give (`FOLIO_SAVED_BASELINES=0`
//! keeps only the breaks and columns, for comparisons). A line a text wrap splits is set as
//! one line per segment on the same baseline; the segments of a drop cap's lines are not
//! lines of their own here (this layout sets no drop cap): such a line takes the area of
//! all its segments.

use folio_model::{SavedComposition, Story};

/// A line of the plan: where it starts (story index) and where it goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ForcedLine {
    pub start: usize,
    /// Frame and column index in the story's flow; `None`: InDesign left it overset.
    pub target: Option<(usize, usize)>,
    /// A later segment of a line a text wrap splits (the right part of a both-sides row).
    pub segment: bool,
    /// InDesign's baseline below the top of the frame's text area (vertical justification
    /// included), and the line's left and right there.
    pub baseline: Option<f64>,
    pub area: Option<(f64, f64)>,
}

/// Whether saved lines also keep InDesign's baselines (`FOLIO_SAVED_BASELINES=0`: only its
/// breaks and columns, for comparisons).
pub(super) fn use_baselines() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("FOLIO_SAVED_BASELINES").map_or(true, |v| v != "0"))
}

/// The saved lines of a story, per paragraph (all lines starting in the paragraph, the
/// first at its start).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct StoryPlan {
    pub paras: Vec<Vec<ForcedLine>>,
}

/// What became of a story's saved lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SavedUse {
    /// The story has none (IDML, or a story the reader gave none for).
    #[default]
    None,
    /// Laid out with them.
    Used,
    /// Composed by layout instead, and why.
    Fallback(String),
}

/// The plan for `story` in its flow: `frames` are the frames of its chain (id, number of
/// columns). `Ok(None)` without saved lines.
pub(super) fn plan(story: &Story, frames: &[(&str, usize)]) -> Result<Option<StoryPlan>, String> {
    let lines = match &story.saved {
        SavedComposition::None => return Ok(None),
        SavedComposition::Unusable(why) => return Err(why.clone()),
        SavedComposition::Lines(l) => l,
    };
    // Story index range [start, end] of every paragraph (end: its return).
    let mut ranges = Vec::with_capacity(story.paragraphs.len());
    let mut at = 0usize;
    for p in &story.paragraphs {
        let n: usize = p.runs.iter().map(|r| r.text.chars().count()).sum();
        ranges.push((at, at + n));
        at += n + 1;
    }
    let mut paras: Vec<Vec<ForcedLine>> = vec![Vec::new(); ranges.len()];
    let mut last_target: Option<(usize, usize)> = None;
    let mut overset = false;
    let mut pi = 0usize;
    for l in lines {
        let target = match &l.frame {
            None => None,
            Some(id) => {
                let Some(fi) = frames.iter().position(|f| f.0 == id.as_str()) else {
                    return Err(format!("frame {id} is not in the story's thread"));
                };
                let ci = l.column as usize;
                if ci >= frames[fi].1 {
                    return Err(format!("frame {id} has no column {ci}"));
                }
                Some((fi, ci))
            }
        };
        match (target, last_target) {
            (Some(t), Some(prev)) if t < prev => return Err("lines go back to an earlier column".into()),
            (Some(_), _) if overset => return Err("a shown line follows overset lines".into()),
            (None, _) => overset = true,
            _ => {}
        }
        if target.is_some() {
            last_target = target;
        }
        while pi < ranges.len() && l.start > ranges[pi].1 {
            pi += 1;
        }
        let Some(&(ps, _)) = ranges.get(pi) else {
            return Err("a saved line starts after the text".into());
        };
        let para = &mut paras[pi];
        if para.is_empty() && l.start != ps {
            return Err("a paragraph does not start a saved line".into());
        }
        if para.last().is_some_and(|p| p.start >= l.start) {
            return Err("saved lines out of order".into());
        }
        let baseline = target.and(l.baseline);
        let area = |g: &folio_model::SavedSegment| (g.x, g.x + g.width);
        // A drop cap's lines: the first segment is the drop cap itself.
        let drop_cap = para.len() < story.paragraphs[pi].attrs.drop_cap_lines as usize;
        let splits: Vec<&folio_model::SavedSegment> = l
            .segments
            .iter()
            .skip(1)
            .filter(|g| g.start.is_some_and(|s| s > l.start && s <= ranges[pi].1))
            .collect();
        let whole = l.segments.iter().map(area).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
        // The text of a line whose later segments hold none is in its first segment.
        let own = if drop_cap { whole } else { l.segments.first().map(area) };
        para.push(ForcedLine { start: l.start, target, segment: false, baseline, area: own });
        if !drop_cap {
            for g in splits {
                let s = g.start.unwrap_or(l.start);
                if para.last().is_some_and(|p| p.start < s) {
                    para.push(ForcedLine { start: s, target, segment: true, baseline, area: Some(area(g)) });
                }
            }
        }
    }
    // Paragraphs after the last saved line: the story ended there (overset), or lines are
    // missing.
    if let Some(i) = paras.iter().position(Vec::is_empty) {
        if !overset {
            return Err(format!("paragraph {i} has no saved line"));
        }
        for p in &mut paras[i..] {
            if p.is_empty() {
                p.push(ForcedLine { start: usize::MAX, target: None, segment: false, baseline: None, area: None });
            }
        }
    }
    // InDesign's baselines go down a column; geometry that does not (a record read
    // wrongly) is not used: the story keeps its breaks and columns only.
    let mut last: std::collections::HashMap<(usize, usize), f64> = std::collections::HashMap::new();
    let plausible = paras.iter().flatten().filter(|l| !l.segment).all(|l| match (l.target, l.baseline, l.area) {
        (Some(t), Some(y), Some((x0, x1))) => {
            let ok = [y, x0, x1].iter().all(|v| v.is_finite() && v.abs() < 1.0e5) && x1 >= x0 && last.get(&t).is_none_or(|&p| y >= p - 0.01);
            last.insert(t, y);
            ok
        }
        _ => true,
    });
    if !plausible {
        for l in paras.iter_mut().flatten() {
            l.baseline = None;
            l.area = None;
        }
    }
    Ok(Some(StoryPlan { paras }))
}

/// The lines of a part of a paragraph whose characters have story indices `index` (one per
/// character and one for its end): those starting in it, when the first starts it.
pub(super) fn part<'p>(forced: &'p [ForcedLine], index: &[usize]) -> Option<&'p [ForcedLine]> {
    let (&first, &last) = (index.first()?, index.last()?);
    let from = forced.iter().position(|l| l.start >= first)?;
    let to = forced.iter().position(|l| l.start > last).unwrap_or(forced.len());
    let lines = &forced[from..to];
    (lines.first()?.start == first || lines.first()?.target.is_none()).then_some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use folio_model::{Paragraph, SavedLine, TextRun};

    fn story(texts: &[&str], lines: Vec<SavedLine>) -> Story {
        let paragraphs = texts
            .iter()
            .map(|t| Paragraph { runs: vec![TextRun { text: (*t).into(), ..TextRun::default() }], ..Paragraph::default() })
            .collect();
        Story { id: "s".into(), paragraphs, saved: SavedComposition::Lines(lines), ..Story::default() }
    }

    fn line(start: usize, frame: Option<&str>, column: u16) -> SavedLine {
        let segments = vec![folio_model::SavedSegment { x: 0.0, width: 100.0, start: Some(start) }];
        SavedLine { start, frame: frame.map(str::to_owned), column, baseline: Some(7.0), segments }
    }

    #[test]
    fn plans_paragraphs_and_checks_frames() {
        let flow = [("a", 2usize), ("b", 1)];
        // "abcdef" ¶ "gh" ¶ "": lines at 0, 3 (column 1), 7 (frame b), 10 (overset).
        let s = story(&["abcdef", "gh", ""], vec![line(0, Some("a"), 0), line(3, Some("a"), 1), line(7, Some("b"), 0), line(10, None, 0)]);
        let p = plan(&s, &flow).unwrap().unwrap();
        let t: Vec<Vec<(usize, Option<(usize, usize)>)>> = p.paras.iter().map(|v| v.iter().map(|l| (l.start, l.target)).collect()).collect();
        assert_eq!(t, vec![vec![(0, Some((0, 0))), (3, Some((0, 1)))], vec![(7, Some((1, 0)))], vec![(10, None)]]);
        // A frame outside the thread, a column the frame lacks, a paragraph without a line.
        assert!(plan(&story(&["ab"], vec![line(0, Some("x"), 0)]), &flow).is_err());
        assert!(plan(&story(&["ab"], vec![line(0, Some("b"), 1)]), &flow).is_err());
        assert!(plan(&story(&["ab", "c"], vec![line(0, Some("a"), 0)]), &flow).is_err());
        // Lines going back to an earlier column.
        assert!(plan(&story(&["abc", "d"], vec![line(0, Some("b"), 0), line(4, Some("a"), 0)]), &flow).is_err());
        assert_eq!(plan(&Story::default(), &flow), Ok(None));
        // Part of a paragraph: the lines starting in it.
        let f = &p.paras[0];
        assert_eq!(part(f, &[3, 4, 5, 6]).map(<[ForcedLine]>::len), Some(1));
        assert_eq!(part(f, &[4, 5, 6]), None);
    }
}
