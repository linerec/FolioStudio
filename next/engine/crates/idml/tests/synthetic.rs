//! Small hand-written packages that pin down reading rules independent of the real sample.

use folio_idml::{Error, read_package};
use folio_model::{
    ColorModel, Document, FirstBaseline, GradientKind, ItemKind, Justification, Leading, PageItem, Paint,
    StrokeAlignment, TextVariable, TextWrapMode, TextWrapSide, VerticalJustification,
};
use std::io::Write;
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("folio-idml-{}-{name}", std::process::id()));
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
<?aid style="50" type="document" readerVersion="6.0" featureSet="257" product="19.4" ?>
<Document xmlns:idPkg="http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging" DOMVersion="19.4" Self="d">
  <idPkg:Graphic src="Resources/Graphic.xml" />
  <idPkg:Fonts src="Resources/Fonts.xml" />
  <idPkg:Styles src="Resources/Styles.xml" />
  <idPkg:Preferences src="Resources/Preferences.xml" />
  <Layer Self="bottom" Name="Art" Visible="false" Printable="false" />
  <Layer Self="top" Name="Text" Visible="true" Printable="true" />
  <idPkg:MasterSpread src="MasterSpreads/MasterSpread_m.xml" />
  <idPkg:Spread src="Spreads/Spread_s.xml" />
  <idPkg:Story src="Stories/Story_st.xml" />
  <idPkg:Story src="Stories/Story_st2.xml" />
</Document>"#;

const GRAPHIC: &str = r#"<idPkg:Graphic xmlns:idPkg="p" DOMVersion="19.4">
  <Color Self="Color/Black" Model="Process" Space="CMYK" ColorValue="0 0 0 100" Name="Black" />
  <Color Self="Color/Paper" Model="Process" Space="CMYK" ColorValue="0 0 0 0" Name="Paper" ColorOverride="Specialpaper" />
  <Color Self="Color/Registration" Model="Registration" Space="CMYK" ColorValue="100 100 100 100" Name="Registration" />
  <Color Self="Color/Red" Model="Spot" Space="RGB" ColorValue="255 0 0" Name="Red" />
  <Color Self="Color/Lab" Model="Process" Space="LAB" ColorValue="50 0 0" Name="Lab grey" />
  <Tint Self="Tint/Black 50" BaseColor="Color/Black" TintValue="50" Name="$ID/" />
  <Swatch Self="Swatch/None" Name="None" />
  <Gradient Self="Gradient/G" Type="Radial" Name="G">
    <GradientStop Self="g0" StopColor="Color/Paper" Location="0" />
    <GradientStop Self="g1" StopColor="Color/Black" Location="100" Midpoint="30" />
  </Gradient>
</idPkg:Graphic>"#;

const FONTS: &str = r#"<idPkg:Fonts xmlns:idPkg="p"><FontFamily Self="f" Name="Minion Pro">
  <Font Self="f1" FontFamily="Minion Pro" Name="Minion Pro Regular" PostScriptName="MinionPro-Regular" FontStyleName="Regular" FontType="OpenTypeCFF" FullName="Minion Pro" Status="Installed" />
</FontFamily></idPkg:Fonts>"#;

const PREFS: &str = r#"<idPkg:Preferences xmlns:idPkg="p">
  <TextPreference SmallCap="75" SuperscriptSize="60" SuperscriptPosition="30" SubscriptSize="55" SubscriptPosition="20" UseParagraphLeading="false" ZOrderTextWrap="true" />
  <GridPreference BaselineStart="40" BaselineDivision="13" BaselineGridRelativeOption="TopOfMarginOfBaselineGridRelativeOption" />
</idPkg:Preferences>"#;

const STYLES: &str = r#"<idPkg:Styles xmlns:idPkg="p">
 <RootCharacterStyleGroup Self="c">
  <CharacterStyle Self="CharacterStyle/$ID/[No character style]" Name="$ID/[No character style]" />
  <CharacterStyle Self="CharacterStyle/Strong" Name="Strong" FontStyle="Bold" FillColor="Color/Red">
   <Properties><BasedOn type="string">$ID/[No character style]</BasedOn></Properties>
  </CharacterStyle>
 </RootCharacterStyleGroup>
 <RootParagraphStyleGroup Self="p">
  <ParagraphStyle Self="ParagraphStyle/$ID/[No paragraph style]" Name="$ID/[No paragraph style]" FillColor="Color/Black" FontStyle="Regular" PointSize="12" Justification="LeftAlign" Hyphenation="true" MinimumWordSpacing="80" DesiredWordSpacing="100" MaximumWordSpacing="133" AutoLeading="120" Tracking="0" HorizontalScale="100" KerningMethod="$ID/Metrics" FillTint="-1" StrokeColor="Swatch/None" AppliedLanguage="$ID/English: USA" GridAlignment="None">
   <Properties><Leading type="enumeration">Auto</Leading><TabList type="list" /><AppliedFont type="string">Minion Pro</AppliedFont></Properties>
  </ParagraphStyle>
  <ParagraphStyle Self="ParagraphStyle/Body" Name="Body" PointSize="9" Tracking="-25" AutoLeading="140" Justification="CenterAlign" KerningMethod="$ID/Optical" Hyphenation="false">
   <Properties><BasedOn type="string">$ID/[No paragraph style]</BasedOn><Leading type="unit">14</Leading><AppliedFont type="string">Pretendard</AppliedFont>
    <TabList type="list"><ListItem type="record"><Alignment type="enumeration">RightAlign</Alignment><AlignmentCharacter type="string">.</AlignmentCharacter><Leader type="string"> </Leader><Position type="unit">72</Position></ListItem></TabList>
   </Properties>
  </ParagraphStyle>
  <ParagraphStyle Self="ParagraphStyle/Group%3aSub" Name="Group:Sub" Justification="LeftJustified" SpaceAfter="6" GridAlignment="AlignBaseline">
   <Properties><BasedOn type="object">ParagraphStyle/Body</BasedOn></Properties>
  </ParagraphStyle>
  <ParagraphStyle Self="ParagraphStyle/Ruled" Name="Ruled" RuleAbove="true"><Properties><BasedOn type="object">ParagraphStyle/Body</BasedOn></Properties></ParagraphStyle>
 </RootParagraphStyleGroup>
 <RootObjectStyleGroup Self="o">
  <ObjectStyle Self="ObjectStyle/$ID/[None]" Name="$ID/[None]" FillColor="Swatch/None" FillTint="-1" StrokeColor="Swatch/None" StrokeTint="-1" StrokeWeight="0" StrokeAlignment="CenterAlignment">
   <TextFramePreference TextColumnCount="1" TextColumnGutter="12" TextColumnFixedWidth="144" UseFixedColumnWidth="false" FirstBaselineOffset="AscentOffset" MinimumFirstBaselineOffset="0" VerticalJustification="TopAlign" IgnoreWrap="false">
    <Properties><InsetSpacing type="list"><ListItem type="unit">0</ListItem><ListItem type="unit">0</ListItem><ListItem type="unit">0</ListItem><ListItem type="unit">0</ListItem></InsetSpacing></Properties>
   </TextFramePreference>
   <TextWrapPreference Inverse="false" ApplyToMasterPageOnly="false" TextWrapSide="BothSides" TextWrapMode="None"><Properties><TextWrapOffset Top="0" Left="0" Bottom="0" Right="0" /></Properties></TextWrapPreference>
  </ObjectStyle>
  <ObjectStyle Self="ObjectStyle/Boxed" Name="Boxed" EnableFill="false" FillColor="Color/Red" EnableStroke="true" StrokeColor="Color/Black" StrokeWeight="2" EnableStrokeAndCornerOptions="true" StrokeAlignment="InsideAlignment" EnableTextFrameGeneralOptions="true" EnableTextFrameBaselineOptions="false" EnableTextWrapAndOthers="true">
   <Properties><BasedOn type="string">$ID/[None]</BasedOn></Properties>
   <TextFramePreference TextColumnCount="2" FirstBaselineOffset="CapHeight" VerticalJustification="BottomAlign"><Properties><InsetSpacing type="unit">6</InsetSpacing></Properties></TextFramePreference>
   <TextWrapPreference TextWrapMode="BoundingBoxTextWrap" TextWrapSide="RightSide"><Properties><TextWrapOffset Top="1" Left="2" Bottom="3" Right="4" /></Properties></TextWrapPreference>
  </ObjectStyle>
 </RootObjectStyleGroup>
</idPkg:Styles>"#;

const MASTER: &str = r#"<idPkg:MasterSpread xmlns:idPkg="p"><MasterSpread Self="m" Name="A-Master" ItemTransform="1 0 0 1 0 0">
  <Page Self="mp" Name="A" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 0 -396" AppliedMaster="n" MasterPageTransform="1 0 0 1 0 0" />
  <TextFrame Self="mtf" ParentStory="st2" PreviousTextFrame="n" NextTextFrame="n" ItemTransform="1 0 0 1 0 0" ItemLayer="top" GeometricBounds="0 0 20 100" />
</MasterSpread></idPkg:MasterSpread>"#;

const RECT_PATH: &str = r#"<Properties><PathGeometry><GeometryPathType PathOpen="false"><PathPointArray>
  <PathPointType Anchor="0 0" LeftDirection="0 0" RightDirection="0 0" /><PathPointType Anchor="0 100" LeftDirection="0 100" RightDirection="0 100" />
  <PathPointType Anchor="200 100" LeftDirection="200 100" RightDirection="200 100" /><PathPointType Anchor="200 0" LeftDirection="200 0" RightDirection="200 0" />
</PathPointArray></GeometryPathType></PathGeometry></Properties>"#;

fn spread() -> String {
    format!(
        r#"<idPkg:Spread xmlns:idPkg="p"><Spread Self="s" ItemTransform="1 0 0 1 0 0">
  <FlattenerPreference />
  <Page Self="p1" Name="1" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 -612 -396" AppliedMaster="m" MasterPageTransform="1 0 0 1 0 0" />
  <Page Self="p2" Name="2" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 0 -396" AppliedMaster="missing" />
  <TextFrame Self="tf1" ParentStory="st" PreviousTextFrame="n" NextTextFrame="tf2" AppliedObjectStyle="ObjectStyle/Boxed" ItemLayer="top" ItemTransform="1 0 0 1 10 20">
    {RECT_PATH}
    <TextFramePreference TextColumnGutter="9" />
  </TextFrame>
  <Group Self="g" ItemLayer="bottom" FillColor="Color/Black" ItemTransform="2 0 0 2 0 0">
    <TextFrame Self="tf2" ParentStory="st" PreviousTextFrame="tf1" NextTextFrame="n" FillColor="Tint/Black 50" FillTint="40" ItemTransform="1 0 0 1 0 0">{RECT_PATH}</TextFrame>
    <Oval Self="ov" GeometricBounds="0 0 50 100" FillColor="Gradient/G" GradientFillStart="5 6" GradientFillLength="70" GradientFillAngle="45" Visible="false" />
  </Group>
  <Rectangle Self="r" FillColor="Color/Lab" StrokeColor="Color/Black" StrokeWeight="1" StrokeType="StrokeStyle/$ID/Dashed" ItemTransform="1 0 0 1 0 300">
    {RECT_PATH}
    <TransparencySetting><BlendingSetting BlendMode="Multiply" Opacity="40" /><DropShadowSetting Mode="Drop" /></TransparencySetting>
    <Image Self="img" ItemTransform="0.5 0 0 0.5 1 2" ActualPpi="300 300">
      <Properties><GraphicBounds Left="0" Top="0" Right="400" Bottom="200" /></Properties>
      <ClippingPathSettings ClippingType="None" />
      <Link Self="l" LinkResourceURI="file:/Users/me/photo.jpg" />
    </Image>
    <Group Self="pasted" ItemTransform="1 0 0 1 0 0"><GraphicLine Self="line" StrokeColor="Color/Black" StrokeWeight="0.5" GeometricBounds="0 0 0 100" /></Group>
  </Rectangle>
  <EPSText Self="eps" ItemTransform="1 0 0 1 0 0" />
  <Widget Self="w" />
</Spread></idPkg:Spread>"#
    )
}

const STORY: &str = "<idPkg:Story xmlns:idPkg=\"p\"><Story Self=\"st\">
  <StoryPreference StoryOrientation=\"Horizontal\" StoryDirection=\"LeftToRightDirection\" />
  <ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Group%3aSub\" SpaceBefore=\"3\">
    <CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\" PointSize=\"10\">
      <Content>Hello\u{2028}World\u{FEFF} </Content><Content><?ACE 18?></Content>
      <HyperlinkTextSource Self=\"h\"><Content>&amp;link</Content></HyperlinkTextSource>
      <Change ChangeType=\"DeletedText\"><Content>GONE</Content></Change>
      <Br />
    </CharacterStyleRange>
    <CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Strong\" KerningValue=\"-50\">
      <Properties><Leading type=\"unit\">20</Leading></Properties>
      <Content>Bold</Content>
      <Rectangle Self=\"anch\" ItemTransform=\"1 0 0 1 0 0\" GeometricBounds=\"0 0 10 10\" FillColor=\"Color/Red\" />
      <Table Self=\"t\"><Cell><Content>skip me</Content></Cell></Table>
    </CharacterStyleRange>
    <CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\" PointSize=\"30\"><Br /></CharacterStyleRange>
  </ParagraphStyleRange>
  <ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Ruled\">
    <CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\" FillColor=\"Color/Nowhere\"><Content>x\ty</Content><Br /></CharacterStyleRange>
  </ParagraphStyleRange>
</Story></idPkg:Story>";

const STORY2: &str = r#"<idPkg:Story xmlns:idPkg="p"><Story Self="st2"></Story></idPkg:Story>"#;

fn package(dir: &TempDir) -> (PathBuf, PathBuf) {
    let idml = dir.0.join("doc.idml");
    let spread = spread();
    zip(
        &idml,
        &[
            ("mimetype", "application/vnd.adobe.indesign-idml-package"),
            ("designmap.xml", DESIGNMAP),
            ("Resources/Graphic.xml", GRAPHIC),
            ("Resources/Fonts.xml", FONTS),
            ("Resources/Styles.xml", STYLES),
            ("Resources/Preferences.xml", PREFS),
            ("MasterSpreads/MasterSpread_m.xml", MASTER),
            ("Spreads/Spread_s.xml", &spread),
            ("Stories/Story_st.xml", STORY),
            ("Stories/Story_st2.xml", STORY2),
        ],
    );
    std::fs::write(dir.0.join("preview-0.bin"), b"jpeg").unwrap();
    let result = dir.0.join("result.json");
    std::fs::write(
        &result,
        r#"{"parser":"test-reader","warnings":["reader note"],"previews":[{"file":"preview-0.bin","graphics":["img"]},{"file":"../escape.bin","graphics":["x"]}]}"#,
    )
    .unwrap();
    (idml, result)
}

fn find<'a>(doc: &'a Document, id: &str) -> &'a PageItem {
    let mut found = None;
    folio_idml::for_each_item(doc, &mut |i| {
        if i.id == id {
            found = Some(i);
        }
    });
    found.unwrap_or_else(|| panic!("item {id}"))
}

#[test]
fn reads_a_small_package() {
    let dir = TempDir::new("small");
    let (idml, result) = package(&dir);
    let doc = read_package(&idml, Some(&result)).unwrap();
    let warn = |s: &str| doc.warnings.iter().any(|w| w.contains(s));

    // Source, layers (IDML lists bottom first: oracle align-page-bottom), preferences, fonts.
    assert_eq!(doc.source.reader, "test-reader + folio-idml");
    assert_eq!(doc.source.dom_version, "19.4");
    assert_eq!(doc.layers.iter().map(|l| (l.id.as_str(), l.order, l.visible, l.printable)).collect::<Vec<_>>(), [
        ("bottom", 0, false, false),
        ("top", 1, true, true)
    ]);
    assert_eq!(doc.text_preferences.small_cap, 75.0);
    assert_eq!(doc.text_preferences.baseline_division, 13.0);
    assert!(doc.text_preferences.z_order_text_wrap);
    assert_eq!(doc.font_table.len(), 1);
    assert_eq!(doc.font_table[0].postscript_name, "MinionPro-Regular");
    assert_eq!(doc.font_table[0].status, "Installed");

    // Swatches and gradients.
    let s = &doc.swatches;
    assert_eq!(s["Color/Paper"].model, ColorModel::Paper);
    assert_eq!(s["Color/Paper"].display_rgb, [255, 255, 255]);
    assert_eq!(s["Color/Registration"].display_rgb, [0, 0, 0]);
    assert_eq!(s["Color/Red"].model, ColorModel::Spot);
    assert_eq!(s["Color/Red"].display_rgb, [255, 0, 0]);
    assert_eq!(s["Swatch/None"].model, ColorModel::None);
    let tint = &s["Tint/Black 50"];
    // A CMYK tint scales the ink: 50 % black is K50 (display_rgb is profile-free).
    assert_eq!(
        (tint.base.as_deref(), tint.tint, tint.display_rgb),
        (Some("Color/Black"), Some(50.0), folio_color::Colors::unmanaged().cmyk_to_srgb(0.0, 0.0, 0.0, 50.0))
    );
    let lab = s["Color/Lab"].display_rgb;
    assert!(lab[0] == lab[1] && lab[1] == lab[2] && (lab[0] as i32 - 119).abs() <= 1, "{lab:?}");
    let g = &doc.gradients["Gradient/G"];
    assert_eq!(g.kind, GradientKind::Radial);
    assert_eq!(g.stops.len(), 2);
    assert_eq!((g.stops[0].midpoint, g.stops[1].midpoint, g.stops[1].location), (50.0, 30.0, 100.0));

    // Pages.
    assert_eq!(doc.master_spreads[0].name.as_deref(), Some("A-Master"));
    let pages = &doc.spreads[0].pages;
    assert_eq!(pages.iter().map(|p| (p.index, p.master.as_deref())).collect::<Vec<_>>(), [(0, Some("m")), (1, Some("missing"))]);
    assert!(warn("page 2 uses missing master missing"));
    assert_eq!(pages[0].transform.tx, -612.0);

    // Text frame with object style "Boxed": fill disabled in the style → [None]; stroke and
    // frame options from Boxed; gutter from the item; baseline options disabled → [None].
    let tf1 = find(&doc, "tf1");
    assert_eq!(tf1.fill, Paint::None);
    assert_eq!(tf1.stroke.paint, Paint::Swatch { id: "Color/Black".into(), tint: 100.0 });
    assert_eq!((tf1.stroke.weight, tf1.stroke.alignment), (2.0, StrokeAlignment::Inside));
    assert_eq!((tf1.transform.tx, tf1.transform.ty), (10.0, 20.0));
    assert_eq!(tf1.paths[0].points.len(), 4);
    let ItemKind::TextFrame(f) = &tf1.kind else { panic!() };
    assert_eq!((f.columns, f.gutter, f.insets), (2, 9.0, [6.0; 4]));
    assert_eq!(f.first_baseline, FirstBaseline::Ascent);
    assert_eq!(f.vertical_justification, VerticalJustification::Bottom);
    assert_eq!((f.story.as_str(), f.next.as_deref(), f.previous.as_deref()), ("st", Some("tf2"), None));
    let wrap = tf1.text_wrap.as_ref().unwrap();
    assert!(tf1.wraps);
    assert_eq!((wrap.mode, wrap.side, wrap.offsets), (TextWrapMode::BoundingBox, TextWrapSide::RightSide, [1.0, 2.0, 3.0, 4.0]));

    // Group: children kept with their own transforms; a group itself paints nothing.
    let group = find(&doc, "g");
    assert_eq!(group.fill, Paint::None);
    assert_eq!(group.layer.as_deref(), Some("bottom"));
    let ItemKind::Group { children } = &group.kind else { panic!() };
    assert_eq!(children.len(), 2);
    let tf2 = find(&doc, "tf2");
    assert_eq!(tf2.fill, Paint::Swatch { id: "Tint/Black 50".into(), tint: 40.0 });
    assert!(!tf2.wraps);
    let oval = find(&doc, "ov");
    assert!(!oval.visible);
    assert!(matches!(&oval.fill, Paint::Gradient { id, .. } if id == "Gradient/G"));
    let gg = oval.fill_gradient.unwrap();
    assert_eq!((gg.start.x, gg.start.y, gg.length, gg.angle), (5.0, 6.0, 70.0, 45.0));
    assert_eq!(oval.paths[0].points.len(), 4);
    assert!(!oval.paths[0].open);

    // Graphic frame with an image and a group pasted inside.
    let r = find(&doc, "r");
    assert!((r.opacity - 0.4).abs() < 1e-9);
    let ItemKind::Rectangle { content: Some(img) } = &r.kind else { panic!() };
    assert_eq!((img.element.as_str(), img.bounds.right, img.bounds.bottom), ("Image", 400.0, 200.0));
    assert_eq!(img.transform.a, 0.5);
    assert_eq!(img.link_uri.as_deref(), Some("file:/Users/me/photo.jpg"));
    assert_eq!(img.actual_ppi, Some((300.0, 300.0)));
    let preview = std::path::Path::new(img.preview.as_deref().unwrap());
    assert!(preview.is_absolute() && preview.is_file());
    assert_eq!(r.clipped.len(), 1);
    assert!(find(&doc, "line").paths[0].open);
    assert_eq!(r.blend_mode, folio_model::BlendMode::Multiply);
    assert!(!warn("blend mode Multiply (object) not modelled"));
    // a drop shadow and a dashed stroke are modelled: no warning, the dashes lowered
    assert!(!warn("effect DropShadowSetting (object) not modelled"));
    assert!(r.print.fx.drop_shadow.is_some());
    assert!(!doc.warnings.iter().any(|w| w.contains("stroke type")));
    assert_eq!(r.stroke.style.as_deref(), Some("StrokeStyle/$ID/Dashed"));
    assert!(r.stroke.art.is_some());
    assert!(warn("page item EPSText not modelled (kept as Other)"));
    assert!(warn("element Widget inside Spread not read"));
    assert!(warn("reader preview path that is not a plain file name ignored"));
    assert!(warn("reader: reader note"));
    assert!(matches!(&find(&doc, "eps").kind, ItemKind::Other { element } if element == "EPSText"));

    // Story text and resolved attributes.
    let st = &doc.stories["st"];
    assert_eq!(st.paragraphs.len(), 4, "{:#?}", st.paragraphs);
    let p0 = &st.paragraphs[0];
    assert_eq!(p0.style, "Group:Sub");
    assert_eq!(p0.attrs.justification, Justification::LeftJustified);
    assert_eq!((p0.attrs.space_after, p0.attrs.space_before), (6.0, 3.0));
    assert!(!p0.attrs.hyphenation);
    assert!(p0.attrs.align_to_baseline);
    assert_eq!(p0.attrs.word_spacing.max, 133.0);
    assert_eq!(p0.attrs.tabs.len(), 1);
    assert_eq!((p0.attrs.tabs[0].position, p0.attrs.tabs[0].leader.as_str()), (72.0, " "));
    let text: String = p0.runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(text, "Hello\nWorld #&link");
    let r0 = &p0.runs[0];
    assert_eq!((r0.attrs.font_family.as_str(), r0.attrs.font_style.as_str()), ("Pretendard", "Regular"));
    assert_eq!((r0.attrs.point_size, r0.attrs.tracking, r0.attrs.leading), (10.0, -25.0, Leading::Fixed(14.0)));
    assert_eq!(r0.attrs.auto_leading, 140.0);
    assert_eq!(r0.attrs.kerning_method, folio_model::KerningMethod::Optical);
    assert_eq!(r0.attrs.fill, Paint::Swatch { id: "Color/Black".into(), tint: 100.0 });
    assert_eq!(r0.attrs.language, "English: USA");
    let marker = p0.runs.iter().find(|r| r.variable.is_some()).unwrap();
    assert_eq!((marker.text.as_str(), marker.variable), ("#", Some(TextVariable::PageNumber)));

    let p1 = &st.paragraphs[1];
    let bold = &p1.runs[0];
    // the anchored rectangle (U+FFFC) and the table anchor (U+0016) each take one character
    assert_eq!(bold.text, "Bold\u{FFFC}\u{16}");
    assert_eq!(bold.inline.len(), 1);
    assert_eq!(bold.tables.len(), 1);
    assert_eq!(bold.inline[0].id, "anch");
    assert_eq!((bold.attrs.font_style.as_str(), bold.attrs.character_style.as_str()), ("Bold", "Strong"));
    assert_eq!(bold.attrs.fill, Paint::Swatch { id: "Color/Red".into(), tint: 100.0 });
    assert_eq!((bold.attrs.leading, bold.attrs.kerning_value), (Leading::Fixed(20.0), -50.0));
    // The return of paragraph 1 sits in a 30 pt range: an empty run carries it.
    let ret = p1.runs.last().unwrap();
    assert_eq!((ret.text.as_str(), ret.attrs.point_size), ("", 30.0));
    assert!(!warn("Table not modelled (content skipped)"));

    let p2 = &st.paragraphs[2];
    assert_eq!(p2.style, "Ruled");
    assert_eq!(p2.runs[0].text, "x\ty");
    assert_eq!(p2.runs[0].attrs.fill, Paint::None);
    assert!(warn("missing swatch Color/Nowhere"));
    assert!(!warn("paragraph rule above not modelled"));
    assert!(p2.attrs.rule_above.is_some());
    // Final return → trailing empty paragraph with the attributes of that return.
    let p3 = &st.paragraphs[3];
    assert_eq!(p3.runs.len(), 1);
    assert!(p3.runs[0].text.is_empty());
    // 18 + "Bold", U+FFFC and the table anchor 6 + "x\ty" 3, plus three paragraph returns.
    assert_eq!(folio_idml::story_chars(st), 18 + 6 + 3 + 3);

    // Empty story: one empty paragraph in the root style.
    let st2 = &doc.stories["st2"];
    assert_eq!(st2.paragraphs.len(), 1);
    assert_eq!(st2.paragraphs[0].style, "[No paragraph style]");
    assert_eq!(st2.paragraphs[0].runs[0].attrs.font_family, "Minion Pro");
    assert_eq!(folio_idml::story_chars(st2), 0);

    assert!(doc.fonts_used.contains(&("Pretendard".into(), "Bold".into())));
    assert!(!doc.fonts_used.iter().any(|(f, _)| f == "Minion Pro"));
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let dir = TempDir::new("bad");
    let not_zip = dir.0.join("not.idml");
    std::fs::write(&not_zip, b"hello").unwrap();
    assert!(matches!(read_package(&not_zip, None), Err(Error::Package(_))));
    assert!(matches!(read_package(&dir.0.join("absent.idml"), None), Err(Error::Io(_))));

    let no_map = dir.0.join("nomap.idml");
    zip(&no_map, &[("mimetype", "application/vnd.adobe.indesign-idml-package")]);
    assert!(matches!(read_package(&no_map, None), Err(Error::Package(_))));

    let wrong_type = dir.0.join("wrong.idml");
    zip(&wrong_type, &[("mimetype", "application/zip"), ("designmap.xml", DESIGNMAP)]);
    assert!(matches!(read_package(&wrong_type, None), Err(Error::Package(_))));

    let broken = dir.0.join("broken.idml");
    zip(&broken, &[("designmap.xml", DESIGNMAP), ("Stories/Story_st.xml", "<Story><Content>x</Story>")]);
    assert!(matches!(read_package(&broken, None), Err(Error::Xml(_))));

    // Parts the designmap names but the ZIP lacks are warnings, not errors.
    let sparse = dir.0.join("sparse.idml");
    zip(&sparse, &[("designmap.xml", DESIGNMAP)]);
    let doc = read_package(&sparse, None).unwrap();
    assert!(doc.warnings.iter().any(|w| w.contains("Story part Stories/Story_st.xml missing from package")));
    assert!(doc.spreads.is_empty() && doc.stories.is_empty());

    let bad_json = dir.0.join("result.json");
    std::fs::write(&bad_json, "{").unwrap();
    assert!(matches!(read_package(&sparse, Some(&bad_json)), Err(Error::ReaderResult(_))));
}

/// Content the INDD reader used to drop, as InDesign's IDML carries it: conditions (text
/// whose conditions are all hidden is left out, hidden text too), a QR code lowered to
/// vector modules, custom anchored-object settings, kenten, paragraph direction and the
/// font table's writing scripts.
#[test]
fn reads_conditions_qr_codes_anchors_kenten_direction_and_writing_scripts() {
    let dir = TempDir::new("drops");
    let idml = dir.0.join("drops.idml");
    let designmap = DESIGNMAP.replace(
        r#"<Layer Self="top""#,
        r#"<Condition Self="Condition/Hidden" Name="Hidden" Visible="false" />
  <Condition Self="Condition/Shown%20Too" Name="Shown Too" Visible="true" />
  <Layer Self="top""#,
    );
    let fonts = FONTS.replace(
        r#"Status="Installed" />"#,
        r#"Status="Installed" WritingScript="0" />
  <Font Self="f2" FontFamily="Missing Gothic" Name="Missing Gothic Regular" PostScriptName="MissingGothic-Regular" FontStyleName="Regular" FontType="OpenTypeCID" FullName="Missing Gothic" Status="NotAvailable" WritingScript="3" />"#,
    );
    let story = r#"<idPkg:Story xmlns:idPkg="p"><Story Self="st">
  <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Body" ParagraphDirection="RightToLeftDirection" Composer="HL Composer J">
    <CharacterStyleRange AppliedCharacterStyle="CharacterStyle/$ID/[No character style]" KentenKind="KentenBlackCircle" KentenFontSize="6" KentenPosition="BelowLeft">
      <Properties><KentenFillColor type="object">Color/Red</KentenFillColor></Properties><Content>seen</Content></CharacterStyleRange>
    <CharacterStyleRange AppliedCharacterStyle="CharacterStyle/$ID/[No character style]" AppliedConditions="Condition/Hidden"><Content>hidden</Content></CharacterStyleRange>
    <CharacterStyleRange AppliedCharacterStyle="CharacterStyle/$ID/[No character style]" AppliedConditions="Condition/Hidden Condition/Shown%20Too"><Content>both</Content></CharacterStyleRange>
    <CharacterStyleRange AppliedCharacterStyle="CharacterStyle/$ID/[No character style]">
      <HiddenText><ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Body"><CharacterStyleRange AppliedCharacterStyle="CharacterStyle/$ID/[No character style]"><Content>secret</Content></CharacterStyleRange></ParagraphStyleRange></HiddenText>
      <Content>x</Content>
      <Rectangle Self="anch" ItemTransform="1 0 0 1 0 0" GeometricBounds="0 0 10 20">
        <AnchoredObjectSetting AnchoredPosition="Anchored" AnchorPoint="TopRightAnchor" HorizontalReferencePoint="TextFrame" HorizontalAlignment="RightAlign" VerticalReferencePoint="LineBaseline" VerticalAlignment="TopAlign" AnchorXoffset="15" AnchorYoffset="3" PinPosition="false" SpineRelative="true" />
      </Rectangle>
    </CharacterStyleRange>
  </ParagraphStyleRange>
</Story></idPkg:Story>"#;
    let spread = spread().replace(
        r#"<EPSText Self="eps""#,
        r#"<Rectangle Self="qrframe" ItemTransform="1 0 0 1 0 500" StrokeWeight="0">
    <EPS Self="qreps" ItemTransform="1 0 0 1 0 0">
      <Properties><GraphicBounds Left="0" Top="0" Right="85.039" Bottom="85.039" /></Properties>
      <PlainTextQRCode Self="qrcode" PlainText="FOLIO"><Properties><QrCodeSwatch type="string">Red</QrCodeSwatch></Properties></PlainTextQRCode>
    </EPS>
  </Rectangle>
  <EPSText Self="eps""#,
    );
    zip(
        &idml,
        &[
            ("mimetype", "application/vnd.adobe.indesign-idml-package"),
            ("designmap.xml", &designmap),
            ("Resources/Graphic.xml", GRAPHIC),
            ("Resources/Fonts.xml", &fonts),
            ("Resources/Styles.xml", STYLES),
            ("Resources/Preferences.xml", PREFS),
            ("MasterSpreads/MasterSpread_m.xml", MASTER),
            ("Spreads/Spread_s.xml", &spread),
            ("Stories/Story_st.xml", story),
            ("Stories/Story_st2.xml", STORY2),
        ],
    );
    let doc = read_package(&idml, None).expect("package reads");

    // Conditions: "hidden" (its only condition hidden) and the hidden text are left out;
    // "both" has a visible condition too and stays.
    let p = &doc.stories["st"].paragraphs[0];
    let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(text, "seenbothx\u{FFFC}");
    assert!(p.attrs.right_to_left);
    assert_eq!(p.attrs.composer, "HL Composer J");

    // Kenten of the first run, in the swatch given.
    let k = p.runs[0].attrs.kenten.as_ref().expect("kenten");
    assert_eq!(k.kind, folio_model::KentenKind::BlackCircle);
    assert_eq!(k.size, Some(6.0));
    assert!(k.below);
    assert!(matches!(&k.paint, Some(Paint::Swatch { id, .. }) if id == "Color/Red"));
    assert!(p.runs[1].attrs.kenten.is_none(), "runs without kenten have none");

    // The anchored rectangle keeps its custom position.
    let item = p.runs.iter().flat_map(|r| &r.inline).find(|i| i.id == "anch").expect("anchored item");
    let a = item.anchor.expect("anchor settings");
    assert_eq!(a.position, folio_model::AnchorPosition::Custom);
    assert_eq!(a.point, (1.0, 0.0));
    assert_eq!((a.horizontal_reference, a.horizontal_alignment), (folio_model::AnchorReference::TextFrame, folio_model::AnchorAlign::End));
    assert_eq!(a.vertical_reference, folio_model::AnchorReference::LineBaseline);
    assert_eq!((a.x_offset, a.y_offset, a.pin, a.spine_relative), (15.0, 3.0, false, true));

    // The QR code: vector modules in its swatch, drawn instead of the EPS, not a link.
    let frame = find(&doc, "qrframe");
    assert!(matches!(frame.kind, ItemKind::Rectangle { content: None }));
    let qr = &frame.clipped[0];
    assert_eq!(qr.id, "qrcode");
    assert!(qr.print.placed_art);
    assert!(matches!(&qr.fill, Paint::Swatch { id, tint } if id == "Color/Red" && *tint == 100.0));
    let b = folio_model::paths_bounds(&qr.paths).expect("modules");
    // Version 1 (21 modules) in 85.039 pt with a 4-module quiet zone: modules from 4 × 85.039 / 29.
    let m = 85.039 / 29.0;
    assert!((b.left - 4.0 * m).abs() < 1e-6 && (b.right - 25.0 * m).abs() < 1e-6, "{b:?}");
    assert!(!doc.warnings.iter().any(|w| w.starts_with("EPS without preview")), "{:?}", doc.warnings);

    // Writing scripts of the font table.
    let ws: Vec<Option<u32>> = doc.font_table.iter().map(|f| f.writing_script).collect();
    assert_eq!(ws, vec![Some(0), Some(3)]);
}
