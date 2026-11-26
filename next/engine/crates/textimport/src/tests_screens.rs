//! screens-import: Word tables whose merged cells say borders, and what a Word or RTF file says that is carried over or noted.
use super::*;

fn docx(name: &str) -> TextSource {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(name)).unwrap();
    read_bytes(&bytes, &Options::default()).unwrap()
}

fn first_table(s: &TextSource) -> &Table {
    s.paragraphs.iter().flat_map(|p| p.runs.iter()).find_map(|r| match &r.inline {
        Some(Inline::Table(t)) => Some(t),
        _ => None,
    }).expect("a table")
}

fn edge<'a>(t: &'a Table, row: usize, column: usize, key: &str) -> Option<&'a str> {
    t.cells.iter().find(|c| c.row == row && c.column == column).and_then(|c| c.attrs.iter().find(|(k, _)| k == key)).map(|(_, v)| v.as_str())
}

#[test]
fn a_merged_cell_gets_the_borders_it_says_and_its_neighbours_the_other_side_of_them() {
    let s = docx("screens-table-merged.docx");
    let t = first_table(&s);
    // the cell of two columns: its bottom is the top of the cells under it, its right side the left side of the cell after it
    assert_eq!((t.cells[0].column_span, t.cells[0].row_span), (2, 1));
    assert_eq!(edge(t, 0, 0, "BottomEdgeStrokeColor"), Some("Color/Word_R255_G0_B0"));
    assert_eq!(edge(t, 0, 0, "RightEdgeStrokeColor"), Some("Color/Word_R255_G0_B0"));
    assert_eq!(edge(t, 0, 2, "LeftEdgeStrokeColor"), Some("Color/Word_R255_G0_B0"));
    assert_eq!(edge(t, 1, 1, "TopEdgeStrokeColor"), Some("Color/Word_R255_G0_B0"));
    // the cell of two rows: its left side is the table's edge, written on the cell alone
    let tall = t.cells.iter().find(|c| c.row == 1 && c.column == 0).unwrap();
    assert_eq!(tall.row_span, 2);
    assert_eq!(edge(t, 1, 0, "LeftEdgeStrokeColor"), Some("Color/Word_R0_G170_B0"));
    // a note says the numbers of the edges are an approximation, and nothing says the borders were left out
    assert!(s.warnings.iter().any(|w| w.contains("병합된 칸이 있는 표의 칸 테두리는 근사로")), "{:?}", s.warnings);
    assert!(!s.warnings.iter().any(|w| w.contains("테두리는 아직 가져오지 않습니다")), "{:?}", s.warnings);
}

#[test]
fn a_plain_table_keeps_its_exact_borders_without_the_note() {
    let s = docx("docxtext3-e04.docx");
    assert!(!s.warnings.iter().any(|w| w.contains("근사로")), "{:?}", s.warnings);
}

/// The edge attributes InDesign wrote on each cell of the table of a golden story (`Name` is `column:row`), by cell name.
fn golden_edges(name: &str) -> std::collections::BTreeMap<String, Vec<(String, String)>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/goldens/docxtext2").join(name).join("after/Stories");
    let mut out = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        if !text.contains("<Table ") {
            continue;
        }
        for cell in text.split("<Cell ").skip(1) {
            let tag = &cell[..cell.find('>').unwrap()];
            let attr = |key: &str| -> Option<String> {
                let at = tag.find(&format!(" {key}=\""))? + key.len() + 3;
                Some(tag[at..tag[at..].find('"')? + at].to_owned())
            };
            let cell_name = tag.split("Name=\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
            let mut edges = Vec::new();
            for side in ["Left", "Top", "Right", "Bottom"] {
                for key in ["Weight", "Color", "Type", "Priority"] {
                    let full = format!("{side}EdgeStroke{key}");
                    // the priority is written on every cell; `0` is no edge
                    if let Some(v) = attr(&full).filter(|_| attr(&format!("{side}EdgeStrokeWeight")).is_some()) {
                        edges.push((full, v));
                    }
                }
            }
            out.insert(cell_name, edges);
        }
    }
    out
}

#[test]
fn rtf_cell_borders_are_what_indesign_made_of_them() {
    for name in ["a", "b", "c"] {
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../oracle/fixtures/Links").join(format!("screens-rtf-border-{name}.rtf"))).unwrap();
        let source = read_bytes(&bytes, &Options::default()).unwrap();
        let table = first_table(&source);
        let golden = golden_edges(&format!("screens-rtf-border-{name}"));
        assert_eq!(golden.len(), table.cells.len(), "{name}: cells");
        for cell in &table.cells {
            let cell_name = format!("{}:{}", cell.column, cell.row);
            let mut ours: Vec<(String, String)> = cell.attrs.iter().filter(|(k, _)| k.contains("EdgeStroke") && ["Weight", "Color", "Type", "Priority"].iter().any(|s| k.ends_with(s))).cloned().collect();
            let mut theirs = golden[&cell_name].clone();
            ours.sort();
            theirs.sort();
            assert_eq!(ours, theirs, "{name}: cell {cell_name}");
        }
        assert!(!source.warnings.iter().any(|w| w.contains("지원하지 않습니다") && w.contains("테두리")), "{name}: {:?}", source.warnings);
    }
}
