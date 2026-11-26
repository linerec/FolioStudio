//! Stories (Stories/*.xml) into paragraphs and runs with resolved attributes.
//!
//! A paragraph runs until `<Br/>` (paragraph ranges do not end paragraphs). U+2028 is a
//! forced line break (`\n`). `<?ACE 18?>` (auto page number) becomes a "#" run marked
//! `TextVariable::PageNumber`; `<?ACE 7?>` / `<?ACE 8?>` keep InDesign's U+0007 / U+0008. `<?ACE 3?>` (End Nested Style Character) takes
//! no room and is no text: its place is `ParagraphAttrs::end_nested_marks`; a break character's kind is `ends_with_break`.
//! Page items inside a range are anchored objects: U+FFFC in the text plus the item in
//! `TextRun::inline`. A table is U+0016 in the text plus the table in `TextRun::tables`
//! (cells read like stories, styles resolved: `tables`). A footnote is its reference
//! marker, a "#" run marked `FootnoteNumber` carrying the footnote's paragraphs; `<?ACE 4?>`
//! in a footnote is its number. An endnote reference is a "#" run marked `EndnoteNumber`
//! and so is `<?ACE 4?>` inside an endnote story's `EndnoteRange` (numbered in endnote
//! order once all stories are read: `number_endnotes`). Editorial notes keep an invisible marker;
//! hyperlink, XML and tracked-change wrappers are transparent (deleted tracked text is
//! dropped).

use crate::color::Colors;
use crate::items::{ItemReader, is_item};
use crate::saved::{Sidecar, UnitTrack};
use crate::styles::{CharState, ListRaw, ParaBase, StyleSheet, apply_char, apply_para};
use crate::warn::Warnings;
use crate::xml::{Element, Node};
use folio_model::{CharacterAttrs, ListKind, Note, Paragraph, ParagraphAttrs, SavedComposition, Story, TextRun, TextVariable};
use std::rc::Rc;

const MAX_TEXT_DEPTH: usize = 64;

/// A ParagraphStyleRange in effect: resolved paragraph attributes and the character base.
struct ParaCtx {
    style: String,
    attrs: ParagraphAttrs,
    auto_leading: f64,
    unmodelled: Vec<&'static str>,
    chr: CharState,
    list: ListRaw,
    grep: std::sync::Arc<Vec<crate::grep::Compiled>>,
    nested: std::sync::Arc<Vec<folio_model::nested::NestedStyle>>,
}

struct Builder<'s, 'a> {
    /// Story index of the next character: characters pushed and paragraph returns, as
    /// layout counts them.
    epos: usize,
    /// The saved lines' unit count of the story's own text (`saved`).
    track: Option<UnitTrack<'s>>,
    colors: &'s Colors,
    sheet: &'s StyleSheet<'a>,
    paragraphs: Vec<Paragraph>,
    cur: Option<Paragraph>,
    para: Vec<Rc<ParaCtx>>,
    /// Character attributes in effect (innermost range last).
    chr: Vec<CharacterAttrs>,
    ranges: Vec<&'a Element<'a>>,
    grep_sources: Vec<crate::text_grep::Source<'a>>,
    last_para: Option<Rc<ParaCtx>>,
    /// Attributes of the last character, return or character range (the story-end
    /// insertion point).
    last_attrs: Option<CharacterAttrs>,
    /// `SourceEndnote` of the `EndnoteRange` being read.
    endnote: Option<String>,
    /// `ParagraphBreakType` of the character ranges being read (innermost last).
    breaks: Vec<folio_model::ParagraphStart>,
    /// The page number a page number marker in the ranges being read shows
    /// (`PageNumberType`, innermost last).
    page_kinds: Vec<TextVariable>,
    /// The paragraph return just read was a break character: where the next paragraph starts.
    pending_start: Option<folio_model::ParagraphStart>,
}

impl Builder<'_, '_> {
    /// One IDML text unit (`saved`) with its digest code; `pending` characters of it are
    /// not pushed yet.
    fn unit(&mut self, code: char, pending: usize) {
        if let Some(t) = &mut self.track {
            t.unit(code, self.epos + pending);
        }
    }

    fn para_ctx(&self) -> Option<Rc<ParaCtx>> {
        self.para.last().or(self.last_para.as_ref()).cloned()
    }

    fn attrs(&self, w: &mut Warnings) -> CharacterAttrs {
        match (self.chr.last(), self.para_ctx()) {
            (Some(a), _) => a.clone(),
            (None, Some(p)) => {
                let mut a = p.chr.finish(self.colors, w);
                a.auto_leading = p.auto_leading;
                a
            }
            (None, None) => CharacterAttrs::default(),
        }
    }

    fn paragraph(&mut self, w: &mut Warnings) -> &mut Paragraph {
        if self.cur.is_none() {
            let p = match self.para_ctx() {
                Some(ctx) => {
                    for u in &ctx.unmodelled {
                        w.add(format!("{u} not modelled"));
                    }
                    let mut attrs = ctx.attrs.clone();
                    if let Some(list) = attrs.list.as_mut() {
                        // The bullet or number takes the first character's attributes, then
                        // its character style and (bullets) font.
                        let first = self.attrs(w);
                        let style = match list.kind {
                            ListKind::Bullet => ctx.list.bullet_style.as_deref(),
                            ListKind::Numbered => ctx.list.number_style.as_deref(),
                        };
                        let mut marker = self.sheet.with_character_style(&first, style, self.colors, w);
                        if list.kind == ListKind::Bullet
                            && let Some(font) = ctx.list.bullet_font.as_deref().filter(|f| !f.is_empty() && *f != "$ID/")
                        {
                            marker.font_family = font.to_owned();
                            if let Some(st) = ctx.list.bullet_font_style.as_deref().filter(|s| !crate::props::is_none_ref(s)) {
                                marker.font_style = st.to_owned();
                            }
                        }
                        list.marker = marker;
                    }
                    Paragraph { style: ctx.style.clone(), attrs, runs: Vec::new() }
                }
                None => Paragraph::default(),
            };
            let mut p = p;
            if let Some(start) = self.pending_start.take() {
                p.attrs.start = start;
            }
            self.cur = Some(p);
        }
        self.cur.get_or_insert_with(Paragraph::default)
    }

    fn push(&mut self, text: &str, variable: Option<TextVariable>, w: &mut Warnings) {
        if text.is_empty() {
            return;
        }
        self.epos += text.chars().count();
        if let Some(ctx) = self.para_ctx().filter(|ctx| !ctx.grep.is_empty() || !ctx.nested.is_empty() || !ctx.attrs.line_styles.is_empty()) {
            self.grep_sources.push(crate::text_grep::Source {
                span: self.epos - text.chars().count()..self.epos, base: ctx.chr.clone(),
                range: self.ranges.last().copied(), rules: ctx.grep.clone(), nested: ctx.nested.clone(),
            });
        }
        let attrs = self.attrs(w);
        self.last_attrs = Some(attrs.clone());
        let p = self.paragraph(w);
        if variable.is_none()
            && let Some(last) = p.runs.last_mut()
            && last.variable.is_none()
            && last.attrs == attrs
        {
            last.text.push_str(text);
            return;
        }
        p.runs.push(TextRun { text: text.to_owned(), attrs, variable, ..TextRun::default() });
    }

    fn push_inline(&mut self, item: folio_model::PageItem, w: &mut Warnings) {
        self.push("\u{FFFC}", None, w);
        if let Some(run) = self.cur.as_mut().and_then(|p| p.runs.last_mut()) {
            run.inline.push(item);
        }
    }

    fn push_table(&mut self, table: folio_model::Table, w: &mut Warnings) {
        self.push("\u{16}", None, w);
        if let Some(run) = self.cur.as_mut().and_then(|p| p.runs.last_mut()) {
            run.tables.push(table);
        }
    }

    /// A footnote / endnote reference or number: a "#" run of its own.
    fn push_note(&mut self, variable: TextVariable, note: Option<Note>, w: &mut Warnings) {
        self.push("#", Some(variable), w);
        if let Some(run) = self.cur.as_mut().and_then(|p| p.runs.last_mut()) {
            run.note = note.map(Box::new);
        }
    }

    /// Ends the paragraph at a paragraph return carrying `attrs`.
    fn end_paragraph(&mut self, w: &mut Warnings) {
        self.epos += 1;
        let attrs = self.attrs(w);
        self.last_attrs = Some(attrs.clone());
        let mut p = match self.cur.take() {
            Some(p) => p,
            None => {
                self.paragraph(w);
                self.cur.take().unwrap_or_default()
            }
        };
        if p.runs.last().is_none_or(|r| r.attrs != attrs) {
            p.runs.push(TextRun { text: String::new(), attrs, ..TextRun::default() });
        }
        self.paragraphs.push(p);
    }

    fn finish(mut self, w: &mut Warnings) -> Vec<Paragraph> {
        match self.cur.take() {
            Some(p) => self.paragraphs.push(p),
            // After a final return (or in an empty story) InDesign still has an empty paragraph.
            None => {
                let attrs = match self.last_attrs.take() {
                    Some(a) => a,
                    None => self.attrs(w),
                };
                self.chr.push(attrs.clone());
                self.paragraph(w);
                self.chr.pop();
                if let Some(mut p) = self.cur.take() {
                    p.runs.push(TextRun { text: String::new(), attrs, ..TextRun::default() });
                    self.paragraphs.push(p);
                }
            }
        }
        crate::text_grep::apply(&mut self.paragraphs, &self.grep_sources, self.sheet, self.colors, w);
        self.paragraphs
    }
}

pub struct StoryReader<'a, 's> {
    pub sheet: &'s StyleSheet<'a>,
    pub colors: &'s Colors,
    pub items: &'s ItemReader<'a, 's>,
    /// `Self` of every hidden `Condition` (designmap `Visible="false"`).
    pub hidden_conditions: &'s std::collections::HashSet<String>,
    /// The INDD reader's saved lines (`saved`); `None` for IDML documents.
    pub saved: Option<&'s Sidecar>,
}

/// What a story part says besides the story: its endnote ranges in order.
#[derive(Default)]
pub struct StoryExtras {
    /// `SourceEndnote` of each `EndnoteRange`, in text order.
    pub endnote_ranges: Vec<String>,
    /// `Self` of each `Endnote` reference, in text order.
    pub endnote_refs: Vec<String>,
    pub is_endnote_story: bool,
}

impl<'a> StoryReader<'a, '_> {
    fn builder(&self, w: &mut Warnings) -> Builder<'_, 'a> {
        Builder {
            epos: 0,
            track: None,
            colors: self.colors,
            sheet: self.sheet,
            paragraphs: Vec::new(),
            cur: None,
            para: Vec::new(),
            chr: Vec::new(),
            ranges: Vec::new(),
            grep_sources: Vec::new(),
            // Text outside any paragraph range (and an empty story) uses the root style.
            last_para: Some(self.para_ctx(None, w)),
            last_attrs: None,
            endnote: None,
            breaks: Vec::new(),
            page_kinds: Vec::new(),
            pending_start: None,
        }
    }

    /// Reads a story part (`root` is the part's root element).
    pub fn story(&self, root: &'a Element<'a>, w: &mut Warnings) -> (Option<Story>, StoryExtras) {
        let mut extras = StoryExtras::default();
        let s = root.child("Story").unwrap_or(root);
        if s.name != "Story" {
            w.add("story part without <Story> skipped");
            return (None, extras);
        }
        let Some(id) = s.attr("Self").map(str::to_owned) else {
            w.add("story without Self id skipped");
            return (None, extras);
        };
        extras.is_endnote_story = s.attr("IsEndnoteStory") == Some("true");
        // Vertical stories are composed vertically (`folio_text::vertical`); right-to-left story direction is not.
        let vertical = s.child("StoryPreference").is_some_and(|p| p.attr("StoryOrientation") == Some("Vertical"));
        if s.child("StoryPreference").is_some_and(|p| p.attr("StoryDirection") == Some("RightToLeftDirection")) {
            w.add("right-to-left story read as left-to-right");
        }
        let mut b = self.builder(w);
        b.track = self.saved.and_then(|s| s.stories.get(&id)).map(UnitTrack::new);
        for c in s.elements() {
            match c.name {
                "StoryPreference" | "InCopyExportOption" | "Properties" | "GridDataInformation" => {}
                _ => self.node(c, &mut b, &mut extras, w, 0),
            }
        }
        // The empty paragraph `finish` may add has no characters.
        let (track, end) = (b.track.take(), b.epos);
        let saved = match (track, self.saved.and_then(|s| s.skipped.get(&id))) {
            (Some(t), _) => t.finish(end),
            (None, Some(why)) => SavedComposition::Unusable(format!("reader: {why}")),
            (None, None) => SavedComposition::None,
        };
        let mut paragraphs = b.finish(w);
        let mut index = 0;
        for run in paragraphs.iter_mut().flat_map(|p| &mut p.runs) {
            if run.variable == Some(TextVariable::FootnoteNumber) && let Some(note) = &mut run.note {
                note.id = crate::footnotes::text_id(&id, index);
                index += 1;
            }
        }
        let (notes, changes) = crate::editorial::read(s);
        let optical_margin = crate::optical_margin::read(|key|s.child("StoryPreference").and_then(|p|p.attr(key)));
        (Some(Story { id, paragraphs, saved, notes, changes, optical_margin, vertical, track_changes: s.attr("TrackChanges") == Some("true") }), extras)
    }

    /// The paragraphs of a table cell, a footnote or another nested text: `e`'s children
    /// read as a story of their own.
    pub fn nested(&self, e: &'a Element<'a>, w: &mut Warnings, depth: usize) -> Vec<Paragraph> {
        let mut b = self.builder(w);
        let mut extras = StoryExtras::default();
        self.children(e, &mut b, &mut extras, w, depth);
        b.finish(w)
    }

    fn para_ctx(&self, range: Option<&'a Element<'a>>, w: &mut Warnings) -> Rc<ParaCtx> {
        let base: std::sync::Arc<ParaBase> = self.sheet.paragraph(range.and_then(|r| r.attr("AppliedParagraphStyle")), w);
        let mut para = base.para.clone();
        let mut chr = base.chr.clone();
        if let Some(r) = range {
            apply_para(&mut para, r, w);
            apply_char(&mut chr, r, w);
        }
        let mut attrs = para.finish(self.colors, w);
        attrs.line_styles = range.and_then(|r| crate::nested::read_local_lines(r)).unwrap_or_else(|| (*base.line_styles).clone());
        Rc::new(ParaCtx {
            style: base.name.clone(),
            attrs,
            auto_leading: para.auto_leading,
            unmodelled: para.unmodelled,
            chr,
            list: para.list,
            grep: base.grep.clone(),
            nested: range.and_then(|r| crate::nested::read_local(r)).map(std::sync::Arc::new).unwrap_or_else(|| base.nested.clone()),
        })
    }

    fn char_attrs(&self, range: &'a Element<'a>, b: &Builder<'_, 'a>, w: &mut Warnings) -> CharacterAttrs {
        let ctx = b.para_ctx().unwrap_or_else(|| self.para_ctx(None, w));
        let mut chr = ctx.chr.clone();
        let name = self.sheet.apply_character_style(&mut chr, range.attr("AppliedCharacterStyle"), w);
        apply_char(&mut chr, range, w);
        let mut a = chr.finish(self.colors, w);
        a.character_style = name;
        // The conditions of the range (hidden ones never get here), for their indicators.
        a.conditions = range.attr("AppliedConditions").map(|l| l.split_whitespace().map(crate::props::percent_decode).collect()).unwrap_or_default();
        // AutoLeading is a paragraph attribute in IDML.
        a.auto_leading = ctx.auto_leading;
        a
    }

    fn node(&self, e: &'a Element<'a>, b: &mut Builder<'_, 'a>, x: &mut StoryExtras, w: &mut Warnings, depth: usize) {
        if depth > MAX_TEXT_DEPTH {
            w.add(format!("story nesting deeper than {MAX_TEXT_DEPTH}; content skipped"));
            return;
        }
        match e.name {
            "ParagraphStyleRange" => {
                let ctx = self.para_ctx(Some(e), w);
                b.para.push(ctx.clone());
                b.last_para = Some(ctx);
                // Character attributes restart from this paragraph range.
                let saved = std::mem::take(&mut b.chr);
                self.children(e, b, x, w, depth);
                b.chr = saved;
                b.para.pop();
            }
            // Text whose conditions are all hidden: InDesign keeps it in `HiddenText` (and
            // a range outside it with only hidden conditions is hidden too, as text with
            // at least one shown condition is shown). It takes no space and does not print;
            // a paragraph return inside it joins the paragraphs around it.
            "HiddenText" => {}
            "CharacterStyleRange" if self.conditionally_hidden(e) => {}
            "CharacterStyleRange" => {
                let a = self.char_attrs(e, b, w);
                // An empty range still sets the attributes of the insertion point after it.
                b.last_attrs = Some(a.clone());
                b.chr.push(a);
                b.ranges.push(e);
                b.breaks.push(folio_model::ParagraphStart::from_idml(e.attr("ParagraphBreakType").unwrap_or("")));
                b.page_kinds.push(match e.attr("PageNumberType") {
                    Some("NextPageNumber") => TextVariable::NextPageNumber,
                    Some("PreviousPageNumber") => TextVariable::PreviousPageNumber,
                    _ => TextVariable::PageNumber,
                });
                self.children(e, b, x, w, depth);
                b.page_kinds.pop();
                b.breaks.pop();
                b.chr.pop();
                b.ranges.pop();
            }
            "Content" => {
                for n in &e.children {
                    match n {
                        Node::Text(t) => self.text(t, b, w),
                        Node::Pi(target, data) => self.pi(target, data, b, w),
                        Node::Element(c) => w.add(format!("element {} inside Content not read", c.name)),
                    }
                }
            }
            "Br" => {
                b.unit('\r', 0);
                b.end_paragraph(w);
                // a column, frame or page break: the next paragraph starts there
                if let Some(&start) = b.breaks.last().filter(|s| **s != folio_model::ParagraphStart::Anywhere) {
                    b.pending_start = Some(start);
                    if let Some(ended) = b.paragraphs.last_mut() { ended.attrs.ends_with_break = start; }
                }
            }
            "Properties" => {}
            // One character of the story (U+0018) however long its result.
            // One character of the story; the layout works out what it shows for the page it is on, `ResultText` is what InDesign
            // stored with it.
            "TextVariableInstance" => {
                b.unit('\u{18}', 0);
                b.push("#", Some(TextVariable::Named), w);
                if let Some(run) = b.cur.as_mut().and_then(|p| p.runs.last_mut()) {
                    run.named = Some(Box::new(folio_model::NamedVariable {
                        id: e.attr("AssociatedTextVariable").unwrap_or_default().to_owned(),
                        result: e.attr("ResultText").unwrap_or_default().to_owned(),
                        instance: e.attr("Self").unwrap_or_default().to_owned(),
                    }));
                }
            }
            "Table" => {
                let table = crate::tables::read_table(self, e, w, depth + 1);
                b.unit('\u{16}', 0);
                b.push_table(table, w);
            }
            "Footnote" => {
                let paragraphs = self.nested(e, w, depth + 1);
                b.unit('\u{4}', 0);
                b.push_note(TextVariable::FootnoteNumber, Some(Note { paragraphs, ..Note::default() }), w);
            }
            "Endnote" => {
                b.unit('\u{4}', 0);
                let id = e.attr("Self").unwrap_or_default().to_owned();
                x.endnote_refs.push(id.clone());
                b.push_note(TextVariable::EndnoteNumber, Some(Note { id, ..Note::default() }), w);
            }
            "EndnoteRange" => {
                let source = e.attr("SourceEndnote").unwrap_or_default().to_owned();
                x.endnote_ranges.push(source.clone());
                let saved = b.endnote.replace(source);
                self.children(e, b, x, w, depth);
                b.endnote = saved;
            }
            // Editorial notes do not print.
            "Note" => {
                b.unit('\u{FEFF}', 0);
                let at = b.paragraph(w).runs.iter().map(|r| r.text.chars().count()).sum();
                b.paragraph(w).attrs.note_anchor_marks.push(at);
                b.push("\u{FEFF}", None, w);
            }
            // An index marker is one U+FEFF in InDesign's character space (index-c1, measured: 70 -> 71 characters).
            "PageReference" if e.elements().all(|c| c.name == "Properties") => { b.unit('\u{FEFF}', 0); b.push("\u{FEFF}", None, w); }
            "Change" if e.attr("ChangeType") == Some("DeletedText") => {}
            "HyperlinkTextSource" | "XMLElement" | "Change" | "CrossReferenceSource" | "ParagraphDestination"
            | "HyperlinkTextDestination" | "PageReference" | "Bookmark" => self.children(e, b, x, w, depth),
            "XMLAttribute" | "XMLComment" | "XMLInstruction" | "DTD" => {}
            // a placed story's content link (kept as written, read by folio-edit `contentlink`)
            "LinkedStoryOption" | "Link" => {}
            // Anchored items: placement from their `AnchoredObjectSetting` (layout).
            n if is_item(n) => {
                let mut item = self.items.item(e, w, 0);
                item.anchor = self.items.anchor(e, w);
                b.unit('\u{FFFC}', 0);
                b.push_inline(item, w);
            }
            other => {
                w.add(format!("story element {other} read through"));
                self.children(e, b, x, w, depth);
            }
        }
    }

    /// Whether a range's `AppliedConditions` are all hidden conditions.
    fn conditionally_hidden(&self, e: &Element<'_>) -> bool {
        let Some(list) = e.attr("AppliedConditions") else { return false };
        let mut refs = list.split_whitespace().peekable();
        refs.peek().is_some() && refs.all(|r| self.hidden_conditions.contains(&crate::props::percent_decode(r)))
    }

    fn children(&self, e: &'a Element<'a>, b: &mut Builder<'_, 'a>, x: &mut StoryExtras, w: &mut Warnings, depth: usize) {
        for n in &e.children {
            match n {
                Node::Element(c) => self.node(c, b, x, w, depth + 1),
                Node::Pi(target, data) => self.pi(target, data, b, w),
                // Text outside <Content> is formatting whitespace.
                Node::Text(_) => {}
            }
        }
    }

    fn text(&self, t: &str, b: &mut Builder<'_, 'a>, w: &mut Warnings) {
        self.text_with(t, b, w, true);
    }

    /// `text`, counting its IDML text units (`saved`) when `units` (not for a text
    /// variable's result, which is one unit).
    fn text_with(&self, t: &str, b: &mut Builder<'_, 'a>, w: &mut Warnings, units: bool) {
        let mut buf = String::with_capacity(t.len());
        // Characters in `buf`, not pushed yet.
        let mut pending = 0usize;
        for ch in t.chars() {
            if units && ch != '\u{FEFF}' {
                b.unit(if ch == '\u{2028}' { '\n' } else { ch }, pending);
            }
            match ch {
                '\u{2028}' => {
                    buf.push('\n');
                    pending += 1;
                }
                '\r' => {
                    b.push(&buf, None, w);
                    buf.clear();
                    pending = 0;
                    b.end_paragraph(w);
                }
                '\t' | '\n' | '\u{7}' | '\u{8}' => {
                    buf.push(ch);
                    pending += 1;
                }
                '\u{FEFF}' => {}
                '\u{FFFC}' => w.add("stray U+FFFC in content dropped"),
                c if (c as u32) < 0x20 => w.add(format!("control character U+{:04X} in content dropped", c as u32)),
                c => {
                    buf.push(c);
                    pending += 1;
                }
            }
        }
        b.push(&buf, None, w);
    }

    fn pi(&self, target: &str, data: &str, b: &mut Builder<'_, 'a>, w: &mut Warnings) {
        if target != "ACE" {
            w.add(format!("processing instruction <?{target}?> in text ignored"));
            return;
        }
        // One character of the story: its code in hex.
        let code = u32::from_str_radix(data.trim(), 16).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}');
        b.unit(code, 0);
        match data {
            // The layout puts the page's number or section marker in place of "#".
            "18" => {
                let kind = b.page_kinds.last().copied().unwrap_or(TextVariable::PageNumber);
                if kind != TextVariable::PageNumber {
                    w.add("next/previous page number: shown as the page of the next/previous frame of its own story (InDesign: of the story its text frame touches)");
                }
                b.push("#", Some(kind), w);
            }
            "19" => b.push("#", Some(TextVariable::SectionMarker), w),
            // The note's own number: an endnote's inside its range, else a footnote's.
            "4" => match b.endnote.clone() {
                Some(id) => b.push_note(TextVariable::EndnoteNumber, Some(Note { id, body: true, ..Note::default() }), w),
                None => b.push_note(TextVariable::FootnoteNumber, None, w),
            },
            "7" => b.push("\u{7}", None, w),
            "8" => b.push("\u{8}", None, w),
            // End nested style here: no visible character, no room. Its place in the paragraph is kept for nested styles and GREP.
            "3" => {
                let at = b.paragraph(w).runs.iter().map(|r| r.text.chars().count()).sum();
                b.paragraph(w).attrs.end_nested_marks.push(at);
            }
            other => w.add(format!("special character <?ACE {other}?> not modelled")),
        }
    }
}

/// Characters as InDesign counts them: run text plus one per paragraph return.
pub fn story_chars(s: &Story) -> usize {
    let text: usize = s.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.text.chars().count()).sum();
    text + s.paragraphs.len().saturating_sub(1)
}

/// Gives every endnote reference and endnote number its number: the order of the endnote
/// ranges in the endnote stories, else the order of the references.
/// `story_of`: the story each reference is in, when the numbering restarts in every story (`RestartEndnoteNumbering`
/// StoryRestart): a reference's number counts only the references of its story before it, from `start`.
pub fn number_endnotes(stories: &mut [&mut Story], ranges: &[String], refs: &[String], start: u32, story_of: Option<&std::collections::HashMap<String, String>>) {
    let order: &[String] = if ranges.is_empty() { refs } else { ranges };
    let number = |id: &str| {
        let at = order.iter().position(|r| r == id)?;
        let before = match story_of {
            Some(map) => {
                let own = map.get(id);
                order[..at].iter().filter(|r| map.get(r.as_str()) == own).count()
            }
            None => at,
        };
        Some(start.saturating_add(before as u32))
    };
    fn visit(paragraphs: &mut [Paragraph], number: &dyn Fn(&str) -> Option<u32>) {
        for p in paragraphs {
            for r in &mut p.runs {
                if r.variable == Some(TextVariable::EndnoteNumber)
                    && let Some(n) = r.note.as_mut()
                {
                    n.number = number(&n.id);
                }
                for t in &mut r.tables {
                    for c in &mut t.cells {
                        visit(&mut c.paragraphs, number);
                    }
                }
            }
        }
    }
    for s in stories {
        visit(&mut s.paragraphs, &number);
    }
}

/// Endnote numbers (build-b3): continuous through the document, or from the start number again in every story
/// (`RestartEndnoteNumbering` StoryRestart); the reference in the text and the number in the endnote text share the number.
#[cfg(test)]
mod endnote_numbering_tests {
    use super::number_endnotes;
    use folio_model::{Note, Paragraph, Story, TextRun, TextVariable};
    use std::collections::HashMap;
    fn story(id: &str, notes: &[(&str, bool)]) -> Story {
        let runs = notes.iter().map(|&(n, body)| TextRun {
            text: "#".into(),
            variable: Some(TextVariable::EndnoteNumber),
            note: Some(Box::new(Note { id: n.into(), body, ..Note::default() })),
            ..TextRun::default()
        }).collect();
        Story { id: id.into(), paragraphs: vec![Paragraph { runs, ..Paragraph::default() }], ..Story::default() }
    }

    fn numbers(s: &Story) -> Vec<Option<u32>> {
        s.paragraphs[0].runs.iter().map(|r| r.note.as_ref().and_then(|n| n.number)).collect()
    }

    #[test]
    fn restart_in_every_story_counts_the_references_of_its_own_story() {
        let refs: Vec<String> = ["e1", "e2", "e3", "e4"].iter().map(|s| s.to_string()).collect();
        let story_of: HashMap<String, String> = [("e1", "a"), ("e2", "a"), ("e3", "b"), ("e4", "a")].iter().map(|(r, s)| (r.to_string(), s.to_string())).collect();
        let (mut a, mut b, mut notes) = (story("a", &[("e1", false), ("e2", false), ("e4", false)]), story("b", &[("e3", false)]),
            story("n", &[("e1", true), ("e2", true), ("e3", true), ("e4", true)]));
        number_endnotes(&mut [&mut a, &mut b, &mut notes], &refs, &refs, 3, Some(&story_of));
        assert_eq!(numbers(&a), [Some(3), Some(4), Some(5)]);
        assert_eq!(numbers(&b), [Some(3)]);
        assert_eq!(numbers(&notes), [Some(3), Some(4), Some(3), Some(5)]);
        // continuous: the document's order
        let (mut a, mut b) = (story("a", &[("e1", false), ("e2", false), ("e4", false)]), story("b", &[("e3", false)]));
        number_endnotes(&mut [&mut a, &mut b], &refs, &refs, 1, None);
        assert_eq!(numbers(&a), [Some(1), Some(2), Some(4)]);
        assert_eq!(numbers(&b), [Some(3)]);
    }
}
