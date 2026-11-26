//! The paragraph style row menu's variants: Apply X with Clear Overrides / Clear Character Styles / Clear All, and "then Next Style"
//! (applyParagraphStyleVariant). Not measured in InDesign (the menu actions need the panel row, scripts cannot call them: probe-apply.json);
//! the chain of next styles is the one measured for the object style's apply-next (oracle os-next-*), the clearing follows the menu names.
use folio_edit::commands::RegisteredCommand;
use folio_edit::Editor;
use std::collections::BTreeMap;
use std::io::Read;

fn fixture() -> BTreeMap<String, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/text-format.idml");
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        if e.is_dir() { continue; }
        let mut t = String::new();
        e.read_to_string(&mut t).unwrap();
        parts.insert(e.name().unwrap().into_owned(), t);
    }
    parts
}

/// The next style of `style` set in Styles.xml (the fixture's styles have none of their own).
fn with_next(mut parts: BTreeMap<String, String>, style: &str, next: &str) -> BTreeMap<String, String> {
    let styles = parts.get_mut("Resources/Styles.xml").unwrap();
    let at = styles.find(&format!("<ParagraphStyle Self=\"ParagraphStyle/{style}\"")).unwrap();
    let tag_end = at + styles[at..].find('>').unwrap();
    let tag = styles[at..tag_end].to_owned();
    let fixed = match tag.find("NextStyle=\"") {
        Some(n) => { let v = n + 11; format!("{}{next_value}{}", &tag[..v], &tag[v + tag[v..].find('"').unwrap()..], next_value = format!("ParagraphStyle/{next}")) }
        None => format!("{tag} NextStyle=\"ParagraphStyle/{next}\""),
    };
    styles.replace_range(at..tag_end, &fixed);
    parts
}

fn len(ed: &Editor) -> usize {
    // characters of the story uf4: the content texts and one per paragraph break
    let t = ed.part("Stories/Story_uf4.xml").unwrap();
    let mut n = 0;
    for piece in t.split("<Content>").skip(1) { n += piece.split("</Content>").next().unwrap().chars().count(); }
    n + t.matches("<Br />").count()
}

fn run(ed: &mut Editor, style: &str, clear: &str, next: bool, start: usize) {
    let end = len(ed);
    ed.apply("variant", &[RegisteredCommand::ApplyParagraphStyleVariant { story: "uf4".into(), start, end, style: style.into(), clear: clear.into(), next }.into()]).unwrap();
}

fn attrs(ed: &Editor, key: &str) -> Vec<String> {
    let t = ed.part("Stories/Story_uf4.xml").unwrap();
    t.split(&format!(" {key}=\"")).skip(1).map(|p| p.split('"').next().unwrap().to_owned()).collect()
}

#[test]
fn clear_variants_and_next_style_chain() {
    let base = with_next(with_next(fixture(), "Format Heading", "Format Other"), "Format Other", "Format Body");
    // plain: the paragraph style applies, character styles and overrides stay
    let mut ed = Editor::new(base.clone());
    run(&mut ed, "Format Heading", "none", false, 0);
    assert!(attrs(&ed, "AppliedParagraphStyle").iter().all(|s| s == "ParagraphStyle/Format Heading"), "{:?}", attrs(&ed, "AppliedParagraphStyle"));
    assert!(attrs(&ed, "AppliedCharacterStyle").iter().any(|s| s == "CharacterStyle/Format Bold"));
    assert!(ed.part("Stories/Story_uf4.xml").unwrap().contains("PointSize=\"14\""));
    // Clear Overrides: the local character formatting goes, the character style stays
    let mut ed = Editor::new(base.clone());
    run(&mut ed, "Format Heading", "overrides", false, 0);
    assert!(attrs(&ed, "AppliedCharacterStyle").iter().any(|s| s == "CharacterStyle/Format Bold"));
    assert!(!ed.part("Stories/Story_uf4.xml").unwrap().contains("PointSize=\"14\""));
    // Clear Character Styles: the character styles go, the local formatting stays
    let mut ed = Editor::new(base.clone());
    run(&mut ed, "Format Heading", "characterStyles", false, 0);
    assert!(attrs(&ed, "AppliedCharacterStyle").iter().all(|s| s == "CharacterStyle/$ID/[No character style]"), "{:?}", attrs(&ed, "AppliedCharacterStyle"));
    assert!(ed.part("Stories/Story_uf4.xml").unwrap().contains("PointSize=\"14\""));
    // Clear All: both go
    let mut ed = Editor::new(base.clone());
    run(&mut ed, "Format Heading", "all", false, 0);
    assert!(attrs(&ed, "AppliedCharacterStyle").iter().all(|s| s == "CharacterStyle/$ID/[No character style]"));
    assert!(!ed.part("Stories/Story_uf4.xml").unwrap().contains("PointSize=\"14\""));
    // then Next Style: the first paragraph takes the style, each later one the next style of the one before
    let mut ed = Editor::new(base.clone());
    run(&mut ed, "Format Heading", "none", true, 0);
    assert_eq!(attrs(&ed, "AppliedParagraphStyle"), ["ParagraphStyle/Format Heading", "ParagraphStyle/Format Other", "ParagraphStyle/Format Body"]);
    // a selection that starts in the second paragraph: the chain starts there
    let mut ed = Editor::new(base);
    let second = len(&ed) - "Third line AV To office.".chars().count() - "Second paragraph here.".chars().count() - 1;
    run(&mut ed, "Format Heading", "none", true, second);
    assert_eq!(attrs(&ed, "AppliedParagraphStyle"), ["ParagraphStyle/Format Body", "ParagraphStyle/Format Heading", "ParagraphStyle/Format Other"]);
    // an unknown clear mode is refused
    let mut ed = Editor::new(fixture());
    let end = len(&ed);
    assert!(ed.apply("bad", &[RegisteredCommand::ApplyParagraphStyleVariant { story: "uf4".into(), start: 0, end, style: "Format Heading".into(), clear: "x".into(), next: false }.into()]).is_err());
}
