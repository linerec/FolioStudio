//! A bounded static SVG reader shared by screen and PDF output. Unsupported content is an
//! error carrying the element or property name; callers retain that in their import report.
//! What the static reader does not draw (text, filters, masks, gradients, clip paths,
//! opacity, CSS, embedded bitmaps) is read by resvg instead ([`rich`], build-a2).
pub(crate) mod path;
mod values;
mod xml;
mod draw;
mod rich;
#[cfg(test)] mod tests;

use folio_model::{Affine, LineCap, LineJoin, Rect, SubPath};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What PDF output writes for a scene, in scene coordinates (y down), in order.
#[derive(Debug)]
pub enum Part {
    /// A path of solid colour (its `transform` maps it into the scene).
    Shape(Shape),
    /// Clips what follows up to the matching `EndClip`.
    Clip { paths: Vec<SubPath>, transform: Affine, evenodd: bool },
    EndClip,
    /// Content drawn as pixels (filters, masks, gradients, patterns, opacity, bitmaps):
    /// premultiplied RGBA covering `rect`.
    Raster { pixmap: tiny_skia::Pixmap, rect: Rect },
}
use values::{length, numbers, transform};
use xml::Node;

#[derive(Clone, Debug)]
pub struct Shape {
    pub paths: Vec<SubPath>,
    pub transform: Affine,
    pub fill: Option<[u8;3]>,
    pub stroke: Option<[u8;3]>,
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter: f64,
    pub dash: Option<folio_model::geom::Dash>,
    pub evenodd: bool,
    color: [u8;3],
    fill_current: bool,
    stroke_current: bool,
    dash_offset: f64,
    visible: bool,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    /// The static reader's shapes (empty when `rich` draws the picture).
    pub shapes: Vec<Shape>,
    /// The picture read by resvg when it has more than the static subset.
    pub rich: Option<Arc<rich::Rich>>,
}

impl Scene {
    pub fn placement(&self, bounds: &Rect) -> Affine {
        Affine::translate(bounds.left,bounds.top).then_inner(&Affine::scale(bounds.width()/self.width,bounds.height()/self.height))
    }

    /// What PDF output writes: the shapes, or for a picture read by resvg its plain paths as vectors and the rest as
    /// pixels, `px` pixels per scene unit.
    pub fn parts(&self, px: f64) -> Vec<Part> {
        match &self.rich {
            Some(r) => r.parts(px),
            None => self.shapes.iter().cloned().map(Part::Shape).collect(),
        }
    }
}

pub fn read(file: &std::path::Path) -> Result<Scene,String> {
    if std::fs::metadata(file).map_err(|e|e.to_string())?.len() > 8*1024*1024 {return Err("SVG: file exceeds 8 MB".into());}
    let bytes = std::fs::read(file).map_err(|e|format!("SVG: {e}"))?;
    // a Windows metafile (EMF, WMF) is drawn through the SVG its records make (build-a)
    if crate::metafile::is_metafile(&bytes) { return parse_any(&crate::metafile::convert(&bytes)?.svg); }
    parse_any(&String::from_utf8(bytes).map_err(|e|format!("SVG: {e}"))?)
}

/// The static reader, else resvg for what it does not draw; the static reader's reason when neither reads it.
pub fn parse_any(text: &str) -> Result<Scene,String> {
    match parse(text) {
        Ok(s) => Ok(s),
        Err(first) => rich::read(text).map_err(|e| format!("{first} ({e})")),
    }
}

fn unsupported(what: &str) -> String { format!("SVG: unsupported {what}") }

pub fn parse(text: &str) -> Result<Scene,String> {
    let root = xml::parse(text)?;
    if ["width", "height"].iter().any(|name| root.attr(name).is_some_and(|v| v.trim().ends_with('%'))) {
        return Err(unsupported("percentage root size"));
    }
    let vb = root.attr("viewBox").map(|v|numbers(v).ok_or("SVG: invalid viewBox")).transpose()?;
    let vb = match vb.as_deref() {
        Some([x,y,w,h]) if *w>0. && *h>0. => Some([*x,*y,*w,*h]),
        None => None,
        _ => return Err("SVG: invalid viewBox".into()),
    };
    let width = root.attr("width").map(|v|length(v,vb.map_or(300.,|v|v[2]))).unwrap_or(Some(vb.map_or(300.,|v|v[2]))).ok_or("SVG: invalid width")?;
    let height = root.attr("height").map(|v|length(v,vb.map_or(150.,|v|v[3]))).unwrap_or(Some(vb.map_or(150.,|v|v[3]))).ok_or("SVG: invalid height")?;
    if width<=0. || height<=0. {return Err("SVG: empty viewport".into());}
    let viewbox = vb.unwrap_or([0.,0.,width,height]);
    let mut sx=width/viewbox[2]; let mut sy=height/viewbox[3];
    let par=root.attr("preserveAspectRatio").unwrap_or("xMidYMid meet");
    let mut dx=0.; let mut dy=0.;
    if par!="none" {
        let mut p=par.split_whitespace(); let align=p.next().unwrap_or(""); let mode=p.next().unwrap_or("meet");
        if !["xMinYMin","xMidYMin","xMaxYMin","xMinYMid","xMidYMid","xMaxYMid","xMinYMax","xMidYMax","xMaxYMax"].contains(&align) || !["meet","slice"].contains(&mode) || p.next().is_some() {return Err(unsupported("preserveAspectRatio"));}
        let scale=if mode=="slice" {sx.max(sy)} else {sx.min(sy)}; sx=scale;sy=scale;
        let factor=|s:&str| if s=="Min" {0.} else if s=="Mid" {0.5} else {1.};
        dx=(width-viewbox[2]*sx)*factor(&align[1..4]); dy=(height-viewbox[3]*sy)*factor(&align[5..8]);
    }
    let view=Affine::new(sx,0.,0.,sy,dx-viewbox[0]*sx,dy-viewbox[1]*sy);
    let mut ids=BTreeMap::new(); collect(&root,&mut ids)?;
    let base=Shape {paths:vec![],transform:view,fill:Some([0,0,0]),stroke:None,width:1.,cap:LineCap::Butt,join:LineJoin::Miter,miter:4.,dash:None,evenodd:false,color:[0,0,0],fill_current:false,stroke_current:false,dash_offset:0.,visible:true};
    let mut shapes=Vec::new();
    walk(&root,&base,&ids,&mut Vec::new(),&mut shapes,[viewbox[2],viewbox[3]],true,&mut 0)?;
    Ok(Scene {width,height,shapes,rich:None})
}

fn collect<'a>(n:&'a Node, ids:&mut BTreeMap<String,&'a Node>)->Result<(),String> {
    if !matches!(n.name.as_str(),"svg"|"g"|"defs"|"use"|"path"|"rect"|"circle"|"ellipse"|"line"|"polyline"|"polygon"|"title"|"desc"|"metadata") {
        return Err(unsupported(&n.name));
    }
    if let Some(id)=n.attr("id") {if ids.insert(id.to_string(),n).is_some() {return Err("SVG: duplicate id".into());}}
    for c in &n.children {collect(c,ids)?;}
    Ok(())
}

fn walk(n:&Node,parent:&Shape,ids:&BTreeMap<String,&Node>,references:&mut Vec<String>,out:&mut Vec<Shape>,viewport:[f64;2],root:bool,visits:&mut usize)->Result<(),String> {
    *visits+=1;
    if *visits>20000 {return Err("SVG: too many expanded elements".into());}
    if matches!(n.name.as_str(),"title"|"desc"|"metadata") { return Ok(()); }
    if n.name=="defs" {return Ok(());}
    let mut attrs=n.attrs.clone();
    if let Some(style)=n.attr("style") {
        for declaration in style.split(';').filter(|s|!s.trim().is_empty()) {
            let (key,value)=declaration.split_once(':').ok_or("SVG: invalid style")?;
            if matches!(key.trim(),"x"|"y"|"width"|"height"|"cx"|"cy"|"r"|"rx"|"ry"|"d") {return Err(unsupported("CSS geometry"));}
            attrs.insert(key.trim().into(),value.trim().into());
        }
    }
    for (key,value) in &attrs {
        if key.starts_with("on") || matches!(key.as_str(),"filter"|"mask"|"clip-path"|"class"|"vector-effect"|"marker-start"|"marker-mid"|"marker-end") {return Err(unsupported(key));}
        if matches!(key.as_str(),"opacity"|"fill-opacity"|"stroke-opacity") && value.parse::<f64>().ok()!=Some(1.) {return Err(unsupported(key));}
        if !matches!(key.as_str(),"id"|"xmlns"|"xmlns:xlink"|"version"|"baseProfile"|"xml:space"|"style"|"transform"|"fill"|"stroke"|"color"|"fill-rule"|"stroke-width"|"stroke-linecap"|"stroke-linejoin"|"stroke-miterlimit"|"stroke-dasharray"|"stroke-dashoffset"|"opacity"|"fill-opacity"|"stroke-opacity"|"display"|"visibility"|"viewBox"|"preserveAspectRatio"|"width"|"height"|"x"|"y"|"x1"|"y1"|"x2"|"y2"|"cx"|"cy"|"r"|"rx"|"ry"|"d"|"points"|"href"|"xlink:href") {return Err(unsupported(key));}
        if key=="display" && !matches!(value.as_str(),"inline"|"none") {return Err(unsupported("display"));}
        if key=="visibility" && !matches!(value.as_str(),"visible"|"hidden") {return Err(unsupported("visibility"));}
    }
    let get=|key:&str|attrs.get(key).map(String::as_str);
    if get("display")==Some("none") {return Ok(());}
    let mut shape=parent.clone();
    if let Some(v)=get("visibility") {shape.visible=v=="visible";}
    if let Some(v)=get("color") {shape.color=values::color(v).flatten().ok_or_else(||unsupported("color"))?;}
    if let Some(v)=get("transform") {shape.transform=parent.transform.then_inner(&transform(v).ok_or("SVG: invalid transform")?);}
    for (name,target,current) in [("fill",&mut shape.fill,&mut shape.fill_current),("stroke",&mut shape.stroke,&mut shape.stroke_current)] {
        if let Some(v)=get(name) {
            *current=v=="currentColor";
            *target=if v=="currentColor" {Some(shape.color)} else {values::color(v).ok_or_else(||unsupported(&format!("{name}={v}")))?};
        }
        if *current {*target=Some(shape.color);}
    }
    let diagonal=viewport[0].hypot(viewport[1])/2_f64.sqrt();
    if let Some(v)=get("stroke-width") {shape.width=length(v,diagonal).filter(|v|*v>=0.).ok_or("SVG: invalid stroke-width")?;}
    if let Some(v)=get("stroke-linecap") {shape.cap=match v {"butt"=>LineCap::Butt,"round"=>LineCap::Round,"square"=>LineCap::Projecting,_=>return Err(unsupported("stroke-linecap"))};}
    if let Some(v)=get("stroke-linejoin") {shape.join=match v {"miter"=>LineJoin::Miter,"round"=>LineJoin::Round,"bevel"=>LineJoin::Bevel,_=>return Err(unsupported("stroke-linejoin"))};}
    if let Some(v)=get("stroke-miterlimit") {shape.miter=v.parse::<f64>().ok().filter(|v|v.is_finite() && *v>=1.).ok_or("SVG: invalid miterlimit")?;}
    if let Some(v)=get("fill-rule") {shape.evenodd=match v {"evenodd"=>true,"nonzero"=>false,_=>return Err(unsupported("fill-rule"))};}
    if let Some(v)=get("stroke-dasharray") {
        shape.dash=if v=="none" {None} else {
            let mut lengths:Vec<f64>=v.split(|c:char|c.is_whitespace()||c==',').filter(|s|!s.is_empty()).map(|v|length(v,diagonal).filter(|v|*v>=0.).ok_or("SVG: invalid dasharray")).collect::<Result<_,_>>()?;
            if lengths.len()%2==1 {lengths.extend(lengths.clone());}
            if lengths.iter().sum::<f64>()==0. {None} else {Some(folio_model::geom::Dash {array:lengths,phase:0.})}
        };
    }
    if let Some(v)=get("stroke-dashoffset") {shape.dash_offset=length(v,diagonal).ok_or("SVG: invalid dashoffset")?;}
    if let Some(d)=&mut shape.dash {d.phase=shape.dash_offset;}
    match n.name.as_str() {
        "svg" if root => {},
        "g" => {},
        "use" => {
            if get("href").is_some() {return Err(unsupported("SVG 2 href; use xlink:href"));}
            let href=get("xlink:href").ok_or("SVG: use without xlink:href")?;
            let id=href.strip_prefix('#').ok_or_else(||unsupported("external reference"))?;
            if references.len()>=32 || references.iter().any(|v|v==id) {return Err("SVG: cyclic use reference".into());}
            let target=ids.get(id).ok_or("SVG: missing use reference")?;
            let x=get("x").map(|v|length(v,viewport[0])).unwrap_or(Some(0.)).ok_or("SVG: invalid use x")?;
            let y=get("y").map(|v|length(v,viewport[1])).unwrap_or(Some(0.)).ok_or("SVG: invalid use y")?;
            shape.transform=shape.transform.then_inner(&Affine::translate(x,y));
            references.push(id.into()); walk(target,&shape,ids,references,out,viewport,false,visits)?; references.pop(); return Ok(());
        },
        "path"|"rect"|"circle"|"ellipse"|"line"|"polyline"|"polygon" => {
            if n.children.iter().any(|c|!matches!(c.name.as_str(),"title"|"desc"|"metadata")) {return Err(unsupported("children of a shape"));}
            shape.paths=geometry(n,viewport)?;
            if shape.visible {out.push(shape);} return Ok(());
        },
        _=>return Err(unsupported(&n.name)),
    }
    for child in &n.children {walk(child,&shape,ids,references,out,viewport,false,visits)?;}
    Ok(())
}

fn geometry(n:&Node,v:[f64;2])->Result<Vec<SubPath>,String> {
    let l=|key:&str,axis:usize|n.attr(key).map(|s|length(s,v[axis])).unwrap_or(Some(0.)).ok_or_else(||format!("SVG: invalid {key}"));
    let d=match n.name.as_str() {
        "path"=>n.attr("d").unwrap_or("").to_string(),
        "line"=>format!("M{} {}L{} {}",l("x1",0)?,l("y1",1)?,l("x2",0)?,l("y2",1)?),
        "polyline"|"polygon"=>{
            let p=numbers(n.attr("points").unwrap_or("")).ok_or("SVG: invalid points")?;
            if p.len()%2!=0 {return Err("SVG: odd points".into());}
            if p.is_empty() {return Ok(vec![]);}
            format!("M{}{}",p.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "),if n.name=="polygon" {"Z"} else {""})
        },
        "rect"=>{
            let (x,y,w,h)=(l("x",0)?,l("y",1)?,l("width",0)?,l("height",1)?);
            if w<0. || h<0. {return Err("SVG: negative rectangle size".into());}
            if w==0. || h==0. {return Ok(vec![]);}
            let rx=n.attr("rx").or_else(||n.attr("ry")).map(|s|length(s,v[0])).unwrap_or(Some(0.)).ok_or("SVG: invalid rx")?.clamp(0.,w/2.);
            let ry=n.attr("ry").or_else(||n.attr("rx")).map(|s|length(s,v[1])).unwrap_or(Some(0.)).ok_or("SVG: invalid ry")?.clamp(0.,h/2.);
            if rx==0. || ry==0. {format!("M{x} {y}h{w}v{h}h{}Z",-w)} else {
                format!("M{} {y}H{}A{rx} {ry} 0 0 1 {} {}V{}A{rx} {ry} 0 0 1 {} {}H{}A{rx} {ry} 0 0 1 {x} {}V{}A{rx} {ry} 0 0 1 {} {y}Z",x+rx,x+w-rx,x+w,y+ry,y+h-ry,x+w-rx,y+h,x+rx,y+h-ry,y+ry,x+rx)
            }
        },
        _=>{
            let (x,y)=(l("cx",0)?,l("cy",1)?);
            let (rx,ry)=if n.name=="circle" {let r=n.attr("r").map(|s|length(s,v[0].hypot(v[1])/2_f64.sqrt())).unwrap_or(Some(0.)).ok_or("SVG: invalid r")?;(r,r)} else {(l("rx",0)?,l("ry",1)?)};
            if rx<0. || ry<0. {return Err("SVG: negative radius".into());}
            if rx==0. || ry==0. {return Ok(vec![]);}
            format!("M{} {y}A{rx} {ry} 0 1 0 {} {y}A{rx} {ry} 0 1 0 {} {y}Z",x+rx,x-rx,x+rx)
        }
    };
    path::parse(&d,&Affine::IDENTITY).ok_or_else(||"SVG: invalid path data".into())
}
