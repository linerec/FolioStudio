pub const PART: &str = "Resources/Preferences.xml";

pub fn read(source: &str) -> Result<(String, String), String> {
    let root = crate::xml::parse(source).map_err(|e| e.0)?;
    Ok(from_element(root.child("ChapterNumberPreference")))
}

pub(crate) fn from_element(p: Option<&crate::xml::Element<'_>>) -> (String, String) {
    (p.and_then(|p| p.attr("ChapterNumber")).unwrap_or("1").into(),
     p.and_then(|p| p.attr("ChapterNumberSource")).map(str::to_owned).unwrap_or_else(folio_model::book::chapter_source))
}
