use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn attr(&self, key: &str) -> Option<&str> { self.attrs.get(key).map(String::as_str) }
}

fn decode(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let end = rest[i..].find(';').ok_or("SVG: unterminated entity")? + i;
        let e = &rest[i+1..end];
        let c = match e {
            "amp" => '&', "lt" => '<', "gt" => '>', "quot" => '"', "apos" => '\'',
            _ => {
                let n = if let Some(v) = e.strip_prefix("#x") { u32::from_str_radix(v, 16).ok() }
                    else { e.strip_prefix('#').and_then(|v| v.parse().ok()) };
                n.and_then(char::from_u32).ok_or("SVG: unknown entity")?
            }
        };
        out.push(c);
        rest = &rest[end+1..];
    }
    out.push_str(rest);
    Ok(out)
}

pub(super) fn parse(text: &str) -> Result<Node, String> {
    if text.len() > 8 * 1024 * 1024 { return Err("SVG: file exceeds 8 MB".into()); }
    let mut rest = text.trim_start_matches('\u{feff}');
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    let mut count = 0;
    while !rest.trim().is_empty() {
        let open = rest.find('<').ok_or("SVG: text outside element")?;
        if !rest[..open].trim().is_empty() && !stack.last().is_some_and(|n|matches!(n.name.as_str(),"title"|"desc"|"metadata")) { return Err("SVG: text content is not supported".into()); }
        rest = &rest[open..];
        if rest.starts_with("<!--") {
            let end = rest.find("-->").ok_or("SVG: unterminated comment")?;
            rest = &rest[end+3..]; continue;
        }
        if rest.starts_with("<?xml ") {
            let end = rest.find("?>").ok_or("SVG: unterminated declaration")?;
            rest = &rest[end+2..]; continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") { return Err("SVG: DTD and processing instructions are not supported".into()); }
        let mut quote = None;
        let end = rest.char_indices().skip(1).find_map(|(i,c)| {
            if Some(c) == quote { quote = None; }
            else if quote.is_none() {
                if c == '\'' || c == '"' { quote = Some(c); }
                else if c == '>' { return Some(i); }
            }
            None
        }).ok_or("SVG: unterminated tag")?;
        let tag = &rest[1..end]; rest = &rest[end+1..];
        if let Some(name) = tag.strip_prefix('/') {
            let node = stack.pop().ok_or("SVG: unexpected closing tag")?;
            if name.trim() != node.name { return Err("SVG: mismatched closing tag".into()); }
            if let Some(parent) = stack.last_mut() { parent.children.push(node); }
            else if root.replace(node).is_some() { return Err("SVG: multiple roots".into()); }
            continue;
        }
        let empty = tag.ends_with('/');
        let tag = tag.strip_suffix('/').unwrap_or(tag).trim();
        let split = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let name = &tag[..split];
        if name.is_empty() { return Err("SVG: empty element".into()); }
        let mut attrs = BTreeMap::new();
        let mut a = tag[split..].trim();
        while !a.is_empty() {
            let eq = a.find('=').ok_or("SVG: attribute without value")?;
            let key = a[..eq].trim();
            if key.is_empty() || key.contains(char::is_whitespace) { return Err("SVG: invalid attribute".into()); }
            a = a[eq+1..].trim_start();
            let q = a.chars().next().filter(|c| *c == '\'' || *c == '"').ok_or("SVG: unquoted attribute")?;
            let end = a[1..].find(q).ok_or("SVG: unterminated attribute")? + 1;
            if attrs.insert(key.to_string(), decode(&a[1..end])?).is_some() { return Err("SVG: duplicate attribute".into()); }
            a = a[end+1..].trim_start();
        }
        count += 1;
        if count > 20000 || stack.len() > 64 { return Err("SVG: too many elements or nesting levels".into()); }
        let node = Node { name: name.to_string(), attrs, children: Vec::new() };
        if empty {
            if let Some(parent) = stack.last_mut() { parent.children.push(node); }
            else if root.replace(node).is_some() { return Err("SVG: multiple roots".into()); }
        } else { stack.push(node); }
    }
    if !stack.is_empty() { return Err("SVG: unclosed element".into()); }
    root.filter(|n| n.name == "svg").ok_or_else(|| "SVG: missing svg root".into())
}
