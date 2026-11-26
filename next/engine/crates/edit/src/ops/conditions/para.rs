//! Hiding and showing conditional text over paragraph breaks. Measured with InDesign 2025 (next/oracle/rules/condx-c3.ko.md,
//! scenarios condx-hide-para-*, condx-show-para-*):
//! - hidden text is one `HiddenText` holding a `ParagraphStyleRange` per paragraph it touched (a copy of that paragraph's start
//!   tag around the hidden ranges, conditions kept). When the last hidden range ends with its paragraph break, the paragraph that
//!   follows loses its start and a last, empty `ParagraphStyleRange` stands for it: a copy of the start tag of the paragraph
//!   that joined the text before the group, `NormalParagraphStyle` when nothing visible came before (nothing was joined);
//! - the visible rest is one paragraph: the paragraph the group started in, or when nothing visible came before the group, the
//!   paragraph that holds the text after it. The `HiddenText` sits at the start of the first range after the group;
//! - showing takes this apart again: the first hidden paragraph continues the text before the group, the last one takes the text
//!   after it; when the group ended with a break, the text after it stays in the paragraph it is in (the empty last hidden
//!   paragraph gives nothing back) and neighbouring paragraphs of one start tag are written as one range.
use super::*;

/// A run of hidden ranges that takes a paragraph break with it.
pub(super) struct Group {
    /// Paragraphs from the one the group starts in to the last one it changes (the one after it when the group ends with a break).
    pub paras: Vec<usize>,
    /// Number of the first hidden range in the first paragraph; the first range after the group in the last one.
    pub first: usize,
    pub end: usize,
    /// The group ends with the break of the paragraph before `paras.last()`, which holds the text that follows.
    pub joined: bool,
}

fn ranges(t: &str, ix: &Index, p: usize) -> Vec<usize> {
    ix.children(p).iter().copied().filter(|&k| ix.local(t, k) == "CharacterStyleRange").collect()
}

fn has_break(t: &str, ix: &Index, k: usize) -> bool {
    ix.children(k).iter().any(|&c| ix.local(t, c) == "Br")
}

fn next_paragraph(t: &str, ix: &Index, p: usize) -> Option<usize> {
    let siblings = ix.children(ix.nodes[p].parent?);
    let at = siblings.iter().position(|&s| s == p)?;
    siblings.get(at + 1).copied().filter(|&n| ix.local(t, n) == "ParagraphStyleRange")
}

fn open_tag<'t>(t: &'t str, ix: &Index, p: usize) -> &'t str { &t[ix.nodes[p].start..ix.nodes[p].open_end] }

/// Ranges that hold text and, last of their paragraph, its break may be hidden or shown.
fn plain_with_break(t: &str, ix: &Index, k: usize) -> Result<(), EditError> {
    for &c in ix.children(k) {
        let tag = ix.local(t, c);
        if !matches!(tag, "Content" | "Properties" | "Br") { return Err(unsupported(format!("hiding or showing text with {tag} inside is not measured"))); }
    }
    Ok(())
}

/// The hidden runs that contain a paragraph break, with everything they touch.
pub(super) fn find_groups(t: &str, ix: &Index, info: &HashMap<usize, Csr>, shown_hosts: &HashMap<usize, usize>) -> Result<Vec<Group>, EditError> {
    let hide = |k: usize| info.get(&k).is_some_and(|c| c.hide);
    let mut groups: Vec<Group> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();
    for p in (0..ix.nodes.len()).filter(|&p| ix.local(t, p) == "ParagraphStyleRange") {
        if used.contains(&p) { continue; }
        let kids = ranges(t, ix, p);
        let mut k = 0;
        while k < kids.len() {
            if !hide(kids[k]) { k += 1; continue; }
            let first = k;
            while k < kids.len() && hide(kids[k]) { k += 1; }
            if !kids[first..k].iter().any(|&x| has_break(t, ix, x)) { continue; }
            if k != kids.len() || kids[first..k - 1].iter().any(|&x| has_break(t, ix, x)) || !has_break(t, ix, kids[k - 1]) {
                return Err(unsupported("hiding text with a paragraph break in the middle of a paragraph is not measured"));
            }
            // on into the next paragraphs while they start with hidden ranges
            let (mut paras, mut cur) = (vec![p], p);
            let (end, joined) = loop {
                let q = next_paragraph(t, ix, cur).ok_or_else(|| unsupported("hiding up to the last paragraph break of a text is not measured"))?;
                paras.push(q);
                let qk = ranges(t, ix, q);
                let n = qk.iter().take_while(|&&x| hide(x)).count();
                if n == 0 { break (0, true); }
                if n < qk.len() || !has_break(t, ix, qk[n - 1]) {
                    if qk[..n - 1].iter().any(|&x| has_break(t, ix, x)) { return Err(unsupported("hiding text with a paragraph break in the middle of a paragraph is not measured")); }
                    break (n, false);
                }
                if qk[..n - 1].iter().any(|&x| has_break(t, ix, x)) { return Err(unsupported("hiding text with a paragraph break in the middle of a paragraph is not measured")); }
                cur = q;
            };
            // nothing else changes in the paragraphs the group rewrites
            for (i, &q) in paras.iter().enumerate() {
                if ix.children(q).iter().any(|&c| ix.local(t, c) != "CharacterStyleRange") { return Err(unsupported("hiding text over a paragraph with properties is not measured")); }
                let qk = ranges(t, ix, q);
                let (from, to) = (if i == 0 { first } else { 0 }, if i + 1 == paras.len() { end } else { qk.len() });
                let in_group = |j: usize| j >= from && j < to && !(i + 1 == paras.len() && joined);
                if qk.iter().enumerate().any(|(j, &x)| !in_group(j) && (shown_hosts.contains_key(&x) || (hide(x) && has_break(t, ix, x)))) {
                    return Err(unsupported("several hidden texts around one paragraph break are not measured"));
                }
                if qk.iter().enumerate().any(|(j, &x)| in_group(j) && !hide(x)) { return Err(unsupported("hidden text with a visible range inside is not measured")); }
            }
            used.extend(paras.iter().copied());
            groups.push(Group { paras, first, end, joined });
            break;
        }
    }
    Ok(groups)
}

/// A range with `hidden` as its first thing after its `Properties` (nothing when the range has no room for it).
fn with_hidden(text: &str, hidden: &str) -> Option<String> {
    let ix = Index::scan(text).ok()?;
    let n = ix.nodes.first()?;
    if ix.local(text, 0) != "CharacterStyleRange" || n.empty || n.end != text.len() { return None; }
    if ix.children(0).iter().any(|&c| ix.local(text, c) == "HiddenText") { return None; }
    let at = ix.children(0).iter().find(|&&c| ix.local(text, c) == "Properties").map_or(n.open_end, |&c| ix.nodes[c].end);
    Some(format!("{}{hidden}{}", &text[..at], &text[at..]))
}

const NORMAL_PARAGRAPH: &str = "<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\">";
const EMPTY_RANGE: &str = "<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\" /></ParagraphStyleRange>";

/// The ranges of a paragraph `p` as pieces: hidden runs without a paragraph break go into a `HiddenText` of their own.
fn plain_pieces(t: &str, ix: &Index, info: &HashMap<usize, Csr>, p: usize, kids: &[usize], edits: &[(Range<usize>, String)], touched: &dyn Fn(usize) -> bool) -> Result<Vec<Piece>, EditError> {
    let mut pieces = Vec::new();
    let mut k = 0;
    while k < kids.len() {
        let kid = kids[k];
        if info.get(&kid).is_some_and(|c| c.hide) {
            let first = k;
            while k < kids.len() && info.get(&kids[k]).is_some_and(|c| c.hide) { k += 1; }
            let group = &kids[first..k];
            for &g in group { plain(ix, t, g)?; }
            let open = &t[ix.nodes[p].start..ix.nodes[p].open_end];
            let hidden: String = group.iter().map(|&g| render(t, ix, g, &info[&g], false)).collect();
            let last = *group.last().unwrap();
            pieces.push(Piece::new(format!("{}<HiddenText>{open}{hidden}</ParagraphStyleRange></HiddenText></CharacterStyleRange>", placeholder_head(t, ix, last, &info[&last])), true));
            continue;
        }
        k += 1;
        pieces.push(Piece::new(apply(t, &(ix.nodes[kid].start..ix.nodes[kid].end), edits), touched(kid)));
    }
    Ok(pieces)
}

/// The paragraphs of a group as one: the range they replace and the text that replaces them.
pub(super) fn hide(t: &str, ix: &Index, info: &HashMap<usize, Csr>, g: &Group, edits: &[(Range<usize>, String)], touched: &dyn Fn(usize) -> bool) -> Result<(Range<usize>, String), EditError> {
    let last_para = if g.joined { g.paras.len() - 2 } else { g.paras.len() - 1 };
    let (a, tail) = (g.paras[0], g.joined.then(|| g.paras[g.paras.len() - 1]));
    let a_ranges = ranges(t, ix, a);
    let before_text = g.first > 0;
    // the hidden paragraphs
    let mut hidden = String::new();
    let mut last_hidden = None;
    for (i, &q) in g.paras[..=last_para].iter().enumerate() {
        let qk = ranges(t, ix, q);
        let (from, to) = (if i == 0 { g.first } else { 0 }, if i == last_para && !g.joined { g.end } else { qk.len() });
        hidden.push_str(open_tag(t, ix, q));
        for &k in &qk[from..to] {
            plain_with_break(t, ix, k)?;
            hidden.push_str(&render(t, ix, k, &info[&k], false));
            last_hidden = Some(k);
        }
        hidden.push_str("</ParagraphStyleRange>");
    }
    if let Some(q) = tail {
        if before_text { hidden.push_str(open_tag(t, ix, q)); } else { hidden.push_str(NORMAL_PARAGRAPH); }
        hidden.push_str(EMPTY_RANGE);
    }
    let hidden = format!("<HiddenText>{hidden}</HiddenText>");
    // what stays: the text before, the text after (the paragraph after the group, or the rest of the last paragraph)
    let (after_para, after_from) = match tail { Some(q) => (q, 0), None => (g.paras[last_para], g.end) };
    let after = ranges(t, ix, after_para);
    let text_of = |k: usize| apply(t, &(ix.nodes[k].start..ix.nodes[k].end), edits);
    let mut pieces = plain_pieces(t, ix, info, a, &a_ranges[..g.first], edits, touched)?;
    let mut rest = after[after_from..].iter().copied();
    let host = rest.next().and_then(|k| with_hidden(&text_of(k), &hidden).map(|h| Piece::new(h, true)));
    match host {
        Some(h) => pieces.push(h),
        None => {
            let l = last_hidden.ok_or_else(|| error("nothing hidden"))?;
            pieces.push(Piece::new(format!("{}{hidden}</CharacterStyleRange>", placeholder_head(t, ix, l, &info[&l])), true));
            if let Some(&k) = after.get(after_from) { pieces.push(Piece::new(text_of(k), touched(k))); }
        }
    }
    pieces.extend(plain_pieces(t, ix, info, after_para, &rest.collect::<Vec<_>>(), edits, touched)?);
    let open = if before_text { open_tag(t, ix, a) } else { open_tag(t, ix, after_para) };
    let end_para = *g.paras.last().unwrap();
    let body = merge(pieces).into_iter().map(|m| m.text).collect::<Vec<_>>().join("\n");
    Ok((ix.nodes[a].start..ix.nodes[end_para].end, format!("{open}{body}</ParagraphStyleRange>")))
}

/// A `HiddenText` over several paragraphs inside the paragraph `p`, in the range `host`.
pub(super) struct Shown {
    pub hidden: usize,
    /// Where the pieces of the text after the hidden text start.
    pub at: usize,
    pub left: String,
    pub right: String,
}

/// The paragraphs that come out of a paragraph `p` whose hidden text shows again: each with its start tag and its pieces.
pub(super) fn show(t: &str, ix: &Index, info: &HashMap<usize, Csr>, p: usize, s: &Shown, mut pieces: Vec<Piece>) -> Result<Vec<(String, Vec<Piece>)>, EditError> {
    let after = pieces.split_off(s.at);
    let mut first = pieces;
    let hidden: Vec<usize> = ix.children(s.hidden).to_vec();
    if hidden.iter().any(|&q| ix.local(t, q) != "ParagraphStyleRange") { return Err(unsupported("hidden text with other content is not measured")); }
    // a last paragraph without text stands for the paragraph that follows the text which was hidden
    let empty_last = |q: usize| { let k = ix.children(q); !k.is_empty() && k.iter().all(|&r| ix.local(t, r) == "CharacterStyleRange" && ix.children(r).iter().all(|&c| ix.local(t, c) == "Properties")) };
    let tail = hidden.len() >= 2 && empty_last(hidden[hidden.len() - 1]);
    let full = &hidden[..hidden.len() - usize::from(tail)];
    let own = |q: usize| -> Result<Vec<Piece>, EditError> {
        let mut v = Vec::new();
        for &r in ix.children(q) {
            if ix.local(t, r) != "CharacterStyleRange" { return Err(unsupported("hidden text with other content is not measured")); }
            plain_with_break(t, ix, r)?;
            v.push(Piece::new(render(t, ix, r, &info[&r], false), true));
        }
        Ok(v)
    };
    let left = Piece::new(s.left.clone(), true);
    let before_text = first.iter().any(|b| !b.empty()) || !left.empty();
    first.push(left);
    first.extend(own(full[0])?);
    let mut out = vec![(if before_text { open_tag(t, ix, p) } else { open_tag(t, ix, full[0]) }.to_owned(), first)];
    for &q in &full[1..full.len() - usize::from(!tail)] { out.push((open_tag(t, ix, q).to_owned(), own(q)?)); }
    let right = Piece::new(s.right.clone(), true);
    if tail {
        // the text after the group keeps the start tag of the paragraph it is in (condx-show-para-br-bold: the empty last
        // paragraph of the hidden text does not give its style back)
        out.push((open_tag(t, ix, p).to_owned(), std::iter::once(right).chain(after).collect()));
    } else {
        let q = full[full.len() - 1];
        out.push((open_tag(t, ix, q).to_owned(), own(q)?.into_iter().chain(std::iter::once(right)).chain(after).collect()));
    }
    // neighbours of the same start tag are one paragraph range in the file (condx-show-para-br-bold)
    let mut one: Vec<(String, Vec<Piece>)> = Vec::new();
    for (open, ps) in out {
        match one.last_mut() {
            Some((o, v)) if *o == open => v.extend(ps),
            _ => one.push((open, ps)),
        }
    }
    Ok(one)
}
