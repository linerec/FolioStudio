//! The four process inks every InDesign document has (Resources/Graphic.xml `Ink`, the Ink Manager's list). InDesign 2025 writes
//! them into every new document, whatever its page setup; measured by `oracle/indesign/newdoc-inks-probe.jsx`, the values are in
//! `oracle/newdoc/reference/inks-new-document.json` (the engine test `a_new_document_has_the_four_process_inks_of_indesign` keeps
//! the two together). Two places need them as written: File > New (`folio-project`'s `newdoc`) and the edit of a document that has
//! no Ink at all (`folio-edit`: one File > New made before it had them), so they are written here once.

/// The Ink elements as InDesign writes them: Cyan, Magenta, Yellow, Black (TrapOrder 1-4); a spot ink comes after them (TrapOrder 5 on).
pub const PROCESS_INKS: [&str; 4] = [
    r#"<Ink Self="Ink/$ID/Process Cyan" Name="$ID/Process Cyan" Angle="75" ConvertToProcess="false" Frequency="70" NeutralDensity="0.61" PrintInk="true" TrapOrder="1" InkType="Normal" />"#,
    r#"<Ink Self="Ink/$ID/Process Magenta" Name="$ID/Process Magenta" Angle="15" ConvertToProcess="false" Frequency="70" NeutralDensity="0.76" PrintInk="true" TrapOrder="2" InkType="Normal" />"#,
    r#"<Ink Self="Ink/$ID/Process Yellow" Name="$ID/Process Yellow" Angle="0" ConvertToProcess="false" Frequency="70" NeutralDensity="0.16" PrintInk="true" TrapOrder="3" InkType="Normal" />"#,
    r#"<Ink Self="Ink/$ID/Process Black" Name="$ID/Process Black" Angle="45" ConvertToProcess="false" Frequency="70" NeutralDensity="1.7" PrintInk="true" TrapOrder="4" InkType="Normal" />"#,
];
