//! Special characters (markers) of a paragraph as GREP styles and nested styles see them (measured with InDesign 2025:
//! rules/specialchars-c1.ko.md). The model keeps a marker as a run of its own (`TextRun::variable`: page number,
//! section marker, note reference, text variable instance), as U+FFFC (anchored object) or U+0016 (table) in the text, or, for the
//! invisible End Nested Style Character, as an offset in `ParagraphAttrs::end_nested_marks`. Matching needs one character per
//! marker; these are private-use characters of plane 16 that never occur in text and are never stored.
use crate::TextVariable;

/// Current page number (InDesign `~N`, DOM `AUTO_PAGE_NUMBER`; U+0018 in `Story.contents`).
pub const PAGE: char = '\u{10FF00}';
/// Next page number (`~X`), previous page number (`~V`).
pub const NEXT_PAGE: char = '\u{10FF01}';
pub const PREVIOUS_PAGE: char = '\u{10FF02}';
/// An instance of a text variable (U+0018 too, but `~#` and `~N` do not take it).
pub const VARIABLE: char = '\u{10FF03}';
/// Section marker (`~x`, U+0019).
pub const SECTION: char = '\u{10FF04}';
/// Footnote or endnote reference (`~F`, U+0004).
pub const NOTE: char = '\u{10FF05}';
/// End Nested Style Character (`~h`, U+0003). Invisible: GREP does not see it unless the expression names it.
pub const END_NESTED: char = '\u{10FF06}';
/// Index marker (`PageReference`; `~I`, U+FEFF in `Story.contents`). Invisible like the End Nested Style Character, but only `~I` names it
/// (`\x{FEFF}` does not: measured). The model's text holds it as U+FEFF; matching sees this stand-in.
pub const INDEX: char = '\u{10FF07}';
/// The anchor of an editorial note (U+FEFF too). Invisible and named by nothing, not even `~I` (measured, grepspell-c3): matching drops it.
pub const NOTE_ANCHOR: char = '\u{10FF08}';

/// What a run with this `variable` stands for in matching.
pub fn of_variable(v: TextVariable) -> char {
    match v {
        TextVariable::PageNumber => PAGE,
        TextVariable::NextPageNumber => NEXT_PAGE,
        TextVariable::PreviousPageNumber => PREVIOUS_PAGE,
        TextVariable::SectionMarker => SECTION,
        TextVariable::FootnoteNumber | TextVariable::EndnoteNumber => NOTE,
        TextVariable::Named => VARIABLE,
    }
}

/// What the nested style delimiter "Auto Page Number" ends at: a page number marker of any kind and, measured, an instance of a text variable
/// (all U+0018 in InDesign).
pub fn is_page_number(c: char) -> bool { matches!(c, PAGE | NEXT_PAGE | PREVIOUS_PAGE | VARIABLE) }
