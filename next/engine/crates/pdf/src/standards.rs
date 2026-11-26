use crate::Preset;

struct Definition {
    preset: Preset,
    key: &'static str,
    name: &'static str,
    standard: &'static str,
    version: (i32, i32),
    pdfx: &'static str,
    conformance: Option<&'static str>,
    flatten: bool,
    cmyk: bool,
}

const DEFINITIONS: &[Definition] = &[
    Definition { preset: Preset::X4, key: "x4", name: "[PDF/X-4:2008]", standard: "PDF/X-4 (ISO 15930-7:2010)", version: (1,6), pdfx: "PDF/X-4", conformance: None, flatten: false, cmyk: false },
    Definition { preset: Preset::X1a, key: "x1a", name: "[PDF/X-1a:2001]", standard: "PDF/X-1a:2001 (ISO 15930-1:2001)", version: (1,3), pdfx: "PDF/X-1:2001", conformance: Some("PDF/X-1a:2001"), flatten: true, cmyk: true },
    Definition { preset: Preset::X3, key: "x3", name: "[PDF/X-3:2002]", standard: "PDF/X-3:2002 (ISO 15930-3:2002)", version: (1,3), pdfx: "PDF/X-3:2002", conformance: Some("PDF/X-3:2002"), flatten: true, cmyk: false },
    Definition { preset: Preset::High, key: "high", name: "[High Quality Print]", standard: "PDF 1.4", version: (1,4), pdfx: "", conformance: None, flatten: false, cmyk: false },
    Definition { preset: Preset::Press, key: "press", name: "[Press Quality]", standard: "PDF 1.4", version: (1,4), pdfx: "", conformance: None, flatten: false, cmyk: true },
    Definition { preset: Preset::Small, key: "small", name: "[Smallest File Size]", standard: "PDF 1.5", version: (1,5), pdfx: "", conformance: None, flatten: false, cmyk: false },
];

impl Preset {
    fn definition(self) -> &'static Definition { DEFINITIONS.iter().find(|d| d.preset == self).expect("every PDF standard has a registered definition") }
    pub fn all() -> impl Iterator<Item = Preset> { DEFINITIONS.iter().map(|d| d.preset) }
    pub fn parse(s: &str) -> Option<Self> {
        let key: String = s.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let key = key.trim_start_matches("pdf");
        DEFINITIONS.iter().find(|d| key == d.key || key == d.name.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().trim_start_matches("pdf")).map(|d| d.preset)
    }
    pub fn key(self) -> &'static str { self.definition().key }
    pub fn standard(self) -> &'static str { self.definition().standard }
    pub fn indesign_preset(self) -> &'static str { self.definition().name }
    pub fn version(self) -> (i32, i32) { self.definition().version }
    pub fn flattens(self) -> bool { self.definition().flatten }
    pub fn cmyk_only(self) -> bool { self.definition().cmyk }
    pub fn is_pdfx(self) -> bool { !self.definition().pdfx.is_empty() }
    pub fn info_version(self) -> (&'static str, Option<&'static str>) { (self.definition().pdfx, self.definition().conformance) }
    /// Whether the PDF can hold Acrobat layers (optional content, PDF 1.5): every preset but the PDF 1.3 ones (PDF/X-1a, PDF/X-3).
    pub fn layers_allowed(self) -> bool { self.version() >= (1, 4) }
}
