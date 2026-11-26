//! The Character panel's Language box (appliedLanguage): the languages InDesign offers, and the `<Language>` element that a
//! document gets in designmap.xml when it first uses one (oracle charpanel-c3-language: cp-lang-uk, cp-lang-french).
use crate::{EditError, Work, xml::{Index, unescape}};
use serde_json::Value;
use std::collections::BTreeMap;

#[path = "languages_table.rs"]
mod table;

const PART: &str = "designmap.xml";

/// One language of the box: `name` is what the file stores (`AppliedLanguage="$ID/<name>"`), `label` what the box shows.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LanguageChoice {
    pub name: String,
    pub label: String,
}

pub fn choices() -> Vec<LanguageChoice> {
    table::LANGUAGES.iter().map(|(name, label, _)| LanguageChoice { name: (*name).into(), label: (*label).into() }).collect()
}

pub(crate) fn known(name: &str) -> bool {
    table::LANGUAGES.iter().any(|(n, _, _)| *n == name)
}

/// Languages a Word file can ask for that the Character panel's box does not offer (oracle docxtext-c1-lang: the `<Language>` elements
/// InDesign 2025 writes for them).
const EXTRA: &[(&str, &str)] = &[
    // (these two differ from the Character panel's element: a Word file brings them in with these ids and vendors)
    ("de_DE_2006", r#"<Language Self="Language/$ID/de_DE_2006" Name="$ID/de_DE_2006" SingleQuotes="‚‘" DoubleQuotes="„“" PrimaryLanguageName="$ID/German" SublanguageName="$ID/German" Id="275" HyphenationVendor="Duden" SpellingVendor="Duden" />"#),
    ("hi_IN", r#"<Language Self="Language/$ID/hi_IN" Name="$ID/hi_IN" SingleQuotes="&apos;&apos;" DoubleQuotes="&quot;&quot;" PrimaryLanguageName="$ID/" SublanguageName="$ID/" Id="309" HyphenationVendor="Hunspell" SpellingVendor="Hunspell" />"#),
    ("Japanese", r#"<Language Self="Language/$ID/Japanese" Name="$ID/Japanese" SingleQuotes="‘’" DoubleQuotes="“”" PrimaryLanguageName="$ID/Japanese" SublanguageName="$ID/" Id="282" HyphenationVendor="$ID/" SpellingVendor="$ID/" />"#),
    ("Chinese: Simplified", r#"<Language Self="Language/$ID/Chinese%3a Simplified" Name="$ID/Chinese: Simplified" SingleQuotes="&apos;&apos;" DoubleQuotes="&quot;&quot;" PrimaryLanguageName="$ID/Chinese" SublanguageName="$ID/Chinese" Id="520" HyphenationVendor="$ID/" SpellingVendor="$ID/" />"#),
    ("Korean", r#"<Language Self="Language/$ID/Korean" Name="$ID/Korean" SingleQuotes="‘’" DoubleQuotes="“”" PrimaryLanguageName="$ID/Korean" SublanguageName="$ID/" Id="283" HyphenationVendor="$ID/" SpellingVendor="$ID/" />"#),
    ("Icelandic", r#"<Language Self="Language/$ID/Icelandic" Name="$ID/Icelandic" SingleQuotes="‘’" DoubleQuotes="“”" PrimaryLanguageName="$ID/Icelandic" SublanguageName="$ID/" Id="279" HyphenationVendor="$ID/" SpellingVendor="$ID/" />"#),
];

fn element_of(name: &str) -> Option<&'static str> {
    table::LANGUAGES.iter().find(|(n, _, _)| *n == name).map(|(_, _, e)| *e)
}

/// The element of a language a text file brings in (a Word file: oracle docxtext-c1-lang).
fn imported_element_of(name: &str) -> Option<&'static str> {
    EXTRA.iter().find(|(n, _)| *n == name).map(|(_, e)| *e).or_else(|| element_of(name))
}

/// The document now uses `appliedLanguage`: its designmap.xml lists the language (after the others) when it did not yet.
pub(super) fn register(w: &mut Work<'_>, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    let Some(name) = props.get("appliedLanguage").and_then(Value::as_str) else { return Ok(()) };
    let Some(element) = element_of(name) else {
        return Err(EditError::new("invalid", format!("unknown language {name}")));
    };
    list_language(w, name, element)
}

/// The same for a language by its name in the file.
pub(super) fn register_name(w: &mut Work<'_>, name: &str) -> Result<(), EditError> {
    let Some(element) = imported_element_of(name) else {
        return Err(EditError::new("invalid", format!("unknown language {name}")));
    };
    list_language(w, name, element)
}

fn list_language(w: &mut Work<'_>, name: &str, element: &str) -> Result<(), EditError> {
    let Some(text) = w.ed.parts.get(PART) else { return Ok(()) };
    let ix = Index::scan(text).map_err(|e| EditError::new("xml", e))?;
    let languages: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(text, i) == "Language").collect();
    let wanted = format!("$ID/{name}");
    if languages.iter().any(|&i| ix.attr(text, i, "Name").is_some_and(|v| unescape(v) == wanted)) {
        return Ok(());
    }
    let (at, xml) = match languages.last() {
        Some(&last) => (ix.nodes[last].end, format!("\n\t{element}")),
        None => {
            let before = (0..ix.nodes.len()).find(|&i| ix.local(text, i) == "Graphic").map(|i| ix.nodes[i].start).unwrap_or(ix.nodes[0].close_start);
            (before, format!("{element}\n\t"))
        }
    };
    w.splice(PART, vec![(at..at, xml)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_is_complete() {
        assert_eq!(table::LANGUAGES.len(), 61);
        assert!(known("English: UK") && known("[No Language]") && !known("Klingon"));
        for (name, _, element) in table::LANGUAGES {
            assert!(element.contains(&format!("Name=\"$ID/{name}\"")), "{name}");
        }
    }
}
