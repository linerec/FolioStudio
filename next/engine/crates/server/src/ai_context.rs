use crate::{Session, requests::Context};
use folio_ai::context::Source;
use folio_engine::mcp::gate;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn build(ctx: &Context<'_>, session: &Session, req: &Value) -> Result<Value, String> {
    build_with_images(ctx, session, req, req["images"] != false)
}
pub fn build_with_images(
    ctx: &Context<'_>,
    session: &Session,
    req: &Value,
    images: bool,
) -> Result<Value, String> {
    let document = crate::ai_runtime::document_id(session);
    if req.get("document").is_some_and(|id| id != document.as_str()) {
        return Err("different attachment document".into());
    }
    let mut context = folio_ai::context::build(&Document { ctx, session }, req, images)?;
    context["document"] = json!(document);
    Ok(context)
}
struct Document<'a, 'b> {
    ctx: &'a Context<'b>,
    session: &'a Session,
}
impl Source for Document<'_, '_> {
    fn read(&self, name: &str, args: Value) -> Result<Value, String> {
        let s = self.session;
        let mut req = gate::prepare(name, &args, false, false)?;
        let boundary = crate::attach_paths::resolve(&mut req, Some(s)).map_err(|e| e.error)?;
        let mut answer = match name {
            "pageItems" => serde_json::from_str(&crate::inspect::command(
                &req, &s.doc, &s.layout, &s.colors, &s.inspect,
            )?)
            .map_err(|_| "invalid page data")?,
            "styleInfo" => crate::text_edit::command(&req, s)?,
            "fileMetadata" => {
                let mut metadata = crate::source_queries::file_metadata(&req)
                    .map_err(|_| "could not read project file")?;
                if let Some(o) = metadata.as_object_mut() {
                    o.remove("packet");
                }
                metadata
            }
            "preflight" => crate::print_command(&req, s, &Value::Null, self.ctx.out)?,
            _ => return Err("unknown context source".into()),
        };
        if let Some(b) = boundary {
            b.recheck()?;
            crate::attach_paths::relativize(&mut answer, &b.root);
        }
        crate::attach_read_reply::scrub(&mut answer);
        Ok(answer)
    }
    fn image(&self, args: Value, ids: &[String]) -> Result<Value, String> {
        let req = gate::prepare("render", &args, false, false)?;
        crate::attach_image::render_selected(&req, self.session, self.ctx.profiles, Some(ids))
    }
    fn text(&self, id: &str, start: usize, end: Option<usize>) -> Result<Value, String> {
        let story = self.session.doc.stories.get(id).ok_or("unknown story")?;
        let mut text = String::new();
        let mut paragraphs = BTreeSet::new();
        let mut characters = BTreeSet::new();
        let mut position = 0;
        let end = end.unwrap_or(usize::MAX);
        if end < start {
            return Err("invalid text range".into());
        }
        for (index, p) in story.paragraphs.iter().enumerate() {
            if index > 0 {
                if (start..end).contains(&position) {
                    text.push('\n');
                }
                position += 1;
            }
            for run in &p.runs {
                for ch in run.text.chars() {
                    if (start..end).contains(&position) {
                        text.push(ch);
                        paragraphs.insert(&p.style);
                        characters.insert(&run.attrs.character_style);
                    }
                    position += 1;
                }
            }
        }
        if start > position {
            return Err("invalid text offset".into());
        }
        let hash = folio_project::hash::sha256_bytes(text.as_bytes());
        if let Some((limit, _)) = text.char_indices().nth(500) {
            text.truncate(limit);
        }
        Ok(
            json!({"story":id,"start":start,"end":end.min(position),"excerpt":text,"sha256":hash,
            "paragraphStyles":paragraphs,"characterStyles":characters}),
        )
    }
}
