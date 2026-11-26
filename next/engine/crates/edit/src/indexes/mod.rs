//! Index topics and page references (index-c1). A topic is a `Topic` inside designmap's `Index`; a
//! reference is a `PageReference` marker in the story text. The marker is one U+FEFF in InDesign's
//! character space (measured: a story of 70 characters has 71 with one marker), so it is a text
//! atom (`text::replace_with_node`, `text::delete`) and keeps its place when the text around it is
//! edited. Rules and measurements: `next/oracle/rules/index-c1.ko.md`, scenarios `index-c1`.
use crate::xml::{escape, Index};
use crate::{EditError, Work};
mod order;
mod han;
mod cross;
mod quotes;
mod generate;
mod generated_styles;
mod generation_input;
mod entries;
mod range;
mod generation_styles;
mod capitalize;
pub(crate) use capitalize::capitalize;
mod import;
pub(crate) use import::import_topics;
pub(crate) use cross::{add as add_cross_reference, delete as delete_cross_reference};
pub(crate) use generate::generate;
pub use entries::book_entries;
pub(crate) use generation_styles::set as set_generation_styles;
pub mod sort_options;
pub(crate) use sort_options::set as set_sort_options;

fn invalid(m: impl Into<String>) -> EditError {
    EditError::new("invalid", m)
}

/// Reference kinds InDesign wrote for `PageReferenceType` (`PageReferenceType` attribute) and the
/// attribute that carries the count of the two counted kinds.
const KINDS: &[&str] = &[
    "CurrentPage", "ToNextStyleChange", "ToEndOfStory", "ToEndOfDocument", "ToEndOfSection",
    "ForNextNParagraphs", "ForNextNPages", "SuppressPageNumbers",
];

fn check_name(name: &str) -> Result<(), EditError> {
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(invalid("a topic needs a name without control characters"));
    }
    Ok(())
}

/// The `Index` element of designmap (created before the first backing story when the document has none).
fn index_node(w: &mut Work<'_>) -> Result<(Index, usize), EditError> {
    let ix = w.index("designmap.xml")?;
    let t = w.text("designmap.xml")?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    if let Some(&i) = ix.children(root).iter().find(|&&c| ix.local(t, c) == "Index") {
        return Ok((ix, i));
    }
    let at = ix.children(root).iter().find(|&&c| ix.local(t, c) == "BackingStory").map(|&c| ix.nodes[c].start)
        .unwrap_or(ix.nodes[root].close_start);
    let id = w.new_id();
    w.splice("designmap.xml", vec![(at..at, format!("<Index Self=\"{id}\" />\n\t"))])?;
    let ix = w.index("designmap.xml")?;
    let t = w.text("designmap.xml")?;
    let root = ix.roots().next().ok_or_else(|| invalid("missing document"))?;
    let i = *ix.children(root).iter().find(|&&c| ix.local(t, c) == "Index").ok_or_else(|| invalid("index not created"))?;
    Ok((ix, i))
}

fn topic_name(ix: &Index, t: &str, i: usize) -> String {
    ix.value(t, i, "Name").unwrap_or_default()
}

/// A topic's `Self`: the owner's, `Topicn` and the name. InDesign writes a colon of the name as a hyphen (a `Self` is an XML name:
/// the topic `:q` is `…Topicn-q`, measured: index-c5-quote-a; no other character of the 130 measured names is changed).
fn topic_id(owner: &str, name: &str) -> String {
    format!("{owner}Topicn{}", name.replace(':', "-"))
}

/// Adds a topic under `parent` (a topic id; empty: top level of the document's index). A topic of
/// that name that exists already is returned, not duplicated (measured: `topics.add` of a taken name).
pub(crate) fn add_topic(w: &mut Work<'_>, parent: &String, name: &str, sort_order: &str) -> Result<(), EditError> {
    let name = name.trim();
    check_name(name)?;
    let (ix, container) = if parent.is_empty() {
        index_node(w)?
    } else {
        let (part, ix, i) = w.find(parent)?;
        if part != "designmap.xml" || ix.local(w.text(&part)?, i) != "Topic" {
            return Err(invalid(format!("{parent} is not a topic")));
        }
        (ix, i)
    };
    let t = w.text("designmap.xml")?;
    let owner = ix.value(t, container, "Self").unwrap_or_default();
    if let Some(&c) = ix.children(container).iter().find(|&&c| ix.local(t, c) == "Topic" && (topic_name(&ix, t, c) == name || order::same_topic(&topic_name(&ix, t, c), name))) {
        w.created.push(ix.value(t, c, "Self").unwrap_or_default());
        return Ok(());
    }
    let id = topic_id(&owner, name);
    if ix.by_self(t, &id).is_some() {
        return Err(invalid(format!("{id} exists already")));
    }
    let raw = format!("<Topic Self=\"{}\" SortOrder=\"{}\" Name=\"{}\" />", escape(&id), escape(sort_order), escape(name));
    let n = &ix.nodes[container];
    let line = t[..n.start].rfind('\n').map_or(0, |p| p + 1);
    let indent = if t[line..n.start].chars().all(char::is_whitespace) { &t[line..n.start] } else { "" };
    let key = if sort_order.is_empty() { name } else { sort_order };
    let top = parent.is_empty();
    let before = ix.children(container).iter().copied().find(|&c| {
        ix.local(t, c) == "Topic" && {
            let so = ix.value(t, c, "SortOrder").unwrap_or_default();
            let k = if so.is_empty() { topic_name(&ix, t, c) } else { so };
            order::compare(top, key, &k) == std::cmp::Ordering::Less
        }
    });
    // a topic's cross references come after its subtopics, however they were added (measured: index6-see-more)
    let before = before.or_else(|| ix.children(container).iter().copied().find(|&c| ix.local(t, c) == "CrossReference"));
    let edit = if let Some(c) = before {
        let s = ix.nodes[c].start;
        let sl = t[..s].rfind('\n').map_or(0, |p| p + 1);
        let si = if t[sl..s].chars().all(char::is_whitespace) { &t[sl..s] } else { "" };
        (s..s, format!("{raw}\n{si}"))
    } else if n.empty {
        let tag = ix.local(t, container);
        (n.start..n.end, format!("{}>\n{indent}\t{raw}\n{indent}</{tag}>", t[n.start..n.open_end - 2].trim_end()))
    } else {
        (n.close_start..n.close_start, format!("\t{raw}\n{indent}"))
    };
    w.splice("designmap.xml", vec![edit])?;
    w.created.push(id);
    Ok(())
}

/// A topic's name and sort order (an empty sort order: sorted by name). InDesign names a topic's `Self`
/// after the path of its names, so a rename changes the `Self` of the topic and its subtopics and
/// the `ReferencedTopic` of every reference to them (measured: index-topic-rename).
pub(crate) fn set_topic(w: &mut Work<'_>, id: &String, name: &str, sort_order: &str) -> Result<(), EditError> {
    let name = name.trim();
    check_name(name)?;
    let (part, ix, i) = w.find(id)?;
    let text = w.text(&part)?.to_owned();
    let t = text.as_str();
    if part != "designmap.xml" || ix.local(t, i) != "Topic" {
        return Err(invalid(format!("{id} is not a topic")));
    }
    let parent = ix.nodes[i].parent.ok_or_else(|| invalid("topic without parent"))?;
    if ix.children(parent).iter().any(|&c| c != i && ix.local(t, c) == "Topic" && topic_name(&ix, t, c) == name) {
        return Err(invalid("a topic of that name exists already at this level"));
    }
    let owner = ix.value(t, parent, "Self").unwrap_or_default();
    let new = topic_id(&owner, name);
    let mut edits = vec![crate::ops::attr_edit(&ix, t, i, "Name", name), crate::ops::attr_edit(&ix, t, i, "SortOrder", sort_order)];
    let mut subtree = vec![i];
    let mut stack = vec![i];
    while let Some(n) = stack.pop() {
        for &c in ix.children(n) {
            if ix.local(t, c) == "Topic" {
                subtree.push(c);
                stack.push(c);
            }
        }
    }
    let moved = |value: &str| -> Option<String> {
        value.strip_prefix(id.as_str()).filter(|rest| rest.is_empty() || rest.starts_with("Topicn")).map(|rest| format!("{new}{rest}"))
    };
    if &new != id {
        for &n in &subtree {
            if let Some(v) = ix.value(t, n, "Self").and_then(|s| moved(&s)) {
                edits.push(crate::ops::attr_edit(&ix, t, n, "Self", &v));
            }
        }
        // references in designmap (cross references) and stories
        for p in w.ed.parts().keys().cloned().collect::<Vec<_>>() {
            let other = w.text(&p)?.to_owned();
            let text = other.as_str();
            if !text.contains("ReferencedTopic=") {
                continue;
            }
            let pix = if p == part { ix.clone() } else { Index::scan(text).map_err(|e| EditError::new("xml", e))? };
            let mut found = Vec::new();
            for n in 0..pix.nodes.len() {
                if let Some(v) = pix.value(text, n, "ReferencedTopic").and_then(|s| moved(&s)) {
                    found.push(crate::ops::attr_edit(&pix, text, n, "ReferencedTopic", &v));
                }
            }
            if p == part {
                edits.extend(found);
            } else if !found.is_empty() {
                w.splice(&p, found)?;
                let story = p.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).unwrap_or_default().to_owned();
                w.recompose(&story);
            }
        }
    }
    // InDesign keeps topics sorted, also when a name or sort order changes (measured: index-topic-sortorder):
    // the topic moves to where its new key belongs among its siblings, with its subtopics.
    let key = if sort_order.is_empty() { name.to_owned() } else { sort_order.to_owned() };
    let old_key = {
        let so = ix.value(t, i, "SortOrder").unwrap_or_default();
        if so.is_empty() { topic_name(&ix, t, i) } else { so }
    };
    if key == old_key {
        return w.splice(&part, edits);
    }
    let top = ix.local(t, parent) == "Index";
    let siblings: Vec<usize> = ix.children(parent).iter().copied().filter(|&c| ix.local(t, c) == "Topic").collect();
    let me = siblings.iter().position(|&c| c == i).unwrap_or(0);
    let before = siblings.iter().copied().filter(|&c| c != i).find(|&c| {
        let so = ix.value(t, c, "SortOrder").unwrap_or_default();
        let k = if so.is_empty() { topic_name(&ix, t, c) } else { so };
        order::compare(top, &key, &k) == std::cmp::Ordering::Less
    });
    // already in the right place: the next sibling (or the end) stays the next
    let stays = match before { Some(c) => siblings.get(me + 1) == Some(&c), None => me + 1 == siblings.len() };
    if stays {
        return w.splice(&part, edits);
    }
    let line_of = |at: usize| t[..at].rfind('\n').map_or(0, |p| p + 1);
    let n = &ix.nodes[i];
    let from = if t[line_of(n.start)..n.start].chars().all(char::is_whitespace) { line_of(n.start) } else { n.start };
    let to = if t[n.end..].starts_with('\n') { n.end + 1 } else { n.end };
    let mut segment = t[from..to].to_owned();
    let mut outside = Vec::new();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    for (range, value) in edits {
        if range.start >= from && range.end <= to {
            segment.replace_range(range.start - from..range.end - from, &value);
        } else {
            outside.push((range, value));
        }
    }
    let at = match before {
        Some(c) => line_of(ix.nodes[c].start),
        None => line_of(ix.nodes[parent].close_start),
    };
    outside.push((from..to, String::new()));
    outside.push((at..at, segment));
    w.splice(&part, outside)
}

/// All `PageReference` markers: (story id, part, marker id, referenced topic).
fn markers(w: &Work<'_>) -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    for part in w.parts_named("Stories/") {
        let Ok(t) = w.text(&part) else { continue };
        if !t.contains("<PageReference") {
            continue;
        }
        let Ok(ix) = Index::scan(t) else { continue };
        // the package root is `idPkg:Story` too: take the element that has a `Self`
        let story = (0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Story").find_map(|i| ix.value(t, i, "Self")).unwrap_or_default();
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "PageReference" {
                out.push((story.clone(), part.clone(), ix.value(t, i, "Self").unwrap_or_default(), ix.value(t, i, "ReferencedTopic").unwrap_or_default()));
            }
        }
    }
    out
}

fn is_word(c: char) -> bool {
    c == '\u{FEFF}' || c.is_alphanumeric()
}

/// InDesign puts a marker given an insertion point inside a word at the start of that word, and a
/// marker already there counts as part of the word (measured: index-ref-positions).
fn snap(chars: &[char], at: usize) -> usize {
    if at == 0 || at >= chars.len() || !is_word(chars[at - 1]) || !is_word(chars[at]) {
        return at;
    }
    let mut i = at;
    while i > 0 && is_word(chars[i - 1]) {
        i -= 1;
    }
    i
}

fn next_reference_number(w: &Work<'_>) -> usize {
    let mut max = 0;
    for part in w.parts_named("Stories/") {
        let Ok(t) = w.text(&part) else { continue };
        let Ok(ix) = Index::scan(t) else { continue };
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "PageReference" {
                max = max.max(ix.value(t, i, "Id").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0));
            }
        }
    }
    max + 1
}

/// Adds a page reference of `kind` to `topic` at story position `at` (a story or text frame id).
/// `limit` is the paragraph or page count of the two counted kinds. The new marker's id is returned in `created`.
pub(crate) fn add_reference(w: &mut Work<'_>, story: &String, at: &usize, topic: &String, kind: &String, limit: &usize) -> Result<(), EditError> {
    if !KINDS.contains(&kind.as_str()) {
        return Err(EditError::new("unsupported", format!("page reference kind {kind}")));
    }
    insert_reference(w, story, at, topic, kind, limit, None)
}

/// Adds a "to next use of style" reference (the page range runs to the next paragraph of `style`, a paragraph style id such as
/// `ParagraphStyle/Heading`; measured, index-c1 and index-c3). The new marker's id is returned in `created`.
pub(crate) fn add_style_reference(w: &mut Work<'_>, story: &String, at: &usize, topic: &String, style: &String) -> Result<(), EditError> {
    if !style.starts_with("ParagraphStyle/") {
        return Err(invalid(format!("{style} is not a paragraph style")));
    }
    let (part, ix, i) = w.find(style).map_err(|_| invalid(format!("no paragraph style {style}")))?;
    if part != "Resources/Styles.xml" || ix.local(w.text(&part)?, i) != "ParagraphStyle" {
        return Err(invalid(format!("{style} is not a paragraph style")));
    }
    insert_reference(w, story, at, topic, &"ToNextUseOfStyle".to_owned(), &0, Some(style))
}

fn insert_reference(w: &mut Work<'_>, story: &str, at: &usize, topic: &String, kind: &String, limit: &usize, style: Option<&String>) -> Result<(), EditError> {
    let (part, ix, i) = w.find(topic)?;
    if part != "designmap.xml" || ix.local(w.text(&part)?, i) != "Topic" {
        return Err(invalid(format!("{topic} is not a topic")));
    }
    let story_id = crate::text::story_of(w, story)?;
    let (sp, _, _) = w.find(&story_id)?;
    let chars: Vec<char> = crate::text::contents(w.text(&sp)?, &story_id)?.chars().collect();
    if *at > chars.len() {
        return Err(invalid("insertion point outside story"));
    }
    let place = snap(&chars, *at);
    let id = w.new_id();
    let number = next_reference_number(w);
    let counted = match kind.as_str() {
        "ForNextNParagraphs" | "ForNextNPages" => {
            if *limit == 0 {
                return Err(invalid("a counted reference needs a count above zero"));
            }
            // InDesign keeps the count as a property of the marker
            format!("
					<Properties>
						<PageReferenceLimit type=\"long\">{limit}</PageReferenceLimit>
					</Properties>
				")
        }
        // the style is a property of the marker, an object (measured: index-c1)
        "ToNextUseOfStyle" => format!("
					<Properties>
						<PageReferenceLimit type=\"object\">{}</PageReferenceLimit>
					</Properties>
				", escape(style.map_or("", |s| s.as_str()))),
        _ => String::new(),
    };
    let head = format!("<PageReference Self=\"{id}\" PageReferenceType=\"{kind}\" ReferencedTopic=\"{}\" Id=\"{number}\"", escape(topic));
    let raw = if counted.is_empty() { format!("{head} />") } else { format!("{head}>{counted}</PageReference>") };
    crate::text::replace_with_node(w, &story_id, place, place, '\u{FEFF}', &raw)?;
    w.created.push(id);
    Ok(())
}

/// Points a reference at another topic. InDesign's DOM has no move (`referencedTopic` is read-only,
/// measured), so this follows the stored form: the `ReferencedTopic` attribute.
pub(crate) fn move_reference(w: &mut Work<'_>, id: &String, topic: &String) -> Result<(), EditError> {
    let (tp, tix, ti) = w.find(topic)?;
    if tp != "designmap.xml" || tix.local(w.text(&tp)?, ti) != "Topic" {
        return Err(invalid(format!("{topic} is not a topic")));
    }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    if ix.local(t, i) != "PageReference" {
        return Err(invalid(format!("{id} is not a page reference")));
    }
    let edit = crate::ops::attr_edit(&ix, t, i, "ReferencedTopic", topic);
    w.splice(&part, vec![edit])?;
    let story = part.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).unwrap_or_default().to_owned();
    w.recompose(&story);
    Ok(())
}

fn story_of_marker(w: &Work<'_>, id: &str) -> Option<(String, String)> {
    markers(w).into_iter().find(|m| m.2 == id).map(|m| (m.0, m.1))
}

/// Removes one marker (one character) from its story.
fn remove_marker(w: &mut Work<'_>, story: &str, id: &str) -> Result<(), EditError> {
    let at = crate::text::node_position(w, story, id)?;
    crate::text::delete(w, story, at, at + 1)
}

pub(crate) fn delete_reference(w: &mut Work<'_>, id: &String) -> Result<(), EditError> {
    let (story, _) = story_of_marker(w, id).ok_or_else(|| EditError::new("notFound", format!("no page reference {id}")))?;
    remove_marker(w, &story, id)
}

/// Deletes a topic with its subtopics and the references to any of them.
pub(crate) fn delete_topic(w: &mut Work<'_>, id: &String) -> Result<(), EditError> {
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    if part != "designmap.xml" || ix.local(t, i) != "Topic" {
        return Err(invalid(format!("{id} is not a topic")));
    }
    let mut doomed = vec![id.clone()];
    let mut stack = vec![i];
    while let Some(n) = stack.pop() {
        for &c in ix.children(n) {
            if ix.local(t, c) == "Topic" {
                doomed.push(ix.value(t, c, "Self").unwrap_or_default());
                stack.push(c);
            }
        }
    }
    // markers first (their stories are other parts), then the topic itself
    for (story, _, marker, topic) in markers(w) {
        if doomed.contains(&topic) {
            remove_marker(w, &story, &marker)?;
        }
    }
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?;
    // the whole line, so no blank line is left
    let whole_line = |at: usize| {
        let n = &ix.nodes[at];
        let line = t[..n.start].rfind('\n').map_or(0, |p| p + 1);
        let from = if t[line..n.start].chars().all(char::is_whitespace) { line } else { n.start };
        let to = if t[n.end..].starts_with('\n') { n.end + 1 } else { n.end };
        (from, to)
    };
    let (from, to) = whole_line(i);
    let mut edits = vec![(from..to, String::new())];
    // the cross references of other topics that point to a deleted topic go with it (measured: index6-see-more)
    for n in 0..ix.nodes.len() {
        if ix.local(t, n) == "CrossReference" && ix.value(t, n, "ReferencedTopic").is_some_and(|r| doomed.contains(&r)) {
            let (f, e) = whole_line(n);
            if f < from || f >= to {
                edits.push((f..e, String::new()));
            }
        }
    }
    w.splice(&part, edits)
}
