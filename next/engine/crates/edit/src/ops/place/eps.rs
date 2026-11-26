//! IDML for a newly placed EPS (build-a): an `EPS` graphic with the EPS link, so InDesign opens the IDML with the file itself and
//! prints its PostScript. Folio shows and prints the preview the file carries (`folio_render::eps`); preflight says so (EPS_PREVIEW).
//! The attributes follow the `PDF` element of a placed PDF (not measured against an InDesign EPS: STATUS of build-a).
use super::*;

/// A placed Windows metafile (EMF or WMF; build-a): InDesign's `WMF` graphic with the file's link; Folio draws the records as vectors.
/// The type names are the ones InDesign's Links panel shows (not measured in an IDML: STATUS of build-a).
pub(super) fn metafile_element(source: &GraphicSource, graphic: &str, link: &str, matrix: Mat) -> String {
    let name = if source.format == "EMF" { "Windows Enhanced Metafile" } else { "Windows Metafile" };
    let uri = escape(&source.uri);
    let matrix = matrix.text();
    let (width, height) = (geom::num(source.size[0]), geom::num(source.size[1]));
    let bytes = format!("{:x}~{:x}", source.bytes >> 32, source.bytes & 0xffffffff);
    let clip = CLIP.split("<ImageIOPreference").next().unwrap_or_default();
    format!(r#"<WMF Self="{graphic}" OverriddenPageItemProps="" LocalDisplaySetting="Default" ImageTypeName="$ID/{name}" AppliedObjectStyle="ObjectStyle/$ID/[None]" ItemTransform="{matrix}" HorizontalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" VerticalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" Visible="true" Name="$ID/"><Properties><GraphicBounds Left="0" Top="0" Right="{width}" Bottom="{height}"/></Properties>{WRAP}<Link Self="{link}" AssetURL="$ID/" AssetID="$ID/" RenditionData="Actual" LinkResourceURI="{uri}" LinkResourceFormat="$ID/{name}" StoredState="Normal" LinkClassID="35906" LinkClientID="257" LinkResourceModified="false" LinkObjectModified="false" ShowInUI="true" CanEmbed="true" CanUnembed="true" CanPackage="true" ImportPolicy="NoAutoImport" ExportPolicy="NoAutoExport" LinkResourceSize="{bytes}"/>{clip}</WMF>"#)
}

pub(super) fn element(source: &GraphicSource, graphic: &str, link: &str, matrix: Mat) -> String {
    let name = "EPS";
    let uri = escape(&source.uri);
    let matrix = matrix.text();
    let (width, height) = (geom::num(source.size[0]), geom::num(source.size[1]));
    let bytes = format!("{:x}~{:x}", source.bytes >> 32, source.bytes & 0xffffffff);
    let metadata = source.xmp.as_deref().map(|x| format!("<MetadataPacketPreference><Properties><Contents><![CDATA[{x}]]></Contents></Properties></MetadataPacketPreference>")).unwrap_or_default();
    let clip = CLIP.split("<ImageIOPreference").next().unwrap_or_default();
    format!(r#"<EPS Self="{graphic}" Space="$ID/#Links_{space}" OverriddenPageItemProps="" LocalDisplaySetting="Default" ImageTypeName="$ID/{name}" AppliedObjectStyle="ObjectStyle/$ID/[None]" ItemTransform="{matrix}" HorizontalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" VerticalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" Visible="true" Name="$ID/"><Properties><GraphicBounds Left="0" Top="0" Right="{width}" Bottom="{height}"/></Properties>{WRAP}{metadata}<Link Self="{link}" AssetURL="$ID/" AssetID="$ID/" RenditionData="Actual" LinkResourceURI="{uri}" LinkResourceFormat="$ID/{name}" StoredState="Normal" LinkClassID="35906" LinkClientID="257" LinkResourceModified="false" LinkObjectModified="false" ShowInUI="true" CanEmbed="true" CanUnembed="true" CanPackage="true" ImportPolicy="NoAutoImport" ExportPolicy="NoAutoExport" LinkResourceSize="{bytes}"/>{clip}</EPS>"#,
        space = source.space)
}
