use super::{
    content::{self, BASIC_CHARACTER, BASIC_PARAGRAPH},
    links, styles, xml,
};
use crate::{
    Command, EditError, ItemKind, Work, ops,
    xml::{escape, unescape},
};
use folio_model::{
    Document,
    toc::{TocSource, TocStyle},
};
use std::path::Path;

pub(crate) fn create(
    w: &mut Work<'_>,
    style: &str,
    frame: &str,
    replace: &bool,
    sources: &[TocSource],
) -> Result<(), EditError> {
    let style = find_style(w, style)?;
    let doc = read_document(w)?;
    let mut same_style = doc.toc_stories.iter().filter(|(_, id)| **id == style.id);
    let existing = match (replace, same_style.next(), same_style.next()) {
        (true, Some((story, _)), None) => Some(story.clone()),
        (true, Some(_), Some(_)) => {
            let story = frame_story(w, frame)?;
            if doc.toc_stories.get(&story) != Some(&style.id) {
                return Err(EditError::new(
                    "invalid",
                    "교체할 목차의 프레임을 지정해야 합니다",
                ));
            }
            Some(story)
        }
        _ => None,
    };
    let story = if let Some(story) = existing {
        story
    } else if frame.is_empty() {
        new_frame_story(w, &doc)?
    } else {
        let story = frame_story(w, frame)?;
        let empty = doc.stories.get(&story).is_some_and(|s| {
            s.paragraphs.len() <= 1
                && s.paragraphs
                    .iter()
                    .flat_map(|p| &p.runs)
                    .all(|r| r.text.is_empty())
        });
        if !empty || doc.toc_stories.contains_key(&story) {
            return Err(EditError::new(
                "invalid",
                "비어 있는 텍스트 프레임을 지정해야 합니다",
            ));
        }
        story
    };
    write_story(w, &doc, &style, &story, sources)
}

pub(crate) fn update(
    w: &mut Work<'_>,
    story: &String,
    sources: &[TocSource],
) -> Result<(), EditError> {
    let doc = read_document(w)?;
    let id = doc
        .toc_stories
        .get(story)
        .ok_or_else(|| EditError::new("invalid", "목차 이야기를 지정해야 합니다"))?;
    let style = find_style(w, id)?;
    write_story(w, &doc, &style, story, sources)
}

fn read_document(w: &Work<'_>) -> Result<Document, EditError> {
    folio_idml::read_parts(w.ed.parts(), Path::new("toc.idml"), &Default::default())
        .map_err(|e| EditError::new("xml", e.to_string()))
}

fn find_style(w: &Work<'_>, id: &str) -> Result<TocStyle, EditError> {
    styles::all(w.ed.parts())?
        .into_iter()
        .find(|style| style.id == id)
        .ok_or_else(|| EditError::new("notFound", "목차 스타일을 찾을 수 없습니다"))
}

fn frame_story(w: &mut Work<'_>, frame: &str) -> Result<String, EditError> {
    let invalid = || EditError::new("invalid", "목차의 텍스트 프레임을 지정해야 합니다");
    if frame.is_empty() {
        return Err(invalid());
    }
    let (part, index, root) = w.find(frame)?;
    let source = w.text(&part)?;
    if index.local(source, root) != "TextFrame" {
        return Err(invalid());
    }
    index
        .attr(source, root, "ParentStory")
        .filter(|id| !id.is_empty() && *id != "n")
        .map(unescape)
        .ok_or_else(invalid)
}

// R2.
fn new_frame_story(w: &mut Work<'_>, doc: &Document) -> Result<String, EditError> {
    let page = doc
        .spreads
        .iter()
        .flat_map(|spread| &spread.pages)
        .min_by_key(|page| page.index)
        .ok_or_else(|| EditError::new("notFound", "목차를 만들 페이지가 없습니다"))?;
    ops::run(
        w,
        &Command::CreateItem {
            page: page.id.clone(),
            kind: ItemKind::TextFrame,
            layer: None,
            sides: 6,
            rect: [
                36.0,
                36.0,
                (page.bounds.width() - 36.0).max(72.0),
                (page.bounds.height() - 36.0).max(72.0),
            ],
        },
    )?;
    let frame = w
        .created
        .last()
        .ok_or_else(|| EditError::new("invalid", "목차 프레임을 만들 수 없습니다"))?
        .clone();
    frame_story(w, &frame)
}

struct Row<'a> {
    style: &'a str,
    level: usize,
    runs: String,
}

fn run(text: &str, style: &str) -> String {
    format!(
        "<CharacterStyleRange AppliedCharacterStyle=\"{}\"><Content>{}</Content></CharacterStyleRange>",
        escape(style),
        escape(text)
    )
}

fn entry_runs(doc: &Document, entry: &content::Entry<'_>) -> String {
    let body = run(&entry.title, BASIC_CHARACTER);
    let options = entry.options;
    let label = || {
        run(
            entry.label,
            content::character_style(doc, &options.page_number_style),
        )
    };
    let separator = || {
        let text = options
            .separator
            .strip_prefix("$ID/")
            .unwrap_or(&options.separator);
        run(
            &text.replace("^t", "\t"),
            content::character_style(doc, &options.separator_style),
        )
    };
    match options.page_number_position.as_str() {
        "BeforeEntry" => label() + &separator() + &body,
        "AfterEntry" => body + &separator() + &label(),
        _ => body,
    }
}

// R4 and R5.
fn write_story(
    w: &mut Work<'_>,
    doc: &Document,
    style: &TocStyle,
    story: &str,
    sources: &[TocSource],
) -> Result<(), EditError> {
    let entries = content::entries(doc, style, story, sources)?;
    links::clear(w, story)?;
    let mut links = links::Links::new();
    let mut rows = Vec::new();
    if !style.title.is_empty() {
        rows.push(Row {
            style: content::paragraph_style(doc, &style.title_style).unwrap_or(BASIC_PARAGRAPH),
            level: 0,
            runs: run(&style.title, BASIC_CHARACTER),
        });
    }
    for entry in entries {
        let runs = entry_runs(doc, &entry);
        let level = entry.options.level;
        let runs = match entry.page {
            Some(page) => links.entry(w, page, &entry.title, level, true, runs),
            None => runs,
        };
        if let Some(row) = rows
            .last_mut()
            .filter(|row| style.run_in && row.level == level)
        {
            row.runs.push_str(&run("; ", BASIC_CHARACTER));
            row.runs.push_str(&runs);
        } else {
            rows.push(Row {
                style: entry.format,
                level,
                runs,
            });
        }
    }
    if rows.is_empty() {
        rows.push(Row {
            style: BASIC_PARAGRAPH,
            level: 0,
            runs: run("", BASIC_CHARACTER),
        });
    }
    let mut paragraphs = String::new();
    for (i, row) in rows.iter().enumerate() {
        paragraphs.push_str(&format!(
            "<ParagraphStyleRange AppliedParagraphStyle=\"{}\">",
            escape(row.style)
        ));
        paragraphs.push_str(&row.runs);
        if i + 1 < rows.len() {
            paragraphs.push_str("<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[None]\"><Br/></CharacterStyleRange>");
        }
        paragraphs.push_str("</ParagraphStyleRange>");
    }
    replace_paragraphs(w, story, &style.id, &paragraphs)?;
    links.finish(w)?;
    w.recompose(story);
    Ok(())
}

fn replace_paragraphs(
    w: &mut Work<'_>,
    story: &str,
    style: &str,
    paragraphs: &str,
) -> Result<(), EditError> {
    let (part, index, root) = w.find(story)?;
    let mut fragment = xml::fragment(w.text(&part)?, &index, root);
    xml::attribute(&mut fragment, "AppliedTOCStyle", style)?;
    let parsed = xml::scan(&fragment)?;
    let element = &parsed.nodes[0];
    if element.empty {
        let name = &fragment[element.name.clone()];
        fragment.replace_range(
            element.open_end - 2..element.open_end,
            &format!(">{paragraphs}</{name}>"),
        );
    } else {
        let paragraphs_at = parsed.named(&fragment, 0, "ParagraphStyleRange");
        let at = paragraphs_at
            .first()
            .map_or(element.close_start, |&i| parsed.nodes[i].start);
        for i in paragraphs_at.into_iter().rev() {
            let node = &parsed.nodes[i];
            fragment.replace_range(node.start..node.end, "");
        }
        fragment.insert_str(at, paragraphs);
    }
    w.splice(
        &part,
        vec![(index.nodes[root].start..index.nodes[root].end, fragment)],
    )
}
