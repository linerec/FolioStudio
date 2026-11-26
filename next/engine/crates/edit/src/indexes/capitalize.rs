//! Index panel ▸ Capitalize… (build-b2; InDesign 2025 DOM `Topic.capitalize` / `Index.capitalize`, artifacts/build-b2-20261023/indexpanel/capitalize.json):
//! the first character of a topic name becomes a capital when it is a lower-case letter ("apple pie" → "Apple pie", "éclair" → "Éclair");
//! a name that starts with a digit, a space or a letter without case ("1st place", "  space lead", Korean) stays. Scopes: `selected` (the
//! topic), `subentries` (the topic and every topic under it), `level1` (every first-level topic of the document), `all` (every topic).
//! Each changed topic is renamed as Rename does it (`set_topic`: its id follows its name, InDesign keeps the siblings sorted), deepest
//! first so the ids collected for the topics above stay valid. One undo step.
use super::*;

fn capital(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() => c.to_uppercase().chain(chars).collect(),
        _ => name.to_owned(),
    }
}

pub(crate) fn capitalize(w: &mut Work<'_>, scope: &String, topic: &String) -> Result<(), EditError> {
    let t = w.text("designmap.xml")?.to_owned();
    let ix = w.index("designmap.xml")?;
    let depth = |i: usize| ix.ancestors(i).filter(|&a| ix.local(&t, a) == "Topic").count();
    let all: Vec<usize> = (0..ix.nodes.len()).filter(|&i| ix.local(&t, i) == "Topic").collect();
    let chosen: Vec<usize> = match scope.as_str() {
        "all" => all,
        "level1" => all.into_iter().filter(|&i| depth(i) == 0).collect(),
        "selected" | "subentries" => {
            let at = all.iter().copied().find(|&i| ix.value(&t, i, "Self").as_deref() == Some(topic.as_str()))
                .ok_or_else(|| EditError::new("notFound", format!("index topic {topic}")))?;
            if scope == "selected" { vec![at] } else { all.into_iter().filter(|&i| i == at || ix.ancestors(i).any(|a| a == at)).collect() }
        }
        _ => return Err(EditError::new("invalid", "scope must be selected, subentries, level1 or all")),
    };
    let mut jobs: Vec<(usize, String, String, String)> = chosen.into_iter().filter_map(|i| {
        let name = topic_name(&ix, &t, i);
        let new = capital(&name);
        (new != name).then(|| (depth(i), ix.value(&t, i, "Self").unwrap_or_default(), new, ix.value(&t, i, "SortOrder").unwrap_or_default()))
    }).collect();
    jobs.sort_by_key(|j| std::cmp::Reverse(j.0));
    for (_, id, name, sort_order) in jobs {
        set_topic(w, &id, &name, &sort_order)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::capital;

    #[test]
    fn only_a_lower_case_first_letter_changes() {
        assert_eq!(capital("apple pie"), "Apple pie");
        assert_eq!(capital("éclair deep"), "Éclair deep");
        for same in ["1st place", "  space lead", "한글 주제", "Banana", ""] {
            assert_eq!(capital(same), same);
        }
    }
}
