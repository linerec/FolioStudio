use crate::{EditError, Work, xml::Index};
use folio_idml::anchored::FIELDS;
use serde_json::{Value,json};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct Placement { spread:String, layer:Option<String>, transform:folio_model::Affine }

pub(crate) fn release(w:&mut Work<'_>,ids:&[String])->Result<(),EditError> {
    for id in ids {
        let (part,ix,i,story)=locate(w,id)?;
        let t=w.text(&part)?.to_owned();
        if options(w,&t,&ix,i)?["anchoredPosition"]!="Anchored" {continue;}
        let Some(p)=w.ed.anchor_placements.get(id).cloned() else {continue;};
        let m=p.transform;
        let mut attrs=vec![("ItemTransform",crate::geom::Mat([m.a,m.b,m.c,m.d,m.tx,m.ty]).text()),("Locked","false".into())];
        if let Some(layer)=p.layer {attrs.push(("ItemLayer",layer));}
        let mut raw=super::element_with(&ix,&t,i,&attrs,&[]);
        let rx=Index::scan(&raw).map_err(|e|EditError::new("xml",e))?;
        if let Some(n)=(0..rx.nodes.len()).find(|&n|rx.local(&raw,n)=="AnchoredObjectSetting") {raw.replace_range(rx.nodes[n].start..rx.nodes[n].end,"");}
        let at=crate::text::node_position(w,&story,id)?;
        crate::text::delete(w,&story,at,at+1)?;
        let (part,ix,i)=w.find(&p.spread)?;
        let at=ix.nodes[i].close_start;
        w.splice(&part,vec![(at..at,raw)])?;
        w.recompose(&story);
    }
    Ok(())
}

fn invalid(s:impl Into<String>)->EditError { EditError::new("invalid",s) }
pub(crate) fn insert(w:&mut Work<'_>,story:&str,at:&usize,width:&f64,height:&f64,position:&str)->Result<(),EditError> {
    let story=crate::text::story_of(w,story)?;
    FIELDS[0].value(&Value::String(position.into())).map_err(invalid)?;
    let mut page=None;
    for part in w.parts_named("Spreads/") {
        let t=w.text(&part)?; let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
        if (0..ix.nodes.len()).any(|i|ix.attr(t,i,"ParentStory")==Some(story.as_str())) {
            page=(0..ix.nodes.len()).find(|&i|ix.local(t,i)=="Page").and_then(|i|ix.attr(t,i,"Self")).map(str::to_owned); break;
        }
    }
    let page=page.ok_or_else(||EditError::new("unsupported","story has no page frame"))?;
    let (spread_part,_,_)=w.find(&page)?;
    let spread_before=w.text(&spread_part)?.to_owned();
    super::create::run(w,&page,super::create::ItemKind::Rectangle,[0.0,0.0,*width,*height],None,6,None)?;
    let id=w.created.last().unwrap().clone();
    let (part,ix,i)=w.find(&id)?;let t=w.text(&part)?.to_owned();
    let weight=super::style_attr(w,&ix,&t,i,"StrokeWeight","EnableStroke").and_then(|v|v.parse::<f64>().ok()).filter(|v|*v>0.0).unwrap_or(1.0);
    super::set_paint(w,&id,None,None,None,None,Some(weight))?;
    let (part,ix,i)=w.find(&id)?; let t=w.text(&part)?.to_owned();
    let stroke=super::textframe::stroke_inset(w,&ix,&t,i);
    let transform=crate::geom::Mat([1.0,0.0,0.0,1.0,5.0+stroke,5.0-height-stroke]).text();
    let mut raw=super::element_with(&ix,&t,i,&[("ItemTransform",transform)],&["ItemLayer"]);
    let rx=Index::scan(&raw).map_err(|e|EditError::new("xml",e))?;
    let attrs=FIELDS.iter().map(|f|format!(" {}=\"{}\"",f.attr,if f.key=="anchoredPosition" {position}else {f.default})).collect::<String>();
    raw.insert_str(settings_at(&rx,&raw,0),&format!("<AnchoredObjectSetting{attrs}/>"));
    w.splice(&spread_part,vec![(0..w.text(&spread_part)?.len(),spread_before)])?;
    crate::text::replace_with_node(w,&story,*at,*at,'\u{fffc}',&raw)?;
    w.recompose(&story); Ok(())
}
fn settings_at(ix:&Index,t:&str,i:usize)->usize {
    ix.nodes.iter().enumerate().find(|(n,node)|node.parent==Some(i)&&ix.local(t,*n)!="Properties").map(|(_,n)|n.start).unwrap_or(ix.nodes[i].close_start)
}
fn locate(w:&mut Work<'_>,id:&str)->Result<(String,Index,usize,String),EditError> {
    let (part,ix,i)=w.find(id)?; let t=w.text(&part)?;
    super::check_item(&ix,t,i,id)?;
    let story=ix.ancestors(i).find(|&a|ix.local(t,a)=="Story").and_then(|a|ix.attr(t,a,"Self")).ok_or_else(||invalid("object is not anchored in a story"))?.to_owned();
    if ix.nodes[i].parent.is_none_or(|p|ix.local(t,p)!="CharacterStyleRange") {return Err(EditError::new("unsupported","nested anchored object"));}
    Ok((part,ix,i,story))
}
fn options(w:&Work<'_>,t:&str,ix:&Index,i:usize)->Result<BTreeMap<String,Value>,EditError> {
    let n=&ix.nodes[i];
    folio_idml::anchored::values(&t[n.start..n.end],w.text("Resources/Styles.xml").unwrap_or("<Styles/>" )).map_err(|e|EditError::new("xml",e))
}
pub(crate) fn set(w:&mut Work<'_>,ids:&[String],props:&BTreeMap<String,Value>)->Result<(),EditError> {
    let changes=props.iter().map(|(k,v)|{
        let f=FIELDS.iter().find(|f|f.key==k).ok_or_else(||invalid(format!("unknown anchored option: {k}")))?;
        Ok((f.attr,f.value(v).map_err(invalid)?))
    }).collect::<Result<Vec<_>,EditError>>()?;
    for id in ids {
        let (part,ix,i,story)=locate(w,id)?; let t=w.text(&part)?.to_owned();
        let option=ix.named(&t,i,"AnchoredObjectSetting").first().copied();
        let mut edits:Vec<_>=if let Some(n)=option {changes.iter().map(|(k,v)|super::attr_edit(&ix,&t,n,k,v)).collect()}
        else {let at=settings_at(&ix,&t,i);vec![(at..at,format!("<AnchoredObjectSetting{}/>",changes.iter().map(|(k,v)|format!(" {k}=\"{}\"",crate::xml::escape(v))).collect::<String>()))]};
        if let Some((_,v))=changes.iter().find(|(k,_)|*k=="LockPosition") {edits.push(super::attr_edit(&ix,&t,i,"Locked",v));}
        w.splice(&part,edits)?;w.recompose(&story);
    }
    Ok(())
}

// ---- anchoring an object that already exists: the control on a selected object, dragged into text ----------------------------
//
// Rules measured in InDesign 2025 with a real mouse (`next/oracle/rules/liveinput-c2.ko.md`, `next/oracle/measured/anchor-control-2025.json`):
// a drag of the blue square onto text anchors the object at the insertion point nearest the pointer as a custom position that keeps it where it is:
// bottom right anchor point, horizontal reference the text frame, vertical the line baseline, left and top aligned, pinning off, and the offsets
// `anchorXoffset` = left of the text frame - right of the object, `anchorYoffset` = bottom of the object - baseline of the line (the object's
// visible bounds, its stroke included; in the text frame's own space). The object's ItemTransform keeps its own scale and turn and moves the
// bottom left of its visible bounds to the origin. The anchored object is a new undo step and the options dialog of an Alt drag works on it afterwards.

/// What `set_anchor_targets` takes from the composition: where text lines and objects are, in their spread's own space.
#[derive(Clone, Default)]
pub(crate) struct Targets { frames: BTreeMap<String, TargetFrame>, items: BTreeMap<String, TargetItem> }
#[derive(Clone)]
struct TargetFrame { spread: String, to_spread: folio_model::Affine, left: f64, story: String, lines: Vec<(usize, usize, f64)> }
#[derive(Clone)]
struct TargetItem { spread: String, to_spread: folio_model::Affine, inner: folio_model::Rect, outset: f64 }

/// The object's own bounds (inner coordinates; a group's members in the group's).
fn inner_rect(item: &folio_model::PageItem) -> Option<folio_model::Rect> {
    match &item.kind {
        folio_model::ItemKind::Group { children } => children.iter().filter_map(|c| inner_rect(c).map(|r| c.transform.apply_rect(&r))).reduce(|a, b| a.union(&b)),
        _ => folio_model::paths_bounds(&item.paths),
    }
}
/// How far the visible stroke reaches outside the path (the composition counts the same).
fn stroke_outset(item: &folio_model::PageItem) -> f64 {
    if matches!(item.stroke.paint, folio_model::Paint::None) || item.stroke.weight <= 0.0 { return 0.0; }
    match item.stroke.alignment {
        folio_model::StrokeAlignment::Center => item.stroke.weight / 2.0,
        folio_model::StrokeAlignment::Outside => item.stroke.weight,
        folio_model::StrokeAlignment::Inside => 0.0,
    }
}
/// The frame's text line for story character `at`: its frame and baseline (the last line when `at` is the end of the story).
fn target_line<'a>(t: &'a Targets, story: &str, at: usize) -> Option<(&'a TargetFrame, f64)> {
    let mut last: Option<(&TargetFrame, f64, usize)> = None;
    for f in t.frames.values().filter(|f| f.story == story) {
        for &(start, end, baseline) in &f.lines {
            if start <= at && (at < end || (start == end && at == end)) { return Some((f, baseline)); }
            if last.is_none_or(|l| end >= l.2) { last = Some((f, baseline, end)); }
        }
    }
    last.filter(|l| l.2 == at).map(|l| (l.0, l.1))
}
/// What the control makes of an object dropped at a line: the settings and the object's ItemTransform in the story.
pub(crate) struct Anchoring { pub x_offset: f64, pub y_offset: f64, pub transform: crate::geom::Mat }
/// `baseline`: of the line, in the text frame's own space.
fn anchoring(frame: &TargetFrame, baseline: f64, item: &TargetItem) -> Result<Anchoring, EditError> {
    // The object keeps its own turn, scale and flip, and its origin (the place its ItemTransform puts it) keeps its place on the spread; what
    // the control writes is where that origin lies in the text frame's own space. In an unturned frame that is moving the object into the
    // frame's space. In a turned, scaled or flipped one InDesign leaves the object's own matrix alone, so the object takes the frame's turn
    // about its origin and lands elsewhere (measured, 30 real-mouse drops: next/oracle/rules/anchorturn-c1.ko.md; the same law for
    // rotation about the centre or a corner, flips, a scaled and a rotated object, a moved frame or object, any drop place).
    let frame_inverse = frame.to_spread.invert().ok_or_else(|| EditError::new("xml", "singular text frame transform"))?;
    let origin = frame_inverse.apply(folio_model::Point::new(item.to_spread.tx, item.to_spread.ty));
    let l = item.to_spread;
    let m = folio_model::Affine::new(l.a, l.b, l.c, l.d, origin.x, origin.y);
    let mut b = m.apply_rect(&item.inner);
    b.left -= item.outset; b.top -= item.outset; b.right += item.outset; b.bottom += item.outset;
    // the object's own scale and turn stay; its visible bounds' bottom left goes to the origin
    let mut own = m; own.tx = 0.0; own.ty = 0.0;
    let mut o = own.apply_rect(&item.inner);
    o.left -= item.outset; o.bottom += item.outset;
    let transform = crate::geom::Mat([m.a, m.b, m.c, m.d, -o.left, -o.bottom]);
    Ok(Anchoring { x_offset: frame.left - b.right, y_offset: b.bottom - baseline, transform })
}
/// The settings the control writes (InDesign's own order).
fn control_settings(a: &Anchoring) -> Vec<(&'static str, String)> {
    let f = |key: &str, v: Value| { let f = FIELDS.iter().find(|f| f.key == key).unwrap(); (f.attr, f.value(&v).unwrap()) };
    vec![
        f("anchoredPosition", json!("Anchored")), f("spineRelative", json!(false)), f("lockPosition", json!(false)), f("pinPosition", json!(false)),
        f("anchorPoint", json!("BottomRightAnchor")), f("horizontalAlignment", json!("LeftAlign")), f("horizontalReferencePoint", json!("TextFrame")),
        f("verticalAlignment", json!("TopAlign")), f("verticalReferencePoint", json!("LineBaseline")),
        f("anchorXoffset", json!(a.x_offset)), f("anchorYoffset", json!(a.y_offset)), f("anchorSpaceAbove", json!(0)),
    ]
}
/// `anchorObject`: the object `id` (on a spread, or already anchored in text) is anchored at story character `at` (an insertion point as the
/// drop shows it: for an already anchored object counted before its own anchor character leaves).
pub(crate) fn anchor(w: &mut Work<'_>, id: &str, story: &str, at: &usize) -> Result<(), EditError> {
    let story = crate::text::story_of(w, story)?;
    let targets = w.ed.anchor_targets.clone();
    let item = targets.items.get(id).ok_or_else(|| EditError::new("notFound", format!("{id}: not an object on a page of this document")))?;
    let (frame, baseline) = target_line(&targets, &story, *at).ok_or_else(|| EditError::new("invalid", format!("no line of text at character {at}")))?;
    if frame.spread != item.spread { return Err(EditError::new("unsupported", "the object and the text are on different spreads")); }
    let made = anchoring(frame, baseline, item)?;
    let (part, ix, i) = w.find(id)?;
    let t = w.text(&part)?.to_owned();
    super::check_item(&ix, &t, i, id)?;
    // InDesign shows no control on a locked object
    if ix.attr(&t, i, "Locked") == Some("true") { return Err(EditError::new("locked", format!("{id} is locked"))); }
    let in_story = ix.ancestors(i).find(|&a| ix.local(&t, a) == "Story").and_then(|a| ix.attr(&t, a, "Self")).map(str::to_owned);
    let mut raw = super::element_with(&ix, &t, i, &[("ItemTransform", made.transform.text())], &["ItemLayer"]);
    let rx = Index::scan(&raw).map_err(|e| EditError::new("xml", e))?;
    if let Some(n) = (0..rx.nodes.len()).find(|&n| rx.local(&raw, n) == "AnchoredObjectSetting") { raw.replace_range(rx.nodes[n].start..rx.nodes[n].end, ""); }
    let rx = Index::scan(&raw).map_err(|e| EditError::new("xml", e))?;
    let settings: String = control_settings(&made).iter().map(|(k, v)| format!(" {k}=\"{}\"", crate::xml::escape(v))).collect();
    raw.insert_str(settings_at(&rx, &raw, 0), &format!("<AnchoredObjectSetting{settings}/>"));
    let mut at = *at;
    match in_story {
        Some(old_story) => {
            if ix.nodes[i].parent.is_none_or(|p| ix.local(&t, p) != "CharacterStyleRange") { return Err(EditError::new("unsupported", "nested anchored object")); }
            let old_at = crate::text::node_position(w, &old_story, id)?;
            crate::text::delete(w, &old_story, old_at, old_at + 1)?;
            if old_story == story && at > old_at { at -= 1; }
            if old_story != story { w.recompose(&old_story); }
        }
        None => {
            if ix.nodes[i].parent.is_none_or(|p| ix.local(&t, p) != "Spread") { return Err(EditError::new("unsupported", "an object inside a group or a frame cannot be anchored")); }
            let n = &ix.nodes[i];
            let from = t[..n.start].rfind('>').map_or(n.start, |p| if t[p + 1..n.start].trim().is_empty() { p + 1 } else { n.start });
            w.splice(&part, vec![(from..n.end, String::new())])?;
        }
    }
    crate::text::replace_with_node(w, &story, at, at, '\u{fffc}', &raw)?;
    w.recompose(&story);
    w.created.push(id.to_owned());
    Ok(())
}

impl crate::Editor {
    pub fn anchored_defaults(&self)->Result<Value,EditError> {
        let t=self.parts().get("Resources/Preferences.xml").map(String::as_str).unwrap_or("<Preferences/>");
        let ix=Index::scan(t).map_err(|e|EditError::new("xml",e))?;
        let n=(0..ix.nodes.len()).find(|&i|ix.local(t,i)=="AnchoredObjectDefault");
        let number=|key|n.and_then(|i|ix.attr(t,i,key)).and_then(|v|v.parse::<f64>().ok()).filter(|v|v.is_finite()&&*v>0.0).unwrap_or(72.0);
        Ok(json!({"width":number("InitialAnchorWidth"),"height":number("InitialAnchorHeight"),"position":FIELDS[0].default}))
    }
    pub fn set_anchor_layout(&mut self,doc:&folio_model::Document,layout:&folio_model::layout::DocumentLayout) {
        use folio_model::{Affine,ItemKind,PageItem};
        fn walk(items:&[PageItem],parent:Affine,layer:Option<&str>,spread:&str,anchors:&BTreeMap<String,Vec<PageItem>>,out:&mut BTreeMap<String,Placement>) {
            for item in items {
                let m=parent.then_inner(&item.transform); let layer=item.layer.as_deref().or(layer);
                if let Some(children)=anchors.get(&item.id) {for child in children {out.insert(child.id.clone(),Placement{spread:spread.into(),layer:child.layer.as_deref().or(layer).map(str::to_owned),transform:m.then_inner(&child.transform)});}}
                if let ItemKind::Group{children}=&item.kind {walk(children,m,layer,spread,anchors,out);}
                walk(&item.clipped,m,layer,spread,anchors,out);
            }
        }
        self.anchor_placements.clear();
        let anchors=folio_model::layout::anchored_items(doc,layout);
        for spread in &doc.spreads {walk(&spread.items,Affine::IDENTITY,None,&spread.id,&anchors,&mut self.anchor_placements);}
    }
    /// The lines of the frames' text and the places of the objects, for `anchorObject` (supplied before an edit that anchors an existing object).
    pub fn set_anchor_targets(&mut self,doc:&folio_model::Document,layout:&folio_model::layout::DocumentLayout) {
        use folio_model::{Affine,ItemKind,PageItem};
        fn walk(items:&[PageItem],parent:Affine,spread:&str,anchors:&BTreeMap<String,Vec<PageItem>>,layout:&folio_model::layout::DocumentLayout,out:&mut Targets) {
            for item in items {
                let m=parent.then_inner(&item.transform);
                if let Some(inner)=inner_rect(item) {out.items.insert(item.id.clone(),TargetItem{spread:spread.into(),to_spread:m,inner,outset:stroke_outset(item)});}
                if let ItemKind::TextFrame(frame)=&item.kind
                    && let (Some(text),Some(bounds))=(layout.frames.get(&item.id),folio_model::paths_bounds(&item.paths)) {
                    out.frames.insert(item.id.clone(),TargetFrame{spread:spread.into(),to_spread:m,left:bounds.left,story:frame.story.clone(),
                        lines:text.lines.iter().filter(|l|l.table.is_none()&&l.cell.is_none()).map(|l|(l.start,l.end,l.baseline)).collect()});
                }
                if let Some(children)=anchors.get(&item.id) {for child in children {
                    if let Some(inner)=inner_rect(child) {out.items.insert(child.id.clone(),TargetItem{spread:spread.into(),to_spread:m.then_inner(&child.transform),inner,outset:stroke_outset(child)});}
                }}
                if let ItemKind::Group{children}=&item.kind {walk(children,m,spread,anchors,layout,out);}
                walk(&item.clipped,m,spread,anchors,layout,out);
            }
        }
        let anchors=folio_model::layout::anchored_items(doc,layout);
        let mut targets=Targets::default();
        for spread in &doc.spreads {walk(&spread.items,Affine::IDENTITY,&spread.id,&anchors,layout,&mut targets);}
        self.anchor_targets=targets;
    }
    pub fn anchored_object(&mut self,id:&str)->Result<Value,EditError> {
        let mut w=Work{ed:self,splices:Vec::new(),recompose:Vec::new(),created:Vec::new()};
        let (part,ix,i,story)=locate(&mut w,id)?;
        let props=options(&w,w.text(&part)?,&ix,i)?;
        let at=crate::text::node_position(&mut w,&story,id)?;
        Ok(json!({"id":id,"story":story,"at":at,"props":props,"fields":FIELDS.iter().map(|f|json!({"key":f.key,"choices":f.choices})).collect::<Vec<_>>()}))
    }
    pub fn anchored_at(&mut self,story:&str,at:usize)->Result<Option<Value>,EditError> {
        let mut w=Work{ed:self,splices:Vec::new(),recompose:Vec::new(),created:Vec::new()};
        let (part,ix,i)=w.find(story)?; let t=w.text(&part)?;
        let ids:Vec<String>=(0..ix.nodes.len()).filter(|&n|ix.ancestors(n).any(|a|a==i)&&super::is_item(&ix,t,n)).filter_map(|n|ix.attr(t,n,"Self").map(str::to_owned)).collect();
        for id in ids {if crate::text::node_position(&mut w,story,&id)?==at {return w.ed.anchored_object(&id).map(Some);}}
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn affine(v: &Value) -> folio_model::Affine {
        let a: Vec<f64> = v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        folio_model::Affine::new(a[0], a[1], a[2], a[3], a[4], a[5])
    }

    /// Every drop into a turned, flipped or scaled text frame measured in InDesign 2025 with a real mouse (30 cases, rules/anchorturn-c1.ko.md): the
    /// offsets, and where InDesign's export has it, the object's ItemTransform in the story, come out as InDesign wrote them.
    #[test]
    fn turned_frames_give_the_offsets_indesign_wrote() {
        let all: Value = serde_json::from_str(include_str!("../../../../../oracle/measured/anchor-control-2025.json")).unwrap();
        let cases = all["turned"].as_array().unwrap();
        assert!(cases.len() >= 30);
        let mut problems = Vec::new();
        let mut turned = 0;
        for c in cases {
            let label = c["label"].as_str().unwrap();
            let frame = TargetFrame { spread: "s".into(), to_spread: affine(&c["frame"]["toSpread"]), left: c["frame"]["left"].as_f64().unwrap(), story: "story".into(), lines: Vec::new() };
            let r: Vec<f64> = c["item"]["inner"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            let item = TargetItem { spread: "s".into(), to_spread: affine(&c["item"]["toSpread"]), inner: folio_model::Rect { left: r[0], top: r[1], right: r[2], bottom: r[3] }, outset: c["item"]["outset"].as_f64().unwrap() };
            let made = anchoring(&frame, c["frame"]["baseline"].as_f64().unwrap(), &item).unwrap();
            for (key, have, want) in [("xOffset", made.x_offset, &c["xOffset"]), ("yOffset", made.y_offset, &c["yOffset"])] {
                let want = want.as_f64().unwrap();
                if (have - want).abs() > 0.001 { problems.push(format!("{label}: {key} {have} (InDesign {want})")); }
            }
            if let Some(want) = c.get("itemTransform").filter(|v| !v.is_null()) {
                let want: Vec<f64> = want.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
                if made.transform.0.iter().zip(&want).any(|(x, y)| (x - y).abs() > 0.001) { problems.push(format!("{label}: ItemTransform {:?} (InDesign {want:?})", made.transform.0)); }
            }
            let f = frame.to_spread;
            if (f.a - 1.0).abs() > 1e-6 || f.b.abs() > 1e-6 || f.c.abs() > 1e-6 || (f.d - 1.0).abs() > 1e-6 { turned += 1; }
        }
        assert!(problems.is_empty(), "{} of {} differ:\n{}", problems.len(), cases.len(), problems.join("\n"));
        assert!(turned >= 20, "only {turned} turned frames");
    }
}
