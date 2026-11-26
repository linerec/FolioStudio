//! Page-item graphics read from IDML: corner options lowered into the path, stroke styles,
//! gap colour and arrowheads lowered into stroke parts, image clipping paths, inner effects.
//! Expected numbers come from InDesign 2025's own PDF output of the same settings
//! (artifacts/next-20261018/graphics/indesign).

use folio_idml::read_package;
use folio_model::geom::{PartShape, StrokeInk};
use folio_model::{BevelStyle, BevelTechnique, Document, ItemKind, PageItem, Paint};
use std::io::Write;
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        // tests run in parallel and share a name: each gets its own folder
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("folio-idml-gfx-{}-{name}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        TempDir(d)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn zip(path: &std::path::Path, files: &[(&str, &str)]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in files {
        z.start_file(*name, opts).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

const DESIGNMAP: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Document xmlns:idPkg="http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging" DOMVersion="20.5" Self="d">
  <idPkg:Graphic src="Resources/Graphic.xml" />
  <idPkg:Styles src="Resources/Styles.xml" />
  <Layer Self="l" Name="Layer 1" Visible="true" Printable="true" />
  <idPkg:Spread src="Spreads/Spread_s.xml" />
</Document>"#;

const GRAPHIC: &str = r#"<idPkg:Graphic xmlns:idPkg="p">
  <Color Self="Color/Black" Model="Process" Space="CMYK" ColorValue="0 0 0 100" Name="Black" />
  <Color Self="Color/Paper" Model="Process" Space="CMYK" ColorValue="0 0 0 0" Name="Paper" ColorOverride="Specialpaper" />
  <Color Self="Color/Red" Model="Process" Space="CMYK" ColorValue="0 100 100 0" Name="Red" />
  <Swatch Self="Swatch/None" Name="None" />
  <StrokeStyle Self="StrokeStyle/$ID/Dashed" Name="$ID/Dashed" />
  <DottedStrokeStyle Self="DottedStrokeStyle/Dots" DotArray="10 18" StrokeCornerAdjustment="Gaps" Name="Dots" />
</idPkg:Graphic>"#;

const STYLES: &str = r#"<idPkg:Styles xmlns:idPkg="p"><RootObjectStyleGroup Self="o">
  <ObjectStyle Self="ObjectStyle/$ID/[None]" Name="$ID/[None]" FillColor="Swatch/None" StrokeColor="Swatch/None" StrokeWeight="0" />
</RootObjectStyleGroup></idPkg:Styles>"#;

fn path(pts: &[(f64, f64)], open: bool) -> String {
    let mut s = format!("<Properties><PathGeometry><GeometryPathType PathOpen=\"{open}\"><PathPointArray>");
    for (x, y) in pts {
        s += &format!("<PathPointType Anchor=\"{x} {y}\" LeftDirection=\"{x} {y}\" RightDirection=\"{x} {y}\" />");
    }
    s + "</PathPointArray></GeometryPathType></PathGeometry></Properties>"
}

fn rect(l: f64, t: f64, r: f64, b: f64) -> String {
    path(&[(l, t), (l, b), (r, b), (r, t)], false)
}

fn spread() -> String {
    let fx = r#"<TransparencySetting>
      <InnerShadowSetting Applied="true" Angle="90" Distance="10" Size="5" ChokeAmount="20" />
      <InnerGlowSetting Applied="true" Source="CenterSourced" Size="9" />
      <BevelAndEmbossSetting Applied="true" Style="Emboss" Technique="ChiselHard" Direction="Down" Size="12" />
      <SatinSetting Applied="true" InvertEffect="true" />
    </TransparencySetting>"#;
    let clip = r#"<Properties><GraphicBounds Left="0" Top="0" Right="100" Bottom="100" />
      <ClippingPathGeometry><GeometryPathType PathOpen="false"><PathPointArray>
        <PathPointType Anchor="0 100" LeftDirection="0 100" RightDirection="0 100" />
        <PathPointType Anchor="50 0" LeftDirection="50 0" RightDirection="50 0" />
        <PathPointType Anchor="100 100" LeftDirection="100 100" RightDirection="100 100" />
      </PathPointArray></GeometryPathType></ClippingPathGeometry></Properties>"#;
    format!(
        r#"<idPkg:Spread xmlns:idPkg="p"><Spread Self="s" ItemTransform="1 0 0 1 0 0">
  <Page Self="p1" Name="1" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 0 0" AppliedMaster="n" />
  <Rectangle Self="rounded" FillColor="Color/Red" ItemTransform="1 0 0 1 0 0" CornerOption="RoundedCorner" CornerRadius="12"
     TopLeftCornerOption="RoundedCorner" TopRightCornerOption="RoundedCorner" BottomLeftCornerOption="RoundedCorner" BottomRightCornerOption="RoundedCorner"
     TopLeftCornerRadius="12" TopRightCornerRadius="12" BottomLeftCornerRadius="12" BottomRightCornerRadius="12">{r1}</Rectangle>
  <Rectangle Self="bevel" FillColor="Color/Red" ItemTransform="1 0 0 1 0 0" TopLeftCornerOption="BevelCorner" TopLeftCornerRadius="10">{r2}</Rectangle>
  <GraphicLine Self="dashed" StrokeWeight="4" StrokeColor="Color/Black" StrokeType="StrokeStyle/$ID/Dashed" StrokeDashAndGap="12 6" ItemTransform="1 0 0 1 0 0">{l1}</GraphicLine>
  <GraphicLine Self="arrows" StrokeWeight="4" StrokeColor="Color/Black" LeftLineEnd="TriangleArrowHead" RightLineEnd="CircleArrowHead" ItemTransform="1 0 0 1 0 0">{l2}</GraphicLine>
  <Rectangle Self="dots" StrokeWeight="8" StrokeColor="Color/Black" StrokeType="DottedStrokeStyle/Dots" GapColor="Color/Red" GapTint="50" OverprintGap="true" ItemTransform="1 0 0 1 0 0">{r3}</Rectangle>
  <Rectangle Self="clipped" ItemTransform="1 0 0 1 0 0">{r4}
    <Image Self="img" ItemTransform="1 0 0 1 300 300">{clip}
      <ClippingPathSettings ClippingType="AlphaChannel" InvertPath="true" />
    </Image>
  </Rectangle>
  <Rectangle Self="edges" ItemTransform="1 0 0 1 0 0">{r4}
    <Image Self="img2" ItemTransform="1 0 0 1 300 300"><Properties><GraphicBounds Left="0" Top="0" Right="100" Bottom="100" /></Properties>
      <ClippingPathSettings ClippingType="DetectEdges" />
    </Image>
  </Rectangle>
  <Rectangle Self="fx" FillColor="Color/Red" ItemTransform="1 0 0 1 0 0">{r1}{fx}</Rectangle>
</Spread></idPkg:Spread>"#,
        r1 = rect(30.0, 30.0, 150.0, 110.0),
        r2 = rect(200.0, 30.0, 300.0, 110.0),
        l1 = path(&[(400.0, 70.0), (550.0, 70.0)], true),
        l2 = path(&[(230.0, 200.0), (380.0, 200.0)], true),
        r3 = rect(30.0, 300.0, 180.0, 360.0),
        r4 = rect(300.0, 300.0, 400.0, 400.0),
    )
}

fn read() -> Document {
    let dir = TempDir::new("doc");
    let idml = dir.0.join("doc.idml");
    let spread = spread();
    zip(
        &idml,
        &[
            ("mimetype", "application/vnd.adobe.indesign-idml-package"),
            ("designmap.xml", DESIGNMAP),
            ("Resources/Graphic.xml", GRAPHIC),
            ("Resources/Styles.xml", STYLES),
            ("Spreads/Spread_s.xml", &spread),
        ],
    );
    read_package(&idml, None).expect("read")
}

fn item<'a>(doc: &'a Document, id: &str) -> &'a PageItem {
    doc.spreads[0].items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("item {id}"))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 2e-3
}

#[test]
fn corner_options_become_the_path() {
    let doc = read();
    let r = item(&doc, "rounded");
    let pts = &r.paths[0].points;
    assert_eq!(pts.len(), 8, "four arcs");
    // InDesign: m 42,30 c 35.373,30 30,35.373 30,42
    let p = pts.iter().find(|p| near(p.anchor.x, 42.0) && near(p.anchor.y, 30.0)).expect("tangent point");
    assert!(near(p.right.x, 35.373) && near(p.right.y, 30.0));
    let b = item(&doc, "bevel");
    assert_eq!(b.paths[0].points.len(), 5, "one corner cut");
    assert!(b.paths[0].points.iter().any(|p| near(p.anchor.x, 210.0) && near(p.anchor.y, 30.0)));
    assert!(b.paths[0].points.iter().any(|p| near(p.anchor.x, 200.0) && near(p.anchor.y, 40.0)));
    assert!(!doc.warnings.iter().any(|w| w.contains("corner option")), "{:?}", doc.warnings);
}

#[test]
fn stroke_styles_and_arrowheads_become_stroke_parts() {
    let doc = read();
    let d = item(&doc, "dashed");
    assert_eq!(d.stroke.style.as_deref(), Some("StrokeStyle/$ID/Dashed"));
    let art = d.stroke.art.as_ref().expect("dashes lowered");
    // a straight two-point line: whole dashes at both ends, 150 = 8 × (12+6) + 12 adjusted
    match &art.parts[0].shape {
        PartShape::Line { dash: Some(dash), width, .. } => {
            assert!(near(*width, 4.0));
            assert!(near(dash.array[0], 11.647) && near(dash.array[1], 5.647), "{:?}", dash.array);
        }
        other => panic!("{other:?}"),
    }
    let a = item(&doc, "arrows").stroke.art.as_ref().expect("arrowheads lowered");
    let line = &a.parts[0];
    assert!(near(line.paths[0].points[0].anchor.x, 239.849), "line pulled back behind the triangle");
    assert!(a.parts.iter().any(|p| p.shape == PartShape::Fill), "solid triangle");
    assert!(a.parts.iter().any(|p| matches!(p.shape, PartShape::Line { width, .. } if near(width, 4.0)) && p.paths[0].points.len() > 2), "hollow circle");
    let dots = item(&doc, "dots");
    assert_eq!(dots.stroke.gap, Paint::Swatch { id: "Color/Red".into(), tint: 50.0 });
    assert!(dots.print.overprint_gap);
    let art = dots.stroke.art.as_ref().expect("dots lowered");
    assert_eq!(art.parts[0].ink, StrokeInk::Gap, "gap colour painted first");
    assert!(art.parts.iter().any(|p| matches!(&p.shape, PartShape::Line { dash: Some(d), .. } if d.array[0] == 0.0)));
    assert!(!doc.warnings.iter().any(|w| w.contains("stroke type") || w.contains("arrowhead")), "{:?}", doc.warnings);
}

#[test]
fn image_clipping_paths_are_read() {
    let doc = read();
    let g = match &item(&doc, "clipped").kind {
        ItemKind::Rectangle { content: Some(g) } => g,
        k => panic!("{k:?}"),
    };
    let clip = g.clip.as_ref().expect("clipping path");
    assert_eq!(clip.kind, "AlphaChannel");
    assert!(clip.invert);
    assert_eq!(clip.paths[0].points.len(), 3);
    // a clipping type InDesign recorded without its geometry stays unclipped, reported
    let g2 = match &item(&doc, "edges").kind {
        ItemKind::Rectangle { content: Some(g) } => g,
        k => panic!("{k:?}"),
    };
    assert!(g2.clip.is_none());
    assert!(doc.warnings.iter().any(|w| w.contains("DetectEdges") && w.contains("without its geometry")));
}

#[test]
fn inner_effects_are_read_with_indesign_defaults() {
    let doc = read();
    let fx = &item(&doc, "fx").print.fx;
    let s = fx.inner_shadow.as_ref().expect("inner shadow");
    // light from 90° (above): the shadow falls 10 pt down; choke 20 %; Multiply 75 % black
    assert!(near(s.x_offset, 0.0) && near(s.y_offset, 10.0), "{s:?}");
    assert!(near(s.spread, 0.2) && near(s.opacity, 0.75) && near(s.size, 5.0));
    assert_eq!(s.blend, folio_model::BlendMode::Multiply);
    let g = fx.inner_glow.as_ref().expect("inner glow");
    assert!(g.center && near(g.size, 9.0) && near(g.opacity, 0.75));
    assert_eq!(g.blend, folio_model::BlendMode::Screen);
    let b = fx.bevel.as_ref().expect("bevel");
    assert_eq!((b.style, b.technique, b.up), (BevelStyle::Emboss, BevelTechnique::ChiselHard, false));
    assert!(near(b.size, 12.0) && near(b.depth, 1.0) && near(b.angle, 120.0) && near(b.altitude, 30.0));
    let st = fx.satin.as_ref().expect("satin");
    assert!(st.invert && near(st.opacity, 0.5) && near(st.distance, 7.0));
}
