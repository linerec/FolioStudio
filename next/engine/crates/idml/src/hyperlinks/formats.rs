//! Cross-reference format definitions written to IDML.
use folio_model::hyperlinks::{BuildingBlock, CrossReferenceFormat};
use quick_xml::escape::escape;

pub fn style(value: &str) -> &str { if value.is_empty() { "n" } else { value } }
fn empty(value: &str) -> &str { if value.is_empty() { "$ID/" } else { value } }

pub fn blocks_xml(format: &CrossReferenceFormat) -> String {
    format.blocks.iter().enumerate().map(|(i,b)| format!(
        "<BuildingBlock Self=\"{}BuildingBlock{i}\" BlockType=\"{}\" AppliedCharacterStyle=\"{}\" CustomText=\"{}\" AppliedDelimiter=\"{}\" IncludeDelimiter=\"{}\" />",
        escape(&format.id),escape(&b.kind),escape(style(&b.character_style)),escape(empty(&b.text)),
        escape(&if b.delimiter.starts_with("$ID/") { b.delimiter.clone() } else { format!("$ID/{}",b.delimiter) }),b.include_delimiter)).collect()
}

pub fn xml(format: &CrossReferenceFormat) -> String {
    format!("<CrossReferenceFormat Self=\"{}\" Name=\"{}\" AppliedCharacterStyle=\"{}\">{}</CrossReferenceFormat>",
        escape(&format.id),escape(&format.name),escape(style(&format.character_style)),blocks_xml(format))
}

pub fn defaults() -> Vec<CrossReferenceFormat> {
    let block = |kind: &str, text: &str| BuildingBlock { kind:kind.into(),text:text.into(),character_style:"n".into(),delimiter:"$ID/".into(),include_delimiter:false };
    let custom = |text: &str| block("CustomStringBuildingBlock",text);
    let page = || block("PageNumberBuildingBlock","$ID/");
    let mut formats = Vec::new();
    for (name,kind,quoted) in [("Full Paragraph","FullParagraphBuildingBlock",true),("Paragraph Text","ParagraphTextBuildingBlock",true),
        ("Paragraph Number","ParagraphNumberBuildingBlock",false),("Text Anchor Name","BookmarkNameBuildingBlock",true)] {
        for with_page in [true,false] {
            let mut blocks=Vec::new();
            if quoted { blocks.push(custom("\"")); }
            blocks.push(block(kind,"$ID/"));
            if quoted || with_page { blocks.push(custom(match (quoted,with_page) { (true,true)=>"\" on page ",(true,false)=>"\"",_=>" on page " })); }
            if with_page { blocks.push(page()); }
            formats.push(CrossReferenceFormat { id:format!("xrefFormat{}",formats.len()),name:format!("{name}{}",if with_page { " & Page Number" } else { "" }),character_style:"n".into(),blocks });
        }
    }
    formats.push(CrossReferenceFormat { id:"xrefFormat8".into(),name:"Page Number".into(),character_style:"n".into(),blocks:vec![custom("page "),page()] });
    formats
}

pub const PAGE_VARIABLE_XML: &str = "<TextVariable Self=\"dTextVariablen&lt;?AID 001b?&gt;TV XRefPageNumber\" Name=\"&lt;?AID 001b?&gt;TV XRefPageNumber\" VariableType=\"XrefPageNumberType\" />";
