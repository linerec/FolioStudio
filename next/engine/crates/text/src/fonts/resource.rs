//! Installed face metadata for the host to supply to the editor. No file paths cross this boundary.
use super::*;
use std::collections::BTreeMap;

impl FontStore {
    pub fn resource_record(&self, family: &str, style: &str, table: &[FontInfo]) -> Option<BTreeMap<String, String>> {
        let resolved = self.resolve(family, style, table);
        if !resolved.exact { return None; }
        let info = self.face_info(resolved.font)?;
        let (script, lang) = match Writing::of_font(&info.family, None) {
            Writing::Korean | Writing::KoreanSerif => (3, 0x412),
            Writing::Japanese | Writing::JapaneseSerif => (1, 0x411),
            Writing::Chinese => (2, 0x404),
            Writing::Arabic => (4, 0x401), Writing::Hebrew => (5, 0x40d),
            Writing::Thai => (21, 0x41e), Writing::Devanagari => (9, 0x439),
            Writing::Latin => (0, 0x409),
        };
        let native = self.with_face(resolved.font, |face| {
            let names = parse_names(face.raw_face().table(ttf_parser::Tag::from_bytes(b"name"))?, &[2,4,17])?;
            Some((names.in_lang(4, lang).unwrap_or_else(|| info.full_name.clone()),
                names.in_lang(17, lang).or_else(|| names.in_lang(2, lang)).unwrap_or_else(|| info.style.clone())))
        })??;
        Some([
            ("FontFamily", info.family.clone()), ("Name", format!("{} {}", info.family, info.style)),
            ("PostScriptName", info.postscript.clone()), ("Status", "Installed".into()),
            ("FontStyleName", info.style.clone()), ("FontType", if info.cff {"OpenTypeCFF"} else {"OpenTypeTT"}.into()),
            ("WritingScript", script.to_string()), ("FullName", info.full_name.clone()),
            ("FullNameNative", native.0), ("FontStyleNameNative", native.1),
            ("PlatformName", "$ID/".into()), ("Version", info.version.clone()), ("TypekitID", "$ID/".into()),
        ].into_iter().map(|(k,v)|(k.to_owned(),v)).collect())
    }
}
