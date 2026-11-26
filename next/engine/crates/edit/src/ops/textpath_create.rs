//! Attach a new story to a supported existing path, preserving its geometry and paint.
use super::*;

pub(crate) fn create(w: &mut Work<'_>, ids: &[String], text: &str) -> Result<(), EditError> {
    if ids.is_empty() { return Err(EditError::new("invalid", "select a path")); }
    for id in ids { attach(w, id, None, text)?; }
    Ok(())
}

/// The Type on a Path tool pressed on a closed shape: `point` is in spread coordinates, `reach` how far from the outline (points) it still counts.
pub(crate) fn create_at(w: &mut Work<'_>, id: &str, point: &[f64; 2], reach: &f64, text: &str) -> Result<(), EditError> {
    if point.iter().chain([reach]).any(|v| !v.is_finite()) || *reach < 0.0 { return Err(EditError::new("invalid", "the press needs a finite point and reach")); }
    attach(w, id, Some((*point, *reach)), text)
}

fn attach(w: &mut Work<'_>, id: &str, press: Option<([f64; 2], f64)>, text: &str) -> Result<(), EditError> {
    {
        let (part, ix, i) = w.find(id)?;
        let t = w.text(&part)?.to_owned();
        check_item(&ix, &t, i, id)?;
        super::textpath::unlocked(w, &ix, &t, i)?;
        let source = &t[ix.nodes[i].start..ix.nodes[i].end];
        let attrs = match press {
            None => folio_idml::textpath::creation_attributes(source)
                .ok_or_else(|| EditError::new("unsupported", "select an empty open path of one line or curve"))?,
            Some((point, reach)) => {
                let inner = super::textpath_tool::to_inner(&ix, &t, i, point)?;
                folio_idml::textpath::creation_attributes_at(source, inner, reach).map_err(|(kind, message)| EditError::new(kind, message))?
            }
        };
        let child = w.new_id();
        let story = w.new_id();
        let mut xml = format!("<TextPath Self=\"{}\" ParentStory=\"{}\"", escape(&child), escape(&story));
        for (k, v) in attrs { xml.push_str(&format!(" {k}=\"{}\"", escape(&v))); }
        xml.push_str("/>");
        let at = ix.children(i).iter().copied()
            .find(|&c| ix.local(&t, c) != "Properties")
            .map(|c| ix.nodes[c].start).unwrap_or(ix.nodes[i].close_start);
        w.splice(&part, vec![(at..at, xml)])?;
        w.add_part(format!("Stories/Story_{story}.xml"), super::create::empty_story(w, &story)?)?;
        super::story_copy::register(w, &story)?;
        if !text.is_empty() { crate::text::insert(w, &story, 0, text)?; }
        w.recompose(&story);
        w.created.extend([child, story]);
    }
    Ok(())
}
