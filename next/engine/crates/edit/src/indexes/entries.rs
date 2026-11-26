use super::{invalid, order, sort_options::SortOptions};
use crate::{EditError, Work};
use folio_model::{indexes::{CrossReference, Topic}, toc::TextLocation, Document};
use std::collections::HashMap;
use super::range::{self, Pages};

/// `marker`: the index marker's `Id` the page number links to (empty for a page of another book document); `doc`: the
/// document's place in the book (0 when the index is the document's own).
pub(super) struct Page { pub first: String, pub last: String, pub marker: String, pub index: usize, pub end: usize, pub doc: usize }
/// `cross`: the printed "See also X" parts of the topic's cross references, in the topic's order (index-c6).
/// `path`: the topic's names from level 1 (a book-wide index merges the documents' topics by it); `key`: its sort key.
pub(super) struct Entry { pub name: String, pub printed: String, pub level: usize, pub heading: String, pub pages: Vec<Page>, pub cross: Vec<Cross>, pub path: Vec<String>, pub key: String }

/// One printed cross reference: `lead` and `tail` are plain text, `words` ("See also", a custom text) has the character style `Index Cross-reference`.
#[derive(Clone, PartialEq)]
pub(super) struct Cross { pub lead: String, pub words: String, pub tail: String }

pub(super) fn collect(w: &mut Work<'_>, doc: &Document, topics: &[Topic], locations: &[TextLocation], opts: &SortOptions) -> Result<Vec<Entry>, EditError> {
    let mut out = Vec::new();
    let mut paths = HashMap::new();
    paths_of(topics, "", &mut paths);
    let mut marker = |id: &str| -> Result<String, EditError> {
        let (part, ix, node) = w.find(id)?;
        ix.value(w.text(&part)?, node, "Id").ok_or_else(|| invalid("index marker has no Id"))
    };
    walk(&mut marker, doc, &Pages::new(doc), &paths, topics, locations, &[], opts, &mut out)?;
    Ok(out)
}

/// The printed entries of a document's index (its first index), for a book-wide index made in another document of the book:
/// the same entries a generation in this document prints, without the links to its markers.
pub fn book_entries(doc: &Document, locations: &[TextLocation]) -> Result<Vec<folio_model::indexes::BookIndexEntry>, EditError> {
    use folio_model::indexes::{BookIndexCross, BookIndexEntry, BookIndexPage};
    let Some(index) = doc.indexes.first() else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    let mut paths = HashMap::new();
    paths_of(&index.topics, "", &mut paths);
    walk(&mut |_: &str| Ok(String::new()), doc, &Pages::new(doc), &paths, &index.topics, locations, &[], &SortOptions::default(), &mut out)?;
    Ok(out.into_iter().map(|e| BookIndexEntry {
        path: e.path, key: e.key, printed: e.printed,
        pages: e.pages.into_iter().map(|p| BookIndexPage { first: p.first, last: p.last, index: p.index, end: p.end }).collect(),
        cross: e.cross.into_iter().map(|c| BookIndexCross { lead: c.lead, words: c.words, tail: c.tail }).collect(),
    }).collect())
}

/// A book-wide index (measured: artifacts/build-b3-20261023/bookindex): the topics of every document of the book together.
/// Topics of the same name path are one entry (names equal under the collation, as a topic is not added twice); its pages
/// are in the book's order (each document's own page names), then its cross references, each once. The entries are sorted
/// as one document's are. `own` is this document's entries; `book` lists the book's documents in order, this one `own`.
pub(super) fn merge_book(own: Vec<Entry>, book: &[folio_model::indexes::BookIndexDocument], opts: &SortOptions) -> Result<Vec<Entry>, EditError> {
    struct Node { entry: Entry, children: Vec<Node> }
    fn insert(nodes: &mut Vec<Node>, mut entry: Entry, depth: usize, opts: &SortOptions) {
        let name = entry.path[depth].clone();
        let at = nodes.iter().position(|n| n.entry.path[depth] == name || order::same_topic(&n.entry.path[depth], &name));
        if depth + 1 == entry.path.len() {
            match at {
                Some(i) => {
                    let into = &mut nodes[i].entry;
                    into.pages.append(&mut entry.pages);
                    for c in entry.cross { if !into.cross.contains(&c) { into.cross.push(c); } }
                }
                None => nodes.push(Node { entry, children: Vec::new() }),
            }
            return;
        }
        let i = at.unwrap_or_else(|| {
            // (a parent prints before its subtopics, so it is there; kept safe for a list that does not)
            let path = entry.path[..=depth].to_vec();
            nodes.push(Node { entry: Entry { name: name.clone(), printed: name.clone(), level: depth + 1, heading: opts.heading(&name), pages: Vec::new(), cross: Vec::new(), path, key: name.clone() }, children: Vec::new() });
            nodes.len() - 1
        });
        insert(&mut nodes[i].children, entry, depth + 1, opts);
    }
    fn flatten(mut nodes: Vec<Node>, top: bool, out: &mut Vec<Entry>, opts: &SortOptions) {
        nodes.sort_by(|a, b| opts.compare(top, &a.entry.key, &b.entry.key));
        for mut n in nodes {
            n.entry.pages.sort_by_key(|p| (p.doc, p.index, p.end));
            out.push(n.entry);
            flatten(n.children, false, out, opts);
        }
    }
    let own_at = book.iter().position(|d| d.own);
    let mut roots = Vec::new();
    let mut add = |entries: Vec<Entry>, doc: usize| -> Result<(), EditError> {
        for mut e in entries {
            if e.path.is_empty() || e.path.len() > 4 { return Err(invalid("책 문서의 색인 항목이 올바르지 않습니다")); }
            for p in &mut e.pages { p.doc = doc; }
            insert(&mut roots, e, 0, opts);
        }
        Ok(())
    };
    let mut own = Some(own);
    for (i, d) in book.iter().enumerate() {
        let doc = if own_at.is_some() { i } else { i + 1 };
        if d.own {
            add(own.take().unwrap_or_default(), doc)?;
            continue;
        }
        let entries = d.entries.iter().map(|b| {
            let level = b.path.len();
            if level == 1 && !opts.supported(&b.key, true) {
                return Err(EditError::new("unsupported", "측정하지 않은 색인 정렬 문자 또는 5단계 주제입니다"));
            }
            Ok(Entry { name: b.path.last().cloned().unwrap_or_default(), printed: b.printed.clone(), level, heading: opts.heading(&b.key),
                pages: b.pages.iter().map(|p| Page { first: p.first.clone(), last: p.last.clone(), marker: String::new(), index: p.index, end: p.end, doc }).collect(),
                cross: b.cross.iter().map(|c| Cross { lead: c.lead.clone(), words: c.words.clone(), tail: c.tail.clone() }).collect(),
                path: b.path.clone(), key: b.key.clone() })
        }).collect::<Result<Vec<_>, _>>()?;
        add(entries, doc)?;
    }
    if let Some(own) = own { add(own, 0)?; }
    drop(add);
    let mut out = Vec::new();
    flatten(roots, true, &mut out, opts);
    Ok(out)
}

/// Every topic's id with its name path (`Parent: Child`, how a cross reference prints the topic it points to).
fn paths_of(topics: &[Topic], parent: &str, out: &mut HashMap<String, String>) {
    for t in topics {
        let path = if parent.is_empty() { t.name.clone() } else { format!("{parent}: {}", t.name) };
        paths_of(&t.topics, &path, out);
        out.insert(t.id.clone(), path);
    }
}

/// What a cross reference prints (index-c6, measured: `See Target`, `See also Target`, `See herein Target`, `See also herein Target`,
/// custom text before or after the topic; a no-break space between the words and the topic). `[See also]` is `See also` after page numbers and `See` without any.
fn cross_text(c: &CrossReference, target: &str, has_pages: bool) -> Result<Cross, EditError> {
    let nb = "\u{a0}";
    let before = |words: &str| Cross { lead: String::new(), words: words.into(), tail: format!("{nb}{target}") };
    Ok(match c.kind.as_str() {
        "See" => before("See"),
        "SeeAlso" => before("See also"),
        "SeeHerein" => before("See herein"),
        "SeeAlsoHerein" => before("See also herein"),
        "SeeOrAlsoBracket" => before(if has_pages { "See also" } else { "See" }),
        "CustomCrossReferenceBefore" | "CustomCrossReference" => before(&c.custom),
        "CustomCrossReferenceAfter" => Cross { lead: format!("{target}{nb}"), words: c.custom.clone(), tail: String::new() },
        other => return Err(EditError::new("unsupported", format!("측정하지 않은 상호 참조 종류입니다: {other}"))),
    })
}

// Recursive topic traversal carries document context and the output accumulator without copying either.
#[allow(clippy::too_many_arguments)]
fn walk(marker_of: &mut dyn FnMut(&str) -> Result<String, EditError>, doc: &Document, pages_of: &Pages, paths: &HashMap<String, String>, topics: &[Topic], locations: &[TextLocation], parent: &[String], opts: &SortOptions, out: &mut Vec<Entry>) -> Result<(), EditError> {
    let level = parent.len() + 1;
    let key = |t: &Topic| if t.sort_order.is_empty() { t.name.clone() } else { t.sort_order.clone() };
    let mut sorted: Vec<_> = topics.iter().collect();
    sorted.sort_by(|a,b| opts.compare(level == 1, &key(a), &key(b)));
    for topic in sorted {
        // a topic with hidden and ordinary references prints the ordinary ones' pages (measured: index-c5-mixed)
        let k = key(topic);
        if !opts.supported(&k, level == 1) || level > 4 {
            return Err(EditError::new("unsupported", "측정하지 않은 색인 정렬 문자 또는 5단계 주제입니다"));
        }
        let printed = super::quotes::printed(&topic.name).ok_or_else(|| EditError::new("unsupported", "측정하지 않은 위치의 곧은 따옴표가 든 주제입니다"))?;
        let mut pages: Vec<Page> = Vec::new();
        let mut suppressed = false;
        for r in &topic.references {
            if r.kind == "SuppressPageNumbers" { suppressed = true; continue; }
            // a marker in a footnote or table is on the page of the place the footnote or table is anchored at; only a single page is made for it
            if r.nested && r.kind != "CurrentPage" { return Err(EditError::new("unsupported", "각주·표 안의 색인 참조는 현재 쪽 종류만 만들 수 있습니다")); }
            let at = if r.nested { r.anchor } else { r.at };
            let lines: Vec<_> = locations.iter().filter(|l| l.source.story == r.story && !l.source.hidden).collect();
            // text past the last composed line is overset: it has no page, so the reference prints no page number (measured: index6-overset)
            if !lines.is_empty() && lines.iter().all(|l| l.end <= at) { continue; }
            let found = lines.iter().find(|l| l.start <= at && at < l.end).copied()
                // (a table has no line of its own in the story: it is on the page of the text before it, else of the text after it)
                .or_else(|| if r.nested { lines.iter().filter(|l| l.start <= at).max_by_key(|l| l.start).or_else(|| lines.iter().min_by_key(|l| l.start)).copied() } else { None });
            let first = found.ok_or_else(|| EditError::new("staleLayout", "색인 참조가 조판된 쪽에 없습니다"))?;
            let end = if r.kind == "CurrentPage" { first.source.page_index } else { range::end_page(doc, pages_of, &lines, r, first)? };
            let last_name = pages_of.name(end).ok_or_else(|| EditError::new("staleLayout", "색인 범위의 끝 쪽을 찾을 수 없습니다"))?.to_owned();
            let marker = marker_of(&r.id)?;
            if let Some(same) = pages.iter_mut().find(|p| p.index == first.source.page_index && p.end == end) {
                if marker.parse::<u64>().unwrap_or(u64::MAX) < same.marker.parse::<u64>().unwrap_or(u64::MAX) { same.marker = marker; }
            } else {
                pages.push(Page { first: first.source.page.clone(), last: last_name, marker,
                    index: first.source.page_index, end, doc: 0 });
            }
        }
        pages.sort_by_key(|p| (p.index, p.end));
        let here = out.len();
        let cross = topic.cross_references.iter().map(|c| {
            let target = paths.get(&c.referenced).ok_or_else(|| invalid("상호 참조가 없는 주제를 가리킵니다"))?;
            cross_text(c, target, !pages.is_empty())
        }).collect::<Result<Vec<_>, _>>()?;
        let mut path = parent.to_vec();
        path.push(topic.name.clone());
        out.push(Entry { name: topic.name.clone(), printed, level, heading: opts.heading(&k), pages, cross, path: path.clone(), key: k.clone() });
        walk(marker_of, doc, pages_of, paths, &topic.topics, locations, &path, opts, out)?;
        // a topic that has only a cross reference is printed (measured: `Lonely. See Void`)
        if out.len() == here + 1 && out[here].pages.is_empty() && out[here].cross.is_empty() && !suppressed { out.pop(); }
    }
    Ok(())
}
