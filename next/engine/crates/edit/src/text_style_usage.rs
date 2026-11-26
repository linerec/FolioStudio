use crate::{EditError, Editor, xml::Index};
use std::collections::BTreeSet;

impl Editor {
    /// Applied styles throughout every story, including overset and nested text.
    pub fn used_text_styles(&self, kind: &str) -> Result<BTreeSet<String>, EditError> {
        let (element, attribute) = match kind {
            "paragraph" => ("ParagraphStyleRange", "AppliedParagraphStyle"),
            "character" => ("CharacterStyleRange", "AppliedCharacterStyle"),
            // Select All Unused in the Table Styles and Cell Styles panels (a table's and a cell's own style)
            "table" => ("Table", "AppliedTableStyle"),
            "cell" => ("Cell", "AppliedCellStyle"),
            _ => return Err(EditError::new("invalid", "unknown text style kind")),
        };
        let mut used = BTreeSet::new();
        for (name, text) in self.parts() {
            // a table or cell style that another style names (BasedOn, a table style's region cell styles) is in use too: deleting it would change that style
            if (kind == "table" || kind == "cell") && name == "Resources/Styles.xml" {
                let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
                const REGIONS: [&str; 5] = ["HeaderRegionCellStyle", "FooterRegionCellStyle", "LeftColumnRegionCellStyle", "RightColumnRegionCellStyle", "BodyRegionCellStyle"];
                for i in 0..ix.nodes.len() {
                    let tag = ix.local(text, i);
                    if tag != "TableStyle" && tag != "CellStyle" { continue; }
                    if let Some(parent) = ix.value(text, i, "BasedOn") { used.insert(parent); }
                    if kind == "cell" && tag == "TableStyle" {
                        for attr in REGIONS { if let Some(cell) = ix.value(text, i, attr) { used.insert(cell); } }
                    }
                }
                continue;
            }
            if !name.starts_with("Stories/") || !name.ends_with(".xml") { continue; }
            let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
            for i in 0..ix.nodes.len() {
                if ix.local(text, i) == element
                    && let Some(style) = ix.value(text, i, attribute) { used.insert(style); }
            }
        }
        Ok(used)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_and_unplaced_stories_count_as_used() {
        let editor = Editor::new(std::collections::BTreeMap::from([
            ("Stories/Story_unplaced.xml".into(), "<Story><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Hidden\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Body\"><Table><Cell><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Cell\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Nested\"/></ParagraphStyleRange></Cell></Table></CharacterStyleRange></ParagraphStyleRange></Story>".into()),
            ("Resources/Styles.xml".into(), "<Styles><ParagraphStyle Self=\"ParagraphStyle/Unused\"/></Styles>".into()),
        ]));
        assert_eq!(editor.used_text_styles("paragraph").unwrap(), BTreeSet::from(["ParagraphStyle/Hidden".into(), "ParagraphStyle/Cell".into()]));
        assert_eq!(editor.used_text_styles("character").unwrap(), BTreeSet::from(["CharacterStyle/Body".into(), "CharacterStyle/Nested".into()]));
    }
    #[test]
    fn table_and_cell_styles_in_use_are_counted() {
        let editor = Editor::new(std::collections::BTreeMap::from([
            ("Stories/Story_u1.xml".into(), "<Story><Table AppliedTableStyle=\"TableStyle/Grid\"><Cell AppliedCellStyle=\"CellStyle/Head\"/><Cell AppliedCellStyle=\"CellStyle/$ID/[None]\"/></Table></Story>".into()),
            ("Resources/Styles.xml".into(), "<Styles><TableStyle Self=\"TableStyle/Grid\"/><TableStyle Self=\"TableStyle/Spare\"/></Styles>".into()),
        ]));
        assert_eq!(editor.used_text_styles("table").unwrap(), BTreeSet::from(["TableStyle/Grid".into()]));
        assert_eq!(editor.used_text_styles("cell").unwrap(), BTreeSet::from(["CellStyle/Head".into(), "CellStyle/$ID/[None]".into()]));
    }
    #[test]
    fn a_style_another_style_names_is_in_use() {
        let editor = Editor::new(std::collections::BTreeMap::from([
            ("Resources/Styles.xml".into(), "<Styles><CellStyle Self=\"CellStyle/Base\"/><CellStyle Self=\"CellStyle/Child\" BasedOn=\"CellStyle/Base\"/><CellStyle Self=\"CellStyle/Region\"/><CellStyle Self=\"CellStyle/Spare\"/><TableStyle Self=\"TableStyle/T\" HeaderRegionCellStyle=\"CellStyle/Region\"/></Styles>".into()),
        ]));
        let used = editor.used_text_styles("cell").unwrap();
        assert!(used.contains("CellStyle/Base") && used.contains("CellStyle/Region") && !used.contains("CellStyle/Spare") && !used.contains("CellStyle/Child"), "{used:?}");
    }
}
