//! Tables, footnotes, endnotes, lists, paragraph rules, shading and borders as the reader
//! resolves them, on a small hand-written package.

use folio_idml::read_package;
use folio_model::{
    BottomOrigin, Document, FirstBaseline, ListKind, Paint, Paragraph, RuleWidth, Table, TextVariable, TopOrigin,
    VerticalJustification,
};
use std::io::Write;
use std::path::PathBuf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        // Several tests call read_doc("doc") concurrently in the same process.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("folio-idml-ts-{}-{name}-{sequence}", std::process::id()));
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
<Document xmlns:idPkg="http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging" DOMVersion="20.4" Self="d">
  <idPkg:Graphic src="Resources/Graphic.xml" />
  <idPkg:Styles src="Resources/Styles.xml" />
  <idPkg:Preferences src="Resources/Preferences.xml" />
  <idPkg:Story src="Stories/Story_t.xml" />
  <idPkg:Story src="Stories/Story_n.xml" />
  <idPkg:Story src="Stories/Story_e.xml" />
</Document>"#;

const GRAPHIC: &str = r#"<idPkg:Graphic xmlns:idPkg="p">
  <Color Self="Color/Black" Model="Process" Space="CMYK" ColorValue="0 0 0 100" Name="Black" />
  <Color Self="Color/Paper" Model="Process" Space="CMYK" ColorValue="0 0 0 0" Name="Paper" />
  <Color Self="Color/Teal" Model="Process" Space="CMYK" ColorValue="85 20 35 5" Name="Teal" />
  <Color Self="Color/Cyan" Model="Process" Space="CMYK" ColorValue="100 0 0 0" Name="Cyan" />
  <Swatch Self="Swatch/None" Name="None" />
</idPkg:Graphic>"#;

const STYLES: &str = r#"<idPkg:Styles xmlns:idPkg="p">
 <RootCharacterStyleGroup Self="c">
  <CharacterStyle Self="CharacterStyle/$ID/[No character style]" Name="$ID/[No character style]" />
  <CharacterStyle Self="CharacterStyle/Num" Name="Num" FillColor="Color/Teal" PointSize="8" />
 </RootCharacterStyleGroup>
 <RootParagraphStyleGroup Self="p">
  <ParagraphStyle Self="ParagraphStyle/$ID/[No paragraph style]" Name="$ID/[No paragraph style]" PointSize="10"
    RuleAbove="false" RuleAboveLineWeight="1" RuleAboveTint="-1" RuleAboveOffset="0" RuleAboveWidth="ColumnWidth"
    RuleBelow="false" ParagraphShadingOn="false" ParagraphShadingTint="20" ParagraphShadingTopOrigin="AscentTopOrigin"
    ParagraphBorderOn="false" MergeConsecutiveParaBorders="true" BulletsAndNumberingListType="NoList" NumberingStartAt="1"
    NumberingContinue="true" NumberingExpression="^#.^t" BulletsTextAfter="^t">
   <Properties>
    <AppliedFont type="string">Minion Pro</AppliedFont>
    <RuleAboveColor type="string">Text Color</RuleAboveColor>
    <ParagraphShadingColor type="object">Color/Black</ParagraphShadingColor>
    <ParagraphBorderColor type="object">Color/Black</ParagraphBorderColor>
    <NumberingFormat type="string">1, 2, 3, 4...</NumberingFormat>
    <BulletChar BulletCharacterType="UnicodeOnly" BulletCharacterValue="8226" />
    <NumberingCharacterStyle type="object">CharacterStyle/$ID/[No character style]</NumberingCharacterStyle>
   </Properties>
  </ParagraphStyle>
  <ParagraphStyle Self="ParagraphStyle/Ruled" Name="Ruled" RuleAbove="true" RuleAboveLineWeight="2" RuleAboveOffset="3"
    RuleAboveWidth="TextWidth" RuleAboveLeftIndent="1" RuleBelow="true" RuleBelowLineWeight="0.5" RuleBelowOffset="4" RuleBelowGapTint="40" RuleBelowGapOverprint="true">
   <Properties><RuleBelowColor type="object">Color/Teal</RuleBelowColor><RuleBelowType type="object">StrokeStyle/$ID/Dashed</RuleBelowType>
    <RuleBelowGapColor type="object">Color/Cyan</RuleBelowGapColor></Properties>
  </ParagraphStyle>
  <ParagraphStyle Self="ParagraphStyle/Boxed" Name="Boxed" ParagraphShadingOn="true" ParagraphShadingTopOrigin="BaselineTopOrigin"
    ParagraphShadingBottomOffset="3" ParagraphBorderOn="true" ParagraphBorderTopLineWeight="1" ParagraphBorderLeftLineWeight="2"
    ParagraphBorderBottomLineWeight="3" ParagraphBorderRightLineWeight="4" ParagraphBorderBottomOrigin="BaselineBottomOrigin"
    ParagraphBorderWidth="TextWidth" ParagraphBorderDisplayIfSplits="true" ParagraphBorderGapTint="60" ParagraphBorderGapOverprint="true">
   <Properties><ParagraphShadingColor type="object">Color/Cyan</ParagraphShadingColor><ParagraphBorderColor type="object">Color/Teal</ParagraphBorderColor>
    <ParagraphBorderGapColor type="object">Color/Cyan</ParagraphBorderGapColor><ParagraphBorderType type="object">StrokeStyle/$ID/Japanese Dots</ParagraphBorderType></Properties>
  </ParagraphStyle>
  <ParagraphStyle Self="ParagraphStyle/Numbered" Name="Numbered" BulletsAndNumberingListType="NumberedList">
   <Properties><NumberingFormat type="string">I, II, III, IV...</NumberingFormat><NumberingCharacterStyle type="object">CharacterStyle/Num</NumberingCharacterStyle></Properties>
  </ParagraphStyle>
 </RootParagraphStyleGroup>
 <RootTableStyleGroup Self="ts">
  <TableStyle Self="TableStyle/$ID/[No table style]" Name="$ID/[No table style]" TextTopInset="4" TextLeftInset="4" TextBottomInset="4" TextRightInset="4"
    TopBorderStrokeWeight="1" TopBorderStrokeColor="Color/Black" LeftBorderStrokeWeight="1" BottomBorderStrokeWeight="1" RightBorderStrokeWeight="1.5"
    RightBorderStrokeColor="Color/Teal" SpaceBefore="4" SpaceAfter="-4" StartRowFillColor="Color/Black" StartRowFillCount="0" StartRowFillTint="20"
    EndRowFillCount="0" EndRowFillColor="Swatch/None" BodyRegionCellStyle="CellStyle/$ID/[None]" HeaderRegionSameAsBodyRegion="true" />
  <TableStyle Self="TableStyle/Zebra" Name="Zebra" StartRowFillColor="Color/Cyan" StartRowFillCount="1" EndRowFillCount="1"
    EndRowFillColor="Color/Paper" BodyRegionCellStyle="CellStyle/Accent" SpaceBefore="6"
    StartRowStrokeCount="1" StartRowStrokeWeight="0.5" EndRowStrokeCount="1" EndRowStrokeWeight="2">
   <Properties><BasedOn type="string">$ID/[No table style]</BasedOn></Properties>
  </TableStyle>
 </RootTableStyleGroup>
 <RootCellStyleGroup Self="cs">
  <CellStyle Self="CellStyle/$ID/[None]" Name="$ID/[None]" />
  <CellStyle Self="CellStyle/Accent" Name="Accent" TextTopInset="8" VerticalJustification="CenterAlign" />
 </RootCellStyleGroup>
</idPkg:Styles>"#;

const PREFS: &str = r#"<idPkg:Preferences xmlns:idPkg="p">
  <FootnoteOption StartAt="3" Prefix="[" Suffix="]" SpaceBetween="2" Spacer="5" FootnoteFirstBaselineOffset="LeadingOffset" EnableStraddling="true"
    RuleOn="true" RuleLineWeight="0.5" RuleOffset="2" RuleLeftIndent="4" RuleWidth="100" RuleTint="50">
   <Properties><FootnoteNumberingStyle type="enumeration">LowerRoman</FootnoteNumberingStyle>
    <ShowPrefixSuffix type="enumeration">PrefixSuffixInText</ShowPrefixSuffix>
    <RuleColor type="object">Color/Black</RuleColor></Properties>
  </FootnoteOption>
</idPkg:Preferences>"#;

const STORY_T: &str = r#"<idPkg:Story xmlns:idPkg="p"><Story Self="t">
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]">
  <CharacterStyleRange AppliedCharacterStyle="CharacterStyle/$ID/[No character style]" PointSize="9">
   <Content>Before</Content>
   <Table Self="tb" HeaderRowCount="1" FooterRowCount="0" BodyRowCount="3" ColumnCount="2" AppliedTableStyle="TableStyle/Zebra">
    <Row Self="r0" Name="0" SingleRowHeight="20" />
    <Row Self="r1" Name="1" SingleRowHeight="20" MinimumHeight="12" />
    <Row Self="r2" Name="2" SingleRowHeight="20" AutoGrow="false" />
    <Row Self="r3" Name="3" SingleRowHeight="20" />
    <Column Self="c0" Name="0" SingleColumnWidth="60" />
    <Column Self="c1" Name="1" SingleColumnWidth="40" />
    <Cell Self="a" Name="0:0" RowSpan="1" ColumnSpan="2" AppliedCellStyle="CellStyle/$ID/[None]" FillColor="Color/Teal">
     <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]"><CharacterStyleRange><Content>Head</Content></CharacterStyleRange></ParagraphStyleRange>
    </Cell>
    <Cell Self="b" Name="0:1" RowSpan="2" ColumnSpan="1" BottomEdgeStrokeWeight="3" BottomEdgeStrokeColor="Color/Teal" BottomEdgeStrokePriority="2">
     <ParagraphStyleRange><CharacterStyleRange><Content>A</Content><Br /><Content>B</Content></CharacterStyleRange></ParagraphStyleRange>
    </Cell>
    <Cell Self="c" Name="1:1" RotationAngle="90"><ParagraphStyleRange><CharacterStyleRange PointSize="7" /></ParagraphStyleRange></Cell>
    <Cell Self="d" Name="1:2" TopLeftDiagonalLine="true"><ParagraphStyleRange><CharacterStyleRange><Content>D</Content></CharacterStyleRange></ParagraphStyleRange></Cell>
    <Cell Self="e" Name="0:3" FirstBaselineOffset="LeadingOffset"><ParagraphStyleRange><CharacterStyleRange><Content>E</Content></CharacterStyleRange></ParagraphStyleRange></Cell>
    <Cell Self="f" Name="1:3"><ParagraphStyleRange><CharacterStyleRange><Content>F</Content></CharacterStyleRange></ParagraphStyleRange></Cell>
   </Table>
   <Content>After</Content>
  </CharacterStyleRange>
 </ParagraphStyleRange>
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Ruled"><CharacterStyleRange FillColor="Color/Cyan"><Br /><Content>Ruled</Content><Br /></CharacterStyleRange></ParagraphStyleRange>
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Boxed"><CharacterStyleRange><Content>Boxed</Content><Br /></CharacterStyleRange></ParagraphStyleRange>
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Numbered"><CharacterStyleRange FillColor="Color/Black" PointSize="11"><Content>One</Content><Br /></CharacterStyleRange></ParagraphStyleRange>
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/Numbered" NumberingContinue="false" NumberingStartAt="9"><CharacterStyleRange><Content>Nine</Content><Br /></CharacterStyleRange></ParagraphStyleRange>
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]" BulletsAndNumberingListType="BulletList" BulletsFont="Arial" BulletsFontStyle="Bold"><CharacterStyleRange><Content>Bullet</Content><Br /></CharacterStyleRange></ParagraphStyleRange>
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]"><CharacterStyleRange PointSize="13"></CharacterStyleRange></ParagraphStyleRange>
</Story></idPkg:Story>"#;

const STORY_N: &str = r#"<idPkg:Story xmlns:idPkg="p"><Story Self="n">
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]">
  <CharacterStyleRange><Content>Text</Content></CharacterStyleRange>
  <CharacterStyleRange Position="Superscript"><Footnote>
    <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]"><CharacterStyleRange PointSize="8"><Content><?ACE 4?>	Note text</Content></CharacterStyleRange></ParagraphStyleRange>
  </Footnote></CharacterStyleRange>
  <CharacterStyleRange><Content> and</Content></CharacterStyleRange>
  <CharacterStyleRange Position="Superscript"><Endnote Self="en2" EndnoteTextRange="r2" /></CharacterStyleRange>
  <CharacterStyleRange Position="Superscript"><Endnote Self="en1" EndnoteTextRange="r1" /></CharacterStyleRange>
  <CharacterStyleRange><Note><ParagraphStyleRange><CharacterStyleRange><Content>editorial</Content></CharacterStyleRange></ParagraphStyleRange></Note><Content>.</Content></CharacterStyleRange>
 </ParagraphStyleRange>
</Story></idPkg:Story>"#;

const STORY_E: &str = r#"<idPkg:Story xmlns:idPkg="p"><Story Self="e" IsEndnoteStory="true">
 <ParagraphStyleRange AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]"><CharacterStyleRange>
  <Content>Endnotes</Content><Br />
  <EndnoteRange Self="r1" SourceEndnote="en1"><Content>&#xFEFF;<?ACE 4?>	First&#xFEFF;</Content></EndnoteRange><Br />
  <EndnoteRange Self="r2" SourceEndnote="en2"><Content>&#xFEFF;<?ACE 4?>	Second&#xFEFF;</Content></EndnoteRange>
 </CharacterStyleRange></ParagraphStyleRange>
</Story></idPkg:Story>"#;

fn read() -> Document {
    let dir = TempDir::new("doc");
    let path = dir.0.join("doc.idml");
    zip(
        &path,
        &[
            ("mimetype", "application/vnd.adobe.indesign-idml-package"),
            ("designmap.xml", DESIGNMAP),
            ("Resources/Graphic.xml", GRAPHIC),
            ("Resources/Styles.xml", STYLES),
            ("Resources/Preferences.xml", PREFS),
            ("Stories/Story_t.xml", STORY_T),
            ("Stories/Story_n.xml", STORY_N),
            ("Stories/Story_e.xml", STORY_E),
        ],
    );
    read_package(&path, None).unwrap()
}

fn table(doc: &Document) -> &Table {
    doc.stories["t"].paragraphs[0].runs.iter().flat_map(|r| &r.tables).next().unwrap()
}

fn text(p: &Paragraph) -> String {
    p.runs.iter().map(|r| r.text.as_str()).collect()
}

fn swatch(id: &str, tint: f64) -> Paint {
    Paint::Swatch { id: id.into(), tint }
}

#[test]
fn table_structure_and_text_position() {
    let doc = read();
    let p0 = &doc.stories["t"].paragraphs[0];
    // One table character between the text, the table on the run that holds it.
    assert_eq!(text(p0), "Before\u{16}After");
    let t = table(&doc);
    assert_eq!((t.rows.len(), t.columns.clone(), t.header_rows, t.footer_rows), (4, vec![60.0, 40.0], 1, 0));
    assert_eq!(t.style, "Zebra");
    // The table style chain: space before from Zebra, after from the root.
    assert_eq!((t.space_before, t.space_after), (6.0, -4.0));
    assert_eq!(t.border_weights, [1.0, 1.0, 1.0, 1.5]);
    assert!(t.rows[1].auto_grow && (t.rows[1].min_height - 12.0).abs() < 1e-9);
    assert!(!t.rows[2].auto_grow, "AutoGrow=false is an exactly-height row");
    // Cells row-major with spans; cell text is read like a story.
    let names: Vec<(usize, usize, usize, usize)> = t.cells.iter().map(|c| (c.row, c.column, c.row_span, c.column_span)).collect();
    assert_eq!(names, vec![(0, 0, 1, 2), (1, 0, 2, 1), (1, 1, 1, 1), (2, 1, 1, 1), (3, 0, 1, 1), (3, 1, 1, 1)]);
    assert_eq!(t.cells[1].paragraphs.len(), 2);
    assert_eq!(text(&t.cells[1].paragraphs[1]), "B");
}

#[test]
fn table_cell_styles_borders_and_alternating_fills() {
    let doc = read();
    let t = table(&doc);
    let cell = |r: usize, c: usize| t.cells.iter().find(|x| x.row == r && x.column == c).unwrap();
    // Header: own fill; table default insets (the region style is not applied to headers?
    // here the header region is "same as body": the Accent cell style).
    assert_eq!(cell(0, 0).fill, swatch("Color/Teal", 100.0));
    // Body region cell style: top inset 8, centred; others from the table (4).
    assert_eq!(cell(1, 0).insets, [8.0, 4.0, 4.0, 4.0]);
    assert_eq!(cell(1, 0).vertical_justification, VerticalJustification::Center);
    // Alternating fills over body rows: cyan (row 1), paper (row 2), cyan (row 3).
    // (The start fill's 20 % tint comes from the root table style.)
    assert_eq!(cell(1, 0).fill, swatch("Color/Cyan", 20.0));
    assert_eq!(cell(2, 1).fill, swatch("Color/Paper", 100.0));
    assert_eq!(cell(3, 0).fill, swatch("Color/Cyan", 20.0));
    // Edges: the cell's own, else the table border on outer edges (right border teal 1.5),
    // else 1 pt black.
    let b = &cell(1, 0).edges;
    assert_eq!((b[2].weight, b[2].paint.clone(), b[2].priority), (3.0, swatch("Color/Teal", 100.0), 2));
    assert_eq!((b[1].weight, b[1].paint.clone()), (1.0, swatch("Color/Black", 100.0)));
    let r = &cell(1, 1).edges[3];
    assert_eq!((r.weight, r.paint.clone()), (1.5, swatch("Color/Teal", 100.0)));
    assert_eq!(cell(1, 1).edges[0].weight, 1.0);
    assert!(cell(2, 1).diagonal_down && !cell(2, 1).diagonal_up);
    // Alternating row strokes on the bottoms of body rows (0.5 pt, 2 pt, …) but not on the
    // table's last row (its border) nor on an edge a cell sets itself.
    assert_eq!(cell(1, 1).edges[2].weight, 0.5);
    assert_eq!(cell(2, 1).edges[2].weight, 2.0);
    assert_eq!(cell(1, 0).edges[2].weight, 3.0);
    assert_eq!(cell(3, 0).edges[2].weight, 1.0);
    assert_eq!(cell(3, 0).first_baseline, FirstBaseline::Leading);
    assert_eq!(cell(1, 1).rotation, 90.0);
    // An empty cell keeps its range's attributes for the empty paragraph (its depth).
    let empty = &cell(1, 1).paragraphs[0];
    assert_eq!(empty.runs.last().unwrap().attrs.point_size, 7.0);
}

#[test]
fn rules_shading_borders() {
    let doc = read();
    let ps = &doc.stories["t"].paragraphs;
    // Paragraph 0 runs on to the first return, inside the "Ruled" range.
    let ruled = &ps[1].attrs;
    let above = ruled.rule_above.as_ref().unwrap();
    assert_eq!((above.weight, above.offset, above.left_indent, above.width), (2.0, 3.0, 1.0, RuleWidth::Text));
    assert_eq!(above.paint, None, "Text Color: the text's fill at layout");
    let below = ruled.rule_below.as_ref().unwrap();
    assert_eq!((below.weight, below.offset, below.paint.clone()), (0.5, 4.0, Some(swatch("Color/Teal", 100.0))));
    assert_eq!(below.stroke_type, "Dashed");
    // The gap colour of a patterned rule: colour, tint, overprint (the rule above has none).
    assert_eq!((below.gap_paint.clone(), below.gap_overprint), (swatch("Color/Cyan", 40.0), true));
    assert_eq!((above.gap_paint.clone(), above.gap_overprint), (Paint::None, false));
    // The layout draws every built-in stroke style on a rule: no warning (pardeco2-rule-type).
    assert!(!doc.warnings.iter().any(|w| w.contains("paragraph rule stroke type Dashed drawn solid")));
    let boxed = &ps[2].attrs;
    let s = boxed.shading.as_ref().unwrap();
    assert_eq!((s.paint.clone(), s.top, s.bottom, s.offsets), (swatch("Color/Cyan", 20.0), TopOrigin::Baseline, BottomOrigin::Descent, [0.0, 0.0, 3.0, 0.0]));
    let b = boxed.border.as_ref().unwrap();
    assert_eq!((b.weights, b.bottom, b.width, b.display_if_splits, b.merge), ([1.0, 2.0, 3.0, 4.0], BottomOrigin::Baseline, RuleWidth::Text, true, true));
    assert_eq!(b.paint, swatch("Color/Teal", 100.0));
    assert_eq!((b.gap_paint.clone(), b.gap_overprint, b.stroke_type.as_str()), (swatch("Color/Cyan", 60.0), true, "Japanese Dots"));
    assert!(!doc.warnings.iter().any(|w| w.contains("paragraph border stroke type")), "{:?}", doc.warnings);
    assert!(ps[0].attrs.rule_above.is_none() && ps[0].attrs.shading.is_none() && ps[0].attrs.border.is_none());
}

#[test]
fn lists_with_marker_attributes() {
    let doc = read();
    let ps = &doc.stories["t"].paragraphs;
    let one = ps[3].attrs.list.as_ref().unwrap();
    assert_eq!((one.kind, one.format.as_str(), one.expression.as_str(), one.restart_at), (ListKind::Numbered, "I, II, III, IV...", "^#.\t", None));
    // The number: first character's attributes with the numbering character style.
    assert_eq!((one.marker.point_size, one.marker.fill.clone()), (8.0, swatch("Color/Teal", 100.0)));
    let nine = ps[4].attrs.list.as_ref().unwrap();
    assert_eq!(nine.restart_at, Some(9));
    let bullet = ps[5].attrs.list.as_ref().unwrap();
    assert_eq!((bullet.kind, bullet.bullet.as_str(), bullet.bullet_text_after.as_str()), (ListKind::Bullet, "\u{2022}", "\t"));
    assert_eq!((bullet.marker.font_family.as_str(), bullet.marker.font_style.as_str()), ("Arial", "Bold"));
    assert!(ps[0].attrs.list.is_none());
    // The story's last (empty) paragraph takes its empty range's attributes.
    assert_eq!(ps.last().unwrap().runs.last().unwrap().attrs.point_size, 13.0);
}

#[test]
fn footnotes_endnotes_and_editorial_notes() {
    let doc = read();
    let p = &doc.stories["n"].paragraphs[0];
    // Reference markers are "#" runs; the editorial note keeps one invisible character.
    assert_eq!(text(p), "Text# and##\u{feff}.");
    let fnr = p.runs.iter().find(|r| r.variable == Some(TextVariable::FootnoteNumber)).unwrap();
    let note = fnr.note.as_ref().unwrap();
    assert_eq!(fnr.attrs.position, folio_model::Position::Superscript);
    assert_eq!(text(&note.paragraphs[0]), "#\tNote text");
    assert_eq!(note.paragraphs[0].runs[0].variable, Some(TextVariable::FootnoteNumber));
    // Endnotes numbered by the order of their ranges in the endnote story.
    let ens: Vec<Option<u32>> = p
        .runs
        .iter()
        .filter(|r| r.variable == Some(TextVariable::EndnoteNumber))
        .map(|r| r.note.as_ref().and_then(|n| n.number))
        .collect();
    assert_eq!(ens, vec![Some(2), Some(1)]);
    let e = &doc.stories["e"].paragraphs;
    assert_eq!((text(&e[1]), text(&e[2])), ("#\tFirst".to_string(), "#\tSecond".to_string()));
    assert_eq!(e[1].runs[0].note.as_ref().and_then(|n| n.number), Some(1));
    // Footnote options.
    let o = &doc.footnotes;
    assert_eq!((o.start_at, o.numbering.as_str(), o.prefix.as_str(), o.show_prefix_suffix.as_str()), (3, "LowerRoman", "[", "PrefixSuffixInText"));
    assert_eq!((o.space_between, o.spacer, o.first_baseline, o.straddle), (2.0, 5.0, FirstBaseline::Leading, true));
    let rule = o.rule.as_ref().unwrap();
    assert_eq!((rule.weight, rule.offset, rule.left_indent, rule.length, rule.paint.clone()), (0.5, 2.0, 4.0, 100.0, swatch("Color/Black", 50.0)));
    assert!(!doc.warnings.iter().any(|w| w.contains("not modelled (content skipped)")));
}
