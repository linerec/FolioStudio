use super::*;

pub(super) fn normalize(a: &mut Attrs, base: &Attrs, props: &BTreeMap<String, Value>) -> Result<(), EditError> {
    if !props.contains_key("spanColumnType") && !props.contains_key("spanSplitColumnCount") { return Ok(()); }
    let get = |key: &str| a.get(key).or_else(|| base.get(key)).map_or("", |p| p.value.as_str());
    let count = get("SpanSplitColumnCount");
    let resolved = folio_idml::paragraph_columns::resolve_count(get("SpanColumnType"), count, props.contains_key("spanSplitColumnCount"))
        .ok_or_else(|| EditError::new("invalid", "split columns require a count from 2 to 40"))?;
    if resolved != count { a.insert("SpanSplitColumnCount".into(), property("SpanSplitColumnCount", "short", resolved)); }
    Ok(())
}
