//! Serialized special values used by text style definitions.
pub const AUTOMATIC_KERNING: &str = "1e+11";

pub fn automatic_kerning(value: f64) -> bool {
    value == AUTOMATIC_KERNING.parse::<f64>().expect("automatic kerning constant is a valid number")
}

#[cfg(test)]
mod tests {
    use crate::{
        styles::{apply_char, StyleSheet},
        warn::Warnings,
        xml,
    };

    #[test]
    fn automatic_kerning_resets_an_inherited_manual_adjustment() {
        let mut warnings = Warnings::default();
        let sheet = StyleSheet::new(&[], &mut warnings);
        let mut state = sheet.paragraph(None, &mut warnings).chr.clone();
        let manual = xml::parse(r#"<CharacterStyle KerningValue="-50"/>"#).unwrap();
        apply_char(&mut state, &manual, &mut warnings);
        assert!(state.attrs.kerning_manual);
        assert_eq!(state.attrs.kerning_value, -50.0);
        let absent = xml::parse(r#"<CharacterStyle Tracking="20"/>"#).unwrap();
        apply_char(&mut state, &absent, &mut warnings);
        assert_eq!(state.attrs.kerning_value, -50.0);
        let automatic =
            xml::parse(r#"<CharacterStyle KerningMethod="$ID/Metrics" KerningValue="1e+11"/>"#)
                .unwrap();
        apply_char(&mut state, &automatic, &mut warnings);
        assert!(!state.attrs.kerning_manual);
        assert_eq!(state.attrs.kerning_value, 0.0);
        assert_eq!(state.attrs.tracking, 20.0);
    }
}
