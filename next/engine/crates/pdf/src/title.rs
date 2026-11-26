//! PDF 제목의 경로 노출을 막는 경계. OS와 무관하게 두 구분자를 처리한다.

pub(crate) fn document_title(explicit: Option<&str>, document_name: &str) -> String {
    let value = explicit.unwrap_or(document_name);
    let bytes = value.as_bytes();
    let path = value.starts_with('/') || value.starts_with('\\') || value.starts_with("file:")
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':');
    let title = if explicit.is_none() || path {
        value.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("")
    } else { value };
    if title.is_empty() { "Untitled".into() } else { title.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_of_all_platforms_are_not_titles() {
        for path in [r"C:\Users\private\book.folio", "/Users/private/book.folio", "/home/private/book.folio",
                     r"\\server\share\book.folio", "file:///home/private/book.folio", "relative/book.folio"] {
            assert_eq!(document_title(None, path), "book.folio");
        }
        assert_eq!(document_title(Some(r"C:\private\chosen.pdf"), "fallback.folio"), "chosen.pdf");
        assert_eq!(document_title(Some("2026/10 특별호"), "fallback.folio"), "2026/10 특별호");
        assert_eq!(document_title(None, ""), "Untitled");
    }
}
