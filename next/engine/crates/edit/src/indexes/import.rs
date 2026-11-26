//! Index panel ▸ Import Topics (build-b3): the topics of another document of the project (its names, sort orders and subtopics)
//! are added to this document's index; a topic of the same name is kept, not doubled (as a topic is added), and nothing else
//! (page references, cross references) comes with them. The server reads the other document (`topics`); one undo step.
use crate::{EditError, Work};
use folio_model::indexes::Topic;

pub(crate) fn import_topics(w: &mut Work<'_>, document: &String, topics: &[Topic]) -> Result<(), EditError> {
    if topics.is_empty() {
        return Err(EditError::new("invalid", format!("가져올 주제가 없습니다: {document}")));
    }
    fn add(w: &mut Work<'_>, parent: &String, topics: &[Topic], depth: usize) -> Result<(), EditError> {
        if depth > 4 { return Err(EditError::new("invalid", "주제는 4단계까지입니다")); }
        for t in topics {
            super::add_topic(w, parent, &t.name, &t.sort_order)?;
            let id = w.created.last().cloned().ok_or_else(|| EditError::new("xml", "topic not added"))?;
            add(w, &id, &t.topics, depth + 1)?;
        }
        Ok(())
    }
    add(w, &String::new(), topics, 1)
}
