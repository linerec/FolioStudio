//! A small owned XML tree for the parts of a .docx (names without namespace prefix: `w:p` is `p`). Text keeps its
//! whitespace; entities and character references are resolved; a DOCTYPE is refused.
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// Deepest nesting accepted (a hostile file must not exhaust the stack).
const MAX_DEPTH: usize = 256;

#[derive(Debug, Default)]
pub(crate) struct El {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub kids: Vec<Node>,
}

#[derive(Debug)]
pub(crate) enum Node {
    El(El),
    Text(String),
}

impl El {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &El> {
        self.kids.iter().filter_map(|n| match n {
            Node::El(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&El> {
        self.elements().find(|e| e.name == name)
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for n in &self.kids {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::El(e) => out.push_str(&e.text()),
            }
        }
        out
    }
}

fn local(qname: &str) -> String {
    qname.rsplit(':').next().unwrap_or("").to_owned()
}

fn start(e: &BytesStart<'_>) -> Result<El, String> {
    let mut attrs = Vec::new();
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|err| format!("속성을 읽을 수 없습니다: {err}"))?;
        let key = a.key.as_ref().to_owned();
        if key == "xmlns" || key.starts_with("xmlns:") {
            continue;
        }
        let value = a.normalized_value(XmlVersion::Implicit1_0).map_err(|err| format!("속성 값을 읽을 수 없습니다: {err}"))?;
        attrs.push((key.rsplit(':').next().unwrap_or("").to_owned(), value.into_owned()));
    }
    Ok(El { name: local(e.name().as_ref()), attrs, kids: Vec::new() })
}

fn push_text(parent: &mut El, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Node::Text(t)) = parent.kids.last_mut() {
        t.push_str(text);
    } else {
        parent.kids.push(Node::Text(text.to_owned()));
    }
}

pub(crate) fn parse(src: &str) -> Result<El, String> {
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;
    let mut stack = vec![El::default()];
    loop {
        match reader.read_event().map_err(|e| format!("XML 오류 (위치 {}): {e}", reader.buffer_position()))? {
            Event::Start(e) => {
                if stack.len() > MAX_DEPTH {
                    return Err("XML 중첩이 너무 깊습니다".into());
                }
                stack.push(start(&e)?);
            }
            Event::Empty(e) => {
                let el = start(&e)?;
                stack.last_mut().expect("the root sentinel remains until parsing finishes").kids.push(Node::El(el));
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("XML 끝 태그가 맞지 않습니다")?;
                stack.last_mut().ok_or("XML 끝 태그가 맞지 않습니다")?.kids.push(Node::El(el));
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                push_text(stack.last_mut().expect("the root sentinel remains until parsing finishes"), &s);
            }
            Event::CData(t) => {
                let s = t.xml10_content();
                push_text(stack.last_mut().expect("the root sentinel remains until parsing finishes"), &s);
            }
            Event::GeneralRef(r) => {
                let resolved = if let Some(c) = r.resolve_char_ref().map_err(|e| format!("문자 참조: {e}"))? {
                    c.to_string()
                } else {
                    let name = r.xml10_content();
                    quick_xml::escape::resolve_predefined_entity(&name).ok_or_else(|| format!("알 수 없는 엔티티 &{name};"))?.to_owned()
                };
                push_text(stack.last_mut().expect("the root sentinel remains until parsing finishes"), &resolved);
            }
            Event::DocType(_) => return Err("DOCTYPE 선언은 받지 않습니다".into()),
            Event::PI(_) | Event::Decl(_) | Event::Comment(_) => {}
            Event::Eof => break,
        }
    }
    if stack.len() != 1 {
        return Err("XML 이 끝나기 전에 문서가 끝났습니다".into());
    }
    let doc = stack.pop().expect("the root sentinel remains until parsing finishes");
    doc.kids.into_iter().find_map(|n| if let Node::El(e) = n { Some(e) } else { None }).ok_or_else(|| "XML 에 루트 요소가 없습니다".to_owned())
}
