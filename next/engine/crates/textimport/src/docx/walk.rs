//! Walks the Word body and converts paragraphs, runs, notes and drawings.

mod table;

use super::Notes;
use super::styles::{Colors, RPr, Styles, language_of, number, paragraph_attrs, read_ppr, read_rpr, run_attrs, style_name};
use crate::messages::msg;
use crate::numbering::Lists;
use crate::xmltree::El;
use crate::{Inline, Options, PageBreaks, Paragraph, Picture, Run, Table, picture};
use std::collections::{BTreeSet, HashMap};

/// The first element called `name` below `e` (depth first).
fn find<'a>(e: &'a El, name: &str) -> Option<&'a El> {
    e.elements().find_map(|c| if c.name == name { Some(c) } else { find(c, name) })
}

/// Where in a paragraph a run's pieces go: text, or a break that splits the paragraph.
enum Piece {
    Text(String),
    Break(&'static str),
    /// A footnote marker with the text of its note.
    Footnote((String, Vec<Run>)),
    /// An endnote reference with the text of its note.
    Endnote((String, Vec<Run>)),
    /// A picture.
    Picture(Picture),
}

type StyledPiece = (Piece, Option<String>, Vec<(String, String)>);

pub(super) struct Walk<'a> {
    pub(super) styles: &'a Styles,
    pub(super) options: &'a Options,
    pub(super) notes: Notes,
    pub(super) colors: Colors,
    pub(super) paragraphs: Vec<Paragraph>,
    pub(super) used_paragraph: BTreeSet<String>,
    pub(super) used_character: BTreeSet<String>,
    /// `word/footnotes.xml`: each note by its id.
    pub(super) footnotes: Vec<(String, El)>,
    /// `word/endnotes.xml`: each note by its id.
    pub(super) endnotes: Vec<(String, El)>,
    /// How many endnote references the text has: a file with endnotes comes in with a return after its last paragraph (measured).
    pub(super) endnote_count: usize,
    /// Tables read and not yet put in the text: a table goes in front of the paragraph that follows it.
    pub(super) pending: Vec<Table>,
    /// Word's lists (`word/numbering.xml`) and what the paragraphs met so far have used of them.
    pub(super) lists: Lists,
    /// The picture files of `word/media` by relationship id.
    pub(super) media: HashMap<String, Vec<u8>>,
}

impl Walk<'_> {
    pub(super) fn body(&mut self, e: &El) -> Result<(), String> {
        for c in e.elements() {
            match c.name.as_str() {
                "p" => self.paragraph(c),
                "tbl" => self.table(c)?,
                "sdt" => {
                    if let Some(content) = c.child("sdtContent") {
                        self.body(content)?;
                    }
                }
                "sectPr" | "bookmarkStart" | "bookmarkEnd" => {}
                other => self.notes.add(msg("textimport.bodyElement", format!("본문 요소 {other} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"other": other}))),
            }
        }
        Ok(())
    }

    /// The text of a footnote (the paragraph text after the note's own number) and, when its runs have character formatting, the runs.
    fn footnote_text(&mut self, id: &str) -> Option<(String, Vec<Run>)> {
        self.note_text(id, false)
    }

    /// The text of a note: all its paragraphs joined by U+000D (InDesign keeps one paragraph range and breaks the line with a return: measured,
    /// docxtext3-q02, q05) and the runs with their own formatting (only what the runs themselves say: InDesign gives a note the first paragraph
    /// style and no character formatting of Word's note styles; measured q01, q03, q07). The runs are given for a footnote only.
    fn note_text(&mut self, id: &str, endnote: bool) -> Option<(String, Vec<Run>)> {
        let list = std::mem::take(if endnote { &mut self.endnotes } else { &mut self.footnotes });
        let result = list.iter().find(|(n, _)| n == id).and_then(|(_, note)| self.note_runs(note, endnote));
        *(if endnote { &mut self.endnotes } else { &mut self.footnotes }) = list;
        result
    }

    fn note_runs(&mut self, note: &El, endnote: bool) -> Option<(String, Vec<Run>)> {
        let paragraphs: Vec<&El> = note.elements().filter(|e| e.name == "p").collect();
        let mut used_p = Vec::new();
        let mut runs: Vec<Run> = Vec::new();
        let reference = if endnote { "endnoteRef" } else { "footnoteRef" };
        for (i, p) in paragraphs.iter().enumerate() {
            if let Some(id) = p.child("pPr").and_then(|x| x.child("pStyle")).and_then(|x| x.attr("val")) {
                used_p.push(id.to_owned());
            }
            if i > 0 {
                // the return between two paragraphs belongs to the last run of the first one
                match runs.last_mut() {
                    Some(last) => last.text.push('\r'),
                    None => runs.push(Run { text: "\r".into(), ..Run::default() }),
                }
            }
            for r in p.elements().filter(|e| e.name == "r") {
                if r.child(reference).is_some() {
                    continue;
                }
                let rpr = r.child("rPr");
                let char_id = rpr.and_then(|x| x.child("rStyle")).and_then(|x| x.attr("val")).filter(|id| self.styles.find(id, false).is_some_and(|s| !s.default));
                if let Some(id) = char_id {
                    self.used_character.insert(id.to_owned());
                }
                let char_style = char_id.and_then(|id| self.styles.find(id, false)).map(style_name).filter(|_| !self.options.remove_formatting);
                let own = rpr.map(|x| read_rpr(x, &mut self.notes)).unwrap_or_default();
                let attrs = if self.options.remove_formatting { Vec::new() } else { run_attrs(&own, &RPr::default(), &mut self.colors) };
                let mut text = String::new();
                for c in r.elements() {
                    match c.name.as_str() {
                        "t" => text.push_str(&c.text()),
                        "tab" => text.push('\t'),
                        "br" | "cr" => text.push('\u{2028}'),
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    runs.push(Run { text, char_style, attrs, ..Run::default() });
                }
            }
        }
        self.used_paragraph.extend(used_p);
        // neighbours with the same formatting are one range (the return between two paragraphs joins its neighbours: measured, docxtext3-q06)
        let mut merged: Vec<Run> = Vec::new();
        for run in runs {
            match merged.last_mut() {
                Some(last) if last.char_style == run.char_style && last.attrs == run.attrs => last.text.push_str(&run.text),
                _ => merged.push(run),
            }
        }
        let runs = merged;
        let text: String = runs.iter().map(|r| r.text.as_str()).collect();
        let formatted = runs.iter().any(|r| r.char_style.is_some() || !r.attrs.is_empty());
        Some((text, if formatted { runs } else { Vec::new() }))
    }

    /// A drawing: a picture becomes the inline frame InDesign makes of it (measured: the frame is `wp:extent`, the picture is stretched to
    /// it and a crop is ignored; a floating picture comes in inline). Anything else a drawing can hold (shapes, charts) is named in the notes.
    fn drawing(&mut self, d: &El) -> Option<Picture> {
        let Some(holder) = d.child("inline").or_else(|| d.child("anchor")) else {
            self.notes.add(msg("textimport.graphics", "그림·도형은 아직 가져오지 않습니다", serde_json::json!({})));
            return None;
        };
        let Some(blip) = find(holder, "pic").and_then(|pic| find(pic, "blip")) else {
            self.notes.add(msg("textimport.shapes", "그림이 아닌 도형·차트·SmartArt 는 아직 가져오지 않습니다", serde_json::json!({})));
            return None;
        };
        let Some(embed) = blip.attr("embed") else {
            self.notes.add(msg("textimport.linkedPicture", "연결된 그림(문서 밖의 파일)은 아직 가져오지 않습니다", serde_json::json!({})));
            return None;
        };
        let Some(bytes) = self.media.get(embed) else {
            self.notes.add(msg("textimport.missingPicture", "그림 파일을 찾을 수 없어 가져오지 않았습니다", serde_json::json!({})));
            return None;
        };
        let Some(format) = picture::sniff(bytes) else {
            self.notes.add(msg("textimport.pictureFormat", "PNG·JPEG·GIF·TIFF 가 아닌 그림(EMF·WMF·BMP·SVG 등)은 아직 가져오지 않습니다", serde_json::json!({})));
            return None;
        };
        let size = holder.child("extent").and_then(|x| Some((number(x, "cx")?, number(x, "cy")?))).filter(|(w, h)| *w > 0.0 && *h > 0.0);
        let Some((w, h)) = size else {
            self.notes.add(msg("textimport.pictureSize", "그림의 크기를 읽을 수 없어 가져오지 않았습니다", serde_json::json!({})));
            return None;
        };
        if holder.name == "anchor" {
            self.notes.add(msg("textimport.floatingPicture", "글 위에 떠 있는 그림은 InDesign과 같이 글 속의 그림으로 놓습니다 (위치·글 감싸기는 가져오지 않습니다)", serde_json::json!({})));
        }
        if find(holder, "blipFill").is_some_and(|f| f.child("srcRect").is_some_and(|r| r.attrs.iter().any(|(_, v)| v != "0"))) {
            self.notes.add(msg("textimport.pictureCrop", "그림의 잘라내기는 InDesign과 같이 반영하지 않습니다 (잘라내지 않고 틀에 맞게 늘립니다)", serde_json::json!({})));
        }
        let alt = holder.child("docPr").and_then(|x| x.attr("descr")).filter(|a| !a.is_empty()).map(str::to_owned);
        // 12700 EMU to the point
        Some(Picture { data: picture::base64_lines(bytes), format: format.into(), width: w / 12700.0, height: h / 12700.0, alt })
    }

    fn paragraph(&mut self, p: &El) {
        let ppr = p.child("pPr");
        let style_id = ppr.and_then(|x| x.child("pStyle")).and_then(|x| x.attr("val")).map(str::to_owned);
        let style = style_id.as_deref().and_then(|id| self.styles.find(id, true)).or_else(|| self.styles.default_paragraph());
        let style_id = style.map(|s| s.id.clone());
        if let Some(id) = &style_id {
            self.used_paragraph.insert(id.clone());
        }
        let mut own = ppr.map(|x| read_ppr(x, &mut self.notes)).unwrap_or_default();
        let (_, base_p) = self.styles.resolved(style_id.as_deref(), None);
        // a paragraph in a list: the list level gives its indents (the paragraph's own win) and InDesign marks the paragraph with the list
        let list_at = own.num_id.clone().or_else(|| base_p.num_id.clone()).filter(|id| id != "0" && !self.options.remove_formatting).map(|id| (id, own.ilvl.or(base_p.ilvl).unwrap_or(0)));
        if let Some((id, level)) = &list_at && let Some((left, first)) = self.lists.indent(id, *level) {
            own.left = own.left.or(left);
            own.first = own.first.or(first);
        }
        let marked = list_at.and_then(|(id, level)| self.lists.mark(&id, level));
        let (mut attrs, tabs) = if self.options.remove_formatting { (Vec::new(), Vec::new()) } else { (paragraph_attrs(&own, &base_p), own.tabs.clone().unwrap_or_default()) };
        let properties = marked.as_ref().map(|m| m.properties.clone()).unwrap_or_default();
        attrs.extend(marked.into_iter().flat_map(|m| m.attrs));
        let name = if self.options.remove_formatting { None } else { style.map(style_name) };
        let new_paragraph = || Paragraph { style: name.clone(), attrs: attrs.clone(), tabs: tabs.clone(), properties: properties.clone(), ..Paragraph::default() };
        let mut current = new_paragraph();
        for table in std::mem::take(&mut self.pending) {
            current.runs.push(Run { text: "\u{16}".into(), inline: Some(Inline::Table(table)), ..Run::default() });
        }
        let mut runs = Vec::new();
        self.runs(p, style_id.as_deref(), &mut runs);
        for piece in runs {
            match piece {
                (Piece::Text(text), char_style, attrs) => current.runs.push(Run { text, char_style, attrs, ..Run::default() }),
                (Piece::Footnote((note, rich)), char_style, attrs) => current.runs.push(Run { text: "\u{4}".into(), char_style, attrs, inline: Some(Inline::Footnote(note)), note: rich, ..Run::default() }),
                (Piece::Endnote((note, rich)), char_style, attrs) => current.runs.push(Run { text: "\u{4}".into(), char_style, attrs, inline: Some(Inline::Endnote(note)), note: rich, ..Run::default() }),
                (Piece::Picture(picture), char_style, attrs) => current.runs.push(Run { text: "\u{fffc}".into(), char_style, attrs, inline: Some(Inline::Picture(picture)), ..Run::default() }),
                (Piece::Break(kind), ..) => {
                    current.break_type = Some(kind.to_owned());
                    self.paragraphs.push(std::mem::replace(&mut current, new_paragraph()));
                }
            }
        }
        self.paragraphs.push(current);
    }

    /// The pieces of the runs of `e` (hyperlinks and the like are looked through), each with its character style and local attributes.
    fn runs(&mut self, e: &El, paragraph_style: Option<&str>, out: &mut Vec<StyledPiece>) {
        for c in e.elements() {
            match c.name.as_str() {
                "r" => self.run(c, paragraph_style, out),
                "hyperlink" => {
                    self.notes.add(msg("textimport.hyperlink", "하이퍼링크는 링크 없이 글만 가져옵니다", serde_json::json!({})));
                    self.runs(c, paragraph_style, out);
                }
                "smartTag" | "fldSimple" => self.runs(c, paragraph_style, out),
                "pPr" | "bookmarkStart" | "bookmarkEnd" | "proofErr" | "commentRangeStart" | "commentRangeEnd" => {}
                "ins" | "del" => self.notes.add(msg("textimport.trackedChanges", "변경 추적한 글은 아직 가져오지 않습니다", serde_json::json!({}))),
                other => self.notes.add(msg("textimport.paragraphElement", format!("문단 안의 요소 {other} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"other": other}))),
            }
        }
    }

    fn run(&mut self, r: &El, paragraph_style: Option<&str>, out: &mut Vec<StyledPiece>) {
        let rpr = r.child("rPr");
        let char_id = rpr.and_then(|x| x.child("rStyle")).and_then(|x| x.attr("val")).filter(|id| self.styles.find(id, false).is_some_and(|s| !s.default));
        let char_name = char_id.and_then(|id| self.styles.find(id, false)).map(style_name);
        if let Some(id) = char_id {
            self.used_character.insert(id.to_owned());
        }
        let own = rpr.map(|x| read_rpr(x, &mut self.notes)).unwrap_or_default();
        let (base, _) = self.styles.resolved(paragraph_style, char_id);
        let (char_style, attrs) = if self.options.remove_formatting { (None, Vec::new()) } else { (char_name, run_attrs(&own, &base, &mut self.colors)) };
        let (lang, lang_ea) = self.styles.languages(char_id, &own);
        let lang = lang.filter(|_| !self.options.remove_formatting);
        let mut text = String::new();
        let flush = |text: &mut String, out: &mut Vec<_>| {
            if !text.is_empty() {
                let mut attrs = attrs.clone();
                if let Some(language) = language_of(text, lang.as_deref(), lang_ea.as_deref()) {
                    attrs.push(("AppliedLanguage".to_owned(), format!("$ID/{language}")));
                }
                out.push((Piece::Text(std::mem::take(text)), char_style.clone(), attrs));
            }
        };
        for c in r.elements() {
            match c.name.as_str() {
                "t" => text.push_str(&c.text()),
                "tab" => text.push('\t'),
                "br" | "cr" => match c.attr("type") {
                    Some(kind @ ("page" | "column")) if c.name == "br" => {
                        let kind = match (kind, self.options.convert_page_breaks) {
                            (_, PageBreaks::None) => continue,
                            ("page", PageBreaks::PageBreak) => "NextPage",
                            _ => "NextColumn",
                        };
                        flush(&mut text, out);
                        if !self.options.remove_formatting {
                            out.push((Piece::Break(kind), None, Vec::new()));
                        }
                    }
                    _ => text.push('\u{2028}'),
                },
                "noBreakHyphen" => text.push('\u{2011}'),
                "rPr" | "lastRenderedPageBreak" | "instrText" | "fldChar" => {}
                "drawing" | "AlternateContent" => {
                    // a drawing may sit in a markup-compatibility wrapper (`mc:AlternateContent/mc:Choice/w:drawing`)
                    let drawing = if c.name == "drawing" { Some(c) } else { c.child("Choice").and_then(|x| x.child("drawing")) };
                    match drawing {
                        Some(d) => {
                            // "remove styles and formatting" and "preserve graphics" off leave the pictures out (measured: docxtext-c3-pic8, -pic9)
                            if self.options.preserve_graphics
                                && !self.options.remove_formatting
                                && let Some(picture) = self.drawing(d)
                            {
                                flush(&mut text, out);
                                out.push((Piece::Picture(picture), char_style.clone(), attrs.clone()));
                            }
                        }
                        None => self.notes.add(msg("textimport.graphics", "그림·도형은 아직 가져오지 않습니다", serde_json::json!({}))),
                    }
                }
                "pict" | "object" => self.notes.add(msg("textimport.graphics", "그림·도형은 아직 가져오지 않습니다", serde_json::json!({}))),
                "footnoteReference" => {
                    let note = c.attr("id").and_then(|id| self.footnote_text(id));
                    match note {
                        Some(note) if !self.options.remove_formatting => {
                            flush(&mut text, out);
                            out.push((Piece::Footnote(note), char_style.clone(), attrs.clone()));
                        }
                        Some(_) => {}
                        None => self.notes.add(msg("textimport.missingFootnote", "각주의 내용을 찾을 수 없어 가져오지 않았습니다", serde_json::json!({}))),
                    }
                }
                "endnoteReference" => {
                    let note = c.attr("id").and_then(|id| self.note_text(id, true));
                    match note {
                        Some(note) if !self.options.remove_formatting => {
                            flush(&mut text, out);
                            self.endnote_count += 1;
                            out.push((Piece::Endnote(note), char_style.clone(), attrs.clone()));
                        }
                        Some(_) => {}
                        None => self.notes.add(msg("textimport.missingEndnote", "미주의 내용을 찾을 수 없어 가져오지 않았습니다", serde_json::json!({}))),
                    }
                }
                other => self.notes.add(msg("textimport.runElement", format!("글 안의 요소 {other} 은(는) 아직 가져오지 않습니다"), serde_json::json!({"other": other}))),
            }
        }
        flush(&mut text, out);
    }
}
