//! Text variables (oracle `variables`: the parent's header frames hold a Running Header by paragraph style, one by character style, the
//! page number, the Last Page Number, the Chapter Number and a custom text; the pages hold the styled paragraphs).
//!
//! An instance of a variable is one character of the story and shows what the variable gives for the page it is laid out on. InDesign
//! stores with each instance the result it had when the file was saved (`ResultText`); the parent shows the placeholder
//! ("<By Head>") for a running header. What this works out, per document page, for the master frames laid out for that page
//! (`layout_master_page_numbers`) and for text on a page:
//! - Last Page Number: the last page of the page's section (scope `SectionScope`) or of the document, in `TextBefore`/`TextAfter`;
//! - Chapter Number: the document's chapter number;
//! - Running Header (paragraph style / character style): the text of the first (or last, `SearchStrategy`) paragraph or run in that
//!   style on the page, with the case changed and the end punctuation dropped as the variable says; a page without one shows what
//!   the page before showed (measured: the header of page 3 is page 2's);
//!   The other kinds (custom text, file name, dates, …) keep the stored result.
use super::*;
use folio_model::{Page, VariableDef};

/// A paragraph that starts on a page: its style, its text and its runs' character styles.
#[derive(Clone, Debug, Default)]
pub struct PageParagraph {
    pub style: String,
    pub runs: Vec<PageRun>,
}

/// A run of a [`PageParagraph`]: its character style and text; the text of an instance of a text variable is its stored result, and
/// `variable` names the variable.
#[derive(Clone, Debug, Default)]
pub struct PageRun {
    pub style: String,
    pub text: String,
    pub variable: Option<String>,
}

impl PageRun {
    /// What the run reads as for the variable `own` (oracle var-insert-two): an instance of that variable itself reads as `own_text`, one
    /// of a variable the page works out without text (`statics`) as its value on the page, any other as its stored result.
    fn text_for<'a>(&'a self, own: &str, own_text: &'a str, statics: &'a BTreeMap<String, String>) -> &'a str {
        match &self.variable {
            Some(v) if v == own => own_text,
            Some(v) => statics.get(v).map_or(self.text.as_str(), String::as_str),
            None => &self.text,
        }
    }
}

impl PageParagraph {
    fn text_for(&self, own: &str, own_text: &str, statics: &BTreeMap<String, String>) -> String {
        self.runs.iter().map(|r| r.text_for(own, own_text, statics)).collect()
    }
}

fn page_run(r: &TextRun) -> PageRun {
    match (&r.variable, &r.named) {
        (Some(TextVariable::Named), Some(n)) => PageRun { style: r.attrs.character_style.clone(), text: n.result.clone(), variable: Some(n.id.clone()) },
        _ => PageRun { style: r.attrs.character_style.clone(), text: r.text.clone(), variable: None },
    }
}

/// The paragraphs that start on each document page (indexed by `Page::index`), in reading order: the frames of the page back to front,
/// the story's paragraphs in order. A paragraph starts where a line begins at its first character.
pub fn page_paragraphs(doc: &Document, frames: &DocumentFrames<'_>, layout: &DocumentLayout) -> Vec<Vec<PageParagraph>> {
    let pages = doc.spreads.iter().flat_map(|s| &s.pages).map(|p| p.index + 1).max().unwrap_or(0);
    let mut out: Vec<Vec<PageParagraph>> = vec![Vec::new(); pages];
    // paragraph start offsets of the stories met
    let mut starts: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut listed: HashSet<(usize, &str, usize)> = HashSet::new();
    let mut refs: Vec<&FrameRef<'_>> = frames.frames.values().collect();
    refs.sort_by_key(|f| f.z);
    for f in refs {
        let (Some(si), Some(pis)) = (f.spread, f.page_in_spread) else { continue };
        let Some(page) = doc.spreads.get(si).and_then(|s| s.pages.get(pis)) else { continue };
        let Some(story) = doc.stories.get(&f.frame.story) else { continue };
        let Some(text) = layout.frames.get(f.item.id.as_str()) else { continue };
        let offs = starts.entry(story.id.as_str()).or_insert_with(|| {
            let mut at = 0;
            story
                .paragraphs
                .iter()
                .map(|p| {
                    let here = at;
                    at += p.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() + 1;
                    here
                })
                .collect()
        });
        for line in text.lines.iter().filter(|l| l.cell.is_none() && l.table.is_none()) {
            let Ok(k) = offs.binary_search(&line.start) else { continue };
            if !listed.insert((page.index, story.id.as_str(), k)) {
                continue;
            }
            let p = &story.paragraphs[k];
            out[page.index].push(PageParagraph { style: p.style.clone(), runs: p.runs.iter().map(page_run).collect() });
        }
    }
    out
}

/// `how` is IDML's `ChangeCase`: "None" or "Uppercase" (measured), "Lowercase", "Titlecase", "Sentencecase" (by the pattern).
fn change_case(text: &str, how: &str) -> String {
    let how = how.to_ascii_lowercase();
    match [("upper", 0), ("lower", 1), ("title", 2), ("sentence", 3)].into_iter().find(|(p, _)| how.starts_with(p)).map(|(_, k)| k) {
        Some(0) => text.to_uppercase(),
        Some(1) => text.to_lowercase(),
        Some(2) => {
            let mut out = String::with_capacity(text.len());
            let mut start = true;
            for c in text.chars() {
                if start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                start = !c.is_alphanumeric() && c != '\'';
            }
            out
        }
        Some(_) => {
            let mut chars = text.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect()).unwrap_or_default()
        }
        _ => text.to_owned(),
    }
}

/// What a match-style variable shows for `found`: the case changed, the end punctuation dropped, the text before and after.
fn shown(def: &VariableDef, found: &str) -> String {
    let mut text = change_case(found.trim_end_matches(['\r', '\n']), &def.change_case);
    if def.delete_end_punctuation {
        text = text.trim_end_matches(['.', ',', ':', ';', '!', '?', '。', '、']).to_owned();
    }
    format!("{}{}{}", def.text_before, text, def.text_after)
}

/// The text of the paragraph (or run) in the style `style` (a style's name as paragraphs and runs carry it) on a page, if the page
/// has one.
fn found(def: &VariableDef, style: &str, paragraphs: &[PageParagraph], statics: &BTreeMap<String, String>, own_text: &str) -> Option<String> {
    let last = def.search == "LastOnPage";
    let mut hits: Vec<String> = Vec::new();
    match def.kind.as_str() {
        "MatchParagraphStyleType" => hits.extend(paragraphs.iter().filter(|p| p.style == style).map(|p| p.text_for(&def.id, own_text, statics))),
        "MatchCharacterStyleType" => {
            let want = style.to_owned();
            for p in paragraphs {
                // contiguous runs in the style are one piece of text
                let mut piece = String::new();
                for r in &p.runs {
                    if r.style == want {
                        piece.push_str(r.text_for(&def.id, own_text, statics));
                    } else if !piece.is_empty() {
                        hits.push(std::mem::take(&mut piece));
                    }
                }
                if !piece.is_empty() {
                    hits.push(piece);
                }
            }
        }
        _ => {}
    }
    if last { hits.pop() } else { hits.into_iter().next() }.filter(|t| !t.is_empty())
}

/// The results InDesign stores with the instances of variables on document pages (`ResultText`) that differ from what the instances
/// show now: `(the instance's Self, its text)`. An instance on a master page keeps its placeholder ("<By Head>").
pub(super) fn results(ctx: &Ctx<'_, '_>, frames: &DocumentFrames<'_>, layout: &DocumentLayout) -> Vec<(String, String)> {
    // what an instance stores: its own variable reads as nothing inside the text it is computed from (oracle var-insert-two)
    let values = values(ctx, frames, layout, false);
    let mut out = Vec::new();
    for story in ctx.doc.stories.values() {
        let named: Vec<&folio_model::NamedVariable> = story.paragraphs.iter().flat_map(|p| &p.runs).filter_map(|r| r.named.as_deref()).collect();
        if named.is_empty() {
            continue;
        }
        let Some(page) = frames.chain(&story.id).first().and_then(|id| frames.frame(id)).and_then(|f| ctx.page_of(f)) else { continue };
        let Some(vars) = values.get(page.index) else { continue };
        let own = ctx.frame_variables(&story.id, frames);
        for n in named {
            if let Some(v) = own.get(&n.id).or_else(|| vars.get(&n.id)).filter(|v| **v != n.result) {
                out.push((n.instance.clone(), v.clone()));
            }
        }
    }
    out
}

/// What the document's text variables show on each page (by `Page::index`; the key is the variable's `Self`), page by page: a
/// running header carries on from the page before where a page has no text in its style.
pub(super) fn page_values(ctx: &Ctx<'_, '_>, frames: &DocumentFrames<'_>, layout: &DocumentLayout) -> Vec<BTreeMap<String, String>> {
    values(ctx, frames, layout, true)
}

/// The values per page; `display`: an instance of a variable inside the text the variable reads shows the value the variable had on the
/// page before (what a running header in the parent shows, oracle var-insert-two: "4DeltBetaa Ending"), else it reads as nothing (what
/// the instance itself stores: "4Delta Ending").
fn values(ctx: &Ctx<'_, '_>, frames: &DocumentFrames<'_>, layout: &DocumentLayout, display: bool) -> Vec<BTreeMap<String, String>> {
    let paragraphs = page_paragraphs(ctx.doc, frames, layout);
    let mut by_page: Vec<&Page> = ctx.doc.spreads.iter().flat_map(|s| &s.pages).collect();
    by_page.sort_by_key(|p| p.index);
    let mut out: Vec<BTreeMap<String, String>> = vec![BTreeMap::new(); by_page.last().map_or(0, |p| p.index + 1)];
    let mut before: Option<BTreeMap<String, String>> = None;
    for p in by_page {
        let vars = ctx.page_variables(p, paragraphs.get(p.index).map(Vec::as_slice).unwrap_or(&[]), before.as_ref(), display);
        before = Some(vars.clone());
        out[p.index] = vars;
    }
    out
}

impl<'c, 'a> Ctx<'c, 'a> {
    /// The variables that do not depend on what is on the page.
    pub(super) fn static_variables(&self, page: Option<&Page>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for d in self.doc.text_variables.values() {
            let value = match d.kind.as_str() {
                "LastPageNumberType" => {
                    let Some(page) = page else { continue };
                    let last = if d.scope == "DocumentScope" || page.section_last.is_empty() {
                        self.doc.spreads.iter().flat_map(|s| &s.pages).max_by_key(|p| p.index).map(|p| p.name.clone()).unwrap_or_default()
                    } else {
                        page.section_last.clone()
                    };
                    format!("{}{}{}", d.text_before, last, d.text_after)
                }
                "ChapterNumberType" if !self.doc.chapter_number.is_empty() => format!("{}{}{}", d.text_before, self.doc.chapter_number, d.text_after),
                "CreationDateType" | "ModificationDateType" | "OutputDateType" => match filevars::date_value(d, &self.doc.facts) {
                    Some(v) => v,
                    None => continue,
                },
                "FileNameType" => match filevars::file_name_value(d, &self.doc.facts) {
                    Some(v) => v,
                    None => continue,
                },
                _ => continue,
            };
            out.insert(d.id.clone(), value);
        }
        out
    }

    /// The variables on one document page: the static ones and the running headers of the page, `before` being what the page
    /// before showed (a page without a match shows that).
    pub(super) fn page_variables(&self, page: &Page, paragraphs: &[PageParagraph], before: Option<&BTreeMap<String, String>>, display: bool) -> BTreeMap<String, String> {
        let statics = self.static_variables(Some(page));
        let mut out = statics.clone();
        for d in self.doc.text_variables.values().filter(|d| matches!(d.kind.as_str(), "MatchParagraphStyleType" | "MatchCharacterStyleType")) {
            let style = self.style_name(&d.style);
            let own = if display { before.and_then(|b| b.get(&d.id)).map_or("", String::as_str) } else { "" };
            let value = found(d, &style, paragraphs, &statics, own).map(|t| shown(d, &t)).or_else(|| before.and_then(|b| b.get(&d.id).cloned()));
            if let Some(v) = value {
                out.insert(d.id.clone(), v);
            }
        }
        out
    }

    /// The variables that depend on the text frame a story starts in (Image Name: the graphics its frame meets).
    pub(super) fn frame_variables(&self, story: &str, frames: &DocumentFrames<'_>) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let Some(frame) = frames.chain(story).first().and_then(|id| frames.frame(id)) else { return out };
        for d in self.doc.text_variables.values().filter(|d| d.kind == "LiveCaptionType") {
            if let Some(v) = filevars::image_name_value(d, self.doc, frame) {
                out.insert(d.id.clone(), v);
            }
        }
        out
    }

    /// A style's name as paragraphs and runs carry it (`Name` without the `$ID/` marker of a built-in), from its `Self`.
    fn style_name(&self, id: &str) -> String {
        let s = &self.doc.styles;
        let entry = s.paragraph.iter().chain(&s.character).find(|e| e.id == id);
        let name = entry.map(|e| e.name.as_str()).unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id));
        name.strip_prefix("$ID/").unwrap_or(name).to_owned()
    }

    /// The page of a frame (document pages only).
    pub(super) fn page_of(&self, f: &FrameRef<'_>) -> Option<&'c Page> {
        let (si, pis) = (f.spread?, f.page_in_spread?);
        self.doc.spreads.get(si)?.pages.get(pis)
    }
}
