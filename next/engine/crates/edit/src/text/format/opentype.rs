use super::*;

/// A set button changes one bit in each run, preserving the other sets in a mixed selection.
pub(super) fn set_index(key: &str) -> Option<u32> {
    key.strip_prefix("otfStylisticSet")?.parse::<u32>().ok().filter(|n| (1..=20).contains(n))
}

pub(super) fn changes(props: &BTreeMap<String, Value>) -> Result<Vec<(u32, bool)>, EditError> {
    props.iter().filter_map(|(key, value)| set_index(key).map(|n| {
        value.as_bool().map(|on| (1 << n, on))
            .ok_or_else(|| EditError::new("invalid", format!("invalid text property {key}: {value}")))
    })).collect()
}

pub(super) fn apply(changes: &[(u32, bool)], inherited: &Attrs, direct: &mut Attrs) {
    if changes.is_empty() { return; }
    let mut mask = direct.get("OTFStylisticSets").or_else(|| inherited.get("OTFStylisticSets"))
        .and_then(|p| p.value.parse::<u32>().ok()).unwrap_or(0);
    for &(bit, on) in changes {
        if on { mask |= bit; } else { mask &= !bit; }
    }
    direct.insert("OTFStylisticSets".into(), attr(mask.to_string()));
}
