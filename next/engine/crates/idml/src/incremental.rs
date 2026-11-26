//! Reading again only what an edit changed.
//!
//! An editor holds the package's parts as text and every edit changes a few of them: usually the spread an object moved on, or the story
//! typed in. `read_parts` reads all of them again (151 MB of spreads in a real magazine: 0.3 s per keystroke). A [`ReadBase`] keeps the
//! document `read_parts` returned and, given the parts that changed since, reads just those again and puts the result in the place of
//! the old one. The result is what `read_parts` would return, except for `warnings` (the caller keeps the warnings it has).
//!
//! What a spread or story part can change outside itself is the reason for the checks in [`ReadBase::update`]: the page numbers of the
//! spreads after it, the `TextPath` links the items of other parts resolve, and the document-wide lists made from the stories (index,
//! hyperlinks, endnote numbers, table-of-contents stories, trap assignments). When a change reaches any of them in a way that is not
//! handled here, `update` says why it cannot and leaves the base as it was: the caller reads everything again.

use crate::{Facts, Globals, assign_sections, fonts_used, globals, items, package, read_items, saved, textpath, xml::Element};
use crate::{Document, Error, ReaderData, Warnings};
use folio_model::{Spread, Story, TextVariable};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// What a document was read from, outside its parts.
#[derive(Clone, Debug, PartialEq)]
struct Inputs {
    parser: Option<String>,
    previews: HashMap<String, String>,
    links: HashMap<String, crate::LinkOverride>,
    embedded: HashMap<String, PathBuf>,
}

impl Inputs {
    fn of(data: &ReaderData) -> Inputs {
        Inputs { parser: data.parser.clone(), previews: data.previews.clone(), links: data.links.clone(), embedded: data.embedded.clone() }
    }
}

/// A document read from parts, kept to be updated when some of the parts change.
#[derive(Clone)]
pub struct ReadBase {
    doc: Document,
    sidecar: Option<saved::Sidecar>,
    /// Each part's `TextPath` children and their owners.
    text_paths: BTreeMap<String, Vec<(String, String)>>,
    /// The same `TextPath` child id twice (the map would depend on the order of the parts).
    text_paths_ambiguous: bool,
    inputs: Inputs,
    idml: PathBuf,
}

/// The part-name prefixes whose parts an update re-reads.
const SPREADS: &str = "Spreads/";
const MASTERS: &str = "MasterSpreads/";
const STORIES: &str = "Stories/";

/// Markers in a story part that put something in a document-wide list (index, hyperlinks, endnotes) this module does not update.
const LIST_MARKERS: &[&str] = &[
    "<PageReference", "<HyperlinkTextDestination", "<ParagraphDestination", "<HyperlinkTextSource", "<CrossReferenceSource", "<Endnote", "IsEndnoteStory=\"true\"",
];

impl ReadBase {
    /// Reads the parts as `read_parts` does.
    pub fn read(parts: &BTreeMap<String, String>, idml: &Path, data: &ReaderData) -> Result<ReadBase, Error> {
        let texts = package::from_parts(parts)?;
        let bytes = parts.values().map(|t| t.len() as u64).sum();
        let pkg = package::parse(&texts)?;
        let (doc, facts) = crate::build_with_facts(&pkg, idml, bytes, data)?;
        Ok(Self::from_document(doc, facts, idml, data))
    }

    pub(crate) fn from_document(doc: Document, Facts { sidecar, text_paths }: Facts, idml: &Path, data: &ReaderData) -> Self {
        let mut seen = HashSet::new();
        let text_paths_ambiguous = text_paths.values().flatten().any(|(child, _)| !seen.insert(child.as_str()));
        ReadBase { doc, sidecar, text_paths, text_paths_ambiguous, inputs: Inputs::of(data), idml: idml.to_path_buf() }
    }

    /// The document as `read_parts` returns it for the parts the base reflects (`warnings` aside).
    pub fn doc(&self) -> &Document {
        &self.doc
    }

    /// Whether `data` is what the document was read with: an update only holds for the same outside facts.
    pub fn same_inputs(&self, data: &ReaderData) -> bool {
        self.inputs == Inputs::of(data)
    }

    /// Brings the base up to date with `parts`, given the names of the parts that changed since it was read (or last updated). On `Ok` the
    /// base is what `ReadBase::read(parts, idml, data)` would give (`warnings` aside); on `Err(why)` nothing changed and the caller reads
    /// again. Only spread, parent spread and story parts that are still there can be named; the designmap, resources and the stories'
    /// and spreads' own lists must be as they were. `data` is the data the base was read with, `saved_lines` aside.
    pub fn update(&mut self, parts: &BTreeMap<String, String>, idml: &Path, data: &ReaderData, changed: &BTreeSet<String>) -> Result<(), String> {
        self.update_with_items(parts, idml, data, changed, &BTreeMap::new())
    }

    /// Like `update`, with complete existing top-level spread children covering every change in
    /// each named part. The caller must guarantee that only attribute values inside those children
    /// changed, and that element structure, Self ids and page settings remain identical.
    /// Unknown or ambiguous item ids refuse the update without changing the base.
    pub fn update_with_items(&mut self, parts: &BTreeMap<String, String>, idml: &Path, data: &ReaderData, changed: &BTreeSet<String>, fragments: &BTreeMap<String, Vec<&str>>) -> Result<(), String> {
        if idml != self.idml {
            return Err("the document file changed".into());
        }
        if !self.same_inputs(data) {
            return Err("the links, previews or embedded files of placed graphics changed".into());
        }
        if self.text_paths_ambiguous {
            return Err("two TextPath children share an id".into());
        }
        // 1. the changed parts: spreads, parent spreads and stories that are still there
        for name in changed {
            let kind = [SPREADS, MASTERS, STORIES].into_iter().any(|p| name.starts_with(p));
            if !kind || !name.ends_with(".xml") {
                return Err(format!("{name} is not a spread, parent spread or story part"));
            }
            if !parts.contains_key(name) {
                return Err(format!("{name} was removed"));
            }
        }
        // 2. the designmap and its lists, as `from_parts` resolves them
        let dm_text = parts.get("designmap.xml").ok_or("designmap.xml is missing")?;
        let dm = package::parse_text("designmap.xml", dm_text).map_err(|e| e.to_string())?;
        if dm.name != "Document" {
            return Err("the designmap is not a Document".into());
        }
        let resolvable = |src: &str| !src.starts_with("XML/") && parts.contains_key(src);
        fn src_list<'d>(dm: &'d Element<'_>, kind: &'static str, resolvable: &dyn Fn(&str) -> bool) -> Vec<&'d str> {
            dm.children_named(kind).filter_map(|e| e.attr("src")).filter(|s| resolvable(s)).collect()
        }
        let list = |kind: &'static str| src_list(&dm, kind, &resolvable);
        let (graphic_src, styles_src, prefs_src) = (list("Graphic"), list("Styles"), list("Preferences"));
        let (master_src, spread_src, story_src) = (list("MasterSpread"), list("Spread"), list("Story"));
        {
            let fonts_src = list("Fonts");
            let mut seen = HashSet::new();
            let every = graphic_src.iter().chain(&styles_src).chain(&prefs_src).chain(&master_src).chain(&spread_src).chain(&story_src).chain(&fonts_src);
            if every.into_iter().any(|s| !seen.insert(*s)) {
                return Err("a part is listed twice in the designmap".into());
            }
        }
        if self.doc.master_spreads.len() != master_src.len() || self.doc.spreads.len() != spread_src.len() || self.doc.stories.len() != story_src.len() {
            return Err("the document does not have one spread or story for each part".into());
        }
        // 3. where each changed part sits in the document
        enum At { Master(usize), Spread(usize), Story }
        let mut at: Vec<(&str, At)> = Vec::new();
        for name in changed {
            let name = name.as_str();
            if name.starts_with(STORIES) {
                if !story_src.contains(&name) {
                    return Err(format!("{name} is not listed in the designmap"));
                }
                at.push((name, At::Story));
            } else if name.starts_with(MASTERS) {
                let k = master_src.iter().position(|s| *s == name).ok_or_else(|| format!("{name} is not listed in the designmap"))?;
                at.push((name, At::Master(k)));
            } else {
                let k = spread_src.iter().position(|s| *s == name).ok_or_else(|| format!("{name} is not listed in the designmap"))?;
                at.push((name, At::Spread(k)));
            }
        }
        // 4. parse them, and the parts every reading uses (all before the sheet that borrows them)
        let parse = |name: &str| package::parse_text(name, &parts[name]).map_err(|e| e.to_string());
        let roots: Vec<Element<'_>> = at.iter().map(|(name, place)| {
            if let Some(items) = fragments.get(*name) {
                if matches!(place, At::Story) || items.is_empty() { return Err(format!("{name} has invalid item fragments")); }
                let children = items.iter().map(|s| package::parse_text(name, s).map(crate::xml::Node::Element).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
                Ok(Element { children, ..Default::default() })
            } else { parse(name) }
        }).collect::<Result<_, String>>()?;
        let graphic_els: Vec<Element<'_>> = graphic_src.iter().map(|s| parse(s)).collect::<Result<_, _>>()?;
        let styles_els: Vec<Element<'_>> = styles_src.iter().map(|s| parse(s)).collect::<Result<_, _>>()?;
        let prefs_els: Vec<Element<'_>> = prefs_src.iter().map(|s| parse(s)).collect::<Result<_, _>>()?;
        // 5. TextPath links: the other parts' items resolve through them, so the changed parts must hold the same ones as before
        for ((name, _), root) in at.iter().zip(&roots) {
            let mut now = textpath::FrameIds::entries(root);
            let mut before = self.text_paths.get(*name).cloned().unwrap_or_default();
            if fragments.contains_key(*name) {
                // Only the unchanged owners inside the fragments belong to this partial tree.
                let mut owners = HashSet::new();
                let mut pending = vec![root];
                while let Some(e) = pending.pop() {
                    if items::is_item(e.name) && let Some(id) = e.attr("Self") { owners.insert(id); }
                    pending.extend(e.elements());
                }
                before.retain(|(_, owner)| owners.contains(owner.as_str()));
            }
            now.sort();
            before.sort();
            if now != before {
                return Err(format!("{name} has other TextPath links than before"));
            }
        }
        let frame_ids = textpath::FrameIds::from_entries(self.text_paths.values().flatten());
        let mut w = Warnings::default();
        let graphic: Vec<&Element<'_>> = graphic_els.iter().collect();
        let styles_parts: Vec<&Element<'_>> = styles_els.iter().collect();
        let prefs_parts: Vec<&Element<'_>> = prefs_els.iter().collect();
        let Globals { colors, sheet, stroke_styles, global_light, view } = globals(&dm, &graphic, &styles_parts, &prefs_parts, &mut w);
        let reader = items::ItemReader { text_path_frames: &frame_ids, sheet: &sheet, colors: &colors, previews: &data.previews, strokes: &stroke_styles, links: &data.links, embedded: &data.embedded, global_light };
        let conditions = crate::conditions::from_element(&dm);
        let hidden_conditions = conditions.iter().filter(|c| !c.visible).map(|c| c.id.clone()).collect();
        let story_reader = crate::text::StoryReader { sheet: &sheet, colors: &colors, items: &reader, hidden_conditions: &hidden_conditions, saved: self.sidecar.as_ref() };

        // 6. read each changed part
        let mut new_spreads: Vec<(bool, usize, Spread)> = Vec::new();
        let mut new_stories: Vec<Story> = Vec::new();
        let mut trap_groups: BTreeMap<usize, Vec<folio_model::traps::Assignment>> = BTreeMap::new();
        let mut toc: Vec<(String, Option<String>)> = Vec::new();
        for ((name, place), root) in at.iter().zip(&roots) {
            match place {
                At::Master(k) | At::Spread(k) => {
                    let master = matches!(place, At::Master(_));
                    let old = if master { &self.doc.master_spreads } else { &self.doc.spreads };
                    if fragments.contains_key(*name) {
                        let mut shell = old[*k].clone();
                        let elements = root.elements().collect::<Vec<_>>();
                        let mut positions = BTreeSet::new();
                        let mut work = Vec::new();
                        for e in elements {
                            if !items::is_item(e.name) { return Err(format!("{name} fragment is not a page item")); }
                            let id = e.attr("Self").ok_or_else(|| format!("{name} fragment has no Self"))?;
                            let found = shell.items.iter().enumerate().filter(|(_, i)| i.id == id).map(|(i, _)| i).collect::<Vec<_>>();
                            if found.len() != 1 || !positions.insert(found[0]) { return Err(format!("{name} fragment {id} is not one existing item")); }
                            work.push((found[0], e));
                        }
                        let read = read_items(&reader, &work, &mut w).map_err(|e| e.to_string())?;
                        if read.len() != work.len() { return Err(format!("{name} fragments did not all give items")); }
                        for (i, item) in read { shell.items[i] = item; }
                        new_spreads.push((master, *k, shell));
                        continue;
                    }
                    let mut counter = if master { folio_model::MASTER_PAGE_BASE } else { 0 } + old[..*k].iter().map(|s| s.pages.len()).sum::<usize>();
                    let (mut shell, elements) = reader.spread(root, master, &mut counter, &mut w).ok_or_else(|| format!("{name} gave no spread"))?;
                    let work: Vec<(usize, &Element<'_>)> = elements.into_iter().map(|e| (0, e)).collect();
                    for (_, item) in read_items(&reader, &work, &mut w).map_err(|e| e.to_string())? {
                        shell.items.push(item);
                    }
                    items::place_guides(&mut shell, &view.ruler_origin);
                    if shell.pages.len() != old[*k].pages.len() {
                        return Err(format!("{name} has another number of pages than before"));
                    }
                    if !master {
                        // the trap presets its pages are given (`traps::read` reads them from the spread parts, not the parents)
                        let pages = root.child("Spread").into_iter().flat_map(|s| s.children_named("Page")).filter_map(|p| {
                            Some(folio_model::traps::Assignment { page: p.attr("Self")?.into(), preset: p.attr("AppliedTrapPreset")?.into() })
                        });
                        trap_groups.insert(*k, pages.collect());
                    }
                    new_spreads.push((master, *k, shell));
                }
                At::Story => {
                    let (story, extras) = story_reader.story(root, &mut w);
                    let story = story.ok_or_else(|| format!("{name} gave no story"))?;
                    if name != &format!("{STORIES}Story_{}.xml", story.id) {
                        return Err(format!("{name} does not hold story {}", story.id));
                    }
                    let Some(old) = self.doc.stories.get(&story.id) else { return Err(format!("story {} is new", story.id)) };
                    // what the story says to the document-wide lists, before and after
                    let text = &parts[*name];
                    if let Some(m) = LIST_MARKERS.iter().find(|m| text.contains(**m)) {
                        return Err(format!("{name} holds {m}, which numbers or lists things across the document"));
                    }
                    if extras.is_endnote_story || !extras.endnote_refs.is_empty() || !extras.endnote_ranges.is_empty() {
                        return Err(format!("{name} holds endnotes"));
                    }
                    if old.all_runs().iter().any(|r| r.variable == Some(TextVariable::EndnoteNumber)) {
                        return Err(format!("story {} had endnotes", story.id));
                    }
                    let id = &story.id;
                    let h = &self.doc.hyperlinks;
                    if h.sources.iter().any(|s| &s.story == id) || h.destinations.iter().any(|d| &d.story == id) {
                        return Err(format!("story {id} had hyperlink sources or destinations"));
                    }
                    fn mentions(t: &folio_model::indexes::Topic, id: &str) -> bool {
                        t.references.iter().any(|r| r.story == id) || t.topics.iter().any(|t| mentions(t, id))
                    }
                    if self.doc.indexes.iter().any(|i| i.topics.iter().any(|t| mentions(t, id))) {
                        return Err(format!("story {id} had index references"));
                    }
                    let style = root.child("Story").and_then(|s| s.attr("AppliedTOCStyle")).filter(|s| *s != "n").map(str::to_owned);
                    toc.push((story.id.clone(), style));
                    new_stories.push(story);
                }
            }
        }
        // 7. the trap assignments of the spreads, in the order of the spreads (a page's group is found by the spread that holds it)
        let new_assignments = if trap_groups.is_empty() {
            None
        } else {
            let mut spread_of: HashMap<&str, usize> = HashMap::new();
            for (k, s) in self.doc.spreads.iter().enumerate() {
                for p in &s.pages {
                    if spread_of.insert(p.id.as_str(), k).is_some() {
                        return Err(format!("page {} is in two spreads", p.id));
                    }
                }
            }
            let mut groups: Vec<Vec<folio_model::traps::Assignment>> = vec![Vec::new(); self.doc.spreads.len()];
            for a in &self.doc.traps.assignments {
                let Some(k) = spread_of.get(a.page.as_str()) else { return Err(format!("trap assignment of unknown page {}", a.page)) };
                groups[*k].push(a.clone());
            }
            // the assignments must be in spread order already, or the replacement would not keep it
            let ordered = self.doc.traps.assignments.iter().map(|a| spread_of[a.page.as_str()]).collect::<Vec<_>>();
            if ordered.windows(2).any(|w| w[0] > w[1]) {
                return Err("trap assignments are not in spread order".into());
            }
            for (k, g) in trap_groups {
                groups[k] = g;
            }
            Some(groups.into_iter().flatten().collect::<Vec<_>>())
        };

        // 8. all went well: put the new in the place of the old
        for (master, k, shell) in new_spreads {
            if master { self.doc.master_spreads[k] = shell } else { self.doc.spreads[k] = shell }
        }
        for story in new_stories {
            self.doc.stories.insert(story.id.clone(), story);
        }
        for (id, style) in toc {
            match style {
                Some(style) => {
                    self.doc.toc_stories.insert(id, style);
                }
                None => {
                    self.doc.toc_stories.remove(&id);
                }
            }
        }
        if let Some(a) = new_assignments {
            self.doc.traps.assignments = a;
        }
        assign_sections(&mut self.doc.spreads, &dm);
        self.doc.fonts_used = fonts_used(&self.doc);
        self.doc.source.bytes = parts.values().map(|t| t.len() as u64).sum();
        Ok(())
    }
}
