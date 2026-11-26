//! Folio's edits against InDesign's (the oracle, `next/oracle/`): for every scenario with a
//! golden, the same steps run in Folio on the fixture, and
//!
//! 1. the IDML changes: the objects Folio changed, added or removed, and their attributes,
//!    path points and child order, equal what InDesign changed (numbers within 1e-6);
//!    attributes InDesign rewrites on every export do not count;
//! 2. undo: every step undone gives the fixture back byte for byte; redo gives the edit back;
//! 3. the model: the edited parts read without new warnings, and each labelled object's
//!    bounds move by what InDesign's own `geometricBounds` moved (origin-free), its lock and
//!    visibility match; the model read from InDesign's parts equals the one from Folio's.
//!
//! `FOLIO_ORACLE_ONLY=name,name` runs some scenarios. The report is printed (`--nocapture`).

use folio_edit::xml::{Index, unescape};
use folio_edit::{Anchor, Arrange, Axis, Command, Editor, Fit};
use folio_model::{Document, PageItem};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
#[path = "support/idml_fixture.rs"]
mod idml_fixture;
use idml_fixture::read_zip;
#[path = "support/toc_oracle.rs"]
mod toc_oracle;
#[path = "support/index_oracle.rs"]
mod index_oracle;
#[path = "support/cleanroom_vectors.rs"]
mod cleanroom_vectors;

const VOLATILE: &[&str] = &[
    "ParentInterfaceChangeCount", "TargetInterfaceChangeCount", "LastUpdatedInterfaceChangeCount", "LinkImportTime",
    "LinkImportModificationTime", "StyleUniqueId", "DestinationUniqueKey",
    // where a link points differs by machine (the fixture keeps none of this one's paths)
    "LinkResourceURI",
    // Relinking on capture refreshes the file timestamp; moved/copied Link elements
    // carry it with them. It is not a change to graphic content.
    "LinkImportStamp",
    // Which properties a copy of a parent item changed, as InDesign's own property numbers (11271 after a move). Folio leaves it
    // empty: InDesign keeps the copy where the IDML puts it (measured: probe-override-reopen.jsx, copy cleared and as written).
    "OverriddenPageItemProps",
    // InDesign numbers the cells a paste makes (2, and 2 4 6 8 down a pasted column; the other cells keep 0). With no cell style applied it
    // means nothing and the script cannot read it (measured: tables/tbl-paste-*).
    "AppliedCellStylePriority",
];

fn oracle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle")
}

fn read_dir_parts(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, std::fs::read_to_string(&p).unwrap());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// element signatures (as next/oracle/delta.py)

#[derive(Clone, Debug)]
enum Body {
    Ref(String),
    Node { tag: String, attrs: BTreeMap<String, String>, text: String, kids: Vec<Body> },
}

#[derive(Clone, Debug)]
struct Sig {
    tag: String,
    attrs: BTreeMap<String, String>,
    body: Vec<Body>,
}

fn attrs_of(ix: &Index, t: &str, i: usize) -> BTreeMap<String, String> {
    // the document's title is the session's window name ("Untitled-48")
    let document = ix.local(t, i) == "Document";
    let clock = (ix.local(t, i) == "TextVariableInstance").then(|| ix.value(t, i, "AssociatedTextVariable").unwrap_or_default());
    ix.attrs(t, i)
        .into_iter()
        .map(|a| (t[a.name.0..a.name.1].to_owned(), unescape(&t[a.value.0..a.value.1])))
        .filter(|(k, _)| !VOLATILE.contains(&k.as_str()) && !(document && k == "Name"))
        .map(|(k, v)| match (&clock, k.as_str()) {
            // the numbers of content links are each program's own: the equal ones must stay equal, so they are compared by order
            (_, "LinkResourceId" | "LinkedSourceTableId") => (k, "#".to_owned()),
            (Some(variable), "ResultText") => (k, clock_shape(variable, &v)),
            _ => (k, v),
        })
        .collect()
}

/// A date or file name variable shows the clock and the name of the InDesign session that captured it ("10/23/26", "Untitled-793"):
/// compared by shape, not value. Dates: digits count as 9, a run of letters as one `a` (month and day names); the file name not at
/// all. The values themselves are tested against the patterns InDesign gave (`folio-text` `layout::filevars`).
fn clock_shape(variable: &str, result: &str) -> String {
    match variable.trim_start_matches("dTextVariablen") {
        "File Name" => "<file name>".to_owned(),
        "Creation Date" | "Modification Date" | "Output Date" => {
            let mut out = String::new();
            let mut letters = false;
            for c in result.chars() {
                if c.is_ascii_alphabetic() {
                    if !letters {
                        out.push('a');
                    }
                    letters = true;
                } else {
                    letters = false;
                    out.push(if c.is_ascii_digit() { '9' } else { c });
                }
            }
            out
        }
        _ => result.to_owned(),
    }
}

fn body(ix: &Index, t: &str, i: usize) -> Vec<Body> {
    ix.children(i)
        .iter()
        // the DocumentUser InDesign adds to a document it opens is not a change a step made
        .filter(|&&c| ix.local(t, c) != "DocumentUser")
        .map(|&c| match ix.attr(t, c, "Self") {
            Some(s) => Body::Ref(s.to_owned()),
            None => {
                let n = &ix.nodes[c];
                let text = if ix.children(c).is_empty() && !n.empty {
                    if ix.local(t, c) == "Content" { unescape(&t[n.open_end..n.close_start]) }
                    else { t[n.open_end..n.close_start].trim().to_owned() }
                } else { String::new() };
                Body::Node { tag: ix.local(t, c).to_owned(), attrs: attrs_of(ix, t, c), text, kids: body(ix, t, c) }
            }
        })
        .collect()
}

fn signatures(t: &str) -> BTreeMap<String, Sig> {
    let ix = Index::scan(t).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..ix.nodes.len() {
        if let Some(s) = ix.attr(t, i, "Self") {
            out.insert(s.to_owned(), Sig { tag: ix.local(t, i).to_owned(), attrs: attrs_of(&ix, t, i), body: body(&ix, t, i) });
        }
    }
    out
}

fn same_value(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // The XMP writer's toolkit identity is not document metadata. Keep every other byte.
    // The dates InDesign writes into the packet of a placed PSD (which has none of its own) are
    // the time of the link import, as volatile as `LinkImportTime`.
    if a.starts_with("<![CDATA[<?xpacket") && b.starts_with("<![CDATA[<?xpacket") {
        let without_toolkit = |s: &str| {
            let mut s = match s.find(" x:xmptk=\"") {
                Some(start) => match s[start + 11..].find('"') {
                    Some(end) => format!("{}{}", &s[..start], &s[start + 11 + end + 1..]),
                    None => s.to_owned(),
                },
                None => s.to_owned(),
            };
            // (the tool that made the packet is InDesign's in the golden and Folio's in the engine)
            for tag in ["xmp:CreatorTool", "xmp:CreateDate", "xmp:ModifyDate", "xmp:MetadataDate"] {
                let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
                if let (Some(i), Some(j)) = (s.find(&open), s.find(&close)) && i + open.len() <= j {
                    s.replace_range(i + open.len()..j, "");
                }
            }
            s
        };
        return without_toolkit(a) == without_toolkit(b);
    }
    let nums = |s: &str| s.split_ascii_whitespace().map(|x| x.parse::<f64>()).collect::<Result<Vec<_>, _>>();
    match (nums(a), nums(b)) {
        (Ok(x), Ok(y)) if x.len() == y.len() && !x.is_empty() => x.iter().zip(&y).all(|(p, q)| (p - q).abs() <= 1e-6 * p.abs().max(1.0)),
        _ => false,
    }
}

fn same_attrs(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Result<(), String> {
    for k in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
        match (a.get(k), b.get(k)) {
            (Some(x), Some(y)) if same_value(x, y) => {}
            (x, y) => return Err(format!("{k}: {x:?} vs {y:?}")),
        }
    }
    Ok(())
}

fn same_body(a: &[Body], b: &[Body], at: &str) -> Result<(), String> {
    if a.len() != b.len() {
        if std::env::var("FOLIO_ORACLE_DEBUG").is_ok() {
            eprintln!("{at}: Folio {a:?} / InDesign {b:?}");
        }
        return Err(format!("{at}: {} vs {} children", a.len(), b.len()));
    }
    for (k, (x, y)) in a.iter().zip(b).enumerate() {
        match (x, y) {
            (Body::Ref(p), Body::Ref(q)) if p == q => {}
            (Body::Node { tag: t1, attrs: a1, text: x1, kids: k1 }, Body::Node { tag: t2, attrs: a2, text: x2, kids: k2 }) if t1 == t2 => {
                let here = format!("{at}/{t1}[{k}]");
                same_attrs(a1, a2).map_err(|e| format!("{here} {e}"))?;
                if (t1 == "Content" && x1 != x2) || (t1 != "Content" && !same_value(x1, x2)) {
                    return Err(format!("{here} text {x1:?} vs {x2:?}"));
                }
                same_body(k1, k2, &here)?;
            }
            _ => return Err(format!("{at}[{k}]: {x:?} vs {y:?}")),
        }
    }
    Ok(())
}

fn same_sig(a: &Sig, b: &Sig) -> Result<(), String> {
    if a.tag != b.tag {
        return Err(format!("{} vs {}", a.tag, b.tag));
    }
    if a.tag == "Font" {
        // `Version` is the version of the font file installed on the machine that wrote the golden
        // (an OS update changes it: Times New Roman 7.12 -> 7.13 on 2026-11-10). It is not Folio logic.
        let strip = |m: &BTreeMap<String, String>| m.iter().filter(|(k, _)| k.as_str() != "Version").map(|(k, v)| (k.clone(), v.clone())).collect::<BTreeMap<_, _>>();
        same_attrs(&strip(&a.attrs), &strip(&b.attrs))?;
    } else {
        same_attrs(&a.attrs, &b.attrs)?;
    }
    same_body(&a.body, &b.body, &a.tag)
}

/// (part, Self) → the element after the change, or None when removed; parts added/removed;
/// elements new in a part, in document order (their ids are each program's own).
#[derive(Default)]
struct Delta {
    elements: BTreeMap<(String, String), Option<Sig>>,
    parts_removed: BTreeSet<String>,
    parts_added: BTreeSet<String>,
    added: Vec<(String, String, String)>,
    /// Newly registered parts, in designmap reference order (not allocator ID order).
    part_order: Vec<String>,
}

fn delta(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>, only: Option<&BTreeSet<String>>) -> Delta {
    let mut d = Delta::default();
    // Reparented pages/items keep their identities. Only genuinely new Self ids may
    // be paired by creation order; otherwise swapped page ownership could be hidden.
    let existing: BTreeSet<String> = before.iter().filter(|(n,_)|n.ends_with(".xml"))
        .flat_map(|(_,t)|order(t).into_iter().map(|(id,_)|id)).collect();
    let names: BTreeSet<&String> = before.keys().chain(after.keys()).filter(|n| n.ends_with(".xml") && !n.starts_with("META-INF/") && only.is_none_or(|o| o.contains(*n))).collect();
    for n in names {
        match (before.get(n), after.get(n)) {
            (Some(_), None) => {
                d.parts_removed.insert(n.clone());
            }
            (None, Some(a)) => {
                d.parts_added.insert(n.clone());
                for (id, tag) in order(a) { if !existing.contains(&id) { d.added.push((n.clone(), id, tag)); } }
                for (id, sig) in signatures(a) { d.elements.insert((n.clone(), id), Some(sig)); }
            }
            (Some(b), Some(a)) if a != b => {
                let (sb, sa) = (signatures(b), signatures(a));
                for (id, tag) in order(a) {
                    if !existing.contains(&id) {
                        d.added.push((n.clone(), id, tag));
                    }
                }
                for id in sb.keys().chain(sa.keys()).collect::<BTreeSet<_>>() {
                    match (sb.get(id), sa.get(id)) {
                        (Some(x), Some(y)) if same_sig(x, y).is_ok() => {}
                        (_, y) => {
                            d.elements.insert((n.clone(), id.clone()), y.cloned());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(dm) = after.get("designmap.xml") {
        let ix = Index::scan(dm).unwrap();
        for i in 0..ix.nodes.len() {
            if let Some(src) = ix.attr(dm, i, "src") && d.parts_added.contains(src) {
                d.part_order.push(src.to_owned());
            }
        }
    }
    for p in &d.parts_added {
        if !d.part_order.contains(p) { d.part_order.push(p.clone()); }
    }
    d
}

fn part_key(d: &Delta, p: &str) -> String {
    d.part_order.iter().position(|n| n == p).map_or_else(|| p.to_owned(), |i| format!("~part{i}"))
}

/// Element ids of a part in document order, with their tags.
fn order(t: &str) -> Vec<(String, String)> {
    let ix = Index::scan(t).unwrap();
    (0..ix.nodes.len()).filter_map(|i| ix.attr(t, i, "Self").map(|s| (s.to_owned(), ix.local(t, i).to_owned()))).collect()
}

/// New objects get ids of their program's own (InDesign counts objects IDML never shows):
/// pair the new elements of both sides by place (part, order, tag) and name both `~k`.
fn rename_new(folio: &Delta, indesign: &Delta) -> (Delta, Delta, Vec<String>) {
    let mut out = Vec::new();
    for part in folio.added.iter().map(|a| part_key(folio, &a.0)).chain(indesign.added.iter().map(|a| part_key(indesign, &a.0))).collect::<BTreeSet<_>>() {
        let f: Vec<_> = folio.added.iter().filter(|a| part_key(folio, &a.0) == part).map(|a| &a.2).collect();
        let i: Vec<_> = indesign.added.iter().filter(|a| part_key(indesign, &a.0) == part).map(|a| &a.2).collect();
        if f != i {
            out.push(format!("{part}: new elements Folio {f:?} / InDesign {i:?}"));
        }
    }
    let (fm, im) = pairing(folio, indesign);
    (renamed(folio, &fm), renamed(indesign, &im), out)
}

/// The ids `rename_new` gives new objects on each side: (Folio's, InDesign's).
fn pairing(folio: &Delta, indesign: &Delta) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let (mut fm, mut im) = (BTreeMap::new(), BTreeMap::new());
    let mut k = 0;
    for part in folio.added.iter().map(|a| part_key(folio, &a.0)).chain(indesign.added.iter().map(|a| part_key(indesign, &a.0))).collect::<BTreeSet<_>>() {
        let f: Vec<_> = folio.added.iter().filter(|a| part_key(folio, &a.0) == part).collect();
        let i: Vec<_> = indesign.added.iter().filter(|a| part_key(indesign, &a.0) == part).collect();
        for (a, b) in f.iter().zip(&i) {
            fm.insert(a.1.clone(), format!("~{k}"));
            im.insert(b.1.clone(), format!("~{k}"));
            k += 1;
        }
    }
    for p in &folio.parts_added { fm.insert(p.clone(), part_key(folio, p)); }
    for p in &indesign.parts_added { im.insert(p.clone(), part_key(indesign, p)); }
    (fm, im)
}

fn renamed(d: &Delta, map: &BTreeMap<String, String>) -> Delta {
    let id = |s: &str| map.get(s).cloned().unwrap_or_else(|| s.to_owned());
    let value = |v: &str| map.get(v).cloned().unwrap_or_else(|| v.split(' ').map(|t| map.get(t).cloned().unwrap_or_else(|| t.to_owned())).collect::<Vec<_>>().join(" "));
    fn body(b: &[Body], id: &dyn Fn(&str) -> String, value: &dyn Fn(&str) -> String) -> Vec<Body> {
        b.iter()
            .map(|x| match x {
                Body::Ref(r) => Body::Ref(id(r)),
                Body::Node { tag, attrs, text, kids } => Body::Node {
                    tag: tag.clone(),
                    attrs: attrs.iter().map(|(k, v)| (k.clone(), value(v))).collect(),
                    text: text.clone(),
                    kids: body(kids, id, value),
                },
            })
            .collect()
    }
    Delta {
        elements: d
            .elements
            .iter()
            .map(|((p, e), sig)| {
                let sig = sig.as_ref().map(|s| Sig { tag: s.tag.clone(), attrs: s.attrs.iter().map(|(k, v)| (k.clone(), value(v))).collect(), body: body(&s.body, &id, &value) });
                ((id(p), id(e)), sig)
            })
            .collect(),
        parts_removed: d.parts_removed.clone(),
        parts_added: d.parts_added.iter().map(|p| id(p)).collect(),
        added: Vec::new(),
        part_order: Vec::new(),
    }
}

#[test]
fn renamed_font_self_with_spaces() {
    let font = "u13eFontnSegoe UI Symbol Regular";
    let map = BTreeMap::from([(font.to_owned(), "~1".to_owned()), ("u1".to_owned(), "~2".to_owned())]);
    let mut delta = Delta::default();
    delta.elements.insert(("Fonts.xml".into(), font.into()), Some(Sig {
        tag: "Font".into(),
        attrs: BTreeMap::from([("Self".into(), font.into()), ("Name".into(), "Segoe UI Symbol Regular".into()), ("Refs".into(), "u1 u2".into())]),
        body: Vec::new(),
    }));
    let actual = renamed(&delta, &map);
    let font = actual.elements.get(&("Fonts.xml".into(), "~1".into())).unwrap().as_ref().unwrap();
    assert_eq!(font.attrs["Self"], "~1");
    assert_eq!(font.attrs["Name"], "Segoe UI Symbol Regular");
    assert_eq!(font.attrs["Refs"], "~2 u2");
}

fn compare(folio: &Delta, indesign: &Delta) -> Vec<String> {
    let (folio, indesign, mut out) = rename_new(folio, indesign);
    let (folio, indesign) = (&folio, &indesign);
    if folio.parts_removed != indesign.parts_removed {
        out.push(format!("parts removed: Folio {:?}, InDesign {:?}", folio.parts_removed, indesign.parts_removed));
    }
    if folio.parts_added != indesign.parts_added {
        out.push(format!("parts added: Folio {:?}, InDesign {:?}", folio.parts_added, indesign.parts_added));
    }
    for k in folio.elements.keys().chain(indesign.elements.keys()).collect::<BTreeSet<_>>() {
        let who = format!("{} {}", k.0, k.1);
        match (folio.elements.get(k), indesign.elements.get(k)) {
            (Some(_), None) => out.push(format!("{who}: Folio changed it, InDesign did not")),
            (None, Some(_)) => out.push(format!("{who}: InDesign changed it, Folio did not")),
            (Some(None), Some(None)) => {}
            (Some(Some(a)), Some(Some(b))) => {
                if let Err(e) = same_sig(a, b) {
                    out.push(format!("{who}: {e}"));
                }
            }
            (Some(a), Some(b)) => out.push(format!("{who}: Folio {}, InDesign {}", if a.is_some() { "kept" } else { "removed" }, if b.is_some() { "kept" } else { "removed" })),
            (None, None) => unreachable!(),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// steps

/// Base64 in 76-column lines, as InDesign writes embedded `Contents` (folio_project::idmlparts::base64_lines).
fn base64_lines(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut col = 0;
    for g in data.chunks(3) {
        if col == 76 {
            out.push('\n');
            col = 0;
        }
        let b = [g[0], g.get(1).copied().unwrap_or(0), g.get(2).copied().unwrap_or(0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= g.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
        col += 4;
    }
    out
}

/// `oracle.id` label → the Self of the object carrying it.
fn labels(parts: &BTreeMap<String, String>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (name, t) in parts {
        if !(name.starts_with("Spreads/") || name.starts_with("MasterSpreads/") || name.starts_with("Stories/")) {
            continue;
        }
        let ix = Index::scan(t).unwrap();
        for i in 0..ix.nodes.len() {
            if ix.local(t, i) == "KeyValuePair" && ix.attr(t, i, "Key") == Some("oracle.id") {
                let owner = ix.ancestors(i).find(|&a| ix.attr(t, a, "Self").is_some()).unwrap();
                out.insert(ix.value(t, i, "Value").unwrap(), ix.attr(t, owner, "Self").unwrap().to_owned());
            }
        }
    }
    out
}

/// InDesign's UIColors enumerator name (GOLD, LIGHT_BLUE) as IDML writes it (Gold, LightBlue).
fn ui_color(s: &str) -> String {
    s.split('_').map(|w| { let w = w.to_lowercase(); let mut c = w.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).collect()
}

/// Every layer of the document, bottom to top.
fn layer_ids(parts: &BTreeMap<String, String>) -> Vec<String> {
    let dm = &parts["designmap.xml"];
    let ix = Index::scan(dm).unwrap();
    (0..ix.nodes.len()).filter(|&i| ix.local(dm, i) == "Layer").filter_map(|i| ix.attr(dm, i, "Self").map(str::to_owned)).collect()
}

fn layer_id(parts: &BTreeMap<String, String>, name: &str) -> String {
    let t = &parts["designmap.xml"];
    let ix = Index::scan(t).unwrap();
    (0..ix.nodes.len())
        .find(|&i| ix.local(t, i) == "Layer" && ix.value(t, i, "Name").as_deref() == Some(name))
        .and_then(|i| ix.attr(t, i, "Self").map(str::to_owned))
        .unwrap_or_else(|| panic!("no layer {name}"))
}

/// A swatch named in a scenario ("Oracle Blue", "None", "Black") as IDML names it.
fn swatch(name: &str) -> String {
    if name.starts_with("MixedInk/") { return name.into(); }
    if name == "None" { "Swatch/None".into() } else { format!("Color/{name}") }
}

/// A run-scenarios `tableProps` object (DOM names: `startRowFillColor`, `endColumnLineStyle`) as table attributes: PascalCase, swatch
/// names as swatch ids, stroke style names as stroke style ids ("Dotted" is InDesign's "Canned Dotted").
fn table_options(props: &Value) -> BTreeMap<String, String> {
    props.as_object().unwrap().iter().map(|(k, v)| {
        let name = k[..1].to_uppercase() + &k[1..];
        let value = match v {
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) if name.ends_with("Color") => swatch(s),
            Value::String(s) if name.ends_with("Type") || name.ends_with("LineStyle") => format!("StrokeStyle/$ID/{}", if s == "Dotted" { "Canned Dotted" } else { s }),
            other => panic!("table property {k}: {other}"),
        };
        (name, value)
    }).collect()
}

/// Run-scenarios' guide steps address `pages[n]` or `spreads[n]` and give `location` from the
/// ruler origin; the command wants a page and a distance from its edge. Each spread's pages:
/// id and top-left corner, with the spread's origin (the fixtures use SpreadOrigin).
fn guide_spreads(parts: &BTreeMap<String, String>) -> Vec<Vec<(String, (f64, f64))>> {
    spreads_in(parts, "idPkg:Spread")
}

/// The same for the parents (`doc.masterSpreads`, in designmap order).
fn master_spreads(parts: &BTreeMap<String, String>) -> Vec<Vec<(String, (f64, f64))>> {
    spreads_in(parts, "idPkg:MasterSpread")
}

fn spreads_in(parts: &BTreeMap<String, String>, tag: &str) -> Vec<Vec<(String, (f64, f64))>> {
    let prefs = &parts["Resources/Preferences.xml"];
    assert!(prefs.contains("RulerOrigin=\"SpreadOrigin\""), "guide scenarios assume the spread ruler origin");
    let dm = &parts["designmap.xml"];
    let dx = Index::scan(dm).unwrap();
    let mut out = Vec::new();
    for i in (0..dx.nodes.len()).filter(|&i| &dm[dx.nodes[i].name.clone()] == tag) {
        let t = &parts[dx.attr(dm, i, "src").unwrap()];
        let ix = Index::scan(t).unwrap();
        out.push((0..ix.nodes.len()).filter(|&j| ix.local(t, j) == "Page").map(|j| {
            let b: Vec<f64> = ix.attr(t, j, "GeometricBounds").unwrap().split(' ').map(|v| v.parse().unwrap()).collect();
            let m: Vec<f64> = ix.attr(t, j, "ItemTransform").unwrap().split(' ').map(|v| v.parse().unwrap()).collect();
            assert!(m[0] == 1.0 && m[1] == 0.0 && m[2] == 0.0 && m[3] == 1.0, "guide scenarios use untransformed pages");
            (ix.attr(t, j, "Self").unwrap().to_owned(), (b[1] + m[4], b[0] + m[5]))
        }).collect());
    }
    out
}

/// The pages a creation step (`createItem`, `createPath`, `createThreadFrame`) counts its `page` in: the document's pages, or with
/// `master: m` the pages of parent m (`doc.masterSpreads[m].pages`).
fn creation_pages(parts: &BTreeMap<String, String>, step: &Value) -> Vec<String> {
    match step["master"].as_u64() {
        Some(m) => master_spreads(parts)[m as usize].iter().map(|p| p.0.clone()).collect(),
        None => page_ids(parts),
    }
}

/// A guide step's page and its distance from that page's edge.
fn guide_at(parts: &BTreeMap<String, String>, step: &Value, horizontal: bool) -> (String, f64) {
    // `master: m` puts the guide on parent m: on its page `page`, or on the parent spread with `spread: true`
    let master = step["master"].as_u64();
    let spreads = if master.is_some() { master_spreads(parts) } else { guide_spreads(parts) };
    let (s, k) = match step["spread"].as_u64() {
        _ if master.is_some() => (master.unwrap() as usize, if step["spread"] == true { 0 } else { step["page"].as_u64().unwrap_or(0) as usize }),
        Some(n) => (n as usize, 0),
        None => {
            let mut n = step["page"].as_u64().unwrap_or(0) as usize;
            let s = spreads.iter().position(|s| if n < s.len() { true } else { n -= s.len(); false }).unwrap();
            (s, n)
        }
    };
    let origin = spreads[s].iter().fold((f64::INFINITY, f64::INFINITY), |a, p| (a.0.min(p.1.0), a.1.min(p.1.1)));
    let (id, corner) = &spreads[s][k];
    let location = step["location"].as_f64().unwrap_or(0.0);
    (id.clone(), if horizontal { location + origin.1 - corner.1 } else { location + origin.0 - corner.0 })
}

/// `pages[n].guides[index]` now, and whether it is horizontal.
fn guide_now(ed: &Editor, step: &Value) -> (String, bool) {
    let parts = ed.parts();
    // a spread guide of a parent sits in the parent spread's first page
    let n = if step["master"].is_u64() && step["spread"] == true { 0 } else { step["page"].as_u64().unwrap_or(0) as usize };
    let pages = match step["master"].as_u64() {
        Some(m) => master_spreads(parts)[m as usize].clone(),
        None => guide_spreads(parts).concat(),
    };
    let page = &pages[n].0;
    for t in parts.values() {
        let Ok(ix) = Index::scan(t) else { continue };
        if let Some(p) = ix.by_self(t, page) && ix.local(t, p) == "Page" {
            let g: Vec<usize> = ix.children(p).iter().copied().filter(|&c| ix.local(t, c) == "Guide").collect();
            let g = g[step["index"].as_u64().unwrap_or(0) as usize];
            return (ix.attr(t, g, "Self").unwrap().to_owned(), ix.attr(t, g, "Orientation") != Some("Vertical"));
        }
    }
    panic!("no page {page}")
}

/// A scenario's transparency settings of one level (`{blendingSettings: {opacity, blendMode},
/// dropShadowSettings: {mode, size, …}}`) as commands: scripting names become IDML ones
/// (`xOffset` → `XOffset`, `BlendMode.NORMAL` → `Normal`, a swatch name → its `Color/`).
fn transparency(id: String, level: folio_edit::BlendLevel, settings: &Value) -> Vec<Command> {
    let mut out = Vec::new();
    for (key, v) in settings.as_object().into_iter().flatten() {
        if key == "blendingSettings" {
            out.push(Command::SetBlending { ids: vec![id.clone()], level, opacity: v["opacity"].as_f64(), mode: v["blendMode"].as_str().map(|m| ui_color(m.trim_start_matches("BlendMode."))),
                isolate: v["isolateBlending"].as_bool(), knockout: v["knockoutGroup"].as_bool() });
            continue;
        }
        let effect: folio_edit::Effect = serde_json::from_value(serde_json::json!(match key.as_str() {
            "featherSettings" => "basicFeather",
            k => k.trim_end_matches("Settings"),
        })).unwrap_or_else(|_| panic!("no effect {key}"));
        let mut on = None;
        let mut values = BTreeMap::new();
        for (k, x) in v.as_object().unwrap() {
            let enumerated = x.as_str().and_then(|e| e.split_once('.')).filter(|(t, _)| t.chars().next().is_some_and(char::is_uppercase)).map(|(_, e)| ui_color(e));
            match k.as_str() {
                "applied" => on = x.as_bool(),
                "mode" => on = Some(enumerated.as_deref().is_none_or(|e| e != "None")),
                _ => {
                    let name = k[..1].to_uppercase() + &k[1..];
                    let value = if k.ends_with("Color") { serde_json::json!(swatch(x.as_str().unwrap())) } else if let Some(e) = enumerated { serde_json::json!(e) } else { x.clone() };
                    values.insert(name, value);
                }
            }
        }
        out.push(Command::SetEffect { ids: vec![id.clone()], level, effect, on, values });
    }
    out
}

/// A style named in a scenario as IDML's `Self` ("[None]" and the other built-ins carry `$ID/`).
fn style_id(kind: &str, builtin: &str, name: &str) -> String {
    if name == builtin || name == "[None]" { format!("{kind}/$ID/{builtin}") } else { format!("{kind}/{}", name.replace(':', "%3a")) }
}

/// A `tableStyleDef` step's DOM properties as the style's attributes: PascalCase, swatches as `Color/…`, styles as their `Self`,
/// stroke types as stroke styles, enumerations (`VerticalJustification.CENTER_ALIGN`) in IDML's words.
fn style_props(tag: &str, props: &Value) -> BTreeMap<String, String> {
    let Some(props) = props.as_object() else { return BTreeMap::new() };
    props.iter().map(|(k, v)| {
        let name = k[..1].to_uppercase() + &k[1..];
        let value = match v {
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) if name == "BasedOn" => style_id(tag, if tag == "CellStyle" { "[None]" } else { "[No table style]" }, s),
            Value::String(s) if name.ends_with("RegionCellStyle") => style_id("CellStyle", "[None]", s),
            Value::String(s) if name == "AppliedParagraphStyle" => format!("ParagraphStyle/{s}"),
            Value::String(s) if name.ends_with("Color") => swatch(s),
            Value::String(s) if name.ends_with("StrokeType") || name.ends_with("LineStyle") => format!("StrokeStyle/$ID/{s}"),
            Value::String(s) if s.contains('.') => s.rsplit('.').next().unwrap().split('_').map(|w| w[..1].to_uppercase() + &w[1..].to_lowercase()).collect(),
            other => panic!("style property {k}: {other}"),
        };
        (name, value)
    }).collect()
}

// The Transform panel menu settings are the application's, not the document's: InDesign's, as the
// golden recorded them before the steps, until a step changes them.
thread_local! {
    static ADJUST: std::cell::Cell<folio_edit::Adjust> = const { std::cell::Cell::new(folio_edit::Adjust { stroke: true, effects: true }) };
    /// When Scaling: Adjust Scaling Percentage (the scale stays in the transform) instead of Apply to Content.
    static KEEP_SCALE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs one scenario step (one or more undo steps); how many it made.
fn run_step(ed: &mut Editor, step: &Value, ids: &HashMap<String, String>, parts: &BTreeMap<String, String>) -> Result<usize, String> {
    // a step on a parent (`master: m`) counts the parents as the earlier steps left them (a New Parent before it)
    let live;
    let parts = if step["master"].is_u64() { live = ed.parts().clone(); &live } else { parts };
    if step["op"]=="insertAnchoredObject" {
        let command=folio_edit::commands::oracle::command(step,step["target"].as_str().and_then(|t|ids.get(t)).map(String::as_str)).map_err(|e|e.to_string())?.unwrap();
        let result=ed.apply("anchored",&[command]).map_err(|e|e.to_string())?;
        if let (Some(label),Some(id))=(step["label"].as_str(),result.created.first()) {
            ed.apply("label",&[Command::SetLabel{id:id.clone(),key:"oracle.id".into(),value:label.into()}]).map_err(|e|e.to_string())?;return Ok(2);
        }
        return Ok(1);
    }
    if step["op"] == "resolveHyperlinkTarget" {
        let data = folio_idml::hyperlinks::read(ed.parts())?;
        let id = data.links.iter().find(|l| Some(l.name.as_str()) == step["link"].as_str()).map(|l| l.id.as_str()).unwrap_or("absent-linkgoto");
        let target = serde_json::to_value(ed.hyperlink_target(id).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let expected = if step["kind"] == "text" {
            let frame = &ids[step["targetLabel"].as_str().unwrap()];
            let story = ed.parts().values().find_map(|xml| {
                let ix = Index::scan(xml).ok()?;
                ix.attr(xml, ix.by_self(xml, frame)?, "ParentStory").map(str::to_owned)
            }).ok_or("target frame has no story")?;
            serde_json::json!({"kind":"text","story":story,"start":step["at"],"end":step["at"]})
        } else if step["kind"] == "unsupported" {
            serde_json::json!({"kind":"unsupported","destination_kind":step["destination_kind"]})
        } else if step["kind"] == "url" {
            serde_json::json!({"kind":"url","url":step["url"]})
        } else if step["kind"] == "page" {
            serde_json::json!({"kind":"page","page":page_ids(parts)[step["pageIndex"].as_u64().ok_or("pageIndex")? as usize],"view":step["view"]})
        } else { serde_json::json!({"kind":step["kind"]}) };
        if target != expected { return Err(format!("hyperlink target: {target} != {expected}")); }
        return Ok(0);
    }
    // InDesign's sort options as the scenario found them (index-sort: a reading, no change)
    if step["op"] == "indexSortProbe" { return Ok(0); }
    if step["op"] == "observeTransformTotals" {
        // Only InDesign's DOM invariance is measured; Folio has no panel totals setting yet.
        return Ok(0);
    }
    if step["op"] == "bookxLibraryPlaceText" {
        // Asset.placeAsset(Text): the library item goes into the story as an inline anchored object (bookx-c4)
        let snapshot = ed.library_snapshot(&[ids[step["target"].as_str().unwrap()].clone()]).map_err(|e| e.to_string())?;
        let frame = &ids[step["story"].as_str().unwrap()];
        let story = ed.parts().values().find_map(|xml| {
            let ix = Index::scan(xml).ok()?;
            ix.attr(xml, ix.by_self(xml, frame)?, "ParentStory").map(str::to_owned)
        }).ok_or("the text frame has no story")?;
        let start = step["start"].as_u64().unwrap();
        let end = step["end"].as_u64().unwrap_or(start);
        let command = serde_json::from_value(serde_json::json!({"op":"placeLibraryAssetInText","parts":snapshot.parts,"ids":snapshot.ids,"story":story,"start":start,"end":end})).map_err(|e| e.to_string())?;
        let outcome = ed.apply("library", &[command]).map_err(|e| e.to_string())?;
        let id = outcome.created.first().ok_or("no library object created")?.clone();
        ed.apply("label", &[Command::SetLabel { id, key: "oracle.id".into(), value: step["label"].as_str().unwrap().into() }]).map_err(|e| e.to_string())?;
        return Ok(2);
    }
    if step["op"] == "bookxLibraryPlace" {
        let snapshot = ed.library_snapshot(&[ids[step["target"].as_str().unwrap()].clone()]).map_err(|e| e.to_string())?;
        let command = serde_json::from_value(serde_json::json!({"op":"placeLibraryAsset","parts":snapshot.parts,"ids":snapshot.ids,"page":page_ids(parts)[0]})).map_err(|e| e.to_string())?;
        let outcome = ed.apply("library", &[command]).map_err(|e| e.to_string())?;
        let id = outcome.created.first().ok_or("no library object created")?.clone();
        ed.apply("label", &[Command::SetLabel { id, key: "oracle.id".into(), value: step["label"].as_str().unwrap().into() }]).map_err(|e| e.to_string())?;
        return Ok(2);
    }
    if step["op"] == "contentPlace" {
        // the Content Placer (Page.contentPlace): the object copied with its top left corner at `at` on the page, Create Link and
        // Map Styles as the step says; InDesign cascades several objects by a fraction of a point, so one object a step
        let sel: Vec<String> = step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect();
        ed.copy(&sel).map_err(|e| e.to_string())?;
        // InDesign puts the top left corner of the visible bounds (stroke included) at the point
        let b = ed.outline_bounds(&sel[0], false).ok_or("the source has no bounds")?;
        let at = [step["at"][0].as_f64().unwrap() + (b[2] - b[0]) / 2.0, step["at"][1].as_f64().unwrap() + (b[3] - b[1]) / 2.0];
        let page = page_ids(parts)[step["page"].as_u64().unwrap_or(0) as usize].clone();
        let mut commands = vec![Command::Paste { page: Some(page), at: Some(at), layer: None, into: None, remember_layers: false }];
        if step["link"] == true {
            commands.push(Command::Registered(folio_edit::commands::RegisteredCommand::LinkPlaced { map_styles: step["mapStyles"] == true, date: "2000-01-01T00:00:00".into() }));
        }
        let o = ed.apply("step", &commands).map_err(|e| e.to_string())?;
        let labels: Vec<String> = step["labels"].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect()).unwrap_or_default();
        let set: Vec<Command> = o.created.iter().zip(&labels).map(|(id, l)| Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.clone() }).collect();
        if set.is_empty() { return Ok(1); }
        ed.apply("labels", &set).map_err(|e| e.to_string())?;
        return Ok(2);
    }
    if step["op"] == "menuPlaceAndLink" {
        // Edit > Place and Link (conveyor-c4): the selection loads the cursor, a click on the page places it, the top left corner of the
        // visible bounds at the point and linked, Map Styles on; the DOM's Page.contentPlace with those flags is the same
        let sel: Vec<String> = step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect();
        ed.copy(&sel).map_err(|e| e.to_string())?;
        let page = page_ids(parts)[step["page"].as_u64().unwrap_or(0) as usize].clone();
        let at = [step["at"][0].as_f64().unwrap(), step["at"][1].as_f64().unwrap()];
        let place = Command::Registered(folio_edit::commands::RegisteredCommand::PlaceAndLink { page, at, map_styles: step["mapStyles"].as_bool(), date: "2000-01-01T00:00:00".into() });
        let o = ed.apply("step", &[place]).map_err(|e| e.to_string())?;
        let labels: Vec<String> = step["labels"].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect()).unwrap_or_default();
        let set: Vec<Command> = o.created.iter().zip(&labels).map(|(id, l)| Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.clone() }).collect();
        if set.is_empty() { return Ok(1); }
        ed.apply("labels", &set).map_err(|e| e.to_string())?;
        return Ok(2);
    }
    if step["op"] == "contentUpdate" || step["op"] == "contentUnlink" {
        // Links panel: Update Link / Unlink on the content links (object and story) of one placed object
        let frame = ids[step["target"].as_str().unwrap()].clone();
        let story = ed.parts().values().find_map(|xml| {
            let ix = Index::scan(xml).ok()?;
            ix.attr(xml, ix.by_self(xml, &frame)?, "ParentStory").map(str::to_owned)
        });
        let links: Vec<_> = ed.content_links().into_iter().filter(|c| c.target == frame || Some(&c.target) == story.as_ref()).collect();
        if links.is_empty() { return Err(format!("no content link on {frame}")); }
        let commands: Vec<Command> = links.iter().map(|c| Command::Registered(if step["op"] == "contentUpdate" {
            folio_edit::commands::RegisteredCommand::UpdateContentLink { link: c.link.clone(), date: "2000-01-01T00:00:00".into(), overwrite: Some(true) }
        } else {
            folio_edit::commands::RegisteredCommand::UnlinkContentLink { link: c.link.clone() }
        })).collect();
        ed.apply("step", &commands).map_err(|e| e.to_string())?;
        return Ok(1);
    }
    if matches!(step["op"].as_str(),Some("insertHyperlink"|"setHyperlink"|"deleteHyperlink"|"insertCrossReference"|"setCrossReference"|"updateCrossReference"|"saveCrossReferenceFormat")) {
        let data=folio_idml::hyperlinks::read(ed.parts())?;
        let mut wire=step.clone();let obj=wire.as_object_mut().unwrap();
        if let Some(name)=obj.remove("targetLabel") {obj.insert("target_story".into(),serde_json::json!(ids[name.as_str().unwrap()]));}
        if let Some(name)=obj.remove("link") {obj.insert("id".into(),serde_json::json!(data.links.iter().find(|l|l.name==name.as_str().unwrap()).ok_or("link missing")?.id));}
        if let Some(name)=obj.remove("destinationName") {obj.insert("destination".into(),serde_json::json!(data.destinations.iter().find(|d|d.name==name.as_str().unwrap()).ok_or("destination missing")?.id));}
        if matches!(step["op"].as_str(),Some("insertCrossReference"|"setCrossReference"|"updateCrossReference"|"saveCrossReferenceFormat")) {
            // the server fills the locations from its composed document: the composed pages and list numbers here
            static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let doc=folio_idml::read_parts(ed.parts(),std::path::Path::new("xref.idml"),&Default::default()).map_err(|e|e.to_string())?;
            let layout=folio_text::layout_document(&doc,FONTS.get_or_init(||folio_text::FontStore::discover(&[])));
            obj.insert("locations".into(),serde_json::to_value(folio_text::layout::toc::sources(&doc,&layout)).map_err(|e|e.to_string())?);
        }
        let command=folio_edit::commands::oracle::command(&wire,step["target"].as_str().and_then(|t|ids.get(t)).map(String::as_str)).map_err(|e|e.to_string())?.unwrap();
        ed.apply("hyperlink",&[command]).map_err(|e|e.to_string())?;return Ok(1);
    }
    if step["op"]=="createQrVector" {
        let mut wire=step.clone();wire["page"]=serde_json::json!(page_ids(parts)[step["page"].as_u64().unwrap_or(0) as usize]);
        let command=folio_edit::commands::oracle::command(&wire,None).map_err(|e|e.to_string())?.unwrap();
        let result=ed.apply("QR",&[command]).map_err(|e|e.to_string())?;
        if let (Some(label),Some(id))=(step["label"].as_str(),result.created.last()) {
            ed.apply("label",&[Command::SetLabel{id:id.clone(),key:"oracle.id".into(),value:label.into()}]).map_err(|e|e.to_string())?;
            return Ok(2);
        }
        return Ok(1);
    }
    if matches!(step["op"].as_str(),Some("captionGenerate" | "captionConvert")) {
        let target=ids[step["target"].as_str().unwrap()].clone();
        let wire=if step["op"]=="captionGenerate" {
            serde_json::json!({"op":"generateCaptions","ids":[target],"page":page_ids(parts)[step["page"].as_u64().unwrap_or(0) as usize],"live":step["live"]})
        } else {serde_json::json!({"op":"convertToStaticCaption","story":target,"start":step["start"],"end":step["end"]})};
        let command=folio_edit::commands::oracle::command(&wire,Some(&target)).map_err(|e|e.to_string())?.unwrap();
        let result=ed.apply("caption",&[command]).map_err(|e|e.to_string())?;
        if let (Some(label),Some(id))=(step["label"].as_str(),result.created.last()) {
            ed.apply("label",&[Command::SetLabel{id:id.clone(),key:"oracle.id".into(),value:label.into()}]).map_err(|e|e.to_string())?;
            return Ok(2);
        }
        return Ok(1);
    }
    if step["op"]=="setUser" {return Ok(0);} // the author is carried by each step; this only switches InDesign's user
    if step["op"]=="pdfCommentsPanel" {return Ok(0);} // opening a view has no edit command
    if step["op"] == "saveAsParent" {
        let dm = &parts["designmap.xml"];
        let ix = Index::scan(dm).unwrap();
        let p = (0..ix.nodes.len()).filter(|&i| &dm[ix.nodes[i].name.clone()] == "idPkg:Spread").nth(step["spread"].as_u64().unwrap() as usize).unwrap();
        let t = &parts[ix.attr(dm,p,"src").unwrap()];
        let sx = Index::scan(t).unwrap();
        let id = (0..sx.nodes.len()).find_map(|i| if sx.local(t,i)=="Spread" {sx.attr(t,i,"Self")} else {None}).unwrap();
        let c = serde_json::from_value(serde_json::json!({"op":"saveAsParent","spread":id})).unwrap();
        ed.apply("save parent", &[c]).map_err(|e|e.to_string())?;
        return Ok(1);
    }
    let mut definition_step=step.clone();
    if step["op"]=="clearOverridesNotInStyle" {definition_step["ids"]=serde_json::json!([ids[step["target"].as_str().unwrap()]]);}
    if let Some(label)=step["from"].as_str().and_then(|l|ids.get(l)) {definition_step["from"]=Value::String(label.clone());}
    if let Ok(definition) = serde_json::from_value::<folio_edit::ObjectStyleDefinition>(definition_step) {
        ed.apply("step", &[Command::ObjectStyleDefinition(definition)]).map_err(|e| e.to_string())?;
        return Ok(1);
    }
    if step["op"]=="rotateSpreadView" {
        // the scenario names the spread by its number in the document and the action by the menu's name (by-cw: "90° CW")
        let dm=&parts["designmap.xml"];let dx=Index::scan(dm).unwrap();let mut spreads=Vec::new();
        for i in 0..dx.nodes.len(){
            if dx.local(dm,i)!="Spread"{continue}
            let Some(part)=dx.attr(dm,i,"src") else{continue};let t=&parts[part];let ix=Index::scan(t).unwrap();
            spreads.extend((0..ix.nodes.len()).filter(|&j|ix.local(t,j)=="Spread").filter_map(|j|ix.attr(t,j,"Self").map(str::to_owned)));
        }
        let how=step["how"].as_str().unwrap();
        let wire=serde_json::json!({"op":"rotateSpreadView","spread":spreads[step["spread"].as_u64().unwrap() as usize],"how":how.strip_prefix("by-").unwrap_or(how)});
        let command=folio_edit::commands::oracle::command(&wire,None).map_err(|e|e.to_string())?.unwrap();
        ed.apply("step",&[command]).map_err(|e|e.to_string())?;
        return Ok(1);
    }
    if step["op"] == "adjustGap" {
        let mut wire = step.clone();
        wire["page"] = serde_json::json!(page_ids(parts)[step["page"].as_u64().unwrap_or(0) as usize]);
        let command = folio_edit::commands::oracle::command(&wire, None).map_err(|e| e.to_string())?.unwrap();
        ed.apply("gap", &[command]).map_err(|e| e.to_string())?;
        return Ok(1);
    }
    // setClipping and createTextPath use this registry bridge, including target -> ids translation.
    let one = |ed: &mut Editor, c: Command| ed.apply("step", &[c]).map_err(|e| e.to_string());
    let label_new = |ed: &mut Editor, created: Vec<String>| -> Result<usize, String> {
        match (step["label"].as_str(), created.first()) {
            (Some(l), Some(id)) => {
                ed.apply("label", &[Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.into() }]).map_err(|e| e.to_string())?;
                Ok(2)
            }
            _ => Ok(1),
        }
    };
    if step["op"] == "createPolygon" {
        let page = page_ids(parts)[step["page"].as_u64().unwrap_or(0) as usize].clone();
        let (kind, rect, layer, sides, inset) = (step["kind"].as_str().unwrap().to_owned(), serde_json::from_value(step["rect"].clone()).unwrap(),
            step["layer"].as_str().map(|l| layer_id(parts, l)).unwrap_or_default(), step["sides"].as_u64().unwrap() as u32, step["inset"].as_f64().unwrap());
        // "view": the DOM's bounds are read as the page is seen in the turned spread (rules/drawtools.toml, drawtools.turned.*)
        let command = if step["view"] == true { folio_edit::commands::RegisteredCommand::CreateInView { page, kind, rect, layer, sides, inset } }
            else { folio_edit::commands::RegisteredCommand::CreatePolygon { page, kind, rect, layer, sides, inset } };
        let result = one(ed, command.into())?;
        return label_new(ed, result.created);
    }
    if let Some(mut command) = folio_edit::commands::oracle::command(step, step["target"].as_str().and_then(|t| ids.get(t)).map(String::as_str)).map_err(|e| e.to_string())? {
        if let Command::Registered(folio_edit::commands::RegisteredCommand::ConvertListsToText{story,labels,..})=&mut command {
            *labels=folio_text::layout::listconvert::labels(&read(ed.parts(),Path::new("oracle.idml")),story)?;
        }
        if let Command::Registered(folio_edit::commands::RegisteredCommand::SetGraphicLayerVisibility{layers,..})=&mut command
            && layers.is_none() && let Some(file)=step["file"].as_str() {
            // the host reads the layers of the placed PSD (server/src/psdlayers.rs does it for the app)
            let bytes=std::fs::read(oracle().join("fixtures/Links").join(file)).map_err(|e|e.to_string())?;
            *layers=folio_links::psdlayers::read(&bytes)?.map(|o|o.layers);
        }
        if let Command::Registered(folio_edit::commands::RegisteredCommand::ConvertStyleListsToText{labels,..})=&mut command {
            *labels=folio_text::layout::listconvert::document_labels(&read(ed.parts(),Path::new("oracle.idml")))?;
        }
        if let Command::Registered(folio_edit::commands::RegisteredCommand::GenerateIndex{locations,..})=&mut command {
            static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let doc = read(ed.parts(), Path::new("oracle.idml"));
            let layout = folio_text::layout_document(&doc, FONTS.get_or_init(|| folio_text::FontStore::discover(&[])));
            *locations = folio_text::layout::toc::locations(&doc, &layout);
        }
        if let Command::Registered(folio_edit::commands::RegisteredCommand::CreateOutlines{ids,shapes})=&mut command {
            // the host measures the glyph shapes (composition + fonts); the editor only places them
            static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let fonts = FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
            let doc = read(ed.parts(), Path::new("oracle.idml"));
            let layout = folio_text::layout_document(&doc, fonts);
            let all = folio_text::outlines::frames_to_convert(&doc, ids).iter().map(|id| folio_text::outlines::frame_shapes(&doc, &layout, fonts, id)).collect::<Result<Vec<_>, _>>()?;
            *shapes = serde_json::to_string(&all).map_err(|e| e.to_string())?;
        }
        if let Command::Registered(folio_edit::commands::RegisteredCommand::CreateOutlinesRange{story,start,end,shapes})=&mut command {
            // a text range: the host measures the run shapes, the editor puts them into the story as inline polygons
            static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let fonts = FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
            let doc = read(ed.parts(), Path::new("oracle.idml"));
            let layout = folio_text::layout_document(&doc, fonts);
            let all = folio_text::outlines::range_shapes(&doc, &layout, fonts, story, *start, *end)?;
            *shapes = serde_json::to_string(&all).map_err(|e| e.to_string())?;
        }
        ed.apply("step", &[command]).map_err(|e| e.to_string())?;
        return Ok(1);
    }
    if folio_edit::color_commands::ColorCommand::supports(step["op"].as_str().unwrap_or_default()) {
        let mut value = step.clone();
        if let Some(target) = step["target"].as_str() { value["id"] = serde_json::json!(ids[target]); }
        let c: Command = serde_json::from_value(value).map_err(|e| e.to_string())?;
        ed.apply("colour", &[c]).map_err(|e| e.to_string())?;
        return Ok(1);
    }
    if step["op"] == "viewPref" && matches!(step["name"].as_str(), Some("adjustStrokeWeightWhenScaling" | "adjustEffectsWhenScaling")) {
        let on = step["value"].as_bool().unwrap();
        ADJUST.with(|a| a.set(if step["name"] == "adjustStrokeWeightWhenScaling" { folio_edit::Adjust { stroke: on, ..a.get() } } else { folio_edit::Adjust { effects: on, ..a.get() } }));
        return Ok(0);
    }
    if step["op"] == "viewPref" && step["name"] == "whenScaling" {
        KEEP_SCALE.with(|d| d.set(step["value"] == "adjustScalingPercentage"));
        return Ok(0);
    }
    // Dimensions Include Stroke Weight is for the Transform panel: a scripted resize ignores it (tp-dim-off-resize)
    if step["op"] == "viewPref" && step["name"] == "dimensionsIncludeStrokeWeight" {
        return Ok(0);
    }
    let target = || {
        let l = step["target"].as_str().unwrap();
        ids.get(l).unwrap_or_else(|| panic!("no object labelled {l}")).clone()
    };
    let pair = |v: &Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
    let c = match step["op"].as_str().unwrap() {
        "embedLink" if step["unembed"].as_bool() == Some(true) => {
            // Unembed Link: the file is written out and the graphic linked to it, as a relink to it
            let file=step["file"].as_str().unwrap();
            Command::UnembedLink { id: target(), uri: format!("file:/oracle/Links/{file}") }
        }
        "embedLink" => {
            // the file the link points at (fixtures/Links, as the fixture's neutral link names it)
            let (part, t) = parts.iter().find(|(n, t)| n.starts_with("Spreads/") && t.contains(&format!("Self=\"{}\"", target()))).unwrap();
            let ix = Index::scan(t).unwrap();
            let frame = ix.by_self(t, &target()).unwrap();
            let link = (0..ix.nodes.len()).find(|&n| ix.local(t, n) == "Link" && ix.ancestors(n).any(|a| a == frame)).unwrap();
            let uri = unescape(ix.attr(t, link, "LinkResourceURI").unwrap());
            let _ = part;
            let name = uri.rsplit('/').next().unwrap().to_owned();
            let bytes = std::fs::read(oracle().join("fixtures/Links").join(&name)).unwrap();
            Command::EmbedLink { id: target(), data: base64_lines(&bytes) }
        }
        "relinkGraphic" => {
            let file=step["file"].as_str().unwrap();
            let pdf = step.get("pdf").map(|v| serde_json::from_value(v.clone())).transpose().map_err(|e|format!("PDF options: {e}"))?;
            let source=folio_edit::GraphicSource::read_with_pdf(&oracle().join("fixtures/Links").join(file),format!("file:/oracle/Links/{file}"),pdf)?;
            Command::RelinkGraphic { source, id: target() }
        }
        "placeGraphic" => {
            let file=step["file"].as_str().unwrap();
            let pdf = step.get("pdf").map(|v| serde_json::from_value(v.clone())).transpose().map_err(|e|format!("PDF options: {e}"))?;
            let source=folio_edit::GraphicSource::read_with_pdf(&oracle().join("fixtures/Links").join(file),format!("file:/oracle/Links/{file}"),pdf)?;
            let pages=creation_pages(parts,step);
            let command=Command::PlaceGraphic {
                source,id:step["target"].as_str().map(|_|target()),
                page:step["page"].as_u64().map(|p|pages[p as usize].clone()),
                point:step.get("point").map(|p|serde_json::from_value(p.clone()).unwrap()),
                rect:step.get("rect").map(|p|serde_json::from_value(p.clone()).unwrap()),
                layer:step["layer"].as_str().map(|l|layer_id(parts,l)),
            };
            let result=one(ed,command)?;
            return label_new(ed,result.created);
        }
        "placeText" => {
            // .txt (later .docx, .rtf) into a text frame, or a new frame at `rect`: the text replaces the story's
            let file=step["file"].as_str().unwrap();
            // `txt` is app.textImportPreferences, `word` is app.wordRTFImportPreferences: one options object takes both
            let mut props=serde_json::Map::new();
            for key in ["txt","word"]{for (k,v) in step.get(key).and_then(|v|v.as_object()).into_iter().flatten(){props.insert(k.clone(),v.clone());}}
            let options=folio_textimport::Options::from_dom(&serde_json::Value::Object(props))?;
            let source=folio_textimport::read(&oracle().join("fixtures/Links").join(file),&options)?;
            // a file that names fonts (RTF): the records of every style of each family, as the host supplies them
            static TEXT_FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let store=TEXT_FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
            ed.set_font_records(source.font_families().iter().flat_map(|f| ["Regular","Italic","Bold","Bold Italic"].map(|s| store.resource_record(f,s,&[]))).flatten().collect());
            let pages=creation_pages(parts,step);
            let command=Command::PlaceText {
                source,id:step["target"].as_str().map(|_|target()),
                page:step["page"].as_u64().map(|p|pages[p as usize].clone()),
                rect:step.get("rect").map(|p|serde_json::from_value(p.clone()).unwrap()),
                layer:step["layer"].as_str().map(|l|layer_id(parts,l)),
                point:step.get("point").map(|p|serde_json::from_value(p.clone()).unwrap()),
            };
            let result=one(ed,command)?;
            return label_new(ed,result.created);
        }
        "insertPages"|"deletePages"|"movePages"|"setPageShuffle"|"setSpreadHidden"|"duplicateSpread"|"applyMaster"|"setSection"|"documentSetup"|"setPageColor"|"setSpreadShowMaster"|"setSpreadFlattener"=>{
            // "current": the page and spread numbers count the document as the earlier steps left it (InDesign's doc.pages does)
            let now=if step["current"].as_bool()==Some(true){Some(ed.parts().clone())}else{None};
            let parts=now.as_ref().unwrap_or(parts);
            let mut c=step.clone();let dm=&parts["designmap.xml"];let dx=Index::scan(dm).unwrap();
            let mut pages=Vec::new();let mut spreads=Vec::new();let mut masters=Vec::new();
            for i in 0..dx.nodes.len(){
                let tag=dx.local(dm,i);
                if !["Spread","MasterSpread"].contains(&tag){continue}
                let Some(part)=dx.attr(dm,i,"src") else{continue};let t=&parts[part];let ix=Index::scan(t).unwrap();
                for j in 0..ix.nodes.len(){
                    if ix.local(t,j)==tag && let Some(id)=ix.attr(t,j,"Self"){
                        if tag=="Spread"{spreads.push(id.to_owned())}else{masters.push(id.to_owned())}
                    }
                    if tag=="Spread"&&ix.local(t,j)=="Page"{pages.push(ix.attr(t,j,"Self").unwrap().to_owned())}
                }
            }
            if step.get("master").is_some()||step.get("masters").is_some(){
                // parents made by an earlier step (Save as Parent) are only in the current parts
                let cur=ed.parts().clone();let dm=&cur["designmap.xml"];let dx=Index::scan(dm).unwrap();masters.clear();
                for i in 0..dx.nodes.len(){
                    if dx.local(dm,i)!="MasterSpread"{continue}
                    let Some(part)=dx.attr(dm,i,"src") else{continue};let t=&cur[part];let ix=Index::scan(t).unwrap();
                    for j in 0..ix.nodes.len(){if ix.local(t,j)=="MasterSpread"&&let Some(id)=ix.attr(t,j,"Self"){masters.push(id.to_owned())}}
                }
            }
            for key in ["page","before"]{if let Some(n)=step[key].as_u64(){c[key]=serde_json::json!(pages[n as usize]);}}
            if let Some(ns)=step["pages"].as_array(){c["pages"]=serde_json::json!(ns.iter().map(|n|pages[n.as_u64().unwrap()as usize].clone()).collect::<Vec<_>>());}
            if let Some(n)=step["spread"].as_u64(){c["spread"]=serde_json::json!(spreads[n as usize]);}
            if let Some(ns)=step["spreads"].as_array(){c["spreads"]=serde_json::json!(ns.iter().map(|n|spreads[n.as_u64().unwrap()as usize].clone()).collect::<Vec<_>>());}
            if let Some(n)=step["master"].as_u64(){c["master"]=serde_json::json!(masters[n as usize]);}
            if let Some(ns)=step["masters"].as_array(){c["masters"]=serde_json::json!(ns.iter().map(|n|masters[n.as_u64().unwrap()as usize].clone()).collect::<Vec<_>>());}
            if let Some(s)=step["style"].as_str(){c["style"]=serde_json::json!(match s{"UPPER_ROMAN"=>"UpperRoman","LOWER_ROMAN"=>"LowerRoman","UPPER_LETTERS"=>"UpperLetters","LOWER_LETTERS"=>"LowerLetters",_=>"Arabic"});}
            serde_json::from_value(c).unwrap()
        },
        "createItem" | "createThreadFrame" => {
            let pages=creation_pages(parts,step);
            if step["view"]==true {
                let kind:String=step["kind"].as_str().unwrap().to_owned();
                let command=folio_edit::commands::RegisteredCommand::CreateInView {
                    page:pages[step["page"].as_u64().unwrap_or(0) as usize].clone(), kind,
                    rect:serde_json::from_value(step["rect"].clone()).unwrap(),
                    layer:step["layer"].as_str().map(|l|layer_id(parts,l)).unwrap_or_default(),
                    sides:step["sides"].as_u64().unwrap_or(6) as u32, inset:0.0 };
                let result=one(ed,command.into())?;
                return label_new(ed,result.created);
            }
            let command=if step["op"]=="createThreadFrame" { Command::CreateThreadFrame {
                id:target(), page:pages[step["page"].as_u64().unwrap_or(0) as usize].clone(),
                rect:serde_json::from_value(step["rect"].clone()).unwrap(),
                layer:step["layer"].as_str().map(|l|layer_id(parts,l)),
            }} else { Command::CreateItem {
                page:pages[step["page"].as_u64().unwrap_or(0) as usize].clone(),
                kind:serde_json::from_value(step["kind"].clone()).unwrap(),
                rect:serde_json::from_value(step["rect"].clone()).unwrap(),
                layer:step["layer"].as_str().map(|l|layer_id(parts,l)),
                sides:step["sides"].as_u64().unwrap_or(6) as u32,
            }};
            let result=one(ed,command)?;
            return label_new(ed,result.created);
        },
        "createPath" => {
            let pages=creation_pages(parts,step);
            let result=one(ed,Command::CreatePath {
                page:pages[step["page"].as_u64().unwrap_or(0) as usize].clone(),
                points:serde_json::from_value(step["points"].clone()).unwrap(),
                closed:step["closed"].as_bool().unwrap(),
                layer:step["layer"].as_str().map(|l|layer_id(parts,l)),
            })?;
            return label_new(ed,result.created);
        },
        "movePoints" => Command::MovePathPoints { id: target(), path: step["path"].as_u64().unwrap_or(0) as usize,
            indices: serde_json::from_value(step["indices"].clone()).unwrap(), by: pair(&step["by"]) },
        "deletePoint" => Command::DeletePathPoint { id: target(), path: step["path"].as_u64().unwrap_or(0) as usize, index: step["index"].as_u64().unwrap() as usize },
        "plainPoint" => Command::PlainPathPoint { id: target(), path: step["path"].as_u64().unwrap_or(0) as usize, index: step["index"].as_u64().unwrap() as usize },
        "convertPoint" => folio_edit::commands::RegisteredCommand::ConvertPathPoints { id: target(), path: step["path"].as_u64().unwrap_or(0) as usize,
            indices: serde_json::from_value(step["indices"].clone()).unwrap(), kind: serde_json::from_value(step["kind"].clone()).unwrap() }.into(),
        "moveHandle" => Command::MovePathHandle { id: target(), path: step["path"].as_u64().unwrap_or(0) as usize, index: step["index"].as_u64().unwrap() as usize,
            side: serde_json::from_value(step["side"].clone()).unwrap(), to: None, by: Some(pair(&step["by"])), independent: step["independent"].as_bool().unwrap_or(false) },
        "findReplace" => {
            let mut c=step.clone();
            if step["target"].is_string() {c["query"]["story"]=serde_json::json!(target());}
            serde_json::from_value(c).unwrap()
        }
        // Edit ▸ Find/Change ▸ Glyph ▸ Change (rules/glyphfind.toml); the host supplies the glyph tables
        "glyphChange" => {
            static GLYPH_FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let fonts = GLYPH_FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
            let faces = [(step["find"]["family"].as_str().unwrap(), step["find"]["style"].as_str().unwrap()), (step["change"]["family"].as_str().unwrap(), step["change"]["style"].as_str().unwrap())];
            ed.set_glyph_fonts(faces.iter().filter_map(|(family, style)| folio_text::glyphs::table(fonts, &[], family, style).ok()
                .map(|(count, rows)| folio_edit::text::GlyphFont::new(family.to_string(), style.to_string(), count, rows))).collect());
            ed.set_font_records(fonts.resource_record(faces[1].0, faces[1].1, &[]).into_iter().collect());
            folio_edit::commands::RegisteredCommand::GlyphReplace { find_family: faces[0].0.into(), find_style: faces[0].1.into(), find_glyph: step["find"]["glyphId"].as_u64().unwrap() as usize,
                scope: step["scope"].as_str().unwrap_or("story").into(), story: target(), start: step["start"].as_u64().unwrap_or(0) as usize, end: step["end"].as_u64().unwrap_or(0) as usize,
                change_family: faces[1].0.into(), change_style: faces[1].1.into(), change_glyph: step["change"]["glyphId"].as_u64().unwrap() as usize }.into()
        }
        // Glyphs panel: insert one glyph by id (rules/glyphs.toml glyphs.insert-by-id); the host supplies the glyph table
        "glyphInsert" => {
            static GLYPH_FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
            let fonts = GLYPH_FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
            let (family, style) = (step["family"].as_str().unwrap(), step["style"].as_str().unwrap());
            ed.set_glyph_fonts(folio_text::glyphs::table(fonts, &[], family, style).ok()
                .map(|(count, rows)| folio_edit::text::GlyphFont::new(family.to_string(), style.to_string(), count, rows)).into_iter().collect());
            ed.set_font_records(fonts.resource_record(family, style, &[]).into_iter().collect());
            folio_edit::commands::RegisteredCommand::InsertGlyph { story: target(), at: step["at"].as_u64().unwrap() as usize,
                family: family.into(), style: style.into(), glyph: step["glyphId"].as_u64().unwrap() as usize }.into()
        }
        "textWrap" => {
            let mut c=step.clone();c["id"]=serde_json::json!(target());serde_json::from_value(c).unwrap()
        }
        "styleCreate" | "styleEdit" | "styleRedefine" | "styleDelete" | "setDropCapStyle" | "setLineStyles" => {
            let mut c=step.clone();
            if step["target"].is_string() {c["story"]=serde_json::json!(target());}
            serde_json::from_value(c).unwrap()
        }
        "threadText" => Command::ThreadText { id: target(), next: step["next"].as_str().map(|n| ids[n].clone()) },
        // Edit ▸ Paste without Formatting: the copied text as plain text where it lands
        "pastePlain" => Command::InsertText {
            text: ed.copy_text(&target(), step["start"].as_u64().unwrap() as usize, step["end"].as_u64().unwrap() as usize).map_err(|e| e.to_string())?.text,
            story: ids[step["destination"].as_str().unwrap()].clone(), at: step["at"].as_u64().unwrap() as usize,
        },
        "insertText" => Command::InsertText { story: target(), at: step["at"].as_u64().unwrap() as usize, text: step["text"].as_str().unwrap().into() },
        "defineVariable" => {
            // the DOM's names (VariableTypes, variableOptions) as the variable's IDML names
            let words = |v: &str| v.rsplit('.').next().unwrap().split('_').map(|w| w[..1].to_uppercase() + &w[1..].to_lowercase()).collect::<String>();
            let options = step["options"].as_object().into_iter().flatten().map(|(k, v)| {
                let value = match v {
                    Value::Bool(b) => b.to_string(),
                    Value::String(s) if k == "appliedParagraphStyle" => format!("ParagraphStyle/{s}"),
                    Value::String(s) if k == "appliedCharacterStyle" => format!("CharacterStyle/{s}"),
                    Value::String(s) if matches!(k.as_str(), "searchStrategy" | "changeCase" | "scope") => words(s),
                    Value::String(s) => s.clone(),
                    other => panic!("variable option {k}: {other}"),
                };
                (k[..1].to_uppercase() + &k[1..], value)
            }).collect();
            let name = step["name"].as_str().unwrap();
            if step["edit"].as_bool() == Some(true) {
                Command::EditTextVariable { id: format!("dTextVariablen{name}"), rename: step["rename"].as_str().map(str::to_owned), options }
            } else {
                Command::DefineTextVariable { name: name.into(), kind: words(step["type"].as_str().unwrap()), options }
            }
        }
        "convertVariable" => Command::ConvertVariableToText { story: target(), start: step["at"].as_u64().unwrap() as usize, end: step["at"].as_u64().unwrap() as usize + 1 },
        "deleteVariable" => Command::DeleteTextVariable { id: format!("dTextVariablen{}", step["name"].as_str().unwrap()) },
        "insertVariable" => Command::InsertVariable { story: target(), at: step["at"].as_u64().unwrap() as usize, variable: format!("dTextVariablen{}", step["variable"].as_str().unwrap()) },
        "insertSpecial" => Command::InsertSpecial { story: target(), at: step["at"].as_u64().unwrap() as usize, special: step["special"].as_str().unwrap().into() },
        "changeCase" => Command::ChangeCase { story: target(), start: step["start"].as_u64().unwrap() as usize, end: step["end"].as_u64().unwrap() as usize, mode: step["mode"].as_str().unwrap().into() },
        "deleteText" => Command::DeleteText { story: target(), start: step["start"].as_u64().unwrap() as usize, end: step["end"].as_u64().unwrap() as usize },
        "list" if step["id"] == 108296 => Command::ToggleNumberingRestart { story: target(), start: step["start"].as_u64().unwrap() as usize, end: step["end"].as_u64().unwrap() as usize },
        "list" => Command::ToggleList { story: target(), start: step["start"].as_u64().unwrap() as usize, end: step["end"].as_u64().unwrap() as usize, numbers: step["numbers"].as_bool().unwrap_or(false) },
        "paragraphStyle" | "characterStyle" | "textFormat" | "clearTextOverrides" => {
            let mut c=step.clone(); c["story"]=serde_json::json!(target());
            serde_json::from_value(c).unwrap()
        }
        "copyText" => Command::PasteText {
            fragment: ed.copy_text(&target(),step["start"].as_u64().unwrap() as usize,step["end"].as_u64().unwrap() as usize).map_err(|e|e.to_string())?,
            story: ids[step["destination"].as_str().unwrap()].clone(), at: step["at"].as_u64().unwrap() as usize,
        },
        "move" => Command::Move { ids: vec![target()], by: pair(&step["by"]) },
        "moveToSpread" => {
            let dm = &parts["designmap.xml"]; let ix = Index::scan(dm).unwrap();
            let refs: Vec<_> = (0..ix.nodes.len()).filter(|&i| &dm[ix.nodes[i].name.clone()] == "idPkg:Spread").collect();
            let p = ix.attr(dm, refs[step["spread"].as_u64().unwrap() as usize], "src").unwrap();
            let st = &parts[p]; let sx = Index::scan(st).unwrap();
            let i = (0..sx.nodes.len()).find(|&i| sx.local(st, i) == "Spread" && sx.attr(st, i, "Self").is_some()).unwrap();
            Command::MoveToSpread { ids: vec![target()], spread: sx.attr(st, i, "Self").unwrap().into(), by: pair(&step["by"]) }
        },
        "align" | "distribute" => {
            let mut c = step.clone();
            c["ids"] = serde_json::json!(step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect::<Vec<_>>());
            if let Some(key) = step["key"].as_str() { c["key"] = serde_json::json!(ids[key]); }
            serde_json::from_value(c).unwrap()
        }
        "resize" => Command::Resize { id: target(), size: pair(&step["size"]), anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or_default(), include_stroke: true },
        "resizeBox" => Command::ResizeBox { id: target(), rect: serde_json::from_value(step["rect"].clone()).unwrap() },
        "resizeMany" => Command::ResizeMany {
            ids: step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect(),
            size: pair(&step["size"]), anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or_default(),
            include_stroke: step["includeStroke"].as_bool().unwrap_or(true),
        },
        "delete" => Command::Delete { ids: vec![target()] },
        "set" => {
            let p = &step["props"];
            let levels = [("transparencySettings", folio_edit::BlendLevel::Object), ("strokeTransparencySettings", folio_edit::BlendLevel::Stroke),
                ("fillTransparencySettings", folio_edit::BlendLevel::Fill), ("contentTransparencySettings", folio_edit::BlendLevel::Text)];
            if p.get("horizontalScale").is_some() || p.get("verticalScale").is_some() {
                // one axis after the other, as the step sets them; the top-left reference point
                let mut cs = Vec::new();
                if let Some(x) = p["horizontalScale"].as_f64() {
                    cs.push(Command::Scale { id: target(), sx: x / 100.0, sy: 1.0, anchor: folio_edit::Anchor::TopLeft, adjust: None });
                }
                if let Some(y) = p["verticalScale"].as_f64() {
                    cs.push(Command::Scale { id: target(), sx: 1.0, sy: y / 100.0, anchor: folio_edit::Anchor::TopLeft, adjust: None });
                }
                ed.apply("step", &cs).map_err(|e| e.to_string())?;
                return Ok(1);
            }
            if p.get("rotationAngle").is_some() || p.get("shearAngle").is_some() {
                // scripting without a layout window: the top-left reference point
                let c = Command::SetAngles { id: target(), rotation: p["rotationAngle"].as_f64(), shear: p["shearAngle"].as_f64(), anchor: folio_edit::Anchor::TopLeft, content: false, totals: true };
                ed.apply("step", &[c]).map_err(|e| e.to_string())?;
                return Ok(1);
            }
            let stroke_keys = ["endCap", "endJoin", "miterLimit", "strokeAlignment", "strokeType", "leftLineEnd", "rightLineEnd", "leftArrowHeadScale", "rightArrowHeadScale", "gapColor", "gapTint"];
            if stroke_keys.iter().any(|k| p.get(*k).is_some()) {
                // enumerations as IDML writes them; built-in stroke styles by their Self
                let e = |k: &str| p[k].as_str().map(|v| ui_color(v.split('.').nth(1).unwrap_or(v)));
                let style = |v: &str| {
                    let id = match v {
                        "Thick - Thick" => "ThickThick", "Thick - Thin" => "ThickThin", "Thick - Thin - Thick" => "ThickThinThick", "Thin - Thick" => "ThinThick",
                        "Thin - Thick - Thin" => "ThinThickThin", "Thin - Thin" => "ThinThin", "Triple" => "Triple_Stroke", "Dashed (3 and 2)" => "Canned Dashed 3x2",
                        "Dashed (4 and 4)" => "Canned Dashed 4x4", "Dotted" => "Canned Dotted", v => v,
                    };
                    format!("StrokeStyle/$ID/{id}")
                };
                let change = folio_edit::StrokeChange {
                    cap: e("endCap"), join: e("endJoin"), miter_limit: p["miterLimit"].as_f64(), alignment: e("strokeAlignment"),
                    kind: p["strokeType"].as_str().map(style), start: e("leftLineEnd"), end: e("rightLineEnd"),
                    start_scale: p["leftArrowHeadScale"].as_f64(), end_scale: p["rightArrowHeadScale"].as_f64(),
                    gap_color: p["gapColor"].as_str().map(swatch), gap_tint: p["gapTint"].as_f64(),
                };
                let mut commands = Vec::new();
                if p.get("strokeWeight").is_some() || p.get("strokeColor").is_some() {
                    let stroke = p["strokeColor"].as_str().map(swatch);
                    commands.push(Command::SetPaint { ids: vec![target()], fill: None, fill_tint: None, stroke, stroke_tint: None, stroke_weight: p["strokeWeight"].as_f64() });
                }
                commands.push(Command::SetStroke { ids: vec![target()], change });
                ed.apply("step", &commands).map_err(|e| e.to_string())?;
                return Ok(1);
            } else if let Some(f) = p.get("textFramePreferences") {
                let e = |k: &str| f[k].as_str().map(|v| ui_color(v.split('.').nth(1).unwrap_or(v)));
                Command::TextFrameOptions { id: target(), options: folio_edit::FrameOptions {
                    columns: f["textColumnCount"].as_u64().map(|v| v as u32), gutter: f["textColumnGutter"].as_f64(),
                    fixed: f["useFixedColumnWidth"].as_bool(), column_width: f["textColumnFixedWidth"].as_f64(),
                    insets: f["insetSpacing"].as_array().map(|a| [0, 1, 2, 3].map(|k| a[k].as_f64().unwrap())),
                    vertical: e("verticalJustification"), ignore_wrap: f["ignoreWrap"].as_bool(),
                    first_baseline: e("firstBaselineOffset"), min_first_baseline: f["minimumFirstBaselineOffset"].as_f64(),
                    auto_size: e("autoSizingType"), auto_size_point: e("autoSizingReferencePoint"),
                    use_min_height: f["useMinimumHeightForAutoSizing"].as_bool(), min_height: f["minimumHeightForAutoSizing"].as_f64(),
                    use_min_width: f["useMinimumWidthForAutoSizing"].as_bool(), min_width: f["minimumWidthForAutoSizing"].as_f64(),
                    no_line_breaks: f["useNoLineBreaksForAutoSizing"].as_bool(),
                } }
            } else if let Some((key, level)) = levels.iter().find(|(k, _)| p.get(*k).is_some()) {
                ed.apply("step", &transparency(target(), *level, &p[*key])).map_err(|e| e.to_string())?;
                return Ok(1);
            } else if p.as_object().is_some_and(|o| o.keys().any(|k| k.contains("Corner"))) {
                // the object's four corners now (as the reader sees them), with the step's changes
                let id = target();
                let doc = read(ed.parts(), Path::new("oracle.idml"));
                let (item, _) = locate(&doc, &id).unwrap_or_else(|| panic!("no {id}"));
                let shape = |k: folio_model::geom::CornerKind| match k {
                    folio_model::geom::CornerKind::Rounded => "RoundedCorner", folio_model::geom::CornerKind::InverseRounded => "InverseRoundedCorner",
                    folio_model::geom::CornerKind::Inset => "InsetCorner", folio_model::geom::CornerKind::Bevel => "BevelCorner",
                    folio_model::geom::CornerKind::Fancy => "FancyCorner", folio_model::geom::CornerKind::None => "None",
                }.to_owned();
                let mut corners = item.stroke.corners.map(|c| folio_edit::Corner { shape: shape(c.kind), radius: c.radius });
                for (k, name) in ["topLeft", "topRight", "bottomLeft", "bottomRight"].iter().enumerate() {
                    if let Some(v) = p[format!("{name}CornerOption")].as_str() { corners[k].shape = ui_color(v.trim_start_matches("CornerOptions.")); }
                    if let Some(v) = p[format!("{name}CornerRadius")].as_f64() { corners[k].radius = v; }
                }
                Command::SetCorners { ids: vec![id], corners }
            } else if p.get("locked").is_some() || p.get("visible").is_some() {
                Command::SetItem { id: target(), locked: p["locked"].as_bool(), visible: p["visible"].as_bool() }
            } else {
                Command::SetPaint {
                    ids: vec![target()],
                    fill: p["fillColor"].as_str().map(swatch),
                    fill_tint: p["fillTint"].as_f64(),
                    stroke: p["strokeColor"].as_str().map(swatch),
                    stroke_tint: p["strokeTint"].as_f64(),
                    stroke_weight: p["strokeWeight"].as_f64(),
                }
            }
        }
        "parentNew" | "parentDuplicate" | "parentDelete" | "parentSet" => {
            // parents in designmap order, as doc.masterSpreads
            let dm = &parts["designmap.xml"];
            let dx = Index::scan(dm).unwrap();
            let masters: Vec<String> = (0..dx.nodes.len()).filter(|&i| &dm[dx.nodes[i].name.clone()] == "idPkg:MasterSpread")
                .map(|i| dx.attr(dm, i, "src").unwrap().trim_start_matches("MasterSpreads/MasterSpread_").trim_end_matches(".xml").to_owned()).collect();
            let m = || masters[step["master"].as_u64().unwrap() as usize].clone();
            let text = |k: &str| step[k].as_str().map(str::to_owned);
            match step["op"].as_str().unwrap() {
                "parentNew" => Command::NewParent { pages: step["pageCount"].as_u64().map(|v| v as u32), prefix: text("prefix"), name: text("name"), based_on: None },
                "parentDuplicate" => Command::DuplicateParent { id: m() },
                "parentDelete" => Command::DeleteParent { id: m() },
                _ => Command::ParentOptions { id: m(), prefix: text("prefix"), name: text("name"),
                    based_on: step["basedOn"].as_i64().map(|b| if b < 0 { "n".to_owned() } else { masters[b as usize].clone() }) },
            }
        }
        "applyObjectStyle" => {
            let s = step["style"].as_str().unwrap();
            let style = if s.starts_with('[') { format!("ObjectStyle/$ID/{s}") } else { format!("ObjectStyle/{s}") };
            if step["content"].as_bool().unwrap_or(false) {
                serde_json::from_value(serde_json::json!({"op":"applyObjectStyleToContent","ids":[target()],"style":style,"clear":step["clear"].as_bool().unwrap_or(false)})).unwrap()
            } else { Command::ApplyObjectStyle { ids: vec![target()], style, clear: step["clear"].as_bool().unwrap_or(false) } }
        }
        "clearObjectStyleOverrides" => Command::ClearObjectStyleOverrides { ids: vec![target()] },
        // InDesign composes lazily; Folio composes after every edit
        "recompose" => return Ok(0),
        "docSet" => {
            let light = &step["props"]["transparencyPreferences"];
            Command::SetGlobalLight { angle: light["globalLightAngle"].as_f64(), altitude: light["globalLightAltitude"].as_f64() }
        }
        "setGraphic" if step["props"].get("shearAngle").is_some() => {
            // the control panel's shear field with the content selected (the Image's own shearAngle; scripting: top-left reference point)
            Command::SetAngles { id: target(), rotation: None, shear: step["props"]["shearAngle"].as_f64(), anchor: folio_edit::Anchor::TopLeft, content: true, totals: true }
        }
        "shearByMany" => Command::Shear { ids: step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect(), by: step["by"].as_f64().unwrap(),
            anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center), adjust: Some(ADJUST.with(|a| a.get())) },
        "scaleByMany" => Command::ScaleMany { ids: step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect(), sx: step["sx"].as_f64().unwrap(), sy: step["sy"].as_f64().unwrap(),
            anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center), adjust: Some(ADJUST.with(|a| a.get())) },
        "setGraphic" => {
            ed.apply("step", &transparency(target(), folio_edit::BlendLevel::Graphic, &step["props"]["transparencySettings"])).map_err(|e| e.to_string())?;
            return Ok(1);
        }
        "layer" => Command::SetLayer {
            id: layer_id(parts, step["name"].as_str().unwrap()),
            visible: step["props"]["visible"].as_bool(),
            locked: step["props"]["locked"].as_bool(),
            printable: step["props"]["printable"].as_bool(),
            name: step["props"]["name"].as_str().map(str::to_owned),
            color: step["props"]["layerColor"].as_str().map(ui_color),
            show_guides: step["props"]["showGuides"].as_bool(),
            lock_guides: step["props"]["lockGuides"].as_bool(),
        },
        "swatchNew" => Command::NewSwatch { name: step["name"].as_str().unwrap().into(), spot: step["model"] == "spot", space: step["space"].as_str().unwrap().into(),
            value: step["value"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() },
        "swatchSet" => Command::SetSwatch { id: swatch(step["name"].as_str().unwrap()),
            value: step["props"]["value"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect()),
            space: step["props"]["space"].as_str().map(str::to_owned), spot: step["props"]["model"].as_str().map(|m| m == "spot"),
            rename: step["props"]["rename"].as_str().map(str::to_owned) },
        "swatchDelete" => Command::DeleteSwatch { id: swatch(step["name"].as_str().unwrap()), replace: swatch(step["replace"].as_str().unwrap()) },
        "mixedGroupDelete" => Command::DeleteSwatch { id: format!("MixedInkGroup/{}", step["name"].as_str().unwrap()), replace: "Swatch/None".into() },
        "mixedGroupRename" => Command::Color(folio_edit::color_commands::ColorCommand::RenameMixedInkGroup(folio_edit::color_commands::RenameMixedInkGroup {
            id: format!("MixedInkGroup/{}", step["name"].as_str().unwrap()), name: step["rename"].as_str().unwrap().into() })),
        "mixedSwatchDelete" => Command::DeleteSwatch { id: format!("MixedInk/{}", step["name"].as_str().unwrap()), replace: "Swatch/None".into() },
        "tintNew" => Command::NewTint { base: swatch(step["base"].as_str().unwrap()), tint: step["tint"].as_f64().unwrap() },
        "swatchDuplicate" => Command::DuplicateSwatch { id: swatch(step["name"].as_str().unwrap()) },
        "inkSet" => Command::SetInk { name: step["name"].as_str().unwrap().into(), ink_type: step["props"]["inkType"].as_str().map(|t| {
                let mut c = t.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }), convert_to_process: step["props"]["convertToProcess"].as_bool(), alias: step["props"]["alias"].as_str().map(str::to_owned) },
        "inkAllSpots" => Command::AllSpotsToProcess { on: step["on"].as_bool().unwrap() },
        "guideNew" => {
            let horizontal = step["orientation"] != "vertical";
            let (page, at) = guide_at(parts, step, horizontal);
            Command::NewGuide { page, orientation: if horizontal { folio_edit::GuideAxis::Horizontal } else { folio_edit::GuideAxis::Vertical }, at,
                spread: step["spread"].is_u64() || step["spread"] == true, layer: step["layer"].as_str().map(|l| layer_id(parts, l)) }
        }
        "guideMove" => {
            let (id, horizontal) = guide_now(ed, step);
            let (page, at) = guide_at(parts, step, horizontal);
            Command::MoveGuide { id, page, at }
        }
        "guideDelete" => Command::DeleteGuide { id: guide_now(ed, step).0 },
        "guideOptions" => serde_json::from_value(serde_json::json!({"op":"setGuideOptions", "ids":[guide_now(ed, step).0], "props":step["props"]})).unwrap(),
        "guideMoveBy" => serde_json::from_value(serde_json::json!({"op":"moveGuidesBy", "ids":[guide_now(ed, step).0], "delta":step["delta"]})).unwrap(),
        "layerNew" => Command::NewLayer { name: step["name"].as_str().map(str::to_owned), color: step["color"].as_str().map(ui_color), above: None },
        "layerMove" => Command::MoveLayer {
            id: layer_id(parts, step["name"].as_str().unwrap()),
            above: step["above"].as_str().map(|n| layer_id(parts, n)),
            below: step["below"].as_str().map(|n| layer_id(parts, n)),
        },
        "layerDelete" => Command::DeleteLayer { id: layer_id(parts, step["name"].as_str().unwrap()) },
        "margins" => {
            let pages = page_ids(parts);
            let p = &step["props"];
            let f = |k: &str| p[k].as_f64();
            Command::SetMargins { pages: vec![pages[step["page"].as_u64().unwrap() as usize].clone()], top: f("top"), bottom: f("bottom"), left: f("left"), right: f("right"),
                columns: p["columnCount"].as_u64().map(|c| c as u32), gutter: f("columnGutter") }
        }
        "createGuides" => Command::CreateGuides {
            page: page_ids(parts)[step["page"].as_u64().unwrap() as usize].clone(),
            rows: step["rows"].as_u64().unwrap() as u32, columns: step["columns"].as_u64().unwrap() as u32,
            row_gutter: step["rowGutter"].as_f64().unwrap_or(0.0), column_gutter: step["columnGutter"].as_f64().unwrap_or(0.0),
            fit_margins: step["fitMargins"].as_bool().unwrap_or(false), remove_existing: step["removeExisting"].as_bool().unwrap_or(false),
        },
        "layerDuplicate" => {
            let o = one(ed, Command::DuplicateLayer { id: layer_id(parts, step["name"].as_str().unwrap()) })?;
            return label_new(ed, o.created);
        }
        "layerMerge" => Command::MergeLayers {
            into: layer_id(parts, step["name"].as_str().unwrap()),
            ids: step["with"].as_array().unwrap().iter().map(|n| layer_id(parts, n.as_str().unwrap())).collect(),
        },
        // Object > Unlock All on Spread (menu action 11395): the spread of the first page, where the window is
        "menu" if step["id"].as_u64() == Some(11395) => Command::UnlockAll { page: Some(page_ids(parts)[0].clone()) },
        // Map the Layers panel action IDs in the compatibility cases to editor commands.
        "menu" if step["panel"] == "$ID/Layers" => {
            let layers = layer_ids(parts);
            let commands: Vec<Command> = match step["id"].as_u64() {
                Some(18441) => vec![Command::DeleteUnusedLayers],
                Some(11396) => vec![Command::UnlockAll { page: None }],
                Some(18437) => layers.iter().map(|id| Command::SetLayer { id: id.clone(), visible: Some(true), locked: None, printable: None, name: None, color: None, show_guides: None, lock_guides: None }).collect(),
                Some(18438) => layers.iter().map(|id| Command::SetLayer { id: id.clone(), visible: None, locked: Some(false), printable: None, name: None, color: None, show_guides: None, lock_guides: None }).collect(),
                other => panic!("layers panel menu {other:?}"),
            };
            ed.apply("step", &commands).map_err(|e| e.to_string())?;
            return Ok(1);
        }
        "layerActive" => Command::ActiveLayer { id: layer_id(parts, step["name"].as_str().unwrap()) },
        "toLayer" => Command::ToLayer {
            ids: match step["targets"].as_array() { Some(a) => a.iter().map(|t| ids[t.as_str().unwrap()].clone()).collect(), None => vec![target()] },
            layer: layer_id(parts, step["layer"].as_str().unwrap()),
        },
        "arrange" => Command::Arrange { id: target(), how: serde_json::from_value::<Arrange>(step["how"].clone()).unwrap() },
        "rotate" => Command::Rotate { ids: vec![target()], by: step["by"].as_f64().unwrap(), anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center) },
        "flip" => Command::Flip { ids: vec![target()], axis: serde_json::from_value::<Axis>(step["how"].clone()).unwrap(), anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center) },
        "rotatePoint" => Command::RotateAbout { ids: vec![target()], by: step["by"].as_f64().unwrap(), point: pair(&step["point"]) },
        "shearBy" => Command::Shear { ids: vec![target()], by: step["by"].as_f64().unwrap(), anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center), adjust: Some(ADJUST.with(|a| a.get())) },
        "viewPref" => Command::SetViewPref { key: step["name"].as_str().unwrap().into(), value: step["value"].clone() },
        // a scale about a point of the spread, kept in the transform (When Scaling: Adjust Scaling Percentage)
        "scalePoint" => {
            assert!(KEEP_SCALE.with(|d| d.get()), "scalePoint needs whenScaling = adjustScalingPercentage");
            let (sx, sy, p) = (step["sx"].as_f64().unwrap(), step["sy"].as_f64().unwrap(), pair(&step["point"]));
            Command::Transform { ids: vec![target()], matrix: [sx, 0.0, 0.0, sy, p[0] - sx * p[0], p[1] - sy * p[1]] }
        }
        "scaleBy" => Command::Scale { id: target(), sx: step["sx"].as_f64().unwrap(), sy: step["sy"].as_f64().unwrap(), anchor: serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center),
            adjust: Some(ADJUST.with(|a| a.get())) },
        "overrideParent" | "removeOverride" | "detachParent" => {
            let dm = &parts["designmap.xml"];
            let dx = Index::scan(dm).unwrap();
            let mut pages = Vec::new();
            for i in (0..dx.nodes.len()).filter(|&i| &dm[dx.nodes[i].name.clone()] == "idPkg:Spread") {
                let t = &parts[dx.attr(dm, i, "src").unwrap()];
                let ix = Index::scan(t).unwrap();
                pages.extend((0..ix.nodes.len()).filter(|&i| ix.local(t, i) == "Page").filter_map(|i| ix.attr(t, i, "Self").map(str::to_owned)));
            }
            let page = pages[step["page"].as_u64().unwrap() as usize].clone();
            let ids = vec![target()];
            match step["op"].as_str().unwrap() {
                "overrideParent" => {
                    let o = one(ed, Command::OverrideParentItems { page, ids })?;
                    return label_new(ed, o.created);
                }
                "removeOverride" => Command::RemoveLocalOverrides { page, ids },
                _ => Command::DetachFromParent { page, ids },
            }
        }
        "allowOverrides" => Command::AllowParentOverrides { ids: vec![target()], allow: step["allow"].as_bool().unwrap() },
        "makeCompound" => {
            let ids: Vec<String> = step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect();
            let o = one(ed, Command::MakeCompoundPath { ids })?;
            return label_new(ed, o.created);
        }
        "releaseCompound" => {
            let o = one(ed, Command::ReleaseCompoundPath { id: target() })?;
            let labels = step["names"].as_array().unwrap();
            // `names`: the label of each object the release leaves, the first being the compound path itself ("" leaves it)
            let mut set = Vec::new();
            for (id, l) in std::iter::once(&target()).chain(o.created.iter()).zip(labels) {
                if let Some(l) = l.as_str().filter(|l| !l.is_empty()) {
                    set.push(Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.into() });
                }
            }
            ed.apply("label", &set).map_err(|e| e.to_string())?;
            return Ok(2);
        }
        "group" => {
            let members = step["targets"].as_array().unwrap().iter().map(|t| ids[t.as_str().unwrap()].clone()).collect();
            let o = one(ed, Command::Group { ids: members })?;
            return label_new(ed, o.created);
        }
        "ungroup" => Command::Ungroup { id: target(), remember_layers: CLIP.with(|c| c.borrow().ungroup_remembers) },
        "duplicate" => {
            let o = one(ed, Command::Duplicate { ids: vec![target()], by: pair(&step["by"]) })?;
            return label_new(ed, o.created);
        }
        "select" | "view" | "activeLayer" | "clipboard" => {
            // a new selection starts a new Transform Sequence Again sequence
            if step["op"] == "select" {
                ed.selection_changed();
            }
            return window_step(ed, step, ids, &ed.parts().clone());
        }
        "copyFrom" => return copy_from(ed, step),
        // application preferences (InDesign's, so not part of the document): they go with the next commands
        "appSet" => {
            CLIP.with(|c| {
                let mut c = c.borrow_mut();
                if let Some(v) = step["props"]["generalPreferences"]["ungroupRemembersLayers"].as_bool() {
                    c.ungroup_remembers = Some(v);
                }
                if let Some(v) = step["props"]["clipboardPreferences"]["pasteRemembersLayers"].as_bool() {
                    c.paste_remembers = v;
                }
            });
            return Ok(0);
        }
        // a selection only: no undo step
        // Cell Styles / Table Styles: new, edit (rename) and delete, the DOM's names as the style's attributes
        "tableStyleDef" => {
            let kind = if step["kind"] == "cell" { folio_edit::TableStyleKind::Cell } else { folio_edit::TableStyleKind::Table };
            let (tag, none) = if step["kind"] == "cell" { ("CellStyle", "[None]") } else { ("TableStyle", "[No table style]") };
            let name = step["name"].as_str().unwrap_or_default();
            match step["do"].as_str().unwrap() {
                "create" => Command::CreateTableStyle { kind, name: name.into(), props: style_props(tag, &step["props"]), group: None },
                "duplicate" => Command::DuplicateTableStyle { kind, style: style_id(tag, none, name), name: None },
                "edit" => Command::EditTableStyle { kind, style: style_id(tag, none, name), rename: step["rename"].as_str().map(str::to_owned), props: style_props(tag, &step["props"]) },
                "delete" => Command::DeleteTableStyle { kind, style: style_id(tag, none, name), replacement: style_id(tag, none, step["replacement"].as_str().unwrap()) },
                other => panic!("style action {other}"),
            }
        }
        // Load Paragraph / Character / Object / All Text Styles: the styles and swatches of the other fixture (no path is kept)
        "importParents" => {
            let path = oracle().join("fixtures").join(format!("{}.idml",step["from"].as_str().unwrap()));
            serde_json::from_value(serde_json::json!({"op":"loadParentPages","parts":read_zip(&path)})).unwrap()
        }
        "importStyles" => {
            let path = oracle().join("fixtures").join(format!("{}.idml", step["from"].as_str().unwrap()));
            let parts: BTreeMap<String, String> = read_zip(&path).into_iter().filter(|(n, _)| matches!(n.as_str(), "Resources/Styles.xml" | "Resources/Graphic.xml")).collect();
            let wire = serde_json::json!({"op": "loadStyles", "kinds": [step["format"]], "parts": parts, "clash": step["clash"], "only": []});
            folio_edit::commands::oracle::command(&wire, None).unwrap().unwrap()
        }
        // Paragraph Styles / Character Styles panel menus: Duplicate Style, New Style Group, move a style into a group, a style made in one
        "textStyle" => {
            let text = |key: &str| step[key].as_str().unwrap_or_default();
            let kind = text("kind");
            match text("do") {
                "createStyle" => serde_json::from_value(serde_json::json!({"op": "styleCreate", "kind": kind, "name": text("name"), "group": text("group")})).unwrap(),
                other => {
                    let wire = match other {
                        "duplicate" => serde_json::json!({"op": "duplicateTextStyle", "kind": kind, "style": text("style"), "name": ""}),
                        "groupCreate" => serde_json::json!({"op": "createTextStyleGroup", "kind": kind, "name": text("name"), "parent": text("parent")}),
                        "move" => serde_json::json!({"op": "moveTextStyle", "kind": kind, "style": text("style"), "group": text("group")}),
                        "copyToGroup" => serde_json::json!({"op": "copyTextStyleToGroup", "kind": kind, "style": text("style"), "group": text("group")}),
                        // Rename / Delete Style Group (styles-c2): the group by its path of names
                        "groupRename" => serde_json::json!({"op": "renameTextStyleGroup", "kind": kind, "group": text("name"), "name": text("rename")}),
                        "groupDelete" => serde_json::json!({"op": "deleteTextStyleGroup", "kind": kind, "group": text("name")}),
                        other => panic!("text style action {other}"),
                    };
                    folio_edit::commands::oracle::command(&wire, None).unwrap().unwrap()
                }
            }
        }
        // Cell Styles / Table Styles: style groups (paths join the group names with ":")
        "tableStyleGroup" => {
            let kind = if step["kind"] == "cell" { folio_edit::TableStyleKind::Cell } else { folio_edit::TableStyleKind::Table };
            let (tag, none) = if step["kind"] == "cell" { ("CellStyle", "[None]") } else { ("TableStyle", "[No table style]") };
            let group_tag = format!("{tag}Group");
            let group_id = |path: &str| format!("{group_tag}/$ID/{}", path.replace(':', "%3a"));
            let text = |key: &str| step[key].as_str().unwrap_or_default();
            match text("do") {
                "create" => Command::CreateTableStyleGroup { kind, name: text("name").into(), parent: step["parent"].as_str().map(&group_id) },
                "rename" => Command::RenameTableStyleGroup { kind, group: group_id(text("name")), name: text("rename").into() },
                "delete" => Command::DeleteTableStyleGroup { kind, group: group_id(text("name")) },
                "move" => Command::MoveTableStyle { kind, style: style_id(tag, none, text("style")), group: Some(text("group")).filter(|g| !g.is_empty()).map(&group_id) },
                "createStyle" => Command::CreateTableStyle { kind, name: text("name").into(), props: style_props(tag, &step["props"]), group: Some(group_id(text("group"))) },
                other => panic!("style group action {other}"),
            }
        }
        "tableStyleRedefine" | "tableStyleSelection" => {
            let mut selection = step.clone();
            selection["do"] = serde_json::json!(if step["op"] == "tableStyleRedefine" { "styleRedefine" } else { "styleSelection" });
            table_step(ed, &selection, &target())?.unwrap()
        }
        "table" => match table_step(ed, step, &target())? {
            Some(c) => c,
            None => return Ok(0),
        },
        // Edit ▸ Duplicate (InDesign menu action 275): the selection copied by the offset Step and
        // Repeat last used (app ui/steprepeat.tsx); the copies become the selection
        // Transform panel menu ▸ Redefine Scaling as 100 %
        // Paragraph Styles / Character Styles panel menus ▸ Break Link to Style, on the text of `select` in the target's story
        "menu" if matches!(step["id"].as_u64(), Some(8500 | 8501)) => folio_edit::commands::oracle::command(&serde_json::json!({
            "op": "breakStyleLink", "kind": if step["id"] == 8500 { "paragraph" } else { "character" }, "start": step["select"]["start"], "end": step["select"]["end"],
        }), Some(&target())).unwrap().unwrap(),
        // Paragraph Styles panel menu ▸ Sort by Name: the panel's order only, the document does not change (tstyle-sort-paragraph)
        "menu" if step["action"] == "$ID/Sort by Name" => return Ok(0),
        "menu" if step["id"] == 118852 => {
            let sel = CLIP.with(|c| c.borrow().selected.clone());
            return one(ed, Command::RedefineScaling { ids: sel }).map(|_| 1);
        }
        // Object ▸ Pathfinder on the selection
        "menu" if matches!(step["id"].as_u64(), Some(99599..=99603)) => {
            let sel = CLIP.with(|c| c.borrow().selected.clone());
            let kind = match step["id"].as_u64().unwrap() {
                99600 => folio_edit::PathfinderOp::Add,
                99601 => folio_edit::PathfinderOp::Subtract,
                99599 => folio_edit::PathfinderOp::Intersect,
                99603 => folio_edit::PathfinderOp::ExcludeOverlap,
                _ => folio_edit::PathfinderOp::MinusBack,
            };
            let o = one(ed, folio_edit::commands::RegisteredCommand::Pathfinder { ids: sel, kind }.into())?;
            CLIP.with(|c| c.borrow_mut().selected = o.created.clone());
            let labels: Vec<Command> = o.created.iter().zip(step["labels"].as_array().into_iter().flatten())
                .map(|(id, l)| Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.as_str().unwrap().into() }).collect();
            if labels.is_empty() {
                return Ok(1);
            }
            ed.apply("label", &labels).map_err(|e| e.to_string())?;
            return Ok(2);
        }
        // Object ▸ Paths ▸ Join on the selection, in the order it was selected
        "menu" if step["id"] == 99621 => {
            let sel = CLIP.with(|c| c.borrow().selected.clone());
            let o = one(ed, folio_edit::commands::RegisteredCommand::JoinPaths { ids: sel }.into())?;
            CLIP.with(|c| c.borrow_mut().selected = o.created.clone());
            let labels: Vec<Command> = o.created.iter().zip(step["labels"].as_array().into_iter().flatten())
                .map(|(id, l)| Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.as_str().unwrap().into() }).collect();
            if labels.is_empty() {
                return Ok(1);
            }
            ed.apply("label", &labels).map_err(|e| e.to_string())?;
            return Ok(2);
        }
        "menu" if step["id"] == 275 => {
            let sel = CLIP.with(|c| c.borrow().selected.clone());
            let o = one(ed, Command::Duplicate { ids: sel, by: DUPLICATE_BY })?;
            CLIP.with(|c| c.borrow_mut().selected = o.created.clone());
            let labels: Vec<Command> = o.created.iter().zip(step["labels"].as_array().into_iter().flatten())
                .map(|(id, l)| Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.as_str().unwrap().into() }).collect();
            if labels.is_empty() {
                return Ok(1);
            }
            ed.apply("label", &labels).map_err(|e| e.to_string())?;
            return Ok(2);
        }
        // Object ▸ Transform (InDesign menu actions): the selection turned or mirrored about the
        // window's reference point
        "menu" if matches!(step["id"].as_u64(), Some(118811 | 118812 | 118813 | 118814 | 118846)) => {
            let ids = CLIP.with(|c| c.borrow().selected.clone());
            let anchor = serde_json::from_value::<Anchor>(step["anchor"].clone()).unwrap_or(Anchor::Center);
            match step["id"].as_u64().unwrap() {
                118811 => Command::Rotate { ids, by: 180.0, anchor },
                118812 => Command::Rotate { ids, by: -90.0, anchor },
                118813 => Command::Rotate { ids, by: 90.0, anchor },
                118814 => Command::Flip { ids, axis: Axis::Horizontal, anchor },
                _ => Command::Flip { ids, axis: Axis::Vertical, anchor },
            }
        }
        // Object Styles panel menu ▸ Break Link to Style, on the selection
        "menu" if step["id"].as_u64() == Some(113166) => {
            let ids = CLIP.with(|c| c.borrow().selected.clone());
            Command::ObjectStyleDefinition(serde_json::from_value(serde_json::json!({"op": "breakLinkToStyle", "ids": ids})).unwrap())
        }
        // Object Styles panel menu ▸ Sort by Name
        "menu" if step["id"].as_u64() == Some(113168) => Command::ObjectStyleDefinition(serde_json::from_value(serde_json::json!({"op": "sortObjectStyles"})).unwrap()),
        // Object Styles panel menu ▸ Default Text Frame Style (113202..4) / Default Graphic Frame Style (113278..80): [None], [Basic Graphics
        // Frame], [Basic Text Frame]
        "menu" if matches!(step["id"].as_u64(), Some(113202..=113204 | 113278..=113280)) => {
            let id = step["id"].as_u64().unwrap();
            let style = match id { 113202 | 113278 => "ObjectStyle/$ID/[None]", 113203 | 113279 => "ObjectStyle/$ID/[Normal Graphics Frame]", _ => "ObjectStyle/$ID/[Normal Text Frame]" };
            let kind = if id < 113278 { "text" } else { "graphic" };
            Command::ObjectStyleDefinition(serde_json::from_value(serde_json::json!({"op": "setDefaultFrameStyle", "kind": kind, "style": style})).unwrap())
        }
        // Object menu (InDesign menu actions) on the selection: Effects ▸ Clear Effects, Clear All
        // Transparency, Transform ▸ Clear Transformations, Content ▸ Graphic / Unassigned
        "menu" if matches!(step["id"].as_u64(), Some(67880 | 67897 | 118851 | 11295 | 11296 | 11297)) => {
            let ids = CLIP.with(|c| c.borrow().selected.clone());
            match step["id"].as_u64().unwrap() {
                67880 => Command::ClearEffects { ids },
                67897 => Command::ClearAllTransparency { ids },
                118851 => Command::ClearTransformations { ids },
                11295 => Command::SetContentType { ids, kind: folio_edit::ContentKind::Graphic },
                11296 => Command::SetContentType { ids, kind: folio_edit::ContentKind::Text },
                _ => Command::SetContentType { ids, kind: folio_edit::ContentKind::Unassigned },
            }
        }
        // Object ▸ Transform Again, Transform Sequence Again and the Individually variants
        "menu" if matches!(step["id"].as_u64(), Some(118805 | 118837 | 118842 | 118843)) => {
            let ids = CLIP.with(|c| c.borrow().selected.clone());
            let id = step["id"].as_u64().unwrap();
            Command::TransformAgain { ids, sequence: matches!(id, 118837 | 118843), individually: matches!(id, 118842 | 118843) }
        }
        // Object ▸ Select ▸ Previous / Next Object in Group: a selection only, no change
        // Swatches panel menu: Sort ▸ All Swatches By Name / By Color Values (selected ones need a panel selection)
        "menu" if matches!(step["id"].as_u64(), Some(16419 | 16421)) => Command::Color(folio_edit::color_commands::ColorCommand::SortSwatches(folio_edit::color_commands::SortSwatches {
            by: if step["id"] == 16419 { "name" } else { "value" }.into(), only: None })),
        "strokeStyleSave" => folio_edit::commands::RegisteredCommand::SaveStrokeStyle {style:serde_json::from_value(step["style"].clone()).unwrap()}.into(),
        "strokeStyleDelete" => folio_edit::commands::RegisteredCommand::DeleteStrokeStyle {id:step["id"].as_str().unwrap().into()}.into(),
        "trapPresetSave" => folio_edit::commands::RegisteredCommand::SaveTrapPreset {preset:serde_json::from_value(step["preset"].clone()).unwrap()}.into(),
        "trapPresetDuplicate" => folio_edit::commands::RegisteredCommand::DuplicateTrapPreset {id:step["id"].as_str().unwrap().into(),name:step["name"].as_str().unwrap_or_default().into()}.into(),
        "trapPresetDelete" => folio_edit::commands::RegisteredCommand::DeleteTrapPreset {id:step["id"].as_str().unwrap().into(),replacement:step["replacement"].as_str().unwrap().into()}.into(),
        "trapAssign" => {let all=page_ids(parts);folio_edit::commands::RegisteredCommand::AssignTrapPreset {pages:step["pages"].as_array().unwrap().iter().map(|n|all[n.as_u64().unwrap() as usize].clone()).collect(),preset:step["preset"].as_str().unwrap().into()}.into()}
        "customStroke" => Command::SetStroke {ids:vec![target()],change:folio_edit::StrokeChange{kind:Some(step["id"].as_str().unwrap().into()),..Default::default()}},
        "preflightSettings" => folio_edit::commands::RegisteredCommand::SetPreflightSettings {state:serde_json::from_value(step["state"].clone()).unwrap()}.into(),
        "preflightProfiles" => folio_edit::commands::RegisteredCommand::SetPreflightProfiles {state:serde_json::from_value(step["state"].clone()).unwrap()}.into(),
        "mixedGroupNew" => Command::Color(folio_edit::color_commands::ColorCommand::NewMixedInkGroup(folio_edit::color_commands::NewMixedInkGroup {
            name: step["name"].as_str().unwrap().into(),
            inks: step["inks"].as_array().unwrap().iter().map(|n| { let n = n.as_str().unwrap(); if n.starts_with("Process ") { format!("Ink/$ID/{n}") } else { format!("Ink/{n}") } }).collect(),
            initial: serde_json::from_value(step["initial"].clone()).unwrap(),
            repeat: serde_json::from_value(step["repeat"].clone()).unwrap(),
            increment: serde_json::from_value(step["increment"].clone()).unwrap() })),
        "mixedNew" => Command::Color(folio_edit::color_commands::ColorCommand::NewMixedInk(folio_edit::color_commands::NewMixedInk {
            name: step["name"].as_str().unwrap().into(),
            inks: step["inks"].as_array().unwrap().iter().map(|n| { let n = n.as_str().unwrap(); if n.starts_with("Process ") { format!("Ink/$ID/{n}") } else { format!("Ink/{n}") } }).collect(),
            percentages: step["percentages"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() })),
        "colorGroupNew" => Command::Color(folio_edit::color_commands::ColorCommand::NewColorGroup(folio_edit::color_commands::NewColorGroup {
            name: step["name"].as_str().unwrap().into(), swatches: step["swatches"].as_array().unwrap().iter().map(|n| swatch(n.as_str().unwrap())).collect() })),
        "menu" if matches!(step["id"].as_u64(), Some(11355 | 11356)) => return Ok(0),
        "menu" => menu_step(step)?,
        "convertShape" => Command::ConvertShape { ids: vec![target()], to: match step["to"].as_str().unwrap() {
            "CONVERT_TO_RECTANGLE" => folio_edit::Shape::Rectangle, "CONVERT_TO_ROUNDED_RECTANGLE" => folio_edit::Shape::RoundedRectangle,
            "CONVERT_TO_BEVELED_RECTANGLE" => folio_edit::Shape::BeveledRectangle, "CONVERT_TO_INVERSE_ROUNDED_RECTANGLE" => folio_edit::Shape::InverseRoundedRectangle,
            "CONVERT_TO_OVAL" => folio_edit::Shape::Ellipse, "CONVERT_TO_TRIANGLE" => folio_edit::Shape::Triangle, "CONVERT_TO_POLYGON" => folio_edit::Shape::Polygon,
            "CONVERT_TO_LINE" => folio_edit::Shape::Line, "CONVERT_TO_STRAIGHT_LINE" => folio_edit::Shape::StraightLine,
            "CONVERT_TO_OPEN_PATH" => folio_edit::Shape::OpenPath, "CONVERT_TO_CLOSED_PATH" => folio_edit::Shape::ClosedPath,
            other => panic!("convert to {other}") } },
        "reversePath" => Command::ReversePath { ids: vec![target()] },
        "content" => Command::MoveContent { id: target(), by: pair(&step["by"]) },
        "fit" => Command::Fit { id: target(), how: serde_json::from_value::<Fit>(step["how"].clone()).unwrap() },
        "primaryTextFrame" => Command::SetPrimaryTextFrame { id: target(), primary: step["primary"].as_bool().unwrap() },
        op => panic!("unknown op {op}"),
    };
    one(ed, c).map(|_| 1)
}

/// Edit ▸ Duplicate's offset: what the InDesign that captured basic-duplicate2 remembered from its
/// last Step and Repeat (a scripted duplicate does not change it: edit-duplicate-after-copy).
const DUPLICATE_BY: [f64; 2] = [50.74806481068566, 0.015769112353154924];

/// A window scenario's state: the objects selected, the page in view, where InDesign's pasted
/// object went ([top, left, bottom, right]).
#[derive(Default)]
struct Clipboard {
    selected: Vec<String>,
    page: usize,
    pasted: Value,
    /// Application preferences a scenario set (`appSet`): Ungroup Remembers Layers (none: InDesign's
    /// default, on) and Paste Remembers Layers (default off).
    ungroup_remembers: Option<bool>,
    paste_remembers: bool,
}

thread_local! {
    static CLIP: std::cell::RefCell<Clipboard> = std::cell::RefCell::new(Clipboard::default());
}

/// The run-scenarios window steps: select, view, activeLayer, clipboard.
fn window_step(ed: &mut Editor, step: &Value, ids: &HashMap<String, String>, parts: &BTreeMap<String, String>) -> Result<usize, String> {
    let label = |l: &Value| ids.get(l.as_str().unwrap()).cloned().ok_or_else(|| format!("no object labelled {l}"));
    match step["op"].as_str().unwrap() {
        "select" => {
            let sel = match step["targets"].as_array() {
                Some(a) => a.iter().map(label).collect::<Result<Vec<_>, _>>()?,
                None if step["target"].is_string() => vec![label(&step["target"])?],
                None => Vec::new(),
            };
            // selecting makes the selection's spread the active one (clip-text-in-place)
            if let Some(first) = sel.first() {
                let pages = page_ids(parts);
                let spread_page = parts.iter().filter(|(n, t)| n.starts_with("Spreads/") && t.contains(&format!(" Self=\"{first}\""))).find_map(|(_, t)| {
                    let ix = Index::scan(t).unwrap();
                    (0..ix.nodes.len()).find(|&k| ix.local(t, k) == "Page").and_then(|k| ix.attr(t, k, "Self")).and_then(|p| pages.iter().position(|q| q == p))
                });
                if let Some(page) = spread_page {
                    CLIP.with(|c| c.borrow_mut().page = page);
                }
            }
            CLIP.with(|c| c.borrow_mut().selected = sel);
            Ok(0)
        }
        "view" => {
            if let Some(p) = step["page"].as_u64() {
                CLIP.with(|c| c.borrow_mut().page = p as usize);
            }
            Ok(0)
        }
        "activeLayer" => {
            ed.apply("step", &[Command::ActiveLayer { id: layer_id(parts, step["name"].as_str().unwrap()) }]).map_err(|e| e.to_string())?;
            Ok(1)
        }
        "clipboard" => {
            let sel = CLIP.with(|c| c.borrow().selected.clone());
            let how = step["how"].as_str().unwrap();
            match how {
                "copy" => { ed.copy(&sel).map_err(|e| e.to_string())?; return Ok(0); }
                "cut" => {
                    ed.copy(&sel).map_err(|e| e.to_string())?;
                    ed.apply("step", &[Command::Delete { ids: sel }]).map_err(|e| e.to_string())?;
                    return Ok(1);
                }
                _ => {}
            }
            let pages = page_ids(parts);
            let (page, pasted) = CLIP.with(|c| { let c = c.borrow(); (c.page, c.pasted.clone()) });
            let command = match how {
                "pasteInto" => Command::Paste { page: None, at: None, layer: None, into: sel.first().cloned(), remember_layers: false },
                "pasteInPlace" => Command::Paste { page: Some(pages[page].clone()), at: None, layer: None, into: None, remember_layers: CLIP.with(|c| c.borrow().paste_remembers) },
                _ => {
                    let g = |k: usize| pasted[k].as_f64().unwrap();
                    Command::Paste { page: Some(pages[page].clone()), at: Some([(g(1) + g(3)) / 2.0, (g(0) + g(2)) / 2.0]), layer: None, into: None, remember_layers: CLIP.with(|c| c.borrow().paste_remembers) }
                }
            };
            let o = ed.apply("step", &[command]).map_err(|e| e.to_string())?;
            let labels: Vec<String> = match step["contentLabel"].as_str() {
                Some(l) => vec![l.to_owned()],
                None => step["labels"].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect()).unwrap_or_default(),
            };
            let commands: Vec<Command> = o.created.iter().zip(&labels).map(|(id, l)| Command::SetLabel { id: id.clone(), key: "oracle.id".into(), value: l.clone() }).collect();
            if commands.is_empty() {
                return Ok(1);
            }
            ed.apply("labels", &commands).map_err(|e| e.to_string())?;
            Ok(2)
        }
        op => Err(format!("unknown window op {op}")),
    }
}

/// What Folio's story text has for an InDesign `SpecialCharacters` value that a character's
/// `contents` reports (four letters, e.g. "SBlt"; oracle text-special, text-insert-forced-break):
/// the character itself, U+FFFC for a marker (`<?ACE n?>`), a return for a column, frame or
/// page break.
fn special_char(code: u32) -> Option<char> {
    let name: String = code.to_be_bytes().iter().map(|&b| b as char).collect();
    Some(match name.as_str() {
        "SFlb" => '\u{2028}', "SPdL" => '\u{200b}', "SBlt" => '\u{2022}', "SCrt" => '\u{a9}', "SLps" => '\u{2026}',
        "SPar" => '\u{b6}', "SRTm" => '\u{ae}', "SsnS" => '\u{a7}', "STmk" => '\u{2122}', "SEmD" => '\u{2014}',
        "SEnD" => '\u{2013}', "SDHp" => '\u{ad}', "SNbh" => '\u{2011}', "SDLq" => '\u{201c}', "SDRq" => '\u{201d}',
        "SSLq" => '\u{2018}', "SSRq" => '\u{2019}', "SDSq" => '"', "SSSq" => '\'', "SPnj" => '\u{200c}',
        "SEmS" => '\u{2003}', "SEnS" => '\u{2002}', "SNbS" => '\u{a0}', "Snnb" => '\u{202f}', "SHrS" => '\u{200a}',
        "SSiS" => '\u{2006}', "STnS" => '\u{2009}', "SQuS" => '\u{2005}', "SThS" => '\u{2004}', "SPnS" => '\u{2008}',
        "SFgS" => '\u{2007}', "SFlS" => '\u{2001}',
        "SApn" | "SNpn" | "SPpn" | "SsnM" | "SBRS" | "SIht" | "SRit" => '\u{fffc}',
        "SClB" | "SFrB" | "SPgB" | "SOpB" | "SEpB" => '\r',
        "SfnM" => '\u{4}',
        "SPtv" => '\u{18}', // a text variable (the cross-reference page number)
        _ => return None,
    })
}

/// The run-scenarios `copyFrom` step: Edit > Copy of labelled objects in another fixture
/// (opened in its own window), for the next paste in this document.
fn copy_from(ed: &mut Editor, step: &Value) -> Result<usize, String> {
    static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
    let path = oracle().join("fixtures").join(format!("{}.idml", step["fixture"].as_str().unwrap()));
    let mut other = Editor::new(read_zip(&path));
    let ids = labels(other.parts());
    let doc = read(other.parts(), &path);
    let layout = folio_text::layout_document(&doc, FONTS.get_or_init(|| folio_text::FontStore::discover(&[])));
    other.set_frame_starts(folio_text::DocumentFrames::new(&doc).frame_ids()
        .map(|id| ((*id).to_owned(), layout.frames.get(*id).and_then(|f| f.lines.first()).map(|l| l.start))).collect());
    let targets = step["targets"].as_array().unwrap().iter()
        .map(|t| ids.get(t.as_str().unwrap()).cloned().ok_or_else(|| format!("no object labelled {t} in the other document")))
        .collect::<Result<Vec<_>, _>>()?;
    other.copy(&targets).map_err(|e| e.to_string())?;
    ed.set_clipboard(other.clipboard());
    Ok(0)
}

/// The document's pages in order (their Self).
fn page_ids(parts: &BTreeMap<String, String>) -> Vec<String> {
    let dm = &parts["designmap.xml"];
    let dx = Index::scan(dm).unwrap();
    let mut pages = Vec::new();
    for i in (0..dx.nodes.len()).filter(|&i| &dm[dx.nodes[i].name.clone()] == "idPkg:Spread") {
        let t = &parts[dx.attr(dm, i, "src").unwrap()];
        let ix = Index::scan(t).unwrap();
        pages.extend((0..ix.nodes.len()).filter(|&k| ix.local(t, k) == "Page").filter_map(|k| ix.attr(t, k, "Self").map(str::to_owned)));
    }
    pages
}

thread_local! {
    /// Cells a scenario selected (table, first and last [column, row]).
    static SELECTED: std::cell::RefCell<Option<(String, [usize; 2], [usize; 2])>> = const { std::cell::RefCell::new(None) };
}

/// A run-scenarios `menu` step: a menu action on the selected cells (InDesign menu ids).
fn menu_step(step: &Value) -> Result<Command, String> {
    let (table, a, b) = SELECTED.with(|s| s.borrow().clone()).ok_or("a menu step without a selection")?;
    Ok(match step["id"].as_u64() {
        Some(66841) => Command::DistributeRows { table, first: a[1].min(b[1]), last: a[1].max(b[1]) },
        Some(66842) => Command::DistributeColumns { table, first: a[0].min(b[0]), last: a[0].max(b[0]) },
        // Convert Cell to Graphic Cell / Text Cell (the cell the cursor is in)
        Some(id @ (66869 | 66868)) => Command::SetCellType { table, cells: vec![a], graphic: id == 66869 },
        // Table Styles / Cell Styles panel menus: Break Link to Style (132107 table, 132129 cells), Clear Attributes Not Defined by Style (132179)
        Some(132107) => folio_edit::commands::RegisteredCommand::BreakTableStyleLink { table }.into(),
        Some(id @ (132129 | 132179)) => {
            let cells: Vec<[usize; 2]> = (a[1].min(b[1])..=a[1].max(b[1])).flat_map(|r| (a[0].min(b[0])..=a[0].max(b[0])).map(move |c| [c, r])).collect();
            if id == 132129 { folio_edit::commands::RegisteredCommand::BreakCellStyleLink { table, cells }.into() } else { folio_edit::commands::RegisteredCommand::ClearCellStyleAttributes { table, cells }.into() }
        }
        other => return Err(format!("menu action {other:?} is not mapped")),
    })
}

/// A run-scenarios `table` step (the frame's first table; cells `[row, column]`) as a command
/// (cells `[column, row]`, as InDesign names them).
/// A selection (`do: select`) gives no command: it is kept for a following `menu` step.
fn table_step(ed: &Editor, step: &Value, frame: &str) -> Result<Option<Command>, String> {
    let parts = ed.parts();
    let story = parts.values().find_map(|t| {
        let ix = Index::scan(t).ok()?;
        let i = ix.by_self(t, frame)?;
        ix.attr(t, i, "ParentStory").map(str::to_owned)
    }).ok_or("no story")?;
    let table = || -> String {
        let t = &parts[&format!("Stories/Story_{story}.xml")];
        let ix = Index::scan(t).unwrap();
        (0..ix.nodes.len()).find(|&i| ix.local(t, i) == "Table").and_then(|i| ix.attr(t, i, "Self")).unwrap().to_owned()
    };
    let n = |k: &str| step[k].as_u64().map(|v| v as usize);
    let cell = |v: &Value| [v[1].as_u64().unwrap() as usize, v[0].as_u64().unwrap() as usize];
    // a cell's Self by its place now (cell text is edited as a text of its own)
    let cell_id = |v: &Value| -> String {
        let [c, r] = cell(v);
        let t = &parts[&format!("Stories/Story_{story}.xml")];
        let ix = Index::scan(t).unwrap();
        let name = format!("{c}:{r}");
        (0..ix.nodes.len()).find(|&k| ix.local(t, k) == "Cell" && ix.attr(t, k, "Name") == Some(name.as_str()))
            .and_then(|k| ix.attr(t, k, "Self")).unwrap().to_owned()
    };
    let props = &step["props"];
    let enumerated = |v: &Value| v.as_str().map(|e| ui_color(e.split_once('.').map_or(e, |(_, e)| e)));
    Ok(Some(match step["do"].as_str().unwrap() {
        "styleSelection" => serde_json::from_value(serde_json::json!({"op":"createTableStyleFromSelection", "kind":step["kind"], "table":table(), "cell":step["cell"], "name":step["name"], "group":""})).unwrap(),
        "styleRedefine" => serde_json::from_value(serde_json::json!({"op":"redefineTableStyle", "table":table()})).unwrap(),
        "insert" => Command::InsertTable { story, at: n("at"), rows: n("rows").unwrap() as u32, columns: n("columns").unwrap() as u32,
            header: n("header").unwrap_or(0) as u32, footer: n("footer").unwrap_or(0) as u32 },
        "convert" => Command::ConvertToTable { story, start: 0, end: None },
        "addRows" => Command::InsertRows { table: table(), at: n("at").unwrap(), count: n("count").unwrap_or(1) as u32, before: step["where"] == "before" },
        "addColumns" => Command::InsertColumns { table: table(), at: n("at").unwrap(), count: n("count").unwrap_or(1) as u32, before: step["where"] == "before" },
        "pasteRows" => Command::PasteRows { table: table(), source: None, from: n("from").unwrap(), to: n("to").unwrap(), at: n("at").unwrap(), before: step["where"] == "before" },
        "pasteColumns" => Command::PasteColumns { table: table(), source: None, from: n("from").unwrap(), to: n("to").unwrap(), at: n("at").unwrap(), before: step["where"] == "before" },
        "deleteRows" => Command::DeleteRows { table: table(), at: n("at").unwrap(), count: n("count").unwrap_or(1) as u32 },
        "deleteColumns" => Command::DeleteColumns { table: table(), at: n("at").unwrap(), count: n("count").unwrap_or(1) as u32 },
        "merge" => Command::MergeCells { table: table(), from: cell(&step["from"]), to: cell(&step["to"]) },
        "unmerge" => Command::UnmergeCell { table: table(), cell: cell(&step["cell"]) },
        "split" => Command::SplitCell { table: table(), cell: cell(&step["cell"]), vertical: step["direction"] == "vertical" },
        // Start Row (DOM `startRow`: StartParagraph.NEXT_ODD_PAGE …) → Cell Options ▸ Keep Options
        "rowProps" if props["startRow"].is_string() || props["keepWithNextRow"].is_boolean() => Command::Registered(folio_edit::commands::RegisteredCommand::SetRowKeeps {
            table: table(),
            rows: vec![n("at").unwrap()],
            keep_with_next: props["keepWithNextRow"].as_bool(),
            start: props["startRow"].as_str().map(|s| {
                let k = s.rsplit('.').next().unwrap_or(s);
                k.split('_').map(|w| w[..1].to_owned() + &w[1..].to_ascii_lowercase()).collect::<String>()
            }),
        }),
        "rowProps" => Command::SetRows { table: table(), rows: vec![n("at").unwrap()], height: props["height"].as_f64(), auto_grow: props["autoGrow"].as_bool() },
        "columnProps" => Command::SetColumns { table: table(), columns: vec![n("at").unwrap()], width: props["width"].as_f64().unwrap() },
        "cellProps" if props["cellType"].is_string() => {
            Command::SetCellType { table: table(), cells: vec![cell(&step["cell"])], graphic: props["cellType"].as_str().unwrap().ends_with("GRAPHIC_TYPE_CELL") }
        }
        "cellProps" => {
            let mut change = folio_edit::CellChange {
                fill: props["fillColor"].as_str().map(swatch),
                fill_tint: props["fillTint"].as_f64(),
                vertical: enumerated(&props["verticalJustification"]),
                ..Default::default()
            };
            change.clip = props["clipContentToCell"].as_bool();
            change.rotation = props["rotationAngle"].as_u64().map(|v| v as u16);
            change.writing_direction = props["writingDirection"].as_str().map(|v| v == "HorizontalOrVertical.HORIZONTAL");
            if props.as_object().is_some_and(|o| o.keys().any(|k| k.contains("DiagonalLine") || k.contains("diagonalLine"))) {
                change.diagonal = Some(folio_edit::DiagonalLines {
                    top_left: props["topLeftDiagonalLine"].as_bool(),
                    top_right: props["topRightDiagonalLine"].as_bool(),
                    in_front: props["diagonalLineInFront"].as_bool(),
                    weight: props["diagonalLineStrokeWeight"].as_f64(),
                    stroke_type: props["diagonalLineStrokeType"].as_str().map(|n| format!("StrokeStyle/$ID/{}", if n == "Dotted" { "Canned Dotted" } else { n })),
                    color: props["diagonalLineStrokeColor"].as_str().map(swatch),
                    tint: props["diagonalLineStrokeTint"].as_f64(),
                });
            }
            if props["topInset"].is_number() {
                let g = |k: &str| props[k].as_f64().unwrap_or(4.0);
                change.insets = Some([g("topInset"), g("leftInset"), g("bottomInset"), g("rightInset")]);
            }
            for side in ["top", "left", "bottom", "right"] {
                let (w, c) = (&props[format!("{side}EdgeStrokeWeight")], &props[format!("{side}EdgeStrokeColor")]);
                if w.is_number() || c.is_string() {
                    change.strokes.insert(side[..1].to_uppercase() + &side[1..], folio_edit::EdgeStroke { weight: w.as_f64(), color: c.as_str().map(swatch) });
                }
            }
            let cells = step["cells"].as_array().map(|cs| cs.iter().map(cell).collect()).unwrap_or_else(|| vec![cell(&step["cell"])]);
            Command::SetCells { table: table(), cells, change }
        }
        "tableProps" if props["footerRowCount"].is_u64() => Command::SetFooterRows { table: table(), footer: props["footerRowCount"].as_u64().unwrap() as usize },
        "tableProps" if props["headerRowCount"].is_u64() => Command::SetHeaderRows { table: table(), header: props["headerRowCount"].as_u64().unwrap() as usize },
        // Table Options > Row Strokes / Column Strokes / Fills: the DOM's table properties (camelCase) as the table's attributes
        "tableProps" => Command::SetTableOptions { table: table(), options: table_options(props) },
        "toText" => Command::ConvertTableToText { table: table() },
        "select" | "cursor" => {
            // the cells a following menu step works on
            let (from, to) = if step["do"] == "cursor" { (&step["cell"], &step["cell"]) } else { (&step["from"], &step["to"]) };
            SELECTED.with(|s| *s.borrow_mut() = Some((table(), cell(from), cell(to))));
            return Ok(None);
        }
        "rowType" => Command::ConvertRows { table: table(), at: n("at").unwrap(), count: n("count").unwrap_or(1),
            to: match step["type"].as_str().unwrap() { "HEADER_ROW" => folio_edit::RowKind::Header, "FOOTER_ROW" => folio_edit::RowKind::Footer, _ => folio_edit::RowKind::Body } },
        "applyCellStyle" | "clearCellOverrides" => {
            let cells: Vec<[usize; 2]> = step["cells"].as_array().unwrap().iter().map(cell).collect();
            if step["do"] == "clearCellOverrides" {
                Command::ClearCellOverrides { table: table(), cells }
            } else {
                Command::ApplyCellStyle { table: table(), cells, style: style_id("CellStyle", "[None]", step["style"].as_str().unwrap()) }
            }
        }
        "applyTableStyle" => Command::ApplyTableStyle { table: table(), style: style_id("TableStyle", "[No table style]", step["style"].as_str().unwrap()) },
        "clearTableOverrides" => Command::ClearTableOverrides { table: table() },
        "deleteTable" => Command::DeleteTable { table: table() },
        "cellText" => Command::InsertText { story: cell_id(&step["cell"]), at: n("at").unwrap(), text: step["text"].as_str().unwrap().into() },
        "cellDelete" => Command::DeleteText { story: cell_id(&step["cell"]), start: n("start").unwrap(), end: n("end").unwrap() },
        "cellFormat" => Command::TextFormat { story: cell_id(&step["cell"]), start: n("start").unwrap(), end: n("end").unwrap(),
            props: serde_json::from_value(props.clone()).unwrap() },
        d => return Err(format!("unknown table action {d}")),
    }))
}

// ---------------------------------------------------------------------------------------
// the model

/// The server's table pass (server/src/edit.rs `row_heights`): the auto-grow rows of the
/// tables in the stories the step changed take their composed heights, in the step's undo step.
fn row_heights(ed: &mut Editor, before: &BTreeMap<String, String>, at: &Path) -> Result<(), String> {
    let stories: Vec<String> = ed.parts().iter()
        .filter(|(n, t)| before.get(*n) != Some(t) && t.contains("<Table "))
        .filter_map(|(n, _)| n.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned))
        .collect();
    if stories.is_empty() {
        return Ok(());
    }
    static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
    let mut doc = read(ed.parts(), at);
    for id in &stories {
        if let Some(s) = doc.stories.get_mut(id) {
            s.saved = folio_model::SavedComposition::None;
        }
    }
    let frames = folio_text::DocumentFrames::new(&doc);
    let fonts = FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
    let commands: Vec<Command> = stories.iter()
        .filter_map(|id| folio_text::layout_story(&doc, &frames, id, fonts))
        .flat_map(|l| l.table_rows)
        .map(|(table, heights)| Command::SetRowHeights { table, heights })
        .collect();
    if commands.is_empty() {
        return Ok(());
    }
    ed.amend(&commands).map(|_| ()).map_err(|e| e.to_string())
}

/// The server's Auto-Size pass (server/src/edit.rs `auto_size`): the auto-sized frames ending
/// the stories the step composed again take the size of their text, in the step's undo step.
fn auto_size(ed: &mut Editor, before: &BTreeMap<String, String>, at: &Path) -> Result<(), String> {
    let on = |t: &str| ["AutoSizingType=\"HeightOnly", "AutoSizingType=\"WidthOnly", "AutoSizingType=\"HeightAndWidth"].iter().any(|k| t.contains(k));
    if !ed.parts().iter().any(|(n, t)| (n.starts_with("Spreads/") || n == "Resources/Styles.xml") && on(t)) {
        return Ok(());
    }
    let changed = ed.parts().iter().filter(|(n, t)| before.get(*n) != Some(t)).filter_map(|(n, _)| n.strip_prefix("Stories/Story_").and_then(|s| s.strip_suffix(".xml")).map(str::to_owned));
    let stories: BTreeSet<String> = ed.recompose().iter().cloned().chain(changed).collect();
    if stories.is_empty() || before == ed.parts() {
        return Ok(());
    }
    static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
    let mut doc = read(ed.parts(), at);
    for id in &stories {
        if let Some(s) = doc.stories.get_mut(id) {
            s.saved = folio_model::SavedComposition::None;
        }
    }
    let ids: Vec<&str> = stories.iter().map(String::as_str).collect();
    let fits = folio_text::fit_frames(&doc, FONTS.get_or_init(|| folio_text::FontStore::discover(&[])), &ids);
    if fits.is_empty() {
        return Ok(());
    }
    let commands: Vec<Command> = fits.into_iter().map(|f| Command::FitFrame { id: f.frame, size: [f.width, f.height], point: [f.point.0, f.point.1] }).collect();
    ed.amend(&commands).map(|_| ()).map_err(|e| e.to_string())
}

/// The server's text variable pass (server/src/edit.rs `variable_results`): the instances of text variables on document pages carry
/// the value they show, in the step's undo step.
fn variable_results(ed: &mut Editor, before: &BTreeMap<String, String>, at: &Path, edited: bool) -> Result<(), String> {
    let has = ed.parts().iter().any(|(n, t)| n.starts_with("Stories/") && before.get(n) != Some(t) && t.contains("<TextVariableInstance "));
    if !has && !edited {
        return Ok(());
    }
    static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(|| folio_text::FontStore::discover(&[]));
    let doc = read(ed.parts(), at);
    let layout = folio_text::layout_document(&doc, fonts);
    let results = folio_text::variable_results(&doc, fonts, &layout);
    if results.is_empty() {
        return Ok(());
    }
    ed.amend(&[Command::SetVariableResults { results }]).map(|_| ()).map_err(|e| e.to_string())
}

fn read(parts: &BTreeMap<String, String>, at: &Path) -> Document {
    let mut doc = folio_idml::read_parts(parts, at, &folio_idml::ReaderData::default()).unwrap();
    // what the server sets for the File Name text variable (server/src/textvars.rs)
    doc.facts.file_name = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if at.file_stem().is_some_and(|n| n == "pdfplace") {
        let cache = oracle().join("../../artifacts/pdfplace-20261022/oracle-preview");
        folio_links::pdfplace::previews::attach(&mut doc, &cache, &|g| {
            let name = g.link_uri.as_ref()?.rsplit('/').next()?;
            Some(oracle().join("fixtures/Links").join(name))
        }).expect("new PDF placement has a renderable source preview");
    }
    doc
}

fn find<'d>(items: &'d [PageItem], id: &str, parent: folio_model::Affine, out: &mut Option<(&'d PageItem, folio_model::Affine)>) {
    for it in items {
        let t = parent.then_inner(&it.transform);
        if it.id == id {
            *out = Some((it, parent));
            return;
        }
        if let folio_model::ItemKind::Group { children } = &it.kind {
            find(children, id, t, out);
        }
        // a frame's pasted content
        find(&it.clipped, id, t, out);
    }
}

fn locate<'d>(doc: &'d Document, id: &str) -> Option<(&'d PageItem, folio_model::Affine)> {
    let mut out = None;
    for s in doc.spreads.iter().chain(&doc.master_spreads) {
        find(&s.items, id, folio_model::Affine::default(), &mut out);
        if out.is_some() {
            break;
        }
    }
    out
}

/// Path bounds in spread coordinates (a group: its members').
fn bounds(it: &PageItem, parent: &folio_model::Affine) -> Option<folio_model::Rect> {
    let t = parent.then_inner(&it.transform);
    if let folio_model::ItemKind::Group { children } = &it.kind {
        return children.iter().filter_map(|c| bounds(c, &t)).reduce(|a, b| a.union(&b));
    }
    // the curve's bounds through the transform, as InDesign's geometricBounds
    let pt = |p: folio_model::Point| { let q = t.apply(p); (q.x, q.y) };
    let mapped: Vec<(Vec<folio_edit::geom::PathPoint>, bool)> = it
        .paths
        .iter()
        .map(|sp| (sp.points.iter().map(|p| folio_edit::geom::PathPoint { anchor: pt(p.anchor), left: pt(p.left), right: pt(p.right) }).collect(), !sp.open))
        .collect();
    folio_edit::geom::path_bounds(&mapped).map(|b| folio_model::Rect { left: b[0], top: b[1], right: b[2], bottom: b[3] })
}

fn model_checks(name: &str, before: &Document, after: &Document, ids: &HashMap<String, String>, indesign: &Value) -> Vec<String> {
    fn composed(doc:&Document)->Document {
        let mut result=doc.clone();
        if !doc.stories.values().flat_map(|s|&s.paragraphs).flat_map(|p|&p.runs).any(|r|!r.inline.is_empty()) {return result;}
        static FONTS:std::sync::OnceLock<folio_text::FontStore>=std::sync::OnceLock::new();
        let layout=folio_text::layout_document(doc,FONTS.get_or_init(||folio_text::FontStore::discover(&[])));
        let anchors=folio_model::layout::anchored_items(doc,&layout);
        fn add(items:&mut [PageItem],anchors:&BTreeMap<String,Vec<PageItem>>) {
            for item in items {if let folio_model::ItemKind::Group{children}=&mut item.kind {add(children,anchors);}
                add(&mut item.clipped,anchors);if let Some(children)=anchors.get(&item.id){item.clipped.extend(children.clone());}}
        }
        for spread in result.spreads.iter_mut().chain(&mut result.master_spreads){add(&mut spread.items,&anchors);}result
    }
    let (before,after)=(composed(before),composed(after));let (before,after)=(&before,&after);
    let origin = |d: &Document, id: &str| {
        for s in d.spreads.iter().chain(&d.master_spreads) {
            let mut found = None;
            find(&s.items, id, folio_model::Affine::IDENTITY, &mut found);
            if found.is_some() {
                return s.pages.iter().map(|p| p.transform.apply_rect(&p.bounds))
                    .reduce(|a,b| a.union(&b)).map(|r| [r.left,r.top]).unwrap_or([0.0,0.0]);
            }
        }
        [0.0,0.0]
    };
    // geometricBounds count from the ruler origin: when a scenario changes the origin (Preferences > Units & Increments), InDesign's
    // numbers move with it while the objects do not, so only the IDML is compared (basic-units-prefs)
    // likewise a spread that View > Rotate Spread turned: InDesign counts the bounds of its objects in the turned spread's own
    // coordinates while the objects stay where they are (spreadrotate), so only the IDML is compared
    let spread_turned = before.spreads.iter().zip(&after.spreads).any(|(x, y)| {
        let (p, q) = (x.transform, y.transform);
        [p.a - q.a, p.b - q.b, p.c - q.c, p.d - q.d].iter().any(|d| d.abs() > 1e-9)
    });
    let ruler_moved = before.view.ruler_origin != after.view.ruler_origin || spread_turned;
    let mut out = Vec::new();
    for (label, id) in ids {
        let (ib, ia) = (&indesign["before"][label], &indesign["after"][label]);
        let fa = locate(after, id);
        match (ia.is_null(), fa) {
            (true, Some(_)) => out.push(format!("{label}: InDesign removed it, Folio kept it")),
            (false, None) => out.push(format!("{label}: Folio lost it")),
            _ => {}
        }
        let (Some((a, pa)), Some((b, pb))) = (fa, locate(before, id)) else { continue };
        if ib.is_null() || ia.is_null() {
            continue;
        }
        if let (false, Some(ra), Some(rb)) = (ruler_moved, bounds(a, &pa), bounds(b, &pb)) {
            let g = |v: &Value, k: usize| v["geometricBounds"][k].as_f64().unwrap();
            // InDesign: [top, left, bottom, right]
            let want = [g(ia, 1) - g(ib, 1), g(ia, 0) - g(ib, 0), g(ia, 3) - g(ib, 3), g(ia, 2) - g(ib, 2)];
            // geometricBounds uses the spread ruler origin. Reparenting from the first
            // right-hand page to a facing spread changes that origin (spread-move-*).
            let (oa, ob) = (origin(after, id), origin(before, id));
            let got = [ra.left - rb.left - oa[0] + ob[0], ra.top - rb.top - oa[1] + ob[1],
                ra.right - rb.right - oa[0] + ob[0], ra.bottom - rb.bottom - oa[1] + ob[1]];
            if want.iter().zip(&got).any(|(w, g)| (w - g).abs() > 1e-3) {
                out.push(format!("{label}: bounds moved {got:?}, InDesign {want:?}"));
            }
        }
        if ia["locked"].as_bool().is_some_and(|l| l != a.locked) {
            out.push(format!("{label}: locked {} vs InDesign {}", a.locked, ia["locked"]));
        }
        if ia["visible"].as_bool().is_some_and(|v| v != a.visible) {
            out.push(format!("{label}: visible {} vs InDesign {}", a.visible, ia["visible"]));
        }
    }
    let _ = name;
    out
}

/// The document with the results of its clock and file name variables reduced to their shape (see `clock_shape`).
fn clock_shaped(d: &Document) -> Document {
    let mut d = d.clone();
    for s in d.stories.values_mut() {
        for n in s.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()).filter_map(|r| r.named.as_mut()) {
            n.result = clock_shape(&n.id, &n.result);
        }
    }
    d
}

fn model_equal(a: &Document, b: &Document, new_a: &BTreeMap<String, String>, new_b: &BTreeMap<String, String>) -> Vec<String> {
    let (a, b) = (&clock_shaped(a), &clock_shaped(b));
    let mut out = Vec::new();
    let flat = |d: &Document, new: &BTreeMap<String, String>| {
        let mut m = BTreeMap::new();
        fn walk(items: &[PageItem], m: &mut BTreeMap<String, String>, new: &BTreeMap<String, String>) {
            for it in items {
                let mut it2 = it.clone();
                if let folio_model::ItemKind::Group { children } = &mut it2.kind {
                    children.clear();
                }
                // where a link points differs by machine (the capture relinks): not compared
                let mut v = serde_json::to_value(&it2).unwrap();
                strip(&mut v, &["link_uri", "link", "preview"]);
                // new objects: each program's own ids, paired by place
                let text = new.iter().fold(v.to_string(), |acc, (old, k)| acc.replace(&format!("\"{old}\""), &format!("\"{k}\"")));
                m.insert(new.get(&it.id).cloned().unwrap_or_else(|| it.id.clone()), text);
                if let folio_model::ItemKind::Group { children } = &it.kind {
                    walk(children, m, new);
                }
            }
        }
        for s in d.spreads.iter().chain(&d.master_spreads) {
            walk(&s.items, &mut m, new);
        }
        m
    };
    let (fa, fb) = (flat(a, new_a), flat(b, new_b));
    for k in fa.keys().chain(fb.keys()).collect::<BTreeSet<_>>() {
        match (fa.get(k), fb.get(k)) {
            (Some(x), Some(y)) if numbers_close(x, y) => {}
            (x, y) => out.push(format!("model item {k}: Folio {} / InDesign {}", short(x), short(y))),
        }
    }
    // new layers carry the paired ids, like new objects (InDesign's numbering cannot match)
    let layers = |d: &Document, map: &BTreeMap<String, String>| d.layers.iter()
        .map(|l| (map.get(&l.id).unwrap_or(&l.id).clone(), l.name.clone(), l.visible, l.locked, l.printable)).collect::<Vec<_>>();
    if layers(a, new_a) != layers(b, new_b) {
        out.push(format!("layers: Folio {:?} / InDesign {:?}", layers(a, new_a), layers(b, new_b)));
    }
    let stories = |d: &Document, map: &BTreeMap<String, String>| d.stories.iter().map(|(id, s)| {
        let text = map.iter().fold(serde_json::to_string(s).unwrap(), |acc, (old, k)| acc
            .replace(&format!("\"{old}\""), &format!("\"{k}\""))
            .replace(&format!("\"{old}/footnote/"), &format!("\"{k}/footnote/")));
        (map.get(id).unwrap_or(id).clone(), mask_embedded_names(&text))
    }).collect::<BTreeMap<_, _>>();
    let (sa, sb) = (stories(a, new_a), stories(b, new_b));
    if sa.keys().ne(sb.keys()) || sa.iter().any(|(key, value)| !numbers_close(value, &sb[key])) {
        out.push("story contents or styles differ".into());
        // FOLIO_ORACLE_DEBUG=1: where each story's model first differs
        if std::env::var("FOLIO_ORACLE_DEBUG").is_ok() {
            for (k, x) in &sa {
                let Some(y) = sb.get(k).filter(|y| *y != x) else { continue };
                let p = x.bytes().zip(y.bytes()).take_while(|(a, b)| a == b).count();
                let around = |s: &str| s.get(p.saturating_sub(200)..(p + 300).min(s.len())).unwrap_or_default().to_owned();
                eprintln!("story {k} at {p}: Folio {} / InDesign {}", around(x), around(y));
            }
        }
    }
    for (id, story) in &a.stories {
        if let Some(other) = b.stories.get(id) {
            let normalized = |p: &Vec<folio_model::Paragraph>, map: &BTreeMap<String, String>| {
                neutral_fixture_links(&map.iter().fold(serde_json::to_string(p).unwrap(), |s, (old, new)| s.replace(&format!("\"{old}\""), &format!("\"{new}\""))))
            };
            if !numbers_close(&normalized(&story.paragraphs, new_a), &normalized(&other.paragraphs, new_b)) {
                out.push(format!("story {id}: text/paragraphs/character formatting differ"));
            }
        }
    }
    out
}

/// The number in the name InDesign gives an embedded picture's link (`file:Image483.PNG`, the decimal of the graphic's id) follows its own
/// object numbering; the link's address is not compared anywhere else either (VOLATILE).
fn mask_embedded_names(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("file:Image") {
        out.push_str(&rest[..i + "file:Image".len()]);
        rest = &rest[i + "file:Image".len()..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            out.push('#');
        }
        rest = &rest[digits..];
    }
    out.push_str(rest);
    neutral_fixture_links(&out)
}

/// A link InDesign made to a file of the fixtures' Links folder carries this computer's path; the fixtures name it `file:/oracle/Links/`.
fn neutral_fixture_links(s: &str) -> String {
    const DIR: &str = "fixtures/Links/";
    let mut out = s.to_owned();
    while let Some(i) = out.find(DIR) {
        let Some(from) = out[..i].rfind("file:").filter(|f| !out[*f..i].contains('"')) else { break };
        out.replace_range(from..i + DIR.len(), "file:/oracle/Links/");
    }
    out
}

/// Document labels only Folio keeps (`folio.contentLinkEdits`: the state of a placed object when its link was made, for `edited`)
/// are not what InDesign writes: the comparison with InDesign's IDML leaves them out.
fn without_folio_only_labels(mut parts: BTreeMap<String, String>) -> BTreeMap<String, String> {
    if let Some(dm) = parts.get_mut("designmap.xml") {
        while let Some(at) = dm.find("<KeyValuePair Key=\"folio.contentLinkEdits\"") {
            let end = at + dm[at..].find("/>").map_or(0, |k| k + 2);
            let mut start = at;
            while start > 0 && dm.as_bytes()[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            dm.replace_range(start..end, "");
        }
        // a Properties/Label that only held that pair
        for empty in ["<Properties><Label></Label></Properties>"] {
            while let Some(at) = dm.find(empty) {
                let mut start = at;
                while start > 0 && dm.as_bytes()[start - 1].is_ascii_whitespace() {
                    start -= 1;
                }
                dm.replace_range(start..at + empty.len(), "");
            }
        }
    }
    parts
}

fn strip(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(m) => {
            for k in keys {
                m.remove(*k);
            }
            m.values_mut().for_each(|x| strip(x, keys));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| strip(x, keys)),
        _ => {}
    }
}

fn short(s: Option<&String>) -> String {
    s.map_or("none".into(), |s| s.chars().take(std::env::var("FOLIO_ORACLE_WIDTH").ok().and_then(|v| v.parse().ok()).unwrap_or(160)).collect())
}

/// JSON texts equal but for float noise.
fn numbers_close(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (x, y): (Value, Value) = (serde_json::from_str(a).unwrap(), serde_json::from_str(b).unwrap());
    fn close(x: &Value, y: &Value) -> bool {
        match (x, y) {
            (Value::Number(p), Value::Number(q)) => {
                let (p, q) = (p.as_f64().unwrap(), q.as_f64().unwrap());
                (p - q).abs() <= 1e-6 * p.abs().max(1.0)
            }
            (Value::Array(p), Value::Array(q)) => p.len() == q.len() && p.iter().zip(q).all(|(a, b)| close(a, b)),
            (Value::Object(p), Value::Object(q)) => p.len() == q.len() && p.iter().all(|(k, v)| q.get(k).is_some_and(|w| close(v, w))),
            _ => x == y,
        }
    }
    close(&x, &y)
}

// ---------------------------------------------------------------------------------------

#[test]
fn edits_match_indesign() {
    let root = oracle();
    let only: Option<BTreeSet<String>> = std::env::var("FOLIO_ORACLE_ONLY").ok().map(|s| s.split(',').map(str::to_owned).collect());
    let mut failures = Vec::new();
    let mut known = 0;
    let gaps: serde_json::Map<String, Value> = serde_json::from_str::<Value>(&std::fs::read_to_string(root.join("known-gaps.json")).unwrap())
        .unwrap()["oracle"].as_object().cloned().unwrap_or_default();
    let omitted_path = root.join("omitted-inputs.json");
    let omitted: std::collections::BTreeMap<String, String> = if omitted_path.exists() {
        serde_json::from_slice(&std::fs::read(omitted_path).unwrap()).unwrap()
    } else {
        Default::default()
    };
    let mut ran = 0;
    let mut groups: Vec<PathBuf> = std::fs::read_dir(root.join("scenarios")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    // Cleanroom measurements are selected by name while Folio is being compared with them.
    if only.is_some() {
        if let Ok(entries) = std::fs::read_dir(root.join("scenarios/cleanroom")) {
            groups.extend(entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")));
        }
    }
    groups.sort();
    for g in groups {
        let spec: Value = serde_json::from_str(&std::fs::read_to_string(&g).unwrap()).unwrap();
        let scenarios = spec["scenarios"].as_array().unwrap();
        if only.as_ref().is_some_and(|names| !scenarios.iter().any(|sc| names.contains(sc["name"].as_str().unwrap()))) {
            continue;
        }
        let fixture = spec["fixture"].as_str().unwrap();
        let fixture_path = root.join("fixtures").join(format!("{fixture}.idml"));
        if let Some(input) = omitted.get(&format!("{fixture}/*")) {
            eprintln!("skip {fixture}: distribution omits {input}");
            continue;
        }
        let base = read_zip(&fixture_path);
        let ids = labels(&base);
        let before_doc = read(&base, &fixture_path);
        for sc in scenarios {
            let name = sc["name"].as_str().unwrap();
            if only.as_ref().is_some_and(|o| !o.contains(name)) {
                continue;
            }
            if let Some(input) = omitted.get(&format!("{fixture}/{name}")) {
                eprintln!("skip {fixture}/{name}: distribution omits {input}");
                continue;
            }
            let golden = root.join("goldens").join(fixture).join(name);
            if !golden.is_dir() {
                failures.push(format!("{name}: no golden (run next/oracle/capture.py capture {name})"));
                continue;
            }
            ran += 1;
            if spec["comparison"]=="index" {
                let problems=index_oracle::check(&base,&golden,sc,&ids);
                if problems.is_empty() {println!("ok   {name} index paragraphs, styles, links, undo/redo");}
                else {for p in problems {eprintln!("{name}: {p}");failures.push(format!("{name}: {p}"));}}
                continue;
            }
            if spec["comparison"]=="toc" && name!="noop" {
                let problems=toc_oracle::check(&base,&golden);
                if problems.is_empty() {println!("ok   {name} TOC paragraphs, styles, links, undo/redo");}
                else {for p in problems {eprintln!("{name}: {p}");failures.push(format!("{name}: {p}"));}}
                continue;
            }
            let mut problems = Vec::new();
            let t = std::time::Instant::now();
            let mut ed = Editor::new(base.clone());
            let mut undo_steps = 0;
            // Paste (not in place) lands in the middle of InDesign's view: the oracle gives the
            // middle InDesign's pasted object has (`pasted`), a view Folio cannot know
            let native: Value = serde_json::from_str(&std::fs::read_to_string(golden.join("indesign.json")).unwrap_or_default()).unwrap_or_default();
            ADJUST.with(|a| a.set(folio_edit::Adjust { stroke: native["transformPrefsBefore"]["adjustStrokeWeightWhenScaling"] != "false", effects: native["transformPrefsBefore"]["adjustEffectsWhenScaling"] != "false" }));
            KEEP_SCALE.with(|d| d.set(native["transformPrefsBefore"]["whenScaling"] == "ADJUST_SCALING_PERCENTAGE"));
            CLIP.with(|c| *c.borrow_mut() = Clipboard { pasted: native["after"]["pasted"]["geometricBounds"].clone(), ..Clipboard::default() });
            let measured_steps = native["extension"]["steps"].as_array().filter(|steps| !steps.is_empty());
            for (k, step) in measured_steps.unwrap_or_else(|| sc["steps"].as_array().unwrap()).iter().enumerate() {
                if let (Some(family), Some(style)) = (step["props"]["appliedFont"].as_str(), step["props"]["fontStyle"].as_str()) {
                    static RESOURCE_FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
                    ed.set_font_records(RESOURCE_FONTS.get_or_init(|| folio_text::FontStore::discover(&[]))
                        .resource_record(family, style, &before_doc.font_table).into_iter().collect());
                } else { ed.set_font_records(Vec::new()); }
                if step["op"] == "releaseAnchoredObject" || step["op"] == "contentPlace" || step["op"] == "menuPlaceAndLink" || step["op"] == "duplicate" || step["op"] == "layerDuplicate" || step["op"] == "scaleByMany" || (step["op"] == "clipboard" && (step["how"] == "copy" || step["how"] == "cut")) {
                    static FONTS: std::sync::OnceLock<folio_text::FontStore> = std::sync::OnceLock::new();
                    let doc = read(ed.parts(), &fixture_path);
                    let layout = folio_text::layout_document(&doc, FONTS.get_or_init(|| folio_text::FontStore::discover(&[])));
                    ed.set_anchor_layout(&doc, &layout);
                    ed.set_frame_starts(folio_text::DocumentFrames::new(&doc).frame_ids()
                        .map(|id| ((*id).to_owned(), layout.frames.get(*id).and_then(|f| f.lines.first()).map(|l| l.start))).collect());
                    // scaling a text frame scales the text the frame shows (the composition's range)
                    ed.set_frame_spans(Editor::spans_of(folio_text::DocumentFrames::new(&doc).frame_ids().copied(), &layout));
                }
                let step_ids = labels(ed.parts());
                let parts_before = ed.parts().clone();
                match run_step(&mut ed, step, &step_ids, &base) {
                    Ok(n) => undo_steps += n,
                    Err(e) => problems.push(format!("step {k}: {e}")),
                }
                // as the server does after an edit: table rows and Auto-Size in the same undo step
                if let Err(e) = row_heights(&mut ed, &parts_before, &fixture_path) {
                    problems.push(format!("step {k} row heights: {e}"));
                }
                if let Err(e) = auto_size(&mut ed, &parts_before, &fixture_path) {
                    problems.push(format!("step {k} auto-size: {e}"));
                }
                let edited_variable = step["op"] == "defineVariable" && step["edit"] == true;
                if let Err(e) = variable_results(&mut ed, &parts_before, &fixture_path, edited_variable) {
                    problems.push(format!("step {k} text variables: {e}"));
                }
            }
            let edit_ms = t.elapsed().as_secs_f64() * 1000.0;
            let after = without_folio_only_labels(ed.parts().clone());

            // 1. the IDML changes
            let (gb, ga) = (read_dir_parts(&golden.join("before")), read_dir_parts(&golden.join("after")));
            let mut id_after = gb.clone();
            for (k, v) in &ga {
                id_after.insert(k.clone(), v.clone());
            }
            for k in gb.keys().filter(|k| !ga.contains_key(*k)) {
                id_after.remove(k);
            }
            let changed: BTreeSet<String> = gb.keys().chain(ga.keys()).cloned().collect();
            let d_indesign = delta(&gb, &id_after, None);
            let d_folio = delta(&base, &after, None);
            let new_pairs = pairing(&d_folio, &d_indesign);
            problems.extend(compare(&d_folio, &d_indesign));
            for p in ed.touched() {
                // text that differs only in formatting (attribute order, indentation) while no element and
                // no order changed is not a change (a group and its ungroup restore what was there)
                if !changed.contains(p) && !d_folio.parts_added.contains(p) && base.get(p) != after.get(p) && (!d_folio.elements.is_empty() || !d_folio.part_order.is_empty()) {
                    problems.push(format!("{p}: Folio wrote a part InDesign left alone"));
                }
            }

            // 2. undo / redo
            for _ in 0..undo_steps {
                ed.undo();
            }
            if ed.parts() != &base {
                problems.push("undo did not give the fixture back byte for byte".into());
            }
            for _ in 0..undo_steps {
                ed.redo();
            }
            if without_folio_only_labels(ed.parts().clone()) != after {
                problems.push("redo did not give the edit back".into());
            }

            // 3. the model
            let indesign: Value = serde_json::from_str(&std::fs::read_to_string(golden.join("indesign.json")).unwrap()).unwrap();
            let doc = read(&after, &fixture_path);
            if doc.warnings.len() > before_doc.warnings.len() {
                // the warnings the edit added, wherever the reader lists them
                let mut left = before_doc.warnings.clone();
                let added: Vec<_> = doc.warnings.iter().filter(|w| match left.iter().position(|b| b == *w) {
                    Some(k) => { left.remove(k); false }
                    None => true,
                }).collect();
                // a graphic the steps placed has no preview file here (only the INDD reader writes one): not a problem of the edit
                let added: Vec<_> = added.into_iter().filter(|w| !w.starts_with("Image without preview file")).collect();
                if !added.is_empty() {
                    problems.push(format!("new warnings: {added:?}"));
                }
            }
            // labels of the fixture (removed objects too) and of objects the steps made
            let mut all_ids = ids.clone();
            all_ids.extend(labels(&after));
            problems.extend(model_checks(name, &before_doc, &doc, &all_ids, &indesign));
            let mut id_doc_parts = base.clone();
            for (k, v) in &ga {
                id_doc_parts.insert(k.clone(), v.clone());
            }
            for k in gb.keys().filter(|k| !ga.contains_key(*k)) {
                id_doc_parts.remove(k);
            }
            problems.extend(model_equal(&doc, &read(&id_doc_parts, &fixture_path), &new_pairs.0, &new_pairs.1));
            // "geometry": the scenario moves objects whose transform and path InDesign writes in its own split (Adjust Layout
            // re-bases each adjusted frame on a transform that depends on the frame's edges). What is compared for those objects
            // is where they end up (the `bounds moved` check above and the printed page), not the split of the numbers.
            if sc["geometry"].as_bool() == Some(true) {
                // every object, the members of groups too (InDesign re-bases their transforms as well)
                fn walk<'a>(items: &'a [PageItem], parent: &folio_model::Affine, out: &mut Vec<(&'a PageItem, folio_model::Affine)>) {
                    for it in items {
                        out.push((it, *parent));
                        if let folio_model::ItemKind::Group { children } = &it.kind { walk(children, &parent.then_inner(&it.transform), out); }
                    }
                }
                let mut every = Vec::new();
                for s in doc.spreads.iter().chain(&doc.master_spreads) { walk(&s.items, &folio_model::Affine::default(), &mut every); }
                let item_ids: Vec<String> = every.iter().map(|(it, _)| format!(" {}: ", it.id)).collect();
                problems.retain(|p| !(p.starts_with("model item ") || item_ids.iter().any(|id| p.contains(id.as_str()))));
                // every object, labelled or not (a parent's primary text frame and its page copies): same outline bounds
                let theirs = read(&id_doc_parts, &fixture_path);
                for (it, parent) in &every {
                    let (Some(a), Some((o, po))) = (bounds(it, parent), locate(&theirs, &it.id)) else { problems.push(format!("{}: missing in InDesign's file", it.id)); continue };
                    let Some(b) = bounds(o, &po) else { continue };
                    if [a.left - b.left, a.top - b.top, a.right - b.right, a.bottom - b.bottom].iter().any(|d| d.abs() > 1e-3) {
                        problems.push(format!("{}: outline bounds Folio {:?}, InDesign {:?}", it.id, [a.left, a.top, a.right, a.bottom], [b.left, b.top, b.right, b.bottom]));
                    }
                }
            }
            if let Some(measured) = indesign["textAfter"].as_object() {
                for (label, native) in measured {
                    let Some((item, _)) = locate(&doc, &all_ids[label]) else { continue };
                    let folio_model::ItemKind::TextFrame(frame) = &item.kind else { continue };
                    let part = format!("Stories/Story_{}.xml", frame.story);
                    let actual = folio_edit::text::contents(&after[&part], &frame.story).unwrap();
                    let mut native_chars: Vec<char> = actual.chars().collect();
                    let mut anchors: Vec<_> = doc.hyperlinks.destinations.iter()
                        .filter(|d| d.kind == "HyperlinkTextDestination" && d.story == frame.story)
                        .map(|d| d.at).collect();
                    anchors.sort_unstable();
                    for at in anchors.into_iter().rev() { native_chars.insert(at, '\u{feff}'); }
                    let actual: String = native_chars.into_iter().collect();
                    let expected: String = native["characters"].as_array().unwrap().iter().map(|c| {
                        c["contents"].as_str().map(str::to_owned).unwrap_or_else(|| {
                            let code = c["contents"].as_u64().unwrap_or(0) as u32;
                            special_char(code).unwrap_or_else(|| panic!("unknown InDesign special character {code}")).to_string()
                        })
                    }).collect();
                    if actual != expected || actual.chars().count() as u64 != native["count"].as_u64().unwrap() {
                        problems.push(format!("{label}: native character indices/text differ"));
                    }
                    if ed.touched().contains(&part) && !ed.recompose().contains(&frame.story) {
                        problems.push(format!("{label}: edited story missing from recompose"));
                    }
                }
            }

            // a known gap (known-gaps.json "oracle"): measured, not hidden. Only differences that
            // all mention its `only` text count as the gap; one that starts to pass fails.
            let gap = gaps.get(&format!("{fixture}/{name}"));
            // `only`: one text, or several (each problem mentions one of them)
            let only: Vec<&str> = match gap.and_then(|g| g.get("only")) {
                Some(Value::String(o)) => vec![o.as_str()],
                Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            let is_gap = !only.is_empty() && !problems.is_empty() && problems.iter().all(|p| only.iter().any(|o| p.contains(o)));
            if gap.is_some() && problems.is_empty() {
                problems.push("known gap now passes: remove it from known-gaps.json".into());
            }
            let status = if problems.is_empty() { "ok" } else if is_gap { "gap" } else { "FAIL" };
            println!("{status:4} {fixture}/{name:28} {edit_ms:7.2} ms");
            for p in &problems {
                println!("       {p}");
            }
            if is_gap {
                known += 1;
            } else if !problems.is_empty() {
                failures.push(format!("{name}: {}", problems.join("; ")));
            }
        }
    }
    println!("{ran} scenarios, {} failed{}", failures.len(), if known > 0 { format!(", {known} known gaps (known-gaps.json)") } else { String::new() });
    assert!(failures.is_empty(), "{} of {ran} scenarios differ from InDesign", failures.len());
}
