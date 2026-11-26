//! File ▸ New ▸ Document: a blank IDML package written from the New Document dialog's settings.
//!
//! Folio's documents are IDML packages, so a new document is one: the smallest package that both
//! Folio and InDesign accept, which the import then turns into a `.folio` in the project (the same
//! way as any IDML, so every later step is the code that already exists). Measured against
//! InDesign (`artifacts/new-document-20261022/`, `next/oracle/newdoc/`): InDesign fills in every
//! attribute the package leaves out with the value a document made by File ▸ New has, so the
//! styles carry only their names (and the font), the preferences only what the document's size,
//! pages, margins and defaults need, and an InDesign re-export of this package equals the re-export
//! of InDesign's own new document except for the font, the units and the ids.
//!
//! Nothing here comes from Adobe's files: structure and names are the IDML format's, the numbers
//! are the dialog's, and the defaults written out are the ones a new document is measured to have.

use std::collections::BTreeMap;
use std::io::Write;

const NS: &str = r#"xmlns:idPkg="http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging" DOMVersion="20.4""#;
const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
/// The gap InDesign leaves between two spreads on the pasteboard.
const SPREAD_GAP: f64 = 180.0;

/// The ruler units a document can start with (IDML `MeasurementUnits`).
pub const UNITS: [&str; 5] = ["Points", "Picas", "Inches", "Millimeters", "Centimeters"];

#[derive(Clone, Debug, PartialEq)]
pub struct FontChoice {
    pub family: String,
    /// The style name (`Regular`).
    pub style: String,
    pub postscript: String,
    /// `TrueType`, `OpenTypeTT` or `OpenTypeCFF`.
    pub font_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewDocument {
    /// The document's name (its file's stem); only the IDML `Document/@Name`.
    pub name: String,
    /// Page size in points.
    pub width: f64,
    pub height: f64,
    pub pages: u32,
    pub facing: bool,
    /// The number of the first page.
    pub start: u32,
    /// Margins in points; with facing pages `inside` is the margin at the spine.
    pub top: f64,
    pub bottom: f64,
    pub inside: f64,
    pub outside: f64,
    pub columns: u32,
    pub gutter: f64,
    /// Bleed and slug: top, bottom, inside, outside, in points.
    pub bleed: [f64; 4],
    pub slug: [f64; 4],
    pub font: FontChoice,
    /// The first layer's name (`Layer 1`, localised by the caller).
    pub layer: String,
    pub horizontal_units: String,
    pub vertical_units: String,
    /// The working spaces (names of installed ICC profiles), as InDesign's Color Settings give a new document.
    pub cmyk_profile: String,
    pub rgb_profile: String,
}

impl NewDocument {
    /// US Letter, one facing page, 36 pt margins, one column, Points.
    pub fn letter(font: FontChoice) -> Self {
        NewDocument {
            name: "Untitled-1".into(),
            width: 612.0,
            height: 792.0,
            pages: 1,
            facing: true,
            start: 1,
            top: 36.0,
            bottom: 36.0,
            inside: 36.0,
            outside: 36.0,
            columns: 1,
            gutter: 12.0,
            bleed: [0.0; 4],
            slug: [0.0; 4],
            font,
            layer: "Layer 1".into(),
            horizontal_units: "Points".into(),
            vertical_units: "Points".into(),
            cmyk_profile: "U.S. Web Coated (SWOP) v2".into(),
            rgb_profile: "sRGB IEC61966-2.1".into(),
        }
    }

    /// Rejects what InDesign's dialog rejects (a page of 0 size, more margin than page, …).
    pub fn validate(&self) -> Result<(), String> {
        let pt = |v: f64| v.is_finite();
        if ![self.width, self.height, self.top, self.bottom, self.inside, self.outside, self.gutter].iter().all(|v| pt(*v)) || !self.bleed.iter().chain(&self.slug).all(|v| pt(*v)) {
            return Err("숫자가 올바르지 않습니다".into());
        }
        if !(1.0 / 72.0..=216.0 * 72.0).contains(&self.width) || !(1.0 / 72.0..=216.0 * 72.0).contains(&self.height) {
            return Err("쪽 크기는 1/72인치부터 216인치까지입니다".into());
        }
        if !(1..=9999).contains(&self.pages) {
            return Err("쪽 수는 1에서 9999 사이여야 합니다".into());
        }
        if !(1..=9999).contains(&self.start) {
            return Err("시작 쪽 번호는 1에서 9999 사이여야 합니다".into());
        }
        if !(1..=40).contains(&self.columns) {
            return Err("단 수는 1에서 40 사이여야 합니다".into());
        }
        if [self.top, self.bottom, self.inside, self.outside, self.gutter].iter().any(|v| *v < 0.0) || self.bleed.iter().chain(&self.slug).any(|v| *v < 0.0) {
            return Err("여백, 간격, 도련, 슬러그는 0 이상이어야 합니다".into());
        }
        if self.top + self.bottom >= self.height || self.inside + self.outside >= self.width {
            return Err("여백이 쪽보다 큽니다".into());
        }
        let text_width = self.width - self.inside - self.outside;
        if self.gutter * (self.columns as f64 - 1.0) >= text_width {
            return Err("단 간격이 너무 큽니다".into());
        }
        for u in [&self.horizontal_units, &self.vertical_units] {
            if !UNITS.contains(&u.as_str()) {
                return Err(format!("지원하지 않는 단위: {u}"));
            }
        }
        if self.font.family.trim().is_empty() || self.font.style.trim().is_empty() {
            return Err("글꼴이 비어 있습니다".into());
        }
        Ok(())
    }

    /// The page numbers of every spread. With facing pages an even page starts a spread and an odd
    /// page joins the even one before it (an odd first page stands alone on the right).
    pub fn spreads(&self) -> Vec<Vec<u32>> {
        let mut out: Vec<Vec<u32>> = Vec::new();
        for n in self.start..self.start + self.pages {
            if !self.facing || n % 2 == 0 || out.is_empty() {
                out.push(vec![n]);
            } else if let Some(last) = out.last_mut() {
                last.push(n);
            }
        }
        out
    }

    /// Every part of the package, in order (`mimetype` is written first by [`package`](Self::package)).
    pub fn parts(&self) -> Result<Vec<(String, String)>, String> {
        self.validate()?;
        let mut ids = (0x10u32..).map(|n| format!("u{n:x}"));
        let mut uid = move || ids.next().expect("the unbounded document ID iterator always yields a value");
        let layer = uid();
        let master = uid();
        let section = uid();
        let spreads = self.spreads();
        let spread_ids: Vec<String> = spreads.iter().map(|_| uid()).collect();
        let mut page_ids: BTreeMap<u32, String> = BTreeMap::new();
        for s in &spreads {
            for n in s {
                page_ids.insert(*n, uid());
            }
        }
        let master_pages: Vec<(String, bool)> = if self.facing { vec![(uid(), true), (uid(), false)] } else { vec![(uid(), false)] };
        let first_page = page_ids[&spreads[0][0]].clone();

        let mut parts = Vec::new();
        // ---- designmap
        let mut dm = String::new();
        dm.push_str(HEAD);
        dm.push_str("<?aid style=\"50\" type=\"document\" readerVersion=\"6.0\" featureSet=\"257\" product=\"20.5(119)\" ?>\n");
        dm.push_str(&format!(
            "<Document {NS} Self=\"d\" StoryList=\"\" Name=\"{}\" ZeroPoint=\"0 0\" ActiveLayer=\"{layer}\" CMYKProfile=\"{}\" RGBProfile=\"{}\" SolidColorIntent=\"UseColorSettings\" AfterBlendingIntent=\"UseColorSettings\" DefaultImageIntent=\"UseColorSettings\" RGBPolicy=\"PreserveEmbeddedProfiles\" CMYKPolicy=\"CombinationOfPreserveAndSafeCmyk\">\n",
            esc(&self.name),
            esc(&self.cmyk_profile),
            esc(&self.rgb_profile)
        ));
        dm.push_str("\t<idPkg:Graphic src=\"Resources/Graphic.xml\" />\n\t<idPkg:Fonts src=\"Resources/Fonts.xml\" />\n\t<idPkg:Styles src=\"Resources/Styles.xml\" />\n\t<idPkg:Preferences src=\"Resources/Preferences.xml\" />\n");
        dm.push_str(&format!(
            "\t<Layer Self=\"{layer}\" Name=\"{}\" Visible=\"true\" Locked=\"false\" IgnoreWrap=\"false\" ShowGuides=\"true\" LockGuides=\"false\" UI=\"true\" Expendable=\"true\" Printable=\"true\">\n\t\t<Properties>\n\t\t\t<LayerColor type=\"enumeration\">LightBlue</LayerColor>\n\t\t</Properties>\n\t</Layer>\n",
            esc(&self.layer)
        ));
        dm.push_str(&format!("\t<idPkg:MasterSpread src=\"MasterSpreads/MasterSpread_{master}.xml\" />\n"));
        for id in &spread_ids {
            dm.push_str(&format!("\t<idPkg:Spread src=\"Spreads/Spread_{id}.xml\" />\n"));
        }
        let numbering = if self.start == 1 { "ContinueNumbering=\"true\"".to_owned() } else { format!("ContinueNumbering=\"false\" PageNumberStart=\"{}\"", self.start) };
        dm.push_str(&format!(
            "\t<Section Self=\"{section}\" Length=\"{}\" Name=\"\" {numbering} IncludeSectionPrefix=\"false\" Marker=\"\" PageStart=\"{first_page}\" SectionPrefix=\"\">\n\t\t<Properties>\n\t\t\t<PageNumberStyle type=\"enumeration\">Arabic</PageNumberStyle>\n\t\t</Properties>\n\t</Section>\n",
            self.pages
        ));
        dm.push_str(TEXT_VARIABLES);
        dm.push_str(folio_idml::hyperlinks::formats::PAGE_VARIABLE_XML);
        for format in folio_idml::hyperlinks::formats::defaults() { dm.push_str(&folio_idml::hyperlinks::formats::xml(&format)); }
        dm.push_str("<Language Self=\"Language/$ID/English%3a USA\" Name=\"$ID/English: USA\" SingleQuotes=\"‘’\" DoubleQuotes=\"“”\" PrimaryLanguageName=\"$ID/English\" SublanguageName=\"$ID/USA\" Id=\"269\" HyphenationVendor=\"Hunspell\" SpellingVendor=\"Hunspell\" />");
        dm.push_str(&root_color_group());
        dm.push_str("</Document>\n");
        parts.push(("designmap.xml".to_owned(), dm));

        // ---- the parent (A-Parent): two pages with facing pages
        let mut m = String::new();
        m.push_str(HEAD);
        m.push_str(&format!("<idPkg:MasterSpread {NS}>\n"));
        m.push_str(&format!(
            "\t<MasterSpread Self=\"{master}\" ItemTransform=\"1 0 0 1 0 0\" Name=\"A-Parent\" NamePrefix=\"A\" BaseName=\"Parent\" ShowMasterItems=\"true\" PageCount=\"{}\" PrimaryTextFrame=\"n\">\n",
            master_pages.len()
        ));
        for (id, left) in &master_pages {
            m.push_str(&self.page(id, "A", self.facing && *left, None));
        }
        m.push_str("\t</MasterSpread>\n</idPkg:MasterSpread>\n");
        parts.push((format!("MasterSpreads/MasterSpread_{master}.xml"), m));

        // ---- spreads
        for (k, (id, pages)) in spread_ids.iter().zip(&spreads).enumerate() {
            let left_pages = pages.iter().filter(|n| self.facing && *n % 2 == 0).count();
            let mut s = String::new();
            s.push_str(HEAD);
            s.push_str(&format!("<idPkg:Spread {NS}>\n"));
            s.push_str(&format!(
                "\t<Spread Self=\"{id}\" AllowPageShuffle=\"true\" ItemTransform=\"1 0 0 1 0 {}\" ShowMasterItems=\"true\" PageCount=\"{}\" BindingLocation=\"{left_pages}\">\n",
                num(k as f64 * (self.height + SPREAD_GAP)),
                pages.len()
            ));
            for n in pages {
                s.push_str(&self.page(&page_ids[n], &n.to_string(), self.facing && n % 2 == 0, Some(&master)));
            }
            s.push_str("\t</Spread>\n</idPkg:Spread>\n");
            parts.push((format!("Spreads/Spread_{id}.xml"), s));
        }

        parts.push(("Resources/Graphic.xml".into(), graphic()));
        parts.push(("Resources/Fonts.xml".into(), self.fonts()));
        parts.push(("Resources/Styles.xml".into(), self.styles()));
        parts.push(("Resources/Preferences.xml".into(), self.preferences()));
        parts.push((
            "META-INF/container.xml".into(),
            format!("{HEAD}<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\t<rootfiles>\n\t\t<rootfile full-path=\"designmap.xml\" media-type=\"text/xml\">\n\t\t</rootfile>\n\t</rootfiles>\n</container>\n"),
        ));
        Ok(parts)
    }

    /// The IDML package (a zip with `mimetype` stored first).
    pub fn package(&self) -> Result<Vec<u8>, String> {
        let parts = self.parts()?;
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            z.start_file("mimetype", stored).map_err(|e| e.to_string())?;
            z.write_all(b"application/vnd.adobe.indesign-idml-package").map_err(|e| e.to_string())?;
            for (name, text) in &parts {
                z.start_file(name.as_str(), deflated).map_err(|e| e.to_string())?;
                z.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            }
            z.finish().map_err(|e| e.to_string())?;
        }
        Ok(out.into_inner())
    }

    /// One `Page`: on a facing spread the left page sits at −width, the right one at 0; a single page
    /// of a non-facing document is centred (−width/2). Left margin is the inside margin.
    fn page(&self, id: &str, name: &str, left: bool, applied_master: Option<&str>) -> String {
        let tx = if self.facing {
            if left { -self.width } else { 0.0 }
        } else {
            -self.width / 2.0
        };
        let rule = if applied_master.is_some() { " LayoutRule=\"UseMaster\"" } else { "" };
        format!(
            "\t\t<Page Self=\"{id}\" GeometricBounds=\"0 0 {} {}\" ItemTransform=\"1 0 0 1 {} {}\" Name=\"{name}\" AppliedMaster=\"{}\"{rule}>\n\t\t\t{}\n\t\t</Page>\n",
            num(self.height),
            num(self.width),
            num(tx),
            num(-self.height / 2.0),
            applied_master.unwrap_or("n"),
            self.margin_preference(),
        )
    }

    /// The page's margins and columns; `ColumnsPositions` are relative to the margin box's left edge.
    fn margin_preference(&self) -> String {
        let text_width = self.width - self.inside - self.outside;
        let n = self.columns as f64;
        let column = (text_width - self.gutter * (n - 1.0)) / n;
        let mut positions = Vec::new();
        for i in 0..self.columns {
            let left = i as f64 * (column + self.gutter);
            positions.push(num(left));
            positions.push(num(left + column));
        }
        format!(
            "<MarginPreference ColumnCount=\"{}\" ColumnGutter=\"{}\" Top=\"{}\" Bottom=\"{}\" Left=\"{}\" Right=\"{}\" ColumnDirection=\"Horizontal\" ColumnsPositions=\"{}\" />",
            self.columns,
            num(self.gutter),
            num(self.top),
            num(self.bottom),
            num(self.inside),
            num(self.outside),
            positions.join(" ")
        )
    }

    fn fonts(&self) -> String {
        let f = &self.font;
        let full = if f.style == "Regular" { f.family.clone() } else { format!("{} {}", f.family, f.style) };
        let name = if f.style == "Regular" { format!("{} Regular", f.family) } else { full.clone() };
        format!(
            "{HEAD}<idPkg:Fonts {NS}>\n\t<FontFamily Self=\"dfont\" Name=\"{fam}\">\n\t\t<Font Self=\"dfontFontn{name}\" FontFamily=\"{fam}\" Name=\"{name}\" PostScriptName=\"{ps}\" Status=\"Installed\" FontStyleName=\"{style}\" FontType=\"{ty}\" WritingScript=\"0\" FullName=\"{full}\" FullNameNative=\"{full}\" FontStyleNameNative=\"{style}\" PlatformName=\"$ID/\" Version=\"\" TypekitID=\"$ID/\" />\n\t</FontFamily>\n</idPkg:Fonts>\n",
            fam = esc(&f.family),
            name = esc(&name),
            full = esc(&full),
            ps = esc(&f.postscript),
            style = esc(&f.style),
            ty = esc(&f.font_type),
        )
    }

    /// The styles InDesign has in every document, by name; `[No paragraph style]` carries the font, the size and the
    /// text colour (the one default Folio does not assume: without it text is not drawn; measured by minimising
    /// the full style, `next/oracle/newdoc/README.ko.md`).
    /// Graphic frames get InDesign's black 1 pt stroke from `[Normal Graphics Frame]`.
    fn styles(&self) -> String {
        let (family, style) = (esc(&self.font.family), esc(&self.font.style));
        format!(
            r#"{HEAD}<idPkg:Styles {NS}>
	<RootCharacterStyleGroup Self="dRootCharacterStyleGroup">
		<CharacterStyle Self="CharacterStyle/$ID/[No character style]" Imported="false" Name="$ID/[No character style]" />
	</RootCharacterStyleGroup>
	<RootParagraphStyleGroup Self="dRootParagraphStyleGroup">
		<ParagraphStyle Self="ParagraphStyle/$ID/[No paragraph style]" Name="$ID/[No paragraph style]" Imported="false" FontStyle="{style}" PointSize="12" FillColor="Color/Black">
			<Properties>
				<AppliedFont type="string">{family}</AppliedFont>
			</Properties>
		</ParagraphStyle>
		<ParagraphStyle Self="ParagraphStyle/$ID/NormalParagraphStyle" Name="$ID/NormalParagraphStyle" Imported="false">
			<Properties>
				<BasedOn type="string">$ID/[No paragraph style]</BasedOn>
			</Properties>
		</ParagraphStyle>
	</RootParagraphStyleGroup>
	<RootCellStyleGroup Self="dRootCellStyleGroup">
		<CellStyle Self="CellStyle/$ID/[None]" AppliedParagraphStyle="ParagraphStyle/$ID/[No paragraph style]" Name="$ID/[None]" />
	</RootCellStyleGroup>
	<RootTableStyleGroup Self="dRootTableStyleGroup">
		<TableStyle Self="TableStyle/$ID/[No table style]" Name="$ID/[No table style]" />
		<TableStyle Self="TableStyle/$ID/[Basic Table]" Name="$ID/[Basic Table]">
			<Properties>
				<BasedOn type="string">$ID/[No table style]</BasedOn>
			</Properties>
		</TableStyle>
	</RootTableStyleGroup>
	<RootObjectStyleGroup Self="dRootObjectStyleGroup">
		<ObjectStyle Self="ObjectStyle/$ID/[None]" Name="$ID/[None]" />
		<ObjectStyle Self="ObjectStyle/$ID/[Normal Graphics Frame]" Name="$ID/[Normal Graphics Frame]" StrokeColor="Color/Black" StrokeWeight="1" EnableStroke="true">
			<Properties>
				<BasedOn type="string">$ID/[None]</BasedOn>
			</Properties>
		</ObjectStyle>
		<ObjectStyle Self="ObjectStyle/$ID/[Normal Text Frame]" Name="$ID/[Normal Text Frame]" AppliedParagraphStyle="ParagraphStyle/$ID/NormalParagraphStyle">
			<Properties>
				<BasedOn type="string">$ID/[None]</BasedOn>
			</Properties>
		</ObjectStyle>
		<ObjectStyle Self="ObjectStyle/$ID/[Normal Grid]" Name="$ID/[Normal Grid]" AppliedParagraphStyle="ParagraphStyle/$ID/NormalParagraphStyle">
			<Properties>
				<BasedOn type="string">$ID/[None]</BasedOn>
			</Properties>
		</ObjectStyle>
	</RootObjectStyleGroup>
</idPkg:Styles>
"#
        )
    }

    fn preferences(&self) -> String {
        let b = self.bleed;
        let s = self.slug;
        let uniform = |v: &[f64; 4]| if v.iter().all(|x| *x == v[0]) { "true" } else { "false" };
        let landscape = if self.width > self.height {
            "\t<PrintPreference PrintPageOrientation=\"Landscape\" />\n\t<PrintBookletPrintPreference PrintPageOrientation=\"Landscape\" />\n"
        } else {
            ""
        };
        format!(
            r#"{HEAD}<idPkg:Preferences {NS}>
	<DocumentPreference PageHeight="{h}" PageWidth="{w}" FacingPages="{facing}" DocumentBleedTopOffset="{b0}" DocumentBleedBottomOffset="{b1}" DocumentBleedInsideOrLeftOffset="{b2}" DocumentBleedOutsideOrRightOffset="{b3}" DocumentBleedUniformSize="{bu}" SlugTopOffset="{s0}" SlugBottomOffset="{s1}" SlugInsideOrLeftOffset="{s2}" SlugRightOrOutsideOffset="{s3}" DocumentSlugUniformSize="{su}" PageBinding="LeftToRight" />
	<MarginPreference ColumnCount="{cols}" ColumnGutter="{gutter}" Top="{top}" Bottom="{bottom}" Left="{inside}" Right="{outside}" ColumnDirection="Horizontal" />
	<TextPreference HonourTextIndentsWithTextWrap="true" />
	<TransparencyPreference BlendingSpace="CMYK" GlobalLightAngle="120" GlobalLightAltitude="30" />
{landscape}	<GridPreference DocumentGridShown="false" DocumentGridSnapto="false" HorizontalGridlineDivision="72" VerticalGridlineDivision="72" HorizontalGridSubdivision="8" VerticalGridSubdivision="8" GridsInBack="true" BaselineGridShown="false" BaselineStart="36" BaselineDivision="12" BaselineViewThreshold="75" BaselineGridRelativeOption="TopOfPageOfBaselineGridRelativeOption" />
	<GuidePreference GuidesInBack="false" GuidesShown="true" GuidesLocked="false" GuidesSnapto="true" RulerGuidesViewThreshold="5" />
	<PasteboardPreference PasteboardMargins="-1 72" MinimumSpaceAboveAndBelow="72" MatchPreviewBackgroundToThemeColor="false" />
	<ViewPreference HorizontalMeasurementUnits="{hu}" VerticalMeasurementUnits="{vu}" />
	<PageItemDefault AppliedGraphicObjectStyle="ObjectStyle/$ID/[Normal Graphics Frame]" AppliedTextObjectStyle="ObjectStyle/$ID/[Normal Text Frame]" AppliedGridObjectStyle="ObjectStyle/$ID/[Normal Grid]" FillColor="Swatch/None" StrokeColor="Swatch/None" StrokeWeight="0" />
	<TextWrapPreference Inverse="false" ApplyToMasterPageOnly="false" TextWrapSide="BothSides" TextWrapMode="None">
		<Properties>
			<TextWrapOffset Top="0" Left="0" Bottom="0" Right="0" />
		</Properties>
	</TextWrapPreference>
	<StoryPreference OpticalMarginAlignment="false" OpticalMarginSize="12" FrameType="TextFrameType" StoryOrientation="Horizontal" StoryDirection="LeftToRightDirection" />
	<TextFramePreference TextColumnCount="1" TextColumnGutter="12" TextColumnFixedWidth="144" UseFixedColumnWidth="false" FirstBaselineOffset="AscentOffset" VerticalJustification="TopAlign" AutoSizingType="Off" AutoSizingReferencePoint="CenterPoint">
		<Properties>
			<InsetSpacing type="list">
				<ListItem type="unit">0</ListItem>
				<ListItem type="unit">0</ListItem>
				<ListItem type="unit">0</ListItem>
				<ListItem type="unit">0</ListItem>
			</InsetSpacing>
		</Properties>
	</TextFramePreference>
	<BaselineFrameGridOption UseCustomBaselineFrameGrid="false" StartingOffsetForBaselineFrameGrid="0" BaselineFrameGridRelativeOption="TopOfInset" BaselineFrameGridIncrement="12" />
	<AnchoredObjectSetting AnchoredPosition="InlinePosition" SpineRelative="false" LockPosition="false" PinPosition="true" AnchorPoint="BottomRightAnchor" HorizontalAlignment="LeftAlign" HorizontalReferencePoint="TextFrame" VerticalAlignment="TopAlign" VerticalReferencePoint="LineBaseline" AnchorXoffset="0" AnchorYoffset="0" AnchorSpaceAbove="0" />
	<AnchoredObjectDefault AnchorContent="Unassigned" InitialAnchorHeight="72" InitialAnchorWidth="72" AnchoredParagraphStyle="ParagraphStyle/$ID/[No paragraph style]" AnchoredObjectStyle="ObjectStyle/$ID/[None]" />
	<FrameFittingOption AutoFit="false" LeftCrop="0" TopCrop="0" RightCrop="0" BottomCrop="0" FittingOnEmptyFrame="None" FittingAlignment="CenterAnchor" />
	<TextDefault FontStyle="{style}" PointSize="12" AppliedParagraphStyle="ParagraphStyle/$ID/NormalParagraphStyle" AppliedCharacterStyle="CharacterStyle/$ID/[No character style]">
		<Properties>
			<AppliedFont type="string">{family}</AppliedFont>
		</Properties>
	</TextDefault>
</idPkg:Preferences>
"#,
            h = num(self.height),
            w = num(self.width),
            facing = self.facing,
            b0 = num(b[0]),
            b1 = num(b[1]),
            b2 = num(b[2]),
            b3 = num(b[3]),
            bu = uniform(&b),
            s0 = num(s[0]),
            s1 = num(s[1]),
            s2 = num(s[2]),
            s3 = num(s[3]),
            su = uniform(&s),
            cols = self.columns,
            gutter = num(self.gutter),
            top = num(self.top),
            bottom = num(self.bottom),
            inside = num(self.inside),
            outside = num(self.outside),
            hu = esc(&self.horizontal_units),
            vu = esc(&self.vertical_units),
            style = esc(&self.font.style),
            family = esc(&self.font.family),
        )
    }
}

/// The ten swatches a document starts with after None, Registration, Paper and Black.
const SWATCHES: [(&str, [u8; 4]); 6] = [
    ("C=100 M=0 Y=0 K=0", [100, 0, 0, 0]),
    ("C=0 M=100 Y=0 K=0", [0, 100, 0, 0]),
    ("C=0 M=0 Y=100 K=0", [0, 0, 100, 0]),
    ("C=15 M=100 Y=100 K=0", [15, 100, 100, 0]),
    ("C=75 M=5 Y=100 K=0", [75, 5, 100, 0]),
    ("C=100 M=90 Y=10 K=0", [100, 90, 10, 0]),
];

fn root_color_group() -> String {
    let mut out = String::from("\t<ColorGroup Self=\"ColorGroup/[Root Color Group]\" Name=\"[Root Color Group]\" IsRootColorGroup=\"true\">\n");
    let names = ["Swatch/None", "Color/Registration", "Color/Paper", "Color/Black"].map(str::to_owned).into_iter().chain(SWATCHES.iter().map(|(n, _)| format!("Color/{n}")));
    for (i, n) in names.enumerate() {
        out.push_str(&format!("\t\t<ColorGroupSwatch Self=\"dColorGroupSwatch{i}\" SwatchItemRef=\"{n}\" />\n"));
    }
    out.push_str("\t</ColorGroup>\n");
    out
}

fn graphic() -> String {
    // Keep each built-in color attribute explicit to preserve its serialized defaults.
    #[allow(clippy::too_many_arguments)]
    fn color(name: &str, v: [u8; 4], model: &str, over: &str, editable: bool, removable: bool, visible: bool, group: &str) -> String {
        format!(
            "\t<Color Self=\"Color/{name}\" Model=\"{model}\" Space=\"CMYK\" ColorValue=\"{} {} {} {}\" ColorOverride=\"{over}\" ConvertToHsb=\"false\" AlternateSpace=\"NoAlternateColor\" AlternateColorValue=\"\" Name=\"{name}\" ColorEditable=\"{editable}\" ColorRemovable=\"{removable}\" Visible=\"{visible}\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"{group}\" />\n",
            v[0], v[1], v[2], v[3]
        )
    }
    let mut out = format!("{HEAD}<idPkg:Graphic {NS}>\n");
    out.push_str(&color("Black", [0, 0, 0, 100], "Process", "Specialblack", false, false, true, "dColorGroupSwatch3"));
    out.push_str(&color("Cyan", [100, 0, 0, 0], "Process", "Hiddenreserved", false, false, false, "n"));
    out.push_str(&color("Magenta", [0, 100, 0, 0], "Process", "Hiddenreserved", false, false, false, "n"));
    out.push_str(&color("Yellow", [0, 0, 100, 0], "Process", "Hiddenreserved", false, false, false, "n"));
    out.push_str(&color("Paper", [0, 0, 0, 0], "Process", "Specialpaper", true, false, true, "dColorGroupSwatch2"));
    out.push_str(&color("Registration", [100, 100, 100, 100], "Registration", "Specialregistration", false, false, true, "dColorGroupSwatch1"));
    for (i, (name, v)) in SWATCHES.iter().enumerate() {
        out.push_str(&color(name, *v, "Process", "Normal", true, true, true, &format!("dColorGroupSwatch{}", i + 4)));
    }
    // the Ink Manager's four process inks follow the colours, as in InDesign (oracle: newdoc/reference/inks-new-document.json)
    for ink in folio_idml::inks::PROCESS_INKS {
        out.push('\t');
        out.push_str(ink);
        out.push('\n');
    }
    out.push_str("\t<Swatch Self=\"Swatch/None\" Name=\"None\" ColorEditable=\"false\" ColorRemovable=\"false\" Visible=\"true\" SwatchCreatorID=\"7937\" SwatchColorGroupReference=\"dColorGroupSwatch0\" />\n");
    out.push_str("</idPkg:Graphic>\n");
    out
}

/// The variables a document has from the start (Chapter Number … Running Header).
const TEXT_VARIABLES: &str = r#"	<TextVariable Self="dTextVariablenChapter Number" Name="Chapter Number" VariableType="ChapterNumberType">
		<ChapterNumberVariablePreference TextBefore="" Format="Current" TextAfter="" />
	</TextVariable>
	<TextVariable Self="dTextVariablenCreation Date" Name="Creation Date" VariableType="CreationDateType">
		<DateVariablePreference TextBefore="" Format="MM/dd/yy" TextAfter="" />
	</TextVariable>
	<TextVariable Self="dTextVariablenFile Name" Name="File Name" VariableType="FileNameType">
		<FileNameVariablePreference TextBefore="" IncludePath="false" IncludeExtension="false" TextAfter="" />
	</TextVariable>
	<TextVariable Self="dTextVariablenImage Name" Name="Image Name" VariableType="LiveCaptionType">
		<CaptionMetadataVariablePreference TextBefore="" MetadataProviderName="$ID/#LinkInfoNameStr" TextAfter="" />
	</TextVariable>
	<TextVariable Self="dTextVariablenLast Page Number" Name="Last Page Number" VariableType="LastPageNumberType">
		<PageNumberVariablePreference TextBefore="" Format="Current" TextAfter="" Scope="SectionScope" />
	</TextVariable>
	<TextVariable Self="dTextVariablenModification Date" Name="Modification Date" VariableType="ModificationDateType">
		<DateVariablePreference TextBefore="" Format="MMMM d, yyyy h:mm aa" TextAfter="" />
	</TextVariable>
	<TextVariable Self="dTextVariablenOutput Date" Name="Output Date" VariableType="OutputDateType">
		<DateVariablePreference TextBefore="" Format="MM/dd/yy" TextAfter="" />
	</TextVariable>
	<TextVariable Self="dTextVariablenRunning Header" Name="Running Header" VariableType="MatchParagraphStyleType">
		<MatchParagraphStylePreference TextBefore="" TextAfter="" AppliedParagraphStyle="ParagraphStyle/$ID/NormalParagraphStyle" SearchStrategy="FirstOnPage" ChangeCase="None" DeleteEndPunctuation="false" />
	</TextVariable>
"#;

/// A number as IDML spells it: the shortest text that reads back as the same f64 (`72`, `12.5`).
fn num(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arial() -> FontChoice {
        FontChoice { family: "Arial".into(), style: "Regular".into(), postscript: "ArialMT".into(), font_type: "TrueType".into() }
    }

    fn doc(pages: u32, facing: bool, start: u32) -> NewDocument {
        NewDocument { pages, facing, start, ..NewDocument::letter(arial()) }
    }

    #[test]
    fn spreads_follow_page_numbers() {
        assert_eq!(doc(4, true, 1).spreads(), vec![vec![1], vec![2, 3], vec![4]]);
        assert_eq!(doc(3, true, 5).spreads(), vec![vec![5], vec![6, 7]]);
        assert_eq!(doc(3, true, 2).spreads(), vec![vec![2, 3], vec![4]]);
        assert_eq!(doc(3, false, 1).spreads(), vec![vec![1], vec![2], vec![3]]);
        assert_eq!(doc(1, true, 1).spreads(), vec![vec![1]]);
    }

    #[test]
    fn numbers_are_idml_spelled() {
        assert_eq!(num(72.0), "72");
        assert_eq!(num(12.5), "12.5");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(-306.0), "-306");
    }

    #[test]
    fn validation_rejects_what_the_dialog_rejects() {
        let mut d = doc(1, true, 1);
        d.pages = 0;
        assert!(d.validate().is_err());
        d = doc(1, true, 1);
        d.top = 400.0;
        d.bottom = 400.0;
        assert!(d.validate().is_err());
        d = doc(1, true, 1);
        d.columns = 3;
        d.gutter = 400.0;
        assert!(d.validate().is_err());
        d = doc(1, true, 1);
        d.horizontal_units = "Furlongs".into();
        assert!(d.validate().is_err());
        assert!(doc(1, true, 1).validate().is_ok());
    }

    /// The package opens with Folio's own reader: sizes, pages, margins and columns come back.
    #[test]
    fn the_package_reads_back() {
        let mut d = NewDocument { top: 20.0, bottom: 30.0, inside: 40.0, outside: 50.0, columns: 3, gutter: 14.0, bleed: [9.0; 4], ..doc(4, true, 1) };
        d.name = "Test & <doc>".into();
        let dir = std::env::temp_dir().join(format!("folio-newdoc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n.idml");
        std::fs::write(&path, d.package().unwrap()).unwrap();
        let doc = folio_idml::read_package(&path, None).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(doc.spreads.len(), 3);
        assert_eq!(doc.spreads.iter().map(|s| s.pages.len()).collect::<Vec<_>>(), vec![1, 2, 1]);
        let page = &doc.spreads[1].pages[0];
        // top, left, bottom, right
        assert_eq!(page.margins, [20.0, 40.0, 30.0, 50.0]);
        assert_eq!(page.columns.count, 3);
        assert_eq!(page.columns.gutter, 14.0);
        assert!(doc.print.facing_pages);
        let measured = folio_idml::read_package(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/xref-cs.idml"), None).unwrap();
        assert_eq!(doc.hyperlinks.formats.len(), 9);
        for (actual, expected) in doc.hyperlinks.formats.iter().zip(measured.hyperlinks.formats.iter()) {
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.character_style, expected.character_style);
            assert_eq!(actual.blocks, expected.blocks);
        }
    }

    /// A new document has the four process inks InDesign's File > New gives (Ink Manager: Angle, NeutralDensity, TrapOrder 1-4), right after
    /// its colours: `oracle/newdoc/reference/inks-new-document.json`, measured in InDesign 2025 (`newdoc-inks-probe.jsx`). Without them
    /// Synchronize Book and a new spot swatch had no place for a spot colour's Ink (mcp-c6, 2026-11-20).
    #[test]
    fn a_new_document_has_the_four_process_inks_of_indesign() {
        use std::io::Read;
        let reference: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/newdoc/reference/inks-new-document.json")).unwrap()).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(doc(2, true, 1).package().unwrap())).unwrap();
        let mut graphic = String::new();
        zip.by_name("Resources/Graphic.xml").unwrap().read_to_string(&mut graphic).unwrap();
        // each <Ink …/> as its attributes, in the order written
        let written: Vec<Vec<(String, String)>> = graphic.split("<Ink ").skip(1).map(|rest| {
            let tag = rest.split("/>").next().unwrap();
            let mut pairs = Vec::new();
            let mut left = tag;
            while let Some((name, tail)) = left.trim_start().split_once("=\"") {
                let (value, after) = tail.split_once('"').unwrap();
                pairs.push((name.trim().to_owned(), value.to_owned()));
                left = after;
            }
            pairs
        }).collect();
        let expected: Vec<Vec<(String, String)>> = reference["inks"].as_array().unwrap().iter().map(|ink| {
            ink.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned())).collect::<Vec<_>>()
        }).collect();
        assert_eq!(expected.len(), 4);
        let sorted = |v: &[Vec<(String, String)>]| v.iter().map(|ink| { let mut ink = ink.clone(); ink.sort(); ink }).collect::<Vec<_>>();
        assert_eq!(sorted(&written), sorted(&expected));
        // the same kinds in the same order around them: Color, Ink, then Swatch
        let kinds: Vec<&str> = graphic.lines().filter_map(|l| l.trim_start().strip_prefix('<')).filter_map(|l| l.split([' ', '>']).next()).filter(|k| ["Color", "Ink", "Swatch"].contains(k)).collect();
        let mut runs = kinds.clone();
        runs.dedup();
        let want: Vec<&str> = reference["kindOrder"].as_array().unwrap().iter().filter_map(|k| k.as_str()).filter(|k| ["Color", "Ink", "Swatch"].contains(k)).collect();
        assert_eq!(runs, want);
        // and the document reads them back as the Ink Manager's list
        let dir = std::env::temp_dir().join(format!("folio-newdoc-inks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n.idml");
        std::fs::write(&path, doc(2, true, 1).package().unwrap()).unwrap();
        let read = folio_idml::read_package(&path, None).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read.print.inks.iter().map(|k| (k.name.as_str(), k.process)).collect::<Vec<_>>(),
            [("Process Cyan", true), ("Process Magenta", true), ("Process Yellow", true), ("Process Black", true)]);
    }
}
