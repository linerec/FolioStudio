//! IDML for a newly placed PDF. Geometry and fitting are shared with other placed graphics.
use super::*;

pub(super) fn element(source: &GraphicSource, graphic: &str, link: &str, matrix: Mat) -> String {
    let pdf = source.pdf.as_ref().unwrap();
    let [left, top, right, bottom] = pdf.bounds.map(geom::num);
    let name = "Adobe Portable Document Format (PDF)";
    let uri = escape(&source.uri);
    let matrix = matrix.text();
    let page = pdf.options.page_number;
    let crop = pdf.options.crop.name();
    let transparent = pdf.options.transparent_background;
    let bytes = format!("{:x}~{:x}", source.bytes >> 32, source.bytes & 0xffffffff);
    let metadata = pdf.metadata.replace("]]>", "]]]]><![CDATA[>");
    let layers = pdf.layers.as_ref().map(|options| options.layers.iter().map(|l| {
        let id=l.id; let name=escape(&l.name); let original=l.original; let visible=l.visible;
        format!(r#"<GraphicLayer Self="{graphic}GraphicLayerOption1i{id}" Name="$ID/{name}" OriginalVisibility="{original}" CurrentVisibility="{visible}" SeparatorLayer="false" AdjustmentLayer="false" FXLayer="false" Locked="false" HasViewState="false" ViewState="false" HasExportState="false" ExportState="false" HasPrintState="false" PrintState="false" Id="{id}"/>"#)
    }).collect::<String>()).unwrap_or_default();
    let clip = CLIP.split("<ImageIOPreference").next().unwrap();
    format!(r#"<PDF Self="{graphic}" GrayVectorPolicy="IgnoreAll" RGBVectorPolicy="HonorAllProfiles" CMYKVectorPolicy="IgnoreAll" OverriddenPageItemProps="" LocalDisplaySetting="Default" ImageTypeName="$ID/{name}" AppliedObjectStyle="ObjectStyle/$ID/[None]" ItemTransform="{matrix}" HorizontalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" VerticalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" Visible="true" Name="$ID/"><Properties><GraphicBounds Left="{left}" Top="{top}" Right="{right}" Bottom="{bottom}"/></Properties>{WRAP}<PDFAttribute PageNumber="{page}" PDFCrop="{crop}" TransparentBackground="{transparent}"/><MetadataPacketPreference><Properties><Contents><![CDATA[{metadata}]]></Contents></Properties></MetadataPacketPreference><Link Self="{link}" AssetURL="$ID/" AssetID="$ID/" RenditionData="Actual" LinkResourceURI="{uri}" LinkResourceFormat="$ID/{name}" StoredState="Normal" LinkClassID="35906" LinkClientID="257" LinkResourceModified="false" LinkObjectModified="false" ShowInUI="true" CanEmbed="true" CanUnembed="true" CanPackage="true" ImportPolicy="NoAutoImport" ExportPolicy="NoAutoExport" LinkResourceSize="{bytes}"/>{clip}<GraphicLayerOption UpdateLinkOption="KeepOverrides">{layers}</GraphicLayerOption></PDF>"#)
}
