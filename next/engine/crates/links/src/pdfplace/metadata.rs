//! The source packet is kept intact. Older PDFs carry only the document information
//! dictionary; its standard fields are represented as XMP in the placed graphic.
use lopdf::{Document, Object};

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn date(s: &str) -> Option<String> {
    let s = s.strip_prefix("D:").unwrap_or(s);
    if s.len() < 14 || !s.is_ascii() || !s[..14].bytes().all(|b| b.is_ascii_digit()) { return None; }
    let zone = s[14..].replace('\'', "");
    let zone = if zone.is_empty() || matches!(zone.as_str(), "Z" | "+0000" | "-0000") { "Z".into() }
        else if zone.len() == 5 && matches!(zone.as_bytes()[0], b'+' | b'-') && zone[1..].bytes().all(|b| b.is_ascii_digit()) {
            format!("{}:{}", &zone[..3], &zone[3..])
        } else { return None };
    Some(format!("{}-{}-{}T{}:{}:{}{zone}",&s[..4],&s[4..6],&s[6..8],&s[8..10],&s[10..12],&s[12..14]))
}

pub(super) fn read(doc: &Document) -> Result<String, String> {
    if let Ok(meta) = doc.catalog().and_then(|c| c.get(b"Metadata")) {
        let (_, obj) = doc.dereference(meta).map_err(|e| e.to_string())?;
        let stream = obj.as_stream().map_err(|e| e.to_string())?;
        let data = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        return String::from_utf8(data).map_err(|_| "PDF XMP is not UTF-8".into());
    }
    let info = doc.trailer.get(b"Info").ok().and_then(|o| doc.dereference(o).ok()).and_then(|(_,o)| o.as_dict().ok());
    let value = |name: &[u8]| -> Option<String> {
        let obj = info?.get(name).ok()?;
        let (_,obj) = doc.dereference(obj).ok()?;
        match obj { Object::Name(n) => Some(String::from_utf8_lossy(n).into()), _ => lopdf::decode_text_string(obj).ok() }
    };
    let mut xmp = String::new(); let mut pdf = String::new(); let mut dc = String::new();
    for (key, tag) in [(b"CreationDate".as_slice(),"CreateDate"),(b"Creator","CreatorTool"),(b"ModDate","ModifyDate")] {
        if let Some(v) = value(key) {
            let v = if tag.ends_with("Date") { date(&v).ok_or("PDF metadata date is not supported yet")? } else { v };
            xmp.push_str(&format!("         <xmp:{tag}>{}</xmp:{tag}>\n",escape(&v)));
        }
    }
    for key in ["Keywords","Producer","Trapped"] {
        if let Some(v) = value(key.as_bytes()) {
            if v.is_empty() { pdf.push_str(&format!("         <pdf:{key}/>\n")); }
            else { pdf.push_str(&format!("         <pdf:{key}>{}</pdf:{key}>\n",escape(&v))); }
        }
    }
    for (key,tag,kind) in [("Author","creator","Seq"),("Subject","description","Alt"),("Title","title","Alt")] {
        if let Some(v) = value(key.as_bytes()) {
            let lang = if kind == "Alt" { " xml:lang=\"x-default\"" } else { "" };
            dc.push_str(&format!("         <dc:{tag}>\n            <rdf:{kind}>\n               <rdf:li{lang}>{}</rdf:li>\n            </rdf:{kind}>\n         </dc:{tag}>\n",escape(&v)));
        }
    }
    let mut description = "      <rdf:Description rdf:about=\"\"".to_owned();
    if xmp.is_empty() && pdf.is_empty() && dc.is_empty() { description.push_str("/>\n"); }
    else {
        for (body,prefix,uri) in [(&xmp,"xmp","http://ns.adobe.com/xap/1.0/"),(&pdf,"pdf","http://ns.adobe.com/pdf/1.3/"),(&dc,"dc","http://purl.org/dc/elements/1.1/")] {
            if !body.is_empty() { description.push_str(&format!("\n            xmlns:{prefix}=\"{uri}\"")); }
        }
        description.push_str(&format!(">\n{xmp}{pdf}{dc}      </rdf:Description>\n"));
    }
    Ok(format!("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Folio Studio\">\n   <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n{description}   </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"r\"?>"))
}
