//! Load, reorder and save the document's spreads, pages and preferences.

use super::*;

pub(super) struct Book {
    pub(super) spreads: Vec<Spread>,
    pub(super) masters: Vec<Spread>,
    pub(super) prefs: String,
    pub(super) sections: Vec<String>,
    pub(super) dm: String,
    pub(super) old_parts: Vec<String>,
    pub(super) gap: f64,
}

impl Book {
    pub(super) fn read(w: &Work<'_>) -> Result<Self, EditError> {
        let dm = w.text("designmap.xml")?.to_owned();
        let ix = scan(&dm)?;
        let mut spreads = Vec::new();
        let mut masters = Vec::new();
        let mut sections = Vec::new();
        for i in 0..ix.nodes.len() {
            match ix.local(&dm, i) {
                "Spread" | "MasterSpread" if ix.attr(&dm, i, "src").is_some() => {
                    let part = ix.value(&dm, i, "src").unwrap();
                    let s = Spread::read(part.clone(), w.text(&part)?)?;
                    if ix.local(&dm, i) == "Spread" {
                        spreads.push(s)
                    } else {
                        masters.push(s)
                    }
                }
                "Section" => sections.push(element(&dm, &ix, i)),
                _ => {}
            }
        }
        let prefs = w.text("Resources/Preferences.xml")?.to_owned();
        if spreads.is_empty() || spreads.iter().any(|s| s.pages.is_empty()) {
            return Err(EditError::new("invalid", "document has an empty spread"));
        }
        let old_parts = spreads.iter().map(|s| s.part.clone()).collect();
        let height = |s: &Spread| {
            s.pages
                .iter()
                .map(|p| {
                    let r = bounds(&p.xml);
                    r[3] - r[1]
                })
                .fold(0., f64::max)
        };
        let gap = if spreads.len() > 1 {
            (mat(&spreads[1].xml).0[5]
                - mat(&spreads[0].xml).0[5]
                - (height(&spreads[0]) + height(&spreads[1])) / 2.)
                .max(0.)
        } else {
            180.
        };
        let mut book = Self {
            spreads,
            masters,
            prefs,
            sections,
            dm,
            old_parts,
            gap,
        };
        // the room between spreads read off a layout that has turned spreads in it would be the turned shape's
        if book.spreads.iter().any(|s| s.rotation() != 0) {
            book.gap = 36. + 2. * book.pasteboard_space();
        }
        Ok(book)
    }
    pub(super) fn pref(&self, k: &str) -> String {
        let ix = Index::scan(&self.prefs).unwrap();
        (0..ix.nodes.len())
            .find(|&i| ix.local(&self.prefs, i) == "DocumentPreference")
            .and_then(|i| ix.value(&self.prefs, i, k))
            .unwrap_or_default()
    }
    /// `MinimumSpaceAboveAndBelow`: the room above and below each spread on the pasteboard.
    pub(super) fn pasteboard_space(&self) -> f64 {
        let ix = Index::scan(&self.prefs).unwrap();
        (0..ix.nodes.len())
            .find(|&i| ix.local(&self.prefs, i) == "PasteboardPreference")
            .and_then(|i| ix.value(&self.prefs, i, "MinimumSpaceAboveAndBelow"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(72.)
    }
    pub(super) fn set_pref(&mut self, k: &str, v: &str) {
        let ix = Index::scan(&self.prefs).unwrap();
        let i = (0..ix.nodes.len())
            .find(|&i| ix.local(&self.prefs, i) == "DocumentPreference")
            .unwrap();
        let (r, v) = attr_edit(&ix, &self.prefs, i, k, v);
        self.prefs.replace_range(r, &v);
    }
    pub(super) fn order(&self) -> Vec<String> {
        self.spreads
            .iter()
            .flat_map(|s| s.pages.iter().map(|p| p.id.clone()))
            .collect()
    }
    pub(super) fn locate(&self, id: &str) -> Result<(usize, usize), EditError> {
        self.spreads
            .iter()
            .enumerate()
            .find_map(|(s, v)| v.pages.iter().position(|p| p.id == id).map(|p| (s, p)))
            .ok_or_else(|| EditError::new("notFound", format!("page {id}")))
    }
    pub(super) fn new_spread(&self, w: &mut Work<'_>) -> Spread {
        let mut s = self.spreads.last().unwrap().clone();
        s.pages.clear();
        s.items.clear();
        let id = w.new_id();
        set(&mut s.xml, "Self", &id);
        set(&mut s.xml, "AllowPageShuffle", "true");
        s.part = format!("Spreads/Spread_{id}.xml");
        s
    }
    pub(super) fn reparent(&mut self, id: &str, to: usize, at: usize) -> Result<(), EditError> {
        let (from, p) = self.locate(id)?;
        let page = self.spreads[from].pages.remove(p);
        let at = at.min(self.spreads[to].pages.len());
        self.spreads[to].pages.insert(at, page);
        if from != to {
            let mut moved = Vec::new();
            self.spreads[from].items.retain(|i| {
                if i.page.as_deref() == Some(id) {
                    moved.push(i.clone());
                    false
                } else {
                    true
                }
            });
            let ix = scan(&self.dm)?;
            let layers: Vec<_> = (0..ix.nodes.len())
                .filter(|&i| ix.local(&self.dm, i) == "Layer")
                .filter_map(|i| ix.value(&self.dm, i, "Self"))
                .collect();
            let rank = |l: &str| layers.iter().position(|s| s == l).unwrap_or(0);
            for item in moved {
                let layer = get(&item.xml, "ItemLayer");
                let dest = &mut self.spreads[to].items;
                let at = dest
                    .iter()
                    .rposition(|i| get(&i.xml, "ItemLayer") == layer)
                    .map(|i| i + 1)
                    .or_else(|| {
                        dest.iter()
                            .position(|i| rank(&get(&i.xml, "ItemLayer")) > rank(&layer))
                    })
                    .unwrap_or(dest.len());
                dest.insert(at, item);
            }
        }
        Ok(())
    }
    pub(super) fn shuffle(&mut self, w: &mut Work<'_>) -> Result<(), EditError> {
        if self.pref("AllowPageShuffle") == "false" {
            return Ok(());
        }
        self.spreads.retain(|s| !s.pages.is_empty());
        let facing = self.pref("FacingPages") == "true";
        let mut index = 0;
        let numbers = self.numbers();
        let order = self.order();
        while index < self.spreads.len() {
            if !self.spreads[index].allowed() {
                index += 1;
                continue;
            }
            if self.spreads[index].pages.is_empty() {
                self.spreads.remove(index);
                continue;
            }
            let first = &self.spreads[index].pages[0].id;
            let number = numbers[first];
            let next = order
                .iter()
                .position(|id| id == first)
                .and_then(|p| order.get(p + 1))
                .map(|id| numbers[id]);
            let count = if !facing || number % 2 == 1 || next.is_some_and(|n| n % 2 == 0) {
                1
            } else {
                2
            };
            while self.spreads[index].pages.len() > count {
                if index + 1 == self.spreads.len() {
                    let s = self.new_spread(w);
                    self.spreads.push(s);
                }
                let id = self.spreads[index].pages.last().unwrap().id.clone();
                self.reparent(&id, index + 1, 0)?;
            }
            while self.spreads[index].pages.len() < count
                && index + 1 < self.spreads.len()
                && self.spreads[index + 1].allowed()
            {
                if self.spreads[index + 1].pages.is_empty() {
                    self.spreads.remove(index + 1);
                    continue;
                }
                let id = self.spreads[index + 1].pages[0].id.clone();
                let at = self.spreads[index].pages.len();
                self.reparent(&id, index, at)?;
            }
            index += 1;
        }
        self.spreads.retain(|s| !s.pages.is_empty());
        Ok(())
    }
    pub(super) fn write(mut self, w: &mut Work<'_>) -> Result<(), EditError> {
        self.number();
        let geometry = |s: &Spread| {
            s.pages
                .iter()
                .map(|p| {
                    (
                        p.id.clone(),
                        [
                            "Name",
                            "GeometricBounds",
                            "ItemTransform",
                            "AppliedMaster",
                            "MasterPageTransform",
                        ]
                        .map(|k| get(&p.xml, k)),
                    )
                })
                .collect::<Vec<_>>()
        };
        let old_geometry = self
            .old_parts
            .iter()
            .filter_map(|p| w.text(p).ok().and_then(|t| Spread::read(p.clone(), t).ok()))
            .flat_map(|s| geometry(&s))
            .collect::<Vec<_>>();
        let new_geometry = self.spreads.iter().flat_map(geometry).collect::<Vec<_>>();
        let recompose = old_geometry != new_geometry;
        for s in &self.spreads {
            rewrite(w, &s.part, s.output()?)?;
        }
        for p in &self.old_parts {
            if !self.spreads.iter().any(|s| s.part == *p) {
                w.remove_part(p)
            }
        }
        let ix = scan(&self.dm)?;
        let root = ix.roots().next().unwrap();
        let mut changes = Vec::new();
        for (tag, strings) in [
            (
                "Spread",
                self.spreads
                    .iter()
                    .map(|s| format!("<idPkg:Spread src=\"{}\"/>", escape(&s.part)))
                    .collect::<Vec<_>>(),
            ),
            ("Section", self.sections.clone()),
        ] {
            let nodes = ix.named(&self.dm, root, tag);
            for (k, &i) in nodes.iter().enumerate() {
                changes.push((
                    element_span(&ix, &self.dm, i),
                    if k == 0 {
                        format!("\n\t{}", strings.join("\n\t"))
                    } else {
                        String::new()
                    },
                ));
            }
            if nodes.is_empty() {
                changes.push((
                    ix.nodes[root].close_start..ix.nodes[root].close_start,
                    strings.join("\n\t"),
                ));
            }
        }
        changes.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        for (r, s) in changes {
            self.dm.replace_range(r, &s)
        }
        rewrite(w, "designmap.xml", self.dm)?;
        rewrite(w, "Resources/Preferences.xml", self.prefs)?;
        if recompose {
            for p in w.parts_named("Stories/") {
                let t = w.text(&p)?.to_owned();
                let ix = scan(&t)?;
                for i in 0..ix.nodes.len() {
                    if ix.local(&t, i) == "Story"
                        && let Some(id) = ix.attr(&t, i, "Self")
                    {
                        w.recompose(id)
                    }
                }
            }
        }
        Ok(())
    }
}
