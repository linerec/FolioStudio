//! Output intent, XMP metadata and document identity for PDF/X-4, PDF/X-1a and PDF/X-3.

/// OutputConditionIdentifier for well-known characterizations (ICC characterization data
/// registry), by profile description. Unknown profiles are written as "Custom" with the
/// profile embedded.
pub fn condition_identifier(description: &str) -> Option<&'static str> {
    let d = description.to_ascii_lowercase();
    let table: &[(&str, &str)] = &[
        ("u.s. web coated (swop) v2", "CGATS TR 001"),
        ("coated fogra27", "FOGRA27"),
        ("coated fogra39", "FOGRA39"),
        ("uncoated fogra29", "FOGRA29"),
        ("web coated fogra28", "FOGRA28"),
        ("coated gracol 2006", "CGATS TR 006"),
        ("web coated swop 2006 grade 3", "CGATS TR 003"),
        ("web coated swop 2006 grade 5", "CGATS TR 005"),
        ("japan color 2001 coated", "JC200103"),
        ("pso coated v3", "FOGRA51"),
        ("pso uncoated v3", "FOGRA52"),
    ];
    table.iter().find(|(k, _)| d.starts_with(k)).map(|(_, v)| *v)
}

/// ICC header facts: data colour space, class, version and the description text.
pub struct IccFacts {
    pub space: [u8; 4],
    pub class: [u8; 4],
    pub version: (u8, u8),
    pub description: String,
}

pub fn icc_facts(p: &[u8]) -> Option<IccFacts> {
    if p.len() < 132 || &p[36..40] != b"acsp" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]) as usize;
    let mut description = String::new();
    let n = be(128).min(1000);
    for t in 0..n {
        let o = 132 + 12 * t;
        if o + 12 > p.len() {
            break;
        }
        if &p[o..o + 4] == b"desc" {
            let (off, len) = (be(o + 4), be(o + 8));
            let Some(tag) = p.get(off..off.saturating_add(len)) else { break };
            if tag.starts_with(b"desc") && tag.len() >= 12 {
                let cnt = u32::from_be_bytes([tag[8], tag[9], tag[10], tag[11]]) as usize;
                if let Some(s) = tag.get(12..12 + cnt) {
                    description = String::from_utf8_lossy(s).trim_end_matches('\0').to_owned();
                }
            } else if tag.starts_with(b"mluc") && tag.len() >= 16 {
                let rec = u32::from_be_bytes([tag[12], tag[13], tag[14], tag[15]]) as usize;
                if rec >= 12 && tag.len() >= 28 {
                    let l = u32::from_be_bytes([tag[20], tag[21], tag[22], tag[23]]) as usize;
                    let so = u32::from_be_bytes([tag[24], tag[25], tag[26], tag[27]]) as usize;
                    if let Some(s) = tag.get(so..so + l) {
                        let u: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                        description = String::from_utf16_lossy(&u).trim_end_matches('\0').to_owned();
                    }
                }
            }
            break;
        }
    }
    Some(IccFacts {
        space: [p[16], p[17], p[18], p[19]],
        class: [p[12], p[13], p[14], p[15]],
        version: (p[8], p[9] >> 4),
        description,
    })
}

/// UTC civil time of a Unix timestamp.
pub fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's days-from-civil inverse.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32)
}

pub(super) fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn xmp_date(secs: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// A random-looking RFC 4122 v4 UUID from a seed (no randomness source needed).
pub fn uuid(seed: u64) -> String {
    let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        // splitmix64
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let (a, b) = (next(), next());
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&a.to_be_bytes());
    bytes[8..].copy_from_slice(&b.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub struct XmpFields<'a> {
    pub title: &'a str,
    pub creator_tool: &'a str,
    pub producer: &'a str,
    pub date: &'a str,
    pub document_id: &'a str,
    pub instance_id: &'a str,
    /// `GTS_PDFXVersion` ("PDF/X-4", "PDF/X-1:2001", "PDF/X-3:2002").
    pub pdfx_version: &'a str,
    /// `GTS_PDFXConformance` ("PDF/X-1a:2001", "PDF/X-3:2002"); none for PDF/X-4.
    pub pdfx_conformance: Option<&'a str>,
}

/// XMP packet with the PDF/X identification InDesign writes (both namespaces), Trapped
/// False and xmpMM identity.
pub fn xmp(f: &XmpFields<'_>) -> String {
    let conformance = f.pdfx_conformance.map(|c| format!("\n   <pdfx:GTS_PDFXConformance>{c}</pdfx:GTS_PDFXConformance>")).unwrap_or_default();
    let identification = if f.pdfx_version.is_empty() { String::new() } else { format!("<pdfxid:GTS_PDFXVersion>{0}</pdfxid:GTS_PDFXVersion><pdfx:GTS_PDFXVersion>{0}</pdfx:GTS_PDFXVersion>{conformance}", f.pdfx_version) };
    format!(
        r#"<?xpacket begin="{bom}" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/"
    xmlns:pdf="http://ns.adobe.com/pdf/1.3/"
    xmlns:pdfxid="http://www.npes.org/pdfx/ns/id/"
    xmlns:pdfx="http://ns.adobe.com/pdfx/1.3/">
   <dc:format>application/pdf</dc:format>
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">{title}</rdf:li></rdf:Alt></dc:title>
   <xmp:CreateDate>{date}</xmp:CreateDate>
   <xmp:ModifyDate>{date}</xmp:ModifyDate>
   <xmp:MetadataDate>{date}</xmp:MetadataDate>
   <xmp:CreatorTool>{tool}</xmp:CreatorTool>
   <xmpMM:DocumentID>uuid:{doc}</xmpMM:DocumentID>
   <xmpMM:InstanceID>uuid:{inst}</xmpMM:InstanceID>
   <xmpMM:VersionID>1</xmpMM:VersionID>
   <xmpMM:RenditionClass>proof:pdf</xmpMM:RenditionClass>
   <pdf:Producer>{producer}</pdf:Producer>
   <pdf:Trapped>False</pdf:Trapped>
   {identification}
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="r"?>"#,
        bom = '\u{FEFF}',
        title = esc(f.title),
        date = f.date,
        tool = esc(f.creator_tool),
        doc = f.document_id,
        inst = f.instance_id,
        producer = esc(f.producer),
        identification = identification,
    )
}
