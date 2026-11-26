//! Facts the editor's canvas and panels show: guides, columns, layer colours and locks, item
//! names and locks, master names, the Swatches panel order, style groups and view settings.

use folio_idml::read_package;
use folio_model::{Document, PageItem, UiColor};
use std::io::Write;
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("folio-idml-editor-{}-{name}", std::process::id()));
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

fn designmap(color_groups: bool) -> String {
    let groups = if color_groups {
        r#"<ColorGroup Self="ColorGroup/[Root Color Group]" Name="[Root Color Group]" IsRootColorGroup="true">
    <ColorGroupSwatch Self="c0" SwatchItemRef="Swatch/None" />
    <ColorGroupSwatch Self="c1" SwatchItemRef="Color/Registration" />
    <ColorGroupSwatch Self="c2" SwatchItemRef="Color/Paper" />
    <ColorGroupSwatch Self="c3" SwatchItemRef="Color/Black" />
    <ColorGroupSwatch Self="c4" SwatchItemRef="Gradient/G" />
    <ColorGroupSwatch Self="c5" SwatchItemRef="Color/Unknown" />
  </ColorGroup>
  <ColorGroup Self="ColorGroup/u1" Name="Brand" IsRootColorGroup="false">
    <ColorGroupSwatch Self="c6" SwatchItemRef="Tint/Red 50%25" />
    <ColorGroupSwatch Self="c7" SwatchItemRef="Color/Red" />
  </ColorGroup>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Document xmlns:idPkg="http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging" DOMVersion="20.4" Self="d">
  <idPkg:Graphic src="Resources/Graphic.xml" />
  <idPkg:Styles src="Resources/Styles.xml" />
  <idPkg:Preferences src="Resources/Preferences.xml" />
  {groups}
  <Layer Self="top" Name="Text" Visible="true" Locked="true" ShowGuides="true" LockGuides="true" Printable="true">
    <Properties><LayerColor type="enumeration">Red</LayerColor></Properties>
  </Layer>
  <Layer Self="bottom" Name="Art" Visible="true" Locked="false" Printable="true">
    <Properties><LayerColor type="list"><ListItem type="double">10</ListItem><ListItem type="double">20.4</ListItem><ListItem type="double">255</ListItem></LayerColor></Properties>
  </Layer>
  <Layer Self="plain" Name="Plain" />
  <idPkg:MasterSpread src="MasterSpreads/MasterSpread_m.xml" />
  <idPkg:Spread src="Spreads/Spread_s.xml" />
</Document>"#
    )
}

const GRAPHIC: &str = r#"<idPkg:Graphic xmlns:idPkg="p" DOMVersion="20.4">
  <Color Self="Color/Black" Model="Process" Space="CMYK" ColorValue="0 0 0 100" Name="Black" Visible="true" />
  <Color Self="Color/Cyan" Model="Process" Space="CMYK" ColorValue="100 0 0 0" Name="Cyan" Visible="false" />
  <Color Self="Color/Paper" Model="Process" Space="CMYK" ColorValue="0 0 0 0" Name="Paper" ColorOverride="Specialpaper" Visible="true" />
  <Color Self="Color/Red" Model="Spot" Space="RGB" ColorValue="255 0 0" Name="Red" Visible="true" />
  <Color Self="Color/Registration" Model="Registration" Space="CMYK" ColorValue="100 100 100 100" Name="Registration" Visible="true" />
  <Tint Self="Tint/Red 50%25" BaseColor="Color/Red" TintValue="50" Name="Red 50%" Visible="true" />
  <Swatch Self="Swatch/None" Name="None" Visible="true" />
  <Gradient Self="Gradient/G" Type="Linear" Name="G" Visible="true">
    <GradientStop Self="g0" StopColor="Color/Paper" Location="0" />
    <GradientStop Self="g1" StopColor="Color/Black" Location="100" />
  </Gradient>
</idPkg:Graphic>"#;

fn preferences(ruler: &str) -> String {
    format!(
        r#"<idPkg:Preferences xmlns:idPkg="p">
  <DocumentPreference FacingPages="true" ColumnGuideLocked="false" />
  <ViewPreference HorizontalMeasurementUnits="Millimeters" VerticalMeasurementUnits="Picas" RulerOrigin="{ruler}" StrokeMeasurementUnits="Millimeters" CursorKeyIncrement="0.5" GuideSnaptoZone="8" />
  <TextPreference LeadingKeyIncrement="1" BaselineShiftKeyIncrement="0.5" KerningKeyIncrement="50" />
  <PasteboardPreference PasteboardMargins="30 40" MatchPreviewBackgroundToThemeColor="true">
    <Properties><PreviewBackgroundColor type="enumeration">Charcoal</PreviewBackgroundColor><BleedGuideColor type="list"><ListItem type="double">1</ListItem><ListItem type="double">2</ListItem><ListItem type="double">3</ListItem></BleedGuideColor></Properties>
  </PasteboardPreference>
  <GuidePreference GuidesInBack="true" GuidesShown="true" GuidesLocked="false" GuidesSnapto="false" RulerGuidesViewThreshold="50"><Properties><RulerGuidesColor type="enumeration">Magenta</RulerGuidesColor></Properties></GuidePreference>
  <GridPreference BaselineStart="20" BaselineDivision="14" BaselineViewThreshold="50" BaselineGridRelativeOption="TopOfMarginOfBaselineGridRelativeOption" BaselineGridShown="true" DocumentGridShown="true" HorizontalGridlineDivision="36" VerticalGridlineDivision="18" HorizontalGridSubdivision="4" VerticalGridSubdivision="2" GridsInBack="false">
    <Properties><GridColor type="enumeration">Red</GridColor><BaselineColor type="list"><ListItem type="double">1</ListItem><ListItem type="double">2.5</ListItem><ListItem type="double">3</ListItem></BaselineColor></Properties>
  </GridPreference>
</idPkg:Preferences>"#
    )
}

const STYLES: &str = r#"<idPkg:Styles xmlns:idPkg="p">
 <RootCharacterStyleGroup Self="c">
  <CharacterStyle Self="CharacterStyle/$ID/[No character style]" Name="$ID/[No character style]" />
  <CharacterStyleGroup Self="CharacterStyleGroup/Accents" Name="Accents">
   <CharacterStyle Self="CharacterStyle/Accents%3aStrong" Name="Accents:Strong" />
  </CharacterStyleGroup>
 </RootCharacterStyleGroup>
 <RootParagraphStyleGroup Self="p">
  <ParagraphStyle Self="ParagraphStyle/Body" Name="Body" />
  <ParagraphStyleGroup Self="ParagraphStyleGroup/$ID/Heads" Name="$ID/Heads">
   <ParagraphStyleGroup Self="ParagraphStyleGroup/Heads%3aSub" Name="Sub">
    <ParagraphStyle Self="ParagraphStyle/Heads%3aSub%3aTitle" Name="Heads:Sub:Title" />
   </ParagraphStyleGroup>
  </ParagraphStyleGroup>
  <ParagraphStyle Self="ParagraphStyle/$ID/[No paragraph style]" Name="$ID/[No paragraph style]" />
 </RootParagraphStyleGroup>
 <RootObjectStyleGroup Self="o">
  <ObjectStyle Self="ObjectStyle/$ID/[None]" Name="$ID/[None]" />
 </RootObjectStyleGroup>
</idPkg:Styles>"#;

const MASTER: &str = r#"<idPkg:MasterSpread xmlns:idPkg="p"><MasterSpread Self="m" Name="B-Chapter" NamePrefix="B" BaseName="Chapter" ItemTransform="1 0 0 1 0 0">
  <Page Self="mp" Name="B" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 0 -396" />
</MasterSpread></idPkg:MasterSpread>"#;

/// Two facing pages (x −612…0 and 0…612, y −396…396). The guides: a vertical one of the
/// right page, a horizontal one of the spread (written in the first page) and a vertical one
/// of the left page.
fn spread(right_guide: f64, left_guide: f64) -> String {
    format!(
        r#"<idPkg:Spread xmlns:idPkg="p"><Spread Self="s" ItemTransform="1 0 0 1 0 0">
  <Page Self="p1" Name="1" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 -612 -396" AppliedMaster="m">
    <MarginPreference ColumnCount="2" ColumnGutter="18" Top="36" Bottom="40" Left="48" Right="24" ColumnDirection="Horizontal" ColumnsPositions="0 261 279 540" />
    <Guide Self="gs" Orientation="Horizontal" Location="100" FitToPage="false" ItemLayer="top" Locked="false" PageIndex="0" GuideType="Ruler" ViewThreshold="5">
      <Properties><GuideColor type="enumeration">Magenta</GuideColor></Properties>
    </Guide>
    <Guide Self="gl" Orientation="Vertical" Location="{left_guide}" FitToPage="true" ItemLayer="bottom" Locked="true" PageIndex="-1" GuideType="Ruler" />
  </Page>
  <Page Self="p2" Name="2" GeometricBounds="0 0 792 612" ItemTransform="1 0 0 1 0 -396" AppliedMaster="m">
    <MarginPreference Top="36" Bottom="36" Left="36" Right="36" />
    <Guide Self="gr" Orientation="Vertical" Location="{right_guide}" FitToPage="true" ItemLayer="bottom" Locked="false" PageIndex="1" GuideType="Liquid">
      <Properties><GuideColor type="list"><ListItem type="double">1</ListItem><ListItem type="double">2</ListItem><ListItem type="double">3</ListItem></GuideColor></Properties>
    </Guide>
  </Page>
  <Rectangle Self="named" Name="Hero image" Locked="true" ItemLayer="bottom" ItemTransform="1 0 0 1 0 0" GeometricBounds="0 0 10 10" />
  <Rectangle Self="unnamed" Name="$ID/" Locked="false" ItemLayer="bottom" ItemTransform="1 0 0 1 0 0" GeometricBounds="0 0 10 10" />
</Spread></idPkg:Spread>"#
    )
}

fn read(name: &str, ruler: &str, color_groups: bool, right_guide: f64, left_guide: f64) -> Document {
    let dir = TempDir::new(name);
    let idml = dir.0.join("doc.idml");
    let (dm, prefs, sp) = (designmap(color_groups), preferences(ruler), spread(right_guide, left_guide));
    zip(
        &idml,
        &[
            ("mimetype", "application/vnd.adobe.indesign-idml-package"),
            ("designmap.xml", &dm),
            ("Resources/Graphic.xml", GRAPHIC),
            ("Resources/Styles.xml", STYLES),
            ("Resources/Preferences.xml", &prefs),
            ("MasterSpreads/MasterSpread_m.xml", MASTER),
            ("Spreads/Spread_s.xml", &sp),
        ],
    );
    read_package(&idml, None).unwrap()
}

fn item<'a>(doc: &'a Document, id: &str) -> &'a PageItem {
    doc.spreads[0].items.iter().find(|i| i.id == id).unwrap()
}

#[test]
fn guides_columns_layers_names_and_locks() {
    // SpreadOrigin: locations from the top-left of the spread's pages (−612, −396).
    let doc = read("spread", "SpreadOrigin", true, 700.0, 300.0);
    let s = &doc.spreads[0];
    let g: Vec<(&str, bool, f64, Option<usize>)> = s.guides.iter().map(|g| (g.id.as_str(), g.horizontal, g.position, g.page)).collect();
    assert_eq!(g, [("gs", true, -296.0, None), ("gl", false, -312.0, Some(0)), ("gr", false, 88.0, Some(1))]);
    let (gs, gl, gr) = (&s.guides[0], &s.guides[1], &s.guides[2]);
    assert_eq!(gs.location, 100.0);
    assert!(!gs.fit_to_page && gl.fit_to_page && gl.locked && !gr.locked);
    assert_eq!(gs.color, UiColor::Named("Magenta".into()));
    assert_eq!(gr.color, UiColor::Rgb([1.0, 2.0, 3.0]));
    // without a colour the samples' Cyan
    assert_eq!(gl.color, UiColor::Named("Cyan".into()));
    assert_eq!(gs.layer.as_deref(), Some("top"));
    assert_eq!(gs.view_threshold, Some(5.0));
    assert_eq!(gr.guide_type.as_deref(), Some("Liquid"));

    // Columns: stored as IDML has them; a page without them gets one column.
    let c = &s.pages[0].columns;
    assert_eq!((c.count, c.gutter, c.direction.as_str()), (2, 18.0, "Horizontal"));
    assert_eq!(c.positions.as_deref(), Some(&[0.0, 261.0, 279.0, 540.0][..]));
    assert_eq!(s.pages[0].margins, [36.0, 48.0, 40.0, 24.0]);
    let c2 = &s.pages[1].columns;
    assert_eq!((c2.count, c2.gutter, c2.positions.is_none()), (1, 12.0, true));

    // Layers: locks, guide settings and colours (a name or an RGB list).
    let top = &doc.layers[0];
    assert!(top.locked && top.show_guides && top.lock_guides);
    assert_eq!(top.color, UiColor::Named("Red".into()));
    assert_eq!(top.color.rgb(), [255, 0, 0]);
    let bottom = &doc.layers[1];
    assert!(!bottom.locked && bottom.show_guides && !bottom.lock_guides);
    assert_eq!(bottom.color, UiColor::Rgb([10.0, 20.4, 255.0]));
    assert_eq!(bottom.color.rgb(), [10, 20, 255]);
    // no colour stored: InDesign's first layer colour
    assert_eq!(doc.layers[2].color, UiColor::Named("LightBlue".into()));
    assert_eq!(UiColor::Named("NoSuchColour".into()).rgb(), [79, 153, 255]);

    // Items: the user's name and the lock; InDesign's own `$ID/` names are not names.
    let named = item(&doc, "named");
    assert_eq!(named.name.as_deref(), Some("Hero image"));
    assert!(named.locked);
    let unnamed = item(&doc, "unnamed");
    assert_eq!(unnamed.name, None);
    assert!(!unnamed.locked);

    // Masters, view settings, facing pages.
    let m = &doc.master_spreads[0];
    assert_eq!((m.name.as_deref(), m.name_prefix.as_deref(), m.base_name.as_deref()), (Some("B-Chapter"), Some("B"), Some("Chapter")));
    assert_eq!(doc.spreads[0].name_prefix, None);
    assert_eq!((doc.view.horizontal_units.as_str(), doc.view.vertical_units.as_str(), doc.view.ruler_origin.as_str()), ("Millimeters", "Picas", "SpreadOrigin"));
    assert!(doc.view.guides_in_back && doc.view.guides_shown && !doc.view.guides_locked);
    assert!(!doc.view.guides_snap && !doc.view.column_guides_locked);
    let v = &doc.view;
    assert_eq!((v.stroke_units.as_str(), v.cursor_key_increment, v.snap_zone), ("Millimeters", 0.5, 8.0));
    assert_eq!((v.leading_key_increment, v.baseline_shift_key_increment, v.kerning_key_increment), (1.0, 0.5, 50.0));
    assert_eq!((v.ruler_guides_threshold, &v.ruler_guides_color), (50.0, &UiColor::Named("Magenta".into())));
    assert_eq!((v.pasteboard_margins, v.match_preview_background), ([30.0, 40.0], true));
    assert_eq!((&v.preview_background_color, &v.bleed_guide_color, &v.slug_guide_color), (&UiColor::Named("Charcoal".into()), &UiColor::Rgb([1.0, 2.0, 3.0]), &UiColor::Named("GridBlue".into())));
    let g = &doc.view.grid;
    assert_eq!((g.baseline_start, g.baseline_division, g.baseline_threshold), (20.0, 14.0, 50.0));
    assert_eq!(g.baseline_relative_to, "TopOfMarginOfBaselineGridRelativeOption");
    assert!(g.baseline_shown && g.document_shown && !g.grids_in_back && !g.document_snap);
    assert_eq!((g.horizontal_division, g.vertical_division, g.horizontal_subdivision, g.vertical_subdivision), (36.0, 18.0, 4.0, 2.0));
    assert_eq!(g.grid_color, folio_model::UiColor::Named("Red".into()));
    assert_eq!(g.baseline_color, folio_model::UiColor::Rgb([1.0, 2.5, 3.0]));
    assert!(doc.print.facing_pages && doc.print.facing_pages_stored);
}

#[test]
fn guide_locations_follow_the_ruler_origin() {
    // PageOrigin: a page's guides from its own top-left, the spread's from the spread's.
    let doc = read("page", "PageOrigin", true, 88.0, 300.0);
    let p: Vec<f64> = doc.spreads[0].guides.iter().map(|g| g.position).collect();
    assert_eq!(p, [-296.0, -312.0, 88.0]);
    // SpineOrigin: vertical guides of a two-page spread from the spine (the right page's left
    // edge); horizontal ones from the top of the pages.
    let doc = read("spine", "SpineOrigin", true, 88.0, -100.0);
    let p: Vec<f64> = doc.spreads[0].guides.iter().map(|g| g.position).collect();
    assert_eq!(p, [-296.0, -100.0, 88.0]);
}

#[test]
fn swatch_panel_order_and_style_groups() {
    // Colour groups: the root group's swatches, then the other groups'; references to
    // swatches the document does not have are left out.
    let doc = read("groups", "SpreadOrigin", true, 0.0, 0.0);
    assert_eq!(
        doc.swatch_order,
        ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black", "Gradient/G", "Tint/Red 50%25", "Color/Red"]
    );
    // Without colour groups: Graphic.xml order, the four specials first, hidden swatches out.
    let doc = read("nogroups", "SpreadOrigin", false, 0.0, 0.0);
    assert_eq!(
        doc.swatch_order,
        ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black", "Color/Red", "Tint/Red 50%25", "Gradient/G"]
    );

    let st = &doc.styles;
    let para: Vec<(&str, Vec<&str>)> = st.paragraph.iter().map(|e| (e.name.as_str(), e.group.iter().map(String::as_str).collect())).collect();
    assert_eq!(para, [("Body", vec![]), ("Heads:Sub:Title", vec!["$ID/Heads", "Sub"]), ("$ID/[No paragraph style]", vec![])]);
    assert_eq!(st.paragraph[1].id, "ParagraphStyle/Heads%3aSub%3aTitle");
    let chars: Vec<(&str, usize)> = st.character.iter().map(|e| (e.name.as_str(), e.group.len())).collect();
    assert_eq!(chars, [("$ID/[No character style]", 0), ("Accents:Strong", 1)]);
    assert_eq!(st.object.len(), 1);
}
