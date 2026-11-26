use super::*;

pub(crate) fn insert(w: &mut Work<'_>, story: &str, at: &usize, field: &String) -> Result<(), EditError> {
    let (dest,k) = destination(w, field, "DBF_")?;
    let source = w.new_id();
    let placeholder = format!("<<{field}>>");
    crate::text::insert(w, story, *at, &placeholder)?;
    nodes::wrap(w, story, *at, *at + placeholder.chars().count(), &format!("<HyperlinkTextSource Self=\"{source}\" Name=\"Source\" Hidden=\"false\" AppliedCharacterStyle=\"n\">"), "</HyperlinkTextSource>", None)?;
    connect(w, &source, &dest, k)
}

fn destination(w: &mut Work<'_>, field: &str, prefix: &str) -> Result<(String,usize), EditError> {
    if field.is_empty() || field.chars().any(char::is_control) {
        return Err(invalid("data merge field must be nonempty and contain no control characters"));
    }
    let name = format!("{prefix}{field}");
    let existing = data(w)?;
    let mut dest_name = name.clone();
    let mut suffix = 1;
    while existing.destinations.iter().any(|d| d.name == dest_name) {
        dest_name = format!("{name}{suffix}"); suffix += 1;
    }
    let dest = format!("HyperlinkURLDestination/{dest_name}");
    let k = key(w)?;
    append(w, &format!("<HyperlinkURLDestination Self=\"{}\" DestinationUniqueKey=\"{k}\" Name=\"{}\" DestinationURL=\"{}\" Hidden=\"true\" />", esc(&dest), esc(&dest_name), esc(&name)))?;
    Ok((dest,k))
}

fn connect(w: &mut Work<'_>, source: &str, dest: &str, k: usize) -> Result<(), EditError> {
    let existing = data(w)?;
    let mut number = 1;
    let mut link_name = "Hyperlink".to_owned();
    while existing.links.iter().any(|l| l.name == link_name) {
        link_name = format!("Hyperlink {number}"); number += 1;
    }
    add_link(w, &link_name, source, dest, k)?;
    Ok(())
}

pub(crate) fn insert_image(w: &mut Work<'_>, ids: &Vec<String>, field: &String) -> Result<(), EditError> { insert_frame_field(w, ids, field, false) }

/// A QR code field (`#Field`): an empty frame bound like an image field, with `QRC_` and `DataMergeQrcodePlaceholder` (oracle `datamerge-c4-w`).
pub(crate) fn insert_qr(w: &mut Work<'_>, ids: &Vec<String>, field: &String) -> Result<(), EditError> { insert_frame_field(w, ids, field, true) }

fn insert_frame_field(w: &mut Work<'_>, ids: &[String], field: &str, qr: bool) -> Result<(), EditError> {
    let (prefix, element) = if qr { ("QRC_", "DataMergeQrcodePlaceholder") } else { ("IMG_", "DataMergeImagePlaceholder") };
    if ids.len()!=1 {return Err(invalid("select one empty rectangle"));}
    let id=&ids[0];
    let (part,ix,i)=w.find(id)?;
    let t=w.text(&part)?;
    // InDesign 2025 takes an oval or polygon frame as an image field too (oracle datamerge-image-oval, -poly)
    if !matches!(ix.local(t,i),"Rectangle"|"Oval"|"Polygon") || ix.children(i).iter().any(|&n|matches!(ix.local(t,n),"Image"|"PDF"|"EPS"|"ImportedPage")) {
        return Err(invalid("select one empty rectangle, oval or polygon"));
    }
    if data(w)?.item_sources.iter().any(|s|s.item==*id) {return Err(invalid("rectangle already has a link"));}
    set_attr(w,id,"ContentType","GraphicType")?;
    let (dest,k)=destination(w,field,prefix)?;
    let source=w.new_id();
    append_before(w,&format!("<HyperlinkPageItemSource Self=\"{source}\" Name=\"{}\" SourcePageItem=\"{}\" Hidden=\"false\" />",esc(field),esc(id)),&["HyperlinkPageItemSource","Hyperlink"])?;
    connect(w,&source,&dest,k)?;
    let ix=w.index("designmap.xml")?;let t=w.text("designmap.xml")?;
    let mut n=0;
    while (0..ix.nodes.len()).any(|i|ix.value(t,i,"Self").as_deref()==Some(&format!("d{element}{n}"))) {n+=1;}
    // InDesign writes the image placeholders first, the QR code placeholders after them (measured)
    let tags: &[&str] = if qr { &["DataMergeQrcodePlaceholder","IndexingSortOption"] } else { &["DataMergeImagePlaceholder","IndexingSortOption"] };
    append_before(w,&format!("<{element} Self=\"d{element}{n}\" Field=\"{}\" PlaceholderPageItem=\"{}\" />",esc(field),esc(id)),tags)
}

fn append_before(w: &mut Work<'_>, raw: &str, tags: &[&str]) -> Result<(), EditError> {
    let ix=w.index("designmap.xml")?;let t=w.text("designmap.xml")?;
    let root=ix.roots().next().ok_or_else(||invalid("missing document"))?;
    let at=ix.children(root).iter().find(|&&i|tags.contains(&ix.local(t,i)))
        .or_else(||ix.children(root).iter().find(|&&i|ix.local(t,i)=="IndexingSortOption"))
        .map(|&i|ix.nodes[i].start).unwrap_or(ix.nodes[root].close_start);
    w.splice("designmap.xml",vec![(at..at,format!("\n{raw}\n"))])
}
