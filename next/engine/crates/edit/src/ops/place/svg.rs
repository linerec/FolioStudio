use super::*;

pub(super) fn element(source: &GraphicSource, graphic: &str, link: &str, m: Mat, embedded: Option<&str>) -> String {
    format!(r#"<SVG Self="{graphic}" UseSVGAs="EmbedCode" IsMathMLObject="false" OverriddenPageItemProps="" LocalDisplaySetting="Default" ImageTypeName="$ID/SVG" AppliedObjectStyle="ObjectStyle/$ID/[None]" ItemTransform="{matrix}" ParentInterfaceChangeCount="" TargetInterfaceChangeCount="" LastUpdatedInterfaceChangeCount="" HorizontalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" VerticalLayoutConstraints="FlexibleDimension FixedDimension FlexibleDimension" Visible="true" Name="$ID/"><Properties>{contents}<GraphicBounds Left="0" Top="0" Right="{width}" Bottom="{height}"/></Properties>{WRAP}<Link Self="{link}" AssetURL="$ID/" AssetID="$ID/" RenditionData="Actual" LinkResourceURI="{uri}" LinkResourceFormat="$ID/SVG" StoredState="{stored}" LinkClassID="35906" LinkClientID="257" LinkResourceModified="false" LinkObjectModified="false" ShowInUI="true" CanEmbed="true" CanUnembed="true" CanPackage="true" ImportPolicy="NoAutoImport" ExportPolicy="NoAutoExport" LinkResourceSize="{bytes}"/></SVG>"#,
        matrix=m.text(), width=geom::num(source.size[0]), height=geom::num(source.size[1]), uri=escape(&source.uri),
        stored=if embedded.is_some() {"Embedded"} else {"Normal"},
        contents=embedded.map(|d|format!("<Contents><![CDATA[{d}]]></Contents>")).unwrap_or_default(),
        bytes=format!("{:x}~{:x}",source.bytes>>32,source.bytes & 0xffffffff))
}
