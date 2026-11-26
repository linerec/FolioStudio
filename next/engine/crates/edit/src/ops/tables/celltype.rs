//! Table ▸ Convert Cell to Graphic Cell / Convert Cell to Text Cell (oracle `tables-cells`, measured with the DOM's
//! `cell.cellType`: cell-to-graphic, cell-to-text, cell-graphic-insets).
//! - A graphic cell is `CellType="GraphicTypeCell"` and holds one `Rectangle` (ContentType GraphicType) instead of text;
//!   converting a cell with text drops the text. Back to a text cell the cell holds one empty paragraph in the basic
//!   paragraph style and the rectangle goes. The cell's other attributes stay, the four insets included (a graphic cell's
//!   insets are the same values as a text cell's: setting `topInset` writes `TextTopInset` and `TopInset`).
//! - Not measured: the menu actions (InDesign was not running). The DOM made the rectangle at the spread's origin, twice
//!   the cell's size less 2 pt, and grew the cell to fit it, which looks like an artefact of the hidden document; Folio
//!   gives the rectangle the cell's content area (known-gaps `tables/cell-to-graphic`).
use super::*;

const BASIC_PARAGRAPH: &str = "ParagraphStyle/$ID/NormalParagraphStyle";

/// The sum of `key` of the `count` lines from `from`.
fn span_size(lines: &[Line], key: &str, from: usize, count: usize) -> f64 {
    (from..from + count).filter_map(|k| lines.get(k)).filter_map(|l| get(&l.attrs, key).and_then(|v| v.parse::<f64>().ok())).sum()
}

/// The XML of an empty graphic cell's rectangle: its path is the content area `width` × `height`, from the origin.
fn graphic_rectangle(id: &str, width: f64, height: f64, indent: &str) -> String {
    let n = geom::num;
    let i = format!("{indent}\t\t");
    let point = |x: f64, y: f64| format!("{i}\t\t\t\t\t<PathPointType Anchor=\"{} {}\" LeftDirection=\"{} {}\" RightDirection=\"{} {}\" />\n", n(x), n(y), n(x), n(y), n(x), n(y));
    let points = [point(0.0, 0.0), point(0.0, height), point(width, height), point(width, 0.0)].concat();
    let head = format!("<Rectangle Self=\"{id}\" ContentType=\"GraphicType\" StoryTitle=\"$ID/\" ParentInterfaceChangeCount=\"\" TargetInterfaceChangeCount=\"\" LastUpdatedInterfaceChangeCount=\"\" \
OverriddenPageItemProps=\"\" HorizontalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" VerticalLayoutConstraints=\"FlexibleDimension FixedDimension FlexibleDimension\" \
GradientFillStart=\"0 0\" GradientFillLength=\"0\" GradientFillAngle=\"0\" GradientStrokeStart=\"0 0\" GradientStrokeLength=\"0\" GradientStrokeAngle=\"0\" LocalDisplaySetting=\"Default\" \
GradientFillHiliteLength=\"0\" GradientFillHiliteAngle=\"0\" GradientStrokeHiliteLength=\"0\" GradientStrokeHiliteAngle=\"0\" AppliedObjectStyle=\"ObjectStyle/$ID/[None]\" Visible=\"true\" Name=\"$ID/\" \
ItemTransform=\"1 0 0 1 0 0\">");
    let export = "<ObjectExportOption EpubType=\"$ID/\" SizeType=\"DefaultSize\" CustomSize=\"$ID/\" PreserveAppearanceFromLayout=\"PreserveAppearanceDefault\" AltTextSourceType=\"SourceXMLStructure\" \
ActualTextSourceType=\"SourceXMLStructure\" CustomAltText=\"$ID/\" CustomActualText=\"$ID/\" ApplyTagType=\"TagFromStructure\" ImageConversionType=\"JPEG\" ImageExportResolution=\"Ppi300\" \
GIFOptionsPalette=\"AdaptivePalette\" GIFOptionsInterlaced=\"true\" JPEGOptionsQuality=\"High\" JPEGOptionsFormat=\"BaselineEncoding\" ImageAlignment=\"AlignLeft\" ImageSpaceBefore=\"0\" ImageSpaceAfter=\"0\" \
UseImagePageBreak=\"false\" ImagePageBreak=\"PageBreakBefore\" CustomImageAlignment=\"false\" SpaceUnit=\"CssPixel\" CustomLayout=\"false\" CustomLayoutType=\"AlignmentAndSpacing\">";
    format!(
        "\n{i}{head}\n{i}\t<Properties>\n{i}\t\t<PathGeometry>\n{i}\t\t\t<GeometryPathType PathOpen=\"false\">\n{i}\t\t\t\t<PathPointArray>\n{points}{i}\t\t\t\t</PathPointArray>\n{i}\t\t\t</GeometryPathType>\n{i}\t\t</PathGeometry>\n{i}\t</Properties>\n\
{i}\t<TextWrapPreference Inverse=\"false\" ApplyToMasterPageOnly=\"false\" TextWrapSide=\"BothSides\" TextWrapMode=\"None\">\n{i}\t\t<Properties>\n{i}\t\t\t<TextWrapOffset Top=\"0\" Left=\"0\" Bottom=\"0\" Right=\"0\" />\n{i}\t\t</Properties>\n{i}\t</TextWrapPreference>\n\
{i}\t<InCopyExportOption IncludeGraphicProxies=\"true\" IncludeAllResources=\"false\" />\n{i}\t<FrameFittingOption FittingOnEmptyFrame=\"FillProportionally\" />\n\
{i}\t{export}\n{i}\t\t<Properties>\n{i}\t\t\t<AltMetadataProperty NamespacePrefix=\"$ID/\" PropertyPath=\"$ID/\" />\n{i}\t\t\t<ActualMetadataProperty NamespacePrefix=\"$ID/\" PropertyPath=\"$ID/\" />\n{i}\t\t</Properties>\n{i}\t</ObjectExportOption>\n\
{i}</Rectangle>\n{indent}\t")
}

/// Table ▸ Convert Cell to Graphic Cell (`graphic`) / Convert Cell to Text Cell for the cells; a cell that is of that
/// kind already stays as it is.
pub(in crate::ops) fn set_cell_type(w: &mut Work<'_>, table: &str, cells: &[[usize; 2]], graphic: bool) -> Result<(), EditError> {
    let mut x = TableX::read(w, table)?;
    let mut changed = false;
    for &[c, r] in cells {
        let k = x.at(c, r).ok_or_else(|| EditError::new("invalid", format!("no cell {c}:{r}")))?;
        if (get(&x.cells[k].attrs, "CellType") == Some("GraphicTypeCell")) == graphic {
            continue;
        }
        let inner = if graphic {
            let cell = &x.cells[k];
            let width = (span_size(&x.columns, "SingleColumnWidth", cell.column, cell.columns) - 1.0).max(0.0);
            let height = (span_size(&x.rows, "SingleRowHeight", cell.row, cell.rows) - 1.0).max(0.0);
            graphic_rectangle(&w.new_id(), width, height, &x.indent)
        } else {
            empty_cell_text(BASIC_PARAGRAPH, &x.indent)
        };
        let cell = &mut x.cells[k];
        put(&mut cell.attrs, "CellType", if graphic { "GraphicTypeCell" } else { "TextTypeCell" }.into());
        cell.inner = inner;
        changed = true;
    }
    if changed { x.write(w) } else { Ok(()) }
}
