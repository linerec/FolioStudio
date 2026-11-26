//! Lay out document stories, including text variables and master-page frames.

use super::*;

/// Lays out every story into its frames. Stories are laid out in parallel; the store is
/// shared (`&FontStore`; it is internally synchronised, a `&mut` caller coerces).
pub fn layout_document(doc: &Document, fonts: &FontStore) -> DocumentLayout {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    layout_document_with_threads(doc, fonts, threads)
}

/// `layout_document` on at most `threads` threads (1 = the calling thread only).
pub fn layout_document_with_threads(doc: &Document, fonts: &FontStore, threads: usize) -> DocumentLayout {
    let started = std::time::Instant::now();
    let frames = DocumentFrames::new(doc);
    let stories: Vec<&Story> = frames.stories().filter_map(|id| doc.stories.get(*id)).collect();
    let fallbacks = resolve_fonts(doc, stories.iter().copied(), fonts);
    let faces = fonts.snapshot();
    let table = FaceTable::new(fonts, &faces);
    let note_starts = footnote_starts(doc, &frames);
    let ctx = Ctx {
        doc,
        list_plan: listplan::Plan::new(doc, &frames),
        note_starts: &note_starts,
        frames: &frames,
        fonts,
        table: &table,
        fallbacks: &fallbacks,
        prefs: ShapePrefs::from(&doc.text_preferences),
    };

    // Largest stories first so threads finish together.
    let mut order: Vec<&Story> = stories;
    order.sort_by_key(|s| std::cmp::Reverse(s.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.text.len()).sum::<usize>()));
    let threads = threads.clamp(1, order.len().max(1));
    let next = AtomicUsize::new(0);
    let mut results: Vec<StoryLayout> = if threads == 1 {
        order.iter().map(|s| ctx.layout_story(s)).collect()
    } else {
        std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(story) = order.get(i) else { break };
                        out.push(ctx.layout_story(story));
                    }
                    out
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
        })
    };
    results.sort_by(|a, b| a.story.cmp(&b.story));

    let mut layout = DocumentLayout::default();
    let mut missing = BTreeSet::new();
    for r in results {
        layout.stats.stories += 1;
        layout.stats.paragraphs += r.paragraphs;
        layout.stats.lines += r.lines;
        layout.stats.glyphs += r.glyphs;
        if r.overset {
            layout.overset.push(r.story.clone());
        }
        match &r.saved {
            SavedUse::None => {}
            SavedUse::Used => layout.stats.saved_stories += 1,
            SavedUse::Fallback(why) => {
                layout.stats.saved_fallbacks += 1;
                layout.saved_fallbacks.push((r.story.clone(), why.clone()));
            }
        }
        missing.extend(r.missing_fonts);
        layout.missing_glyphs.extend(r.missing_glyphs);
        for f in r.frames {
            layout.frames.insert(f.frame.clone(), f);
        }
    }
    // Frames of stories that do not exist still get an (empty) entry.
    for id in frames.frame_ids() {
        layout.frames.entry(id.to_string()).or_insert_with(|| FrameText { frame: id.to_string(), lines: Vec::new() });
    }
    layout.missing_fonts = missing.into_iter().collect();
    layout.stats.elapsed_ms = started.elapsed().as_millis() as u64;
    layout
}

/// Lays out one story alone (e.g. after an edit); the result equals that story's part of
/// `layout_document`. `None` when the story has no frame.
pub fn layout_story(doc: &Document, frames: &DocumentFrames<'_>, story: &str, fonts: &FontStore) -> Option<StoryLayout> {
    let s = doc.stories.get(story)?;
    if frames.chain(story).is_empty() {
        return None;
    }
    let fallbacks = resolve_fonts(doc, std::iter::once(s), fonts);
    let faces = fonts.snapshot();
    let table = FaceTable::new(fonts, &faces);
    let note_starts = footnote_starts(doc, frames);
    let ctx = Ctx {
        doc,
        list_plan: listplan::Plan::new(doc, frames),
        note_starts: &note_starts,
        frames,
        fonts,
        table: &table,
        fallbacks: &fallbacks,
        prefs: ShapePrefs::from(&doc.text_preferences),
    };
    Some(ctx.layout_story(s))
}

/// Text of one master-page frame as it appears on one document page.
#[derive(Clone, Debug, PartialEq)]
pub struct PageFrameText {
    /// `Page::index` of the document page.
    pub page: usize,
    pub frame: FrameText,
}

/// What the document's text variables show on each document page (index = `Page::index`; keyed by the variable's `Self`), from the
/// layout of the document: Last Page Number, Chapter Number and the running headers by paragraph or character style.
pub fn text_variable_values(doc: &Document, fonts: &FontStore, layout: &DocumentLayout) -> Vec<BTreeMap<String, String>> {
    let frames = DocumentFrames::new(doc);
    let faces = fonts.snapshot();
    let table = FaceTable::new(fonts, &faces);
    let note_starts = footnote_starts(doc, &frames);
    let fallbacks = HashMap::new();
    let ctx = Ctx { doc, list_plan: listplan::Plan::new(doc, &frames), note_starts: &note_starts, frames: &frames, fonts, table: &table, fallbacks: &fallbacks, prefs: ShapePrefs::from(&doc.text_preferences) };
    variables::page_values(&ctx, &frames, layout)
}

/// The `ResultText` the instances of text variables on document pages should carry (`(instance Self, text)`), where it differs from
/// the stored one: what InDesign writes when it saves.
pub fn variable_results(doc: &Document, fonts: &FontStore, layout: &DocumentLayout) -> Vec<(String, String)> {
    let frames = DocumentFrames::new(doc);
    let faces = fonts.snapshot();
    let table = FaceTable::new(fonts, &faces);
    let note_starts = footnote_starts(doc, &frames);
    let fallbacks = HashMap::new();
    let ctx = Ctx { doc, list_plan: listplan::Plan::new(doc, &frames), note_starts: &note_starts, frames: &frames, fonts, table: &table, fallbacks: &fallbacks, prefs: ShapePrefs::from(&doc.text_preferences) };
    variables::results(&ctx, &frames, layout)
}

/// Master text frames whose story shows a page number, laid out once per document page
/// that uses the master page they are on (the left or right page of the master spread, by
/// the document page's side), with that page's name as the number. `layout_document` lays
/// these frames out once with the master's prefix ("A"), as InDesign shows them on the
/// master itself.
pub fn layout_master_page_numbers(doc: &Document, fonts: &FontStore, layout: &DocumentLayout) -> Vec<PageFrameText> {
    let frames = DocumentFrames::new(doc);
    let numbered: Vec<&Story> = frames
        .stories()
        .filter_map(|id| doc.stories.get(*id))
        .filter(|s| {
            s.paragraphs.iter().flat_map(|p| &p.runs).any(|r| {
                matches!(r.variable, Some(TextVariable::PageNumber | TextVariable::NextPageNumber | TextVariable::PreviousPageNumber | TextVariable::SectionMarker | TextVariable::Named))
            })
        })
        .collect();
    if numbered.is_empty() {
        return Vec::new();
    }
    let fallbacks = resolve_fonts(doc, numbered.iter().copied(), fonts);
    let faces = fonts.snapshot();
    let table = FaceTable::new(fonts, &faces);
    let note_starts = footnote_starts(doc, &frames);
    let ctx = Ctx {
        doc,
        list_plan: listplan::Plan::new(doc, &frames),
        note_starts: &note_starts,
        frames: &frames,
        fonts,
        table: &table,
        fallbacks: &fallbacks,
        prefs: ShapePrefs::from(&doc.text_preferences),
    };
    let first_master = doc.spreads.len();
    let mut out = Vec::new();
    // what the text variables show on each page
    let shown: HashMap<usize, BTreeMap<String, String>> = variables::page_values(&ctx, &frames, layout).into_iter().enumerate().collect();
    for spread in &doc.spreads {
        for page in &spread.pages {
            for source in folio_model::parents::sources(doc, spread, page) {
                let mi = doc.master_spreads.iter().position(|m| m.id == source.spread.id).unwrap();
                let master_page = source.spread.pages.iter().position(|p| p.id == source.page.id).unwrap();
                for story in &numbered {
                    let chain = frames.chain(&story.id);
                    let on_this_master = chain.iter().filter_map(|id| frames.frame(id)).all(|f| {
                        f.spread == Some(first_master + mi) && f.page_in_spread == Some(master_page)
                    });
                    if chain.is_empty() || !on_this_master {
                        continue;
                    }
                    let laid = ctx.layout_story_on(story, Some((&page.name, &page.section_marker)), shown.get(&page.index));
                    out.extend(laid.frames.into_iter().map(|frame| PageFrameText { page: page.index, frame }));
                }
            }
        }
    }
    out
}

/// Characters as a set: a bitset for the BMP, a sorted set beyond.
#[derive(Default)]
struct CharSet {
    bmp: Vec<u64>,
    astral: BTreeSet<char>,
}

impl CharSet {
    fn insert(&mut self, c: char) {
        let u = c as usize;
        if u < 0x10000 {
            if self.bmp.is_empty() {
                self.bmp = vec![0; 0x10000 / 64];
            }
            self.bmp[u / 64] |= 1 << (u % 64);
        } else {
            self.astral.insert(c);
        }
    }

    fn iter(&self) -> impl Iterator<Item = char> + '_ {
        let bmp = self.bmp.iter().enumerate().filter(|(_, bits)| **bits != 0).flat_map(|(w, &bits)| {
            let mut left = bits;
            std::iter::from_fn(move || {
                (left != 0).then(|| {
                    let b = left.trailing_zeros();
                    left &= left - 1;
                    char::from_u32(w as u32 * 64 + b)
                })
            })
            .flatten()
        });
        bmp.chain(self.astral.iter().copied())
    }
}

/// Resolves (and loads) every face the stories use, including anchored stories, and the
/// faces for characters their substitutes lack ([`FontStore::fallbacks`]), so one snapshot
/// of the store holds them all. Returns those per-character faces.
fn resolve_fonts<'a>(doc: &Document, stories: impl Iterator<Item = &'a Story>, fonts: &FontStore) -> HashMap<char, FontId> {
    let mut resolved: HashMap<(&str, &str), Resolved> = HashMap::new();
    // Characters set in each substitute face.
    let mut in_substitute: HashMap<FontId, CharSet> = HashMap::new();
    fn visit<'s>(ps: &'s [Paragraph], f: &mut dyn FnMut(&'s CharacterAttrs, &mut dyn Iterator<Item = char>)) {
        for p in ps {
            if let Some(l) = &p.attrs.list {
                // The label is generated: its literal text plus every character a number
                // in its format can take.
                let mut label: String = format!("{}{}{}", l.bullet, l.bullet_text_after, l.expression);
                for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 14, 15, 40, 49, 400, 900] {
                    label.push_str(&format_number(n, &l.format));
                }
                f(&l.marker, &mut label.chars());
            }
            for r in &p.runs {
                if r.variable.is_some() {
                    f(&r.attrs, &mut r.text.chars().chain('0'..='9'));
                } else {
                    f(&r.attrs, &mut r.text.chars());
                }
                // The fonts of the nested line styles are set when the lines are known, so they must be in the snapshot too.
                for a in &r.line_attrs {
                    f(a, &mut r.text.chars());
                }
                for t in &r.tables {
                    for c in &t.cells {
                        visit(&c.paragraphs, f);
                    }
                }
                if let Some(n) = &r.note {
                    visit(&n.paragraphs, f);
                }
            }
        }
    }
    for s in stories {
        visit(&s.paragraphs, &mut |a, text| {
            let key = (a.font_family.as_str(), a.font_style.as_str());
            let r = *resolved.entry(key).or_insert_with(|| fonts.resolve(&a.font_family, &a.font_style, &doc.font_table));
            if !r.exact {
                let set = in_substitute.entry(r.font).or_default();
                for c in text {
                    if !c.is_whitespace() && !c.is_control() {
                        set.insert(c);
                    }
                }
            }
        });
    }
    // Characters a substitute lacks need a face of their own.
    let mut lacking: Vec<char> = Vec::new();
    for (font, set) in &in_substitute {
        if fonts.is_synthetic(*font) {
            continue;
        }
        fonts.with_face(*font, |face| lacking.extend(set.iter().filter(|&c| face.glyph_index(c).is_none())));
    }
    if lacking.is_empty() { HashMap::new() } else { fonts.fallbacks(lacking) }
}

impl<'c, 'a> Ctx<'c, 'a> {
    fn layout_story(&self, story: &Story) -> StoryLayout {
        self.layout_story_on(story, None, None)
    }

    /// Lays out a story; `page_name` replaces page-number variables (default: the name of
    /// the page of the story's first frame), `variables` what the document's text variables show (default: the variables that do
    /// not depend on the page's text, for the page of the story's first frame).
    fn layout_story_on(&self, story: &Story, page: Option<(&str, &str)>, variables: Option<&BTreeMap<String, String>>) -> StoryLayout {
        let page_name = page.map(|p| p.0);
        // A story set for one page's numbers is composed (its text differs from the saved).
        let for_page = page_name.is_some();
        let chain = self.frames.chain(&story.id);
        let opts = &self.doc.footnotes;
        let make_flow = |reserve: &[f64]| {
            let vertical = story.vertical;
            let frames = chain
                .iter()
                .filter_map(|id| self.frames.frame(id))
                .enumerate()
                .map(|(i, f)| {
                    // A vertical frame (not text on a path) is composed in its virtual horizontal frame.
                    let turned = vertical && f.item.text_path.is_none();
                    let cols = if turned { vertical_columns(f.item, f.frame) } else { columns_of(f.item, f.frame) };
                    let area = paths_bounds(&f.item.paths).unwrap_or_default();
                    FlowFrame {
                        facing: self.doc.print.facing_pages,
                        grid: if turned { baseline_grid::Grid::for_vertical_frame(&self.doc.text_preferences, f) } else { baseline_grid::Grid::for_frame(&self.doc.text_preferences, f) },
                        id: f.item.id.as_str(),
                        frame: f.frame,
                        verso: f.verso,
                        base_columns: cols.len(),
                        cols,
                        start: 0,
                        // (a vertical frame's wraps and shape are in its virtual frame already)
                        wraps: &f.wraps,
                        shape: f.shape.as_ref(),
                        lines: Vec::new(),
                        notes: Vec::new(),
                        reserve: reserve.get(i).copied().unwrap_or(0.0),
                        area: if turned { crate::vertical::virtual_rect(area) } else { area },
                        page: if turned { None } else { f.page_rect.and_then(|r| f.to_spread.invert().map(|inv| (inv.apply_rect(&r), f.margins))) },
                        vmap: turned.then(|| crate::vertical::map(area.left, area.right)),
                    }
                })
                .collect();
            let mut flow = Flow::new(frames, Some(opts));
            flow.vertical = vertical;
            flow
        };
        let page_name = page_name.map(str::to_string).unwrap_or_else(|| {
            chain.first().and_then(|id| self.frames.frame(id)).and_then(|f| f.page_name.clone()).unwrap_or_else(|| "#".into())
        });
        let section_marker = page.map(|p| p.1.to_owned()).unwrap_or_else(|| {
            chain.first().and_then(|id| self.frames.frame(id)).and_then(|f| f.section_marker.clone()).unwrap_or_default()
        });
        // Next / previous page number: the page of the frame after the story's first frame,
        // and of the one before it (none: the page itself).
        let next_page = chain.get(1).and_then(|id| self.frames.frame(id)).and_then(|f| f.page_name.clone()).unwrap_or_else(|| page_name.clone());
        let first_note = self.note_starts.get(story.id.as_str()).copied().unwrap_or(opts.start_at);
        let variables = match variables {
            Some(v) => v.clone(),
            None if story.paragraphs.iter().flat_map(|p| &p.runs).any(|r| r.variable == Some(TextVariable::Named)) => {
                let mut v = self.static_variables(chain.first().and_then(|id| self.frames.frame(id)).and_then(|f| self.page_of(f)));
                v.extend(self.frame_variables(&story.id, self.frames));
                v
            }
            None => BTreeMap::new(),
        };
        let mut out = StoryLayout { story: story.id.clone(), ..StoryLayout::default() };
        let plan = if for_page || story.paragraphs.iter().any(|p| p.attrs.columns.layout != folio_model::paragraph_columns::ColumnLayout::Single) {
            None
        } else {
            let probe = make_flow(&[]);
            let frames: Vec<(&str, usize)> = probe.frames.iter().map(|f| (f.id, f.cols.len())).collect();
            match saved::plan(story, &frames) {
                Ok(Some(p)) => {
                    out.saved = SavedUse::Used;
                    Some(p)
                }
                Ok(None) => None,
                Err(why) => {
                    out.saved = SavedUse::Fallback(why);
                    None
                }
            }
        };
        // Footnotes spanning a frame's columns shorten its earlier columns too: lay the story
        // out again with the depth they took kept from the start (InDesign 2025, scratch
        // experiment exp4), until no frame needs more.
        let mut reserve: Vec<f64> = Vec::new();
        let mut balanced_prefix = BTreeSet::new();
        let mut note_numbers = BTreeMap::new();
        let mut pass = 0;
        let (mut flow, st) = loop {
            let mut flow = make_flow(&reserve);
            let mut st = TextState::new(page_name.clone(), first_note, None);
            st.list_labels = self.list_plan.0.get(&story.id).cloned();
            st.section_marker = section_marker.clone();
            st.next_page = next_page.clone();
            st.variables = variables.clone();
            st.optical = story.optical_margin.enabled.then_some(story.optical_margin.size);
            st.note_numbers = note_numbers.clone();
            out.overset = !self.lay_text(&mut flow, &story.paragraphs, &mut st, plan.as_ref());
            out.overset |= flow.frames.iter().any(|f| f.notes.iter().any(|b| b.overset));
            pass += 1;
            let need: Vec<f64> = flow
                .frames
                .iter()
                .enumerate()
                .map(|(fi, f)| {
                    if !f.spans_notes(opts) || f.notes.is_empty() {
                        return f.reserve;
                    }
                    let sum: f64 = f.notes.iter().map(|n| n.height).sum();
                    let depth = sum + opts.space_between * (f.notes.len() - 1) as f64 + opts.spacer;
                    if pass == 1 && let Some(note) = f.notes.last().filter(|n| !n.continued && n.continues)
                        && let Some(reference) = note.lines.first()
                        && let Some(body_bottom) = paragraph_columns::prefix_bottom(&flow, fi, reference.line.start, &story.paragraphs)
                    {
                        balanced_prefix.insert(fi);
                        return depth.max(f.cols[0].bottom - body_bottom);
                    }
                    // After balancing the reference prefix, keep just the measured note depth.
                    if balanced_prefix.remove(&fi) { depth } else { depth.max(f.reserve) }
                })
                .collect();
            let changed = need.iter().zip(&flow.frames).any(|(n, f)| (*n - f.reserve).abs() > EPS);
            let numbers = footnotes::numbers(self, &flow);
            if (!changed && numbers == note_numbers) || pass >= 4 {
                break (flow, st);
            }
            reserve = need;
            note_numbers = numbers;
        };
        out.paragraphs = st.paragraphs;
        self.decorate(&mut flow, &story.paragraphs);
        let blocks = paragraph_columns::justify(&mut flow);
        justify_vertically(&mut flow, &blocks);
        self.place_notes(&mut flow);
        for f in flow.frames {
            out.lines += f.lines.len();
            out.glyphs += f.lines.iter().map(|l| l.line.glyphs.len()).sum::<usize>();
            // Where a text gradient sits: against the column of its line and the frame's text area (nested text has no area).
            // (text on a path has no column to measure against: InDesign's rule there (the path's own box) is not measured, so it shows flat)
            let on_path = self.frames.frame(f.id).is_some_and(|frame| frame.item.text_path.as_ref().is_some_and(|p| p.curve.is_some()));
            let origin = f.cols.first().map(|c| folio_model::Point::new(c.left, c.top)).filter(|_| !on_path);
            out.frames.push(FrameText { frame: f.id.to_string(), lines: f.lines.into_iter().map(|mut l| {
                // Cell / footnote indexes belong to their own nested text, not this story.
                if l.para.is_none() { l.line.text_clusters.clear(); }
                if let (Some(c), Some(origin), Some(_)) = (f.cols.get(l.col), origin, l.para) {
                    let area = folio_model::GradientArea { column: folio_model::Rect { left: c.left, top: c.top, right: c.right, bottom: c.bottom }, origin };
                    for g in &mut l.line.glyphs {
                        for placed in [&mut g.fill_gradient, &mut g.stroke_gradient].into_iter().flatten() { placed.area = Some(area); }
                    }
                }
                if let Some(frame) = self.frames.frame(f.id) { crate::textpath::place(frame.item, &mut l.line, self.fonts); }
                if let Some(m) = &f.vmap { crate::vertical::place_line(&mut l.line, m); }
                l.line
            }).collect() });
        }
        out.missing_fonts = st.missing.into_iter().collect();
        out.missing_glyphs = st.missing_glyphs.into_values().map(|m| MissingGlyphs { story: story.id.clone(), ..m }).collect();
        out.table_rows = st.table_rows;
        out
    }
}
