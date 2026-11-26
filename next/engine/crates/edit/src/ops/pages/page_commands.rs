//! Apply page-panel commands to the document and its spreads.

use super::*;

pub(in crate::ops) fn run(w: &mut Work<'_>, c: &Command) -> Result<(), EditError> {
    run_with(w, c, false)
}

/// `run` for Adjust Layout, which resizes the pages of a right-to-left document by their place on the spread (layoutadj-c4).
pub(in crate::ops) fn run_resizing(w: &mut Work<'_>, c: &Command) -> Result<(), EditError> {
    run_with(w, c, matches!(c, Command::DocumentSetup { .. }))
}

fn run_with(w: &mut Work<'_>, c: &Command, right_to_left_ok: bool) -> Result<(), EditError> {
    let mut b = Book::read(w)?;
    b.check_numbering_styles()?;
    if b.pref("PageBinding") == "RightToLeft" && !right_to_left_ok {
        return Err(EditError::new(
            "unsupported",
            "page-panel editing of right-to-left binding has not been measured yet",
        ));
    }
    match c {
        Command::SetSpreadHidden { spreads, hidden } => {
            for id in spreads {
                let s = b.spreads.iter_mut().find(|s| s.id() == *id).ok_or_else(|| EditError::new("notFound", "spread"))?;
                set(&mut s.xml, "SpreadHidden", bool_text(*hidden).as_str());
            }
            b.position();
        }
        Command::SetPageShuffle { allowed, spread } => {
            if let Some(id) = spread {
                let s = b
                    .spreads
                    .iter_mut()
                    .find(|s| s.id() == *id)
                    .ok_or_else(|| EditError::new("notFound", "spread"))?;
                set(&mut s.xml, "AllowPageShuffle", bool_text(*allowed).as_str());
            } else {
                if *allowed && b.pref("PreserveLayoutWhenShuffling") == "true" {
                    for s in &mut b.spreads {
                        if s.pages.len() > 2 {
                            set(&mut s.xml, "AllowPageShuffle", "false")
                        }
                    }
                }
                b.set_pref("AllowPageShuffle", &allowed.to_string())
            }
            if *allowed {
                b.shuffle_and_position(w)?;
            }
        }
        Command::MovePages { pages, before } => move_pages(&mut b, w, pages, before.as_deref())?,
        Command::ApplyMaster { pages, master } => b = apply_master(b, w, pages, master.as_deref())?,
        Command::DeletePages { pages } => b = delete_pages(b, w, pages)?,
        Command::InsertPages {
            count,
            before,
            master,
        } => insert_pages(&mut b, w, *count, before.as_deref(), master.as_deref())?,
        Command::SetSection {
            page,
            start,
            style,
            prefix,
            include_prefix,
            marker,
        } => b.set_section(w, page, (*start, style), prefix, *include_prefix, marker)?,
        Command::DuplicateSpread { spread } => duplicate_spread(&mut b, w, spread)?,
        Command::DocumentSetup { width, height } => document_setup(&mut b, w, *width, *height)?,
        _ => unreachable!(),
    }
    b.write(w)
}

fn move_pages(b: &mut Book, w: &mut Work<'_>, pages: &[String], before: Option<&str>) -> Result<(), EditError> {
    let mut unique = BTreeSet::new();
    for id in pages {
        b.locate(id)?;
        if !unique.insert(id.as_str()) {
            return Err(EditError::new("invalid", "duplicate page"));
        }
    }
    if before.is_some_and(|id| unique.contains(id)) {
        return Err(EditError::new("invalid", "destination is a selected page"));
    }
    for id in pages {
        let (to, at) = if let Some(dest) = before {
            b.locate(dest)?
        } else {
            let to = b.spreads.len() - 1;
            (to, b.spreads[to].pages.len())
        };
        let (from, pos) = b.locate(id)?;
        if b.pref("FacingPages") != "true" {
            b.spreads[from].pages[pos].binding = get(&b.spreads[to].xml, "BindingLocation")
                .parse()
                .unwrap_or(0);
        }
        b.reparent(id, to, if from == to && pos < at { at - 1 } else { at })?;
        b.shuffle_and_position(w)?;
    }
    Ok(())
}

fn apply_master(mut b: Book, w: &mut Work<'_>, pages: &[String], master: Option<&str>) -> Result<Book, EditError> {
    for id in pages {
        // the same parent again takes the page's overrides away (their copies go, the parent's items show); another parent
        // (or none) leaves the copies as the page's own objects (oracle: parent-apply-same-after-override and -none-)
        let (s, p) = b.locate(id)?;
        if get(&b.spreads[s].pages[p].xml, "AppliedMaster") == master.unwrap_or("n") {
            b.write(w)?;
            super::overrides::remove_overrides(w, id, &[])?;
            b = Book::read(w)?;
        }
        b.apply_master(id, master)?
    }
    Ok(b)
}

fn delete_pages(mut b: Book, w: &mut Work<'_>, pages: &[String]) -> Result<Book, EditError> {
    let selected: BTreeSet<_> = pages.iter().collect();
    if selected.len() >= b.order().len() {
        return Err(EditError::new("invalid", "a document needs one page"));
    }
    for id in pages {
        b.locate(id)?;
    }
    // Delete complete page-owned objects, retaining the existing thread/story rules.
    for page in pages.iter().rev() {
        let ids: Vec<_> = b
            .spreads
            .iter()
            .flat_map(|s| s.items.iter())
            .filter(|i| i.page.as_deref() == Some(page))
            .map(|i| get(&i.xml, "Self"))
            .collect();
        for id in ids {
            delete(w, &id)?;
        }
        b = Book::read(w)?;
        let (s, p) = b.locate(page)?;
        b.spreads[s].pages.remove(p);
        b.spreads.retain(|s| !s.pages.is_empty());
        b.shuffle_and_position(w)?;
        b.write(w)?;
        b = Book::read(w)?;
    }
    Ok(b)
}

fn insert_pages(
    b: &mut Book,
    w: &mut Work<'_>,
    count: usize,
    before: Option<&str>,
    master: Option<&str>,
) -> Result<(), EditError> {
    if count == 0 || count > 9999 {
        return Err(EditError::new("invalid", "page count must be 1–9999"));
    }
    if let Some(id) = before {
        b.locate(id)?;
    }
    for _ in 0..count {
        let mut pg = b.spreads.last().unwrap().pages.last().unwrap().clone();
        pg.id = w.new_id();
        pg.binding = 1;
        // A blank page inherits page preferences, never the last page's guides,
        // script labels, accessibility order or Folio identity.
        let ix = scan(&pg.xml)?;
        let mut remove = Vec::new();
        for &i in ix.children(0) {
            if !["Properties", "MarginPreference", "GridDataInformation"]
                .contains(&ix.local(&pg.xml, i))
            {
                remove.push(element_span(&ix, &pg.xml, i));
            } else if ix.local(&pg.xml, i) == "Properties" {
                for &j in ix.children(i) {
                    if !["Descriptor", "PageColor"].contains(&ix.local(&pg.xml, j)) {
                        remove.push(element_span(&ix, &pg.xml, j));
                    }
                }
            }
        }
        remove.sort_by_key(|r| std::cmp::Reverse(r.start));
        for r in remove {
            pg.xml.replace_range(r, "");
        }
        set(&mut pg.xml, "Self", &pg.id);
        set(&mut pg.xml, "LayoutRule", "Off");
        set(&mut pg.xml, "OverrideList", "");
        set(&mut pg.xml, "TabOrder", "");
        // a colour label belongs to the page it was set on
        let cx = scan(&pg.xml)?;
        if let Some(c) = (0..cx.nodes.len()).find(|&k| cx.local(&pg.xml, k) == "PageColor" && !cx.nodes[k].empty) {
            let (a, z) = (cx.nodes[c].open_end, cx.nodes[c].close_start);
            pg.xml.replace_range(a..z, "UseMasterColor");
        }
        let width = b.pref("PageWidth");
        let height = b.pref("PageHeight");
        set(
            &mut pg.xml,
            "GeometricBounds",
            &format!("0 0 {height} {width}"),
        );
        set(&mut pg.xml, "ItemTransform", "1 0 0 1 0 0");
        let id = pg.id.clone();
        let mut s = b.new_spread(w);
        s.pages.push(pg);
        b.spreads.push(s);
        b.shuffle_and_position(w)?;
        b.apply_master(&id, master)?;
        if let Some(dest) = before {
            let (s, p) = b.locate(dest)?;
            b.reparent(&id, s, p)?;
            b.shuffle_and_position(w)?;
        }
        w.created.push(id);
    }
    Ok(())
}

fn duplicate_spread(b: &mut Book, w: &mut Work<'_>, spread: &str) -> Result<(), EditError> {
    let at = b
        .spreads
        .iter()
        .position(|s| s.id() == spread)
        .ok_or_else(|| EditError::new("notFound", "spread"))?;
    let source = b.spreads[at].output()?;
    let ix = scan(&source)?;
    let mut map = Vec::new();
    let mut edits = Vec::new();
    for i in 0..ix.nodes.len() {
        if let Some(id) = ix.attr(&source, i, "Self") {
            map.push((id.to_owned(), w.new_id()));
        }
    }
    let stories = super::clipboard::Source::of(w.ed);
    story_copy::copy_stories(w, &stories, &ix, &source, &mut map, &mut edits)?;
    for i in 0..ix.nodes.len() {
        for a in ix.attrs(&source, i) {
            let value = &source[a.value.0..a.value.1];
            if &source[a.name.0..a.name.1] == "OverrideList" {
                // "parent item, copy" pairs: the copies are the duplicate's own now, the parent items are the same
                let list: Vec<String> = value.split_whitespace().map(|v| map.iter().find(|(old, _)| old == v).map_or_else(|| v.to_owned(), |(_, n)| n.clone())).collect();
                edits.push((a.value.0..a.value.1, list.join(" ")));
            } else if let Some((_, id)) = map.iter().find(|(old, _)| old == value) {
                edits.push((a.value.0..a.value.1, id.clone()));
            }
        }
        if let Some(edit) = clipboard::copied_profile(&ix, &source, i) { edits.push(edit); }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut copy = source;
    for (r, s) in edits {
        copy.replace_range(r, &s)
    }
    let id = map.iter().find(|(old, _)| old == spread).unwrap().1.clone();
    let s = Spread::read(format!("Spreads/Spread_{id}.xml"), &copy)?;
    w.created.extend(s.pages.iter().map(|p| p.id.clone()));
    b.spreads.insert(at + 1, s);
    // Story-copy has registered the new stories and updated StoryList.
    b.dm = w.text("designmap.xml")?.to_owned();
    let original = get(w.text("designmap.xml")?, "StoryList");
    let mut stories: Vec<_> = original.split_whitespace().map(str::to_owned).collect();
    let used: BTreeSet<_> = b
        .masters
        .iter()
        .chain(&b.spreads)
        .flat_map(|s| s.items.iter())
        .flat_map(|it| {
            let ix = Index::scan(&it.xml).unwrap();
            (0..ix.nodes.len())
                .filter_map(|i| ix.value(&it.xml, i, "ParentStory"))
                .collect::<Vec<_>>()
        })
        .collect();
    stories.sort_by_key(|s| {
        (
            !used.contains(s),
            u64::from_str_radix(s.trim_start_matches('u'), 16).unwrap_or(u64::MAX),
        )
    });
    set(&mut b.dm, "StoryList", &stories.join(" "));
    b.shuffle_and_position(w)?;
    Ok(())
}

fn document_setup(b: &mut Book, w: &mut Work<'_>, width: f64, height: f64) -> Result<(), EditError> {
    if !width.is_finite()
        || !height.is_finite()
        || width < 1.
        || height < 1.
        || width > 15552.
        || height > 15552.
    {
        return Err(EditError::new(
            "invalid",
            "page dimensions must be 1–15552 pt",
        ));
    }
    let old_width: f64 = b
        .pref("PageWidth")
        .parse()
        .map_err(|_| EditError::new("xml", "page width"))?;
    let px = scan(&b.prefs)?;
    let mut flags = Vec::new();
    for i in 0..px.nodes.len() {
        for (tag, attr) in [("LayoutAdjustmentPreference", "EnableLayoutAdjustment"), ("AdjustLayoutPreference", "EnableAdjustLayout")] {
            if px.local(&b.prefs, i) == tag { flags.push(attr_edit(&px, &b.prefs, i, attr, "false")); }
        }
    }
    flags.sort_by_key(|(r,_)|std::cmp::Reverse(r.start));
    for (r,v) in flags { b.prefs.replace_range(r,&v); }
    let old_height: f64 = b
        .pref("PageHeight")
        .parse()
        .map_err(|_| EditError::new("xml", "page height"))?;
    let facing = b.pref("FacingPages") == "true";
    for s in b.masters.iter_mut().chain(b.spreads.iter_mut()) {
        let master = s.part.starts_with("MasterSpreads/");
        for pg in &mut s.pages {
            let mut r = bounds(&pg.xml);
            let m = mat(&pg.xml);
            let left = r[0] + m.0[4] < -0.001;
            let cy = (r[1] + r[3]) / 2.;
            r[1] = cy - height / 2.;
            r[3] = cy + height / 2.;
            if left {
                r[0] = r[2] - width
            } else {
                r[2] = r[0] + width
            }
            set(
                &mut pg.xml,
                "GeometricBounds",
                &[r[1], r[0], r[3], r[2]]
                    .iter()
                    .map(|n| geom::num(*n))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            if !master && !facing {
                shift(&mut pg.xml, (width - old_width) / 2., 0.);
            }
            geometry::update_columns(&mut pg.xml, width)?;
        }
        // InDesign centers the parent's items horizontally in the resized page;
        // document-page objects retain their spread coordinates with Adjust Layout off.
        if master && (width < old_width || (width > height) != (old_width > old_height))
        {
            for it in &mut s.items {
                let ix = scan(&it.xml)?;
                if let Some(r) = spread_bounds(&ix, &it.xml, 0) {
                    let cx = (r[0] + r[2]) / 2.;
                    shift(&mut it.xml, cx * (width / old_width - 1.), 0.);
                }
            }
        } else if !master && !facing {
            for it in &mut s.items {
                shift(&mut it.xml, (width - old_width) / 2., 0.);
            }
        }
    }
    b.set_pref("PageWidth", &geom::num(width));
    b.set_pref("PageHeight", &geom::num(height));
    let mut y = 0.;
    for s in &mut b.spreads {
        let mut m = mat(&s.xml);
        m.0[5] = y;
        set(&mut s.xml, "ItemTransform", &m.text());
        y += height + b.gap;
    }
    for s in &b.masters {
        rewrite(w, &s.part, s.output()?)?;
    }
    Ok(())
}
