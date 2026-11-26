use folio_model::numbering::NumberingList;
pub const DEFAULT: &str = "NumberingList/$ID/[Default]";
pub fn read(source: &str) -> Result<Vec<NumberingList>, String> {
    let root = crate::xml::parse(source).map_err(|e| e.0)?;
    Ok(from_element(&root))
}
pub(crate) fn from_element(root: &crate::xml::Element<'_>) -> Vec<NumberingList> {
    root.elements().filter(|e| e.name == "NumberingList").map(|e| NumberingList {
        id: e.attr("Self").unwrap_or_default().into(), name: e.attr("Name").unwrap_or_default().into(),
        across_stories: e.attr("ContinueNumbersAcrossStories") == Some("true"),
        across_documents: e.attr("ContinueNumbersAcrossDocuments") == Some("true"),
    }).collect()
}
