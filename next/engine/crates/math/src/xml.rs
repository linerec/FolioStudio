//! A small reader for MathML source: elements, attributes, text, entities. Namespace prefixes
//! (`m:math`) are dropped; comments, processing instructions and a doctype are skipped.

use crate::MathError;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Child>,
}

#[derive(Clone, Debug)]
pub enum Child {
    Elem(Node),
    Text(String),
}

impl Node {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    pub fn elems(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|c| match c {
            Child::Elem(n) => Some(n),
            Child::Text(_) => None,
        })
    }
    /// The text of a token element: its text children, white space collapsed and trimmed.
    pub fn text(&self) -> String {
        let mut raw = String::new();
        for c in &self.children {
            match c {
                Child::Text(t) => raw.push_str(t),
                Child::Elem(n) => raw.push_str(&n.text()),
            }
        }
        raw.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "lt" => "<", "gt" => ">", "amp" => "&", "quot" => "\"", "apos" => "'",
        "nbsp" | "NonBreakingSpace" => "\u{A0}",
        "InvisibleTimes" | "it" => "\u{2062}", "ApplyFunction" | "af" => "\u{2061}",
        "InvisibleComma" | "ic" => "\u{2063}", "InvisiblePlus" => "\u{2064}",
        "ThinSpace" | "thinsp" => "\u{2009}", "MediumSpace" => "\u{205F}", "ThickSpace" => "\u{2005}\u{200A}",
        "emsp" => "\u{2003}", "ensp" => "\u{2002}", "hairsp" => "\u{200A}",
        "plusmn" | "pm" | "PlusMinus" => "\u{B1}", "mnplus" | "mp" | "MinusPlus" => "\u{2213}",
        "times" => "\u{D7}", "divide" | "div" => "\u{F7}", "middot" | "centerdot" => "\u{B7}",
        "sdot" => "\u{22C5}", "minus" => "\u{2212}", "lowast" | "ast" => "\u{2217}", "compfn" | "SmallCircle" => "\u{2218}",
        "sum" | "Sum" => "\u{2211}", "prod" | "Product" => "\u{220F}", "coprod" => "\u{2210}",
        "int" | "Integral" => "\u{222B}", "Int" => "\u{222C}", "tint" => "\u{222D}", "oint" | "ContourIntegral" => "\u{222E}",
        "infin" => "\u{221E}", "radic" | "Sqrt" => "\u{221A}", "part" | "PartialD" => "\u{2202}", "nabla" | "Del" => "\u{2207}",
        "le" | "leq" => "\u{2264}", "ge" | "geq" => "\u{2265}", "ne" | "NotEqual" => "\u{2260}",
        "approx" | "ap" | "asymp" => "\u{2248}", "equiv" | "Congruent" => "\u{2261}", "sim" => "\u{223C}", "cong" => "\u{2245}",
        "prop" | "propto" => "\u{221D}", "ll" => "\u{226A}", "gg" => "\u{226B}",
        "rarr" | "rightarrow" | "RightArrow" | "srarr" => "\u{2192}", "larr" | "leftarrow" | "LeftArrow" => "\u{2190}",
        "uarr" | "uparrow" => "\u{2191}", "darr" | "downarrow" => "\u{2193}", "harr" | "leftrightarrow" => "\u{2194}",
        "rArr" | "Rightarrow" | "Implies" => "\u{21D2}", "lArr" | "Leftarrow" => "\u{21D0}", "hArr" | "iff" | "Leftrightarrow" => "\u{21D4}",
        "mapsto" | "map" => "\u{21A6}",
        "forall" | "ForAll" => "\u{2200}", "exist" | "Exists" => "\u{2203}", "nexist" => "\u{2204}", "empty" | "emptyset" => "\u{2205}",
        "isin" | "in" | "Element" => "\u{2208}", "notin" | "NotElement" => "\u{2209}", "ni" | "niv" => "\u{220B}",
        "sub" | "subset" => "\u{2282}", "sup" | "supset" => "\u{2283}", "sube" | "subseteq" => "\u{2286}", "supe" | "supseteq" => "\u{2287}",
        "cap" => "\u{2229}", "cup" => "\u{222A}", "and" | "wedge" => "\u{2227}", "or" | "vee" => "\u{2228}", "not" | "neg" => "\u{AC}",
        "perp" | "bottom" => "\u{22A5}", "parallel" | "par" => "\u{2225}", "ang" | "angle" => "\u{2220}",
        "deg" => "\u{B0}", "prime" => "\u{2032}", "Prime" => "\u{2033}",
        "hellip" | "mldr" => "\u{2026}", "cdots" | "ctdot" => "\u{22EF}", "vellip" => "\u{22EE}", "ddots" | "dtdot" => "\u{22F1}", "ldots" => "\u{2026}",
        "lang" | "langle" | "LeftAngleBracket" => "\u{27E8}", "rang" | "rangle" | "RightAngleBracket" => "\u{27E9}",
        "lfloor" | "LeftFloor" => "\u{230A}", "rfloor" | "RightFloor" => "\u{230B}", "lceil" | "LeftCeiling" => "\u{2308}", "rceil" | "RightCeiling" => "\u{2309}",
        "Vert" | "Verbar" => "\u{2016}", "verbar" | "vert" => "|",
        "OverBar" | "macr" => "\u{AF}", "UnderBar" => "_", "hat" | "Hat" => "^", "tilde" | "Tilde" => "\u{2DC}", "circ" => "\u{2C6}",
        "OverBrace" => "\u{23DE}", "UnderBrace" => "\u{23DF}", "OverParenthesis" => "\u{23DC}", "UnderParenthesis" => "\u{23DD}",
        "alpha" => "\u{3B1}", "beta" => "\u{3B2}", "gamma" => "\u{3B3}", "delta" => "\u{3B4}", "epsilon" | "epsi" => "\u{3B5}", "varepsilon" | "epsiv" => "\u{3F5}",
        "zeta" => "\u{3B6}", "eta" => "\u{3B7}", "theta" => "\u{3B8}", "vartheta" | "thetav" => "\u{3D1}", "iota" => "\u{3B9}", "kappa" => "\u{3BA}",
        "lambda" => "\u{3BB}", "mu" => "\u{3BC}", "nu" => "\u{3BD}", "xi" => "\u{3BE}", "omicron" => "\u{3BF}", "pi" => "\u{3C0}", "varpi" | "piv" => "\u{3D6}",
        "rho" => "\u{3C1}", "varrho" | "rhov" => "\u{3F1}", "sigma" => "\u{3C3}", "varsigma" | "sigmav" => "\u{3C2}", "tau" => "\u{3C4}", "upsilon" => "\u{3C5}",
        "phi" => "\u{3C6}", "varphi" | "phiv" => "\u{3D5}", "chi" => "\u{3C7}", "psi" => "\u{3C8}", "omega" => "\u{3C9}",
        "Gamma" => "\u{393}", "Delta" => "\u{394}", "Theta" => "\u{398}", "Lambda" => "\u{39B}", "Xi" => "\u{39E}", "Pi" => "\u{3A0}",
        "Sigma" => "\u{3A3}", "Upsilon" => "\u{3A5}", "Phi" => "\u{3A6}", "Psi" => "\u{3A8}", "Omega" => "\u{3A9}",
        "aleph" => "\u{2135}", "hbar" | "planck" => "\u{210F}", "ell" => "\u{2113}", "Re" => "\u{211C}", "Im" => "\u{2111}",
        "naturals" | "Nopf" => "\u{2115}", "integers" | "Zopf" => "\u{2124}", "rationals" | "Qopf" => "\u{211A}", "reals" | "Ropf" => "\u{211D}", "complexes" | "Copf" => "\u{2102}",
        "plus" => "+", "equals" => "=", "comma" => ",", "colon" => ":", "semi" => ";", "excl" => "!", "quest" => "?", "sol" => "/", "bsol" => "\\",
        "lpar" => "(", "rpar" => ")", "lsqb" | "lbrack" => "[", "rsqb" | "rbrack" => "]", "lcub" | "lbrace" => "{", "rcub" | "rbrace" => "}",
        "copy" => "\u{A9}", "star" => "\u{2606}", "therefore" => "\u{2234}", "because" => "\u{2235}", "square" => "\u{25A1}",
        _ => return None,
    })
}

fn decode(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_owned();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|&e| e <= 32) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = if let Some(num) = name.strip_prefix('#') {
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => num.parse().ok(),
            };
            code.and_then(char::from_u32).map(String::from)
        } else {
            named_entity(name).map(str::to_owned)
        };
        match ch {
            Some(c) => out.push_str(&c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn parse(src: &str) -> Result<Node, MathError> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let err = |m: &str| MathError(format!("MathML: {m}"));
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let end = src[i..].find('<').map_or(bytes.len(), |e| i + e);
            let text = decode(&src[i..end]);
            if let Some(top) = stack.last_mut() {
                if !text.trim().is_empty() || matches!(top.name.as_str(), "mtext" | "mi" | "mn" | "mo" | "ms") {
                    top.children.push(Child::Text(text));
                }
            } else if !text.trim().is_empty() {
                return Err(err("text outside the root element"));
            }
            i = end;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").ok_or_else(|| err("unterminated comment"))? + 3;
        } else if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").ok_or_else(|| err("unterminated CDATA"))?;
            if let Some(top) = stack.last_mut() {
                top.children.push(Child::Text(rest[9..end].to_owned()));
            }
            i += end + 3;
        } else if rest.starts_with("<?") {
            i += rest.find("?>").ok_or_else(|| err("unterminated declaration"))? + 2;
        } else if rest.starts_with("<!") {
            i += rest.find('>').ok_or_else(|| err("unterminated doctype"))? + 1;
        } else if let Some(close) = rest.strip_prefix("</") {
            let end = close.find('>').ok_or_else(|| err("unterminated end tag"))?;
            let name = local(close[..end].trim()).to_owned();
            let node = stack.pop().ok_or_else(|| err("end tag without start tag"))?;
            if node.name != name {
                return Err(err(&format!("<{}> closed by </{name}>", node.name)));
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(Child::Elem(node)),
                None => root = Some(node),
            }
            i += end + 3;
        } else {
            let mut j = 1;
            let mut quote = None;
            while j < rest.len() {
                let b = rest.as_bytes()[j];
                match quote {
                    Some(q) if b == q => quote = None,
                    Some(_) => {}
                    None if b == b'"' || b == b'\'' => quote = Some(b),
                    None if b == b'>' => break,
                    None => {}
                }
                j += 1;
            }
            if j >= rest.len() {
                return Err(err("unterminated start tag"));
            }
            let mut tag = &rest[1..j];
            let empty = tag.ends_with('/');
            if empty {
                tag = &tag[..tag.len() - 1];
            }
            let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
            let name = local(&tag[..name_end]).to_owned();
            if name.is_empty() {
                return Err(err("empty tag name"));
            }
            let mut attrs = Vec::new();
            let mut a = &tag[name_end..];
            loop {
                a = a.trim_start();
                if a.is_empty() {
                    break;
                }
                let eq = a.find('=').ok_or_else(|| err("attribute without a value"))?;
                let key = a[..eq].trim();
                let after = a[eq + 1..].trim_start();
                let q = after.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or_else(|| err("attribute value is not quoted"))?;
                let vend = after[1..].find(q).ok_or_else(|| err("unterminated attribute value"))?;
                let value = decode(&after[1..1 + vend]);
                if !key.starts_with("xmlns") {
                    attrs.push((local(key).to_owned(), value));
                }
                a = &after[vend + 2..];
            }
            let node = Node { name, attrs, children: Vec::new() };
            if empty {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Child::Elem(node)),
                    None => root = Some(node),
                }
            } else {
                if stack.len() >= 100 {
                    return Err(err("elements are nested too deeply"));
                }
                stack.push(node);
            }
            i += j + 1;
        }
    }
    if !stack.is_empty() {
        return Err(err("unclosed element"));
    }
    root.ok_or_else(|| err("empty input"))
}
