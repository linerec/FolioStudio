use super::*;

#[test]
fn commands_and_arcs_keep_endpoints_and_open_paths() {
    let p = path::parse("M0 0l10 0h5v5c1 2 3 4 5 5s3 4 5 5q2 3 4 0t4 0a5 5 30 0 1 10 10", &Affine::IDENTITY).unwrap();
    assert!(p[0].open);
    let end = p[0].points.last().unwrap().anchor;
    assert_eq!((end.x,end.y), (43.,25.));
    let circle = parse(r#"<svg width="20" height="20"><circle cx="10" cy="10" r="8"/></svg>"#).unwrap();
    let b = circle.shapes[0].paths[0].bounds().unwrap();
    assert!((b.left-2.).abs()<1e-8 && (b.right-18.).abs()<1e-8);
}

#[test]
fn transform_viewbox_use_style_and_dash() {
    let s=parse(r##"<svg width="100" height="50" viewBox="0 0 20 10"><defs><rect id="a" width="2" height="3"/></defs><g transform="translate(1 2) scale(2)"><use xlink:href="#a" x="3" style="fill:#f00;stroke:rgb(0,0,255);stroke-dasharray:2 1 3"/></g></svg>"##).unwrap();
    assert_eq!(s.shapes.len(),1);
    assert_eq!(s.shapes[0].fill,Some([255,0,0]));
    assert_eq!(s.shapes[0].dash.as_ref().unwrap().array,vec![2.,1.,3.,2.,1.,3.]);
    let t=s.shapes[0].transform; assert_eq!((t.a,t.d,t.tx,t.ty),(10.,10.,35.,10.));
}

#[test]
fn malformed_and_unsupported_are_explicit_errors() {
    assert!(path::parse("M1e308 0l1e308 0", &Affine::IDENTITY).is_none());
    assert!(parse("<svg width='50%' height='50%' viewBox='0 0 100 100'/>").unwrap_err().contains("percentage root size"));
    for xml in ["<svg><path></svg>", "<svg><path d='M0 0A1'/></svg>", "<svg><script/></svg>", "<svg><filter/></svg>", "<svg><rect opacity='.5'/></svg>", "<svg><linearGradient/></svg>", "<svg><radialGradient/></svg>", "<svg><clipPath/></svg>", "<svg><image/></svg>", "<svg><text>words</text></svg>", "<svg><use xlink:href='elsewhere.svg'/></svg>"] {assert!(parse(xml).is_err(),"{xml}");}
    assert!(parse(r##"<svg><defs><g id="a"><use xlink:href="#a"/></g></defs><use xlink:href="#a"/></svg>"##).is_err());
}

#[test]
fn inherited_paint_visibility_and_dash_offset() {
    let s=parse(r#"<svg><g color="red" fill="currentColor" visibility="hidden" stroke-dashoffset="3"><rect width="5" height="5"/><rect width="5" height="5" visibility="visible" color="blue" stroke="currentColor" stroke-dasharray="2 1"><title>Visible</title></rect></g></svg>"#).unwrap();
    assert_eq!(s.shapes.len(),1);
    assert_eq!(s.shapes[0].fill,Some([0,0,255]));
    assert_eq!(s.shapes[0].stroke,Some([0,0,255]));
    assert_eq!(s.shapes[0].dash.as_ref().unwrap().phase,3.);
    assert!(parse(r#"<svg><rect style="width:10px" height="5"/></svg>"#).unwrap_err().contains("CSS geometry"));
}

#[test]
fn repeated_empty_references_have_a_work_limit() {
    let mut text=String::from("<svg><defs><g id='a0'/>");
    for i in 1..20 {text.push_str(&format!("<g id='a{i}'><use xlink:href='#a{}'/><use xlink:href='#a{}'/></g>",i-1,i-1));}
    text.push_str("</defs><use xlink:href='#a19'/></svg>");
    assert!(parse(&text).unwrap_err().contains("too many expanded elements"));
}
