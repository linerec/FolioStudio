//! Group the document's used faces by font file and determine which files can be copied.

use super::{FileCopy, FontFile, FontRow};
use crate::names::NameTable;
use crate::{FONTS_DIR, Inputs, Options, file_name, fonts};
use folio_text::FontSource;
use std::collections::HashMap;
use std::path::PathBuf;

fn norm_family(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

pub(super) fn plan(inp: &Inputs, opts: &Options) -> (Vec<FontFile>, Vec<FontRow>) {
    let store = inp.fonts;
    // the preflight's own reading of missing vs substituted (one owner of that rule)
    let facts = folio_preflight::font_facts(store, inp.doc, inp.layout);
    struct Group {
        path: PathBuf,
        source: FontSource,
        faces: Vec<(u32, bool)>,
        names: Vec<String>,
        rows: Vec<usize>,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_file: HashMap<PathBuf, usize> = HashMap::new();
    let mut rows: Vec<FontRow> = Vec::new();
    for (family, style) in &inp.doc.fonts_used {
        let mut row = FontRow {
            family: family.clone(),
            style: style.clone(),
            status: "copied",
            code: None,
            reason: None,
            face: None,
            postscript: None,
            file: None,
            source: None,
            origin: None,
            substitute: None,
        };
        let Some((face, cjk_glyphs)) = fonts::exact_face(store, family, style, &inp.doc.font_table) else {
            let listed = |l: &[(String, String, String)]| l.iter().find(|(f, s, _)| f == family && s == style).map(|t| t.2.clone());
            let (status, sub) = if let Some(s) = listed(&facts.substituted) {
                ("substituted", s)
            } else if let Some(s) = listed(&facts.missing) {
                ("missing", s)
            } else {
                // not set by the layout (e.g. only in empty runs): classify the same way
                let r = store.resolve(family, style, &inp.doc.font_table);
                let sub = store.face_info(r.font).map(|i| format!("{} {}", i.family, i.style)).unwrap_or_else(|| "(대체 글꼴 없음)".into());
                let installed = store.faces().iter().any(|i| norm_family(&i.family) == norm_family(family));
                (if installed { "substituted" } else { "missing" }, sub)
            };
            row.status = status;
            row.substitute = Some(sub);
            rows.push(row);
            continue;
        };
        row.face = Some(format!("{} {}", face.family, face.style));
        row.postscript = Some(face.postscript.clone());
        row.source = Some((*face.path).clone());
        row.origin = Some(fonts::origin_word(face.source));
        row.file = Some(file_name(&face.path));
        if !opts.fonts {
            row.status = "notSelected";
            rows.push(row);
            continue;
        }
        let cjk_script = cjk_glyphs
            || inp.doc.font_table.iter().any(|f| &f.family == family && &f.style == style && f.writing_script.is_some_and(fonts::is_cjk_script));
        let path = (*face.path).clone();
        let g = *by_file.entry(path.clone()).or_insert_with(|| {
            groups.push(Group { path, source: face.source, faces: Vec::new(), names: Vec::new(), rows: Vec::new() });
            groups.len() - 1
        });
        let group = &mut groups[g];
        if let Some(f) = group.faces.iter_mut().find(|f| f.0 == face.index) {
            f.1 |= cjk_script;
        } else {
            group.faces.push((face.index, cjk_script));
        }
        let label = format!("{} {}", face.family, face.style);
        if !group.names.contains(&label) {
            group.names.push(label);
        }
        group.rows.push(rows.len());
        rows.push(row);
    }
    let mut table = NameTable::new();
    let mut files = Vec::new();
    for g in groups {
        match fonts::decide(&g.path, g.source, &g.faces, opts.cjk_fonts) {
            Ok(_) => {
                let name = table.unique(&file_name(&g.path));
                let dest = format!("{FONTS_DIR}/{name}");
                let bytes = std::fs::metadata(&g.path).map_or(0, |m| m.len());
                for &r in &g.rows {
                    rows[r].file = Some(dest.clone());
                }
                files.push(FontFile { copy: FileCopy { source: g.path, dest, bytes }, faces: g.names, origin: fonts::origin_word(g.source) });
            }
            Err(skip) => {
                for &r in &g.rows {
                    rows[r].status = "notCopied";
                    rows[r].code = Some(skip.code);
                    rows[r].reason = Some(skip.reason.clone());
                }
            }
        }
    }
    (files, rows)
}
