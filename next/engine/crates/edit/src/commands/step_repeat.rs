use crate::{Command, EditError, Work};

pub(super) fn run(w: &mut Work<'_>, ids: &[String], grid: &bool, count: &usize,
    rows: &usize, columns: &usize, h: &f64, v: &f64) -> Result<(), EditError> {
    let valid_count = if *grid { (1..=100).contains(rows) && (1..=100).contains(columns) && rows * columns > 1 }
        else { (1..=1000).contains(count) };
    if ids.is_empty() || !valid_count || !h.is_finite() || !v.is_finite() || h.abs() > 1e9 || v.abs() > 1e9 {
        return Err(EditError::new("invalid", "invalid step and repeat values"));
    }
    let mut places = Vec::new();
    if *grid {
        for row in 0..*rows { for column in 0..*columns {
            if row != 0 || column != 0 { places.push([column as f64 * h, row as f64 * v]); }
        } }
    } else {
        for n in 1..=*count { places.push([n as f64 * h, n as f64 * v]); }
    }
    for by in places.into_iter().rev() {
        crate::ops::run(w, &Command::Duplicate { ids: ids.to_vec(), by })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Command, Editor};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn editor() -> Editor {
        Editor::new(BTreeMap::from([("Spreads/s.xml".into(),
            r#"<idPkg:Spread><Spread Self="s"><Rectangle Self="r" ItemTransform="1 0 0 1 5 6"/></Spread></idPkg:Spread>"#.into())]))
    }

    #[test]
    fn row_and_grid_match_duplicate_order_and_undo_exactly() {
        for (grid, offsets) in [(false, vec![[20., 10.], [10., 5.]]),
            (true, vec![[10., 5.], [0., 5.], [10., 0.]])] {
            let mut actual = editor();
            let before = actual.parts().clone();
            let command: Command = serde_json::from_value(json!({"op":"stepRepeat", "ids":["r"],
                "grid":grid, "count":2, "rows":2, "columns":2, "h":10, "v":5})).unwrap();
            let outcome = actual.apply("repeat", &[command]).unwrap();
            let mut expected = editor();
            let duplicates: Vec<_> = offsets.into_iter().map(|by| Command::Duplicate { ids:vec!["r".into()], by }).collect();
            let reference = expected.apply("repeat", &duplicates).unwrap();
            assert_eq!(actual.parts(), expected.parts());
            assert_eq!(outcome.created, reference.created);
            actual.undo().unwrap();
            assert_eq!(actual.parts(), &before);
        }
    }

    #[test]
    fn invalid_count_or_missing_item_leaves_document_unchanged() {
        for (count, ids) in [(0, vec!["r"]), (1001, vec!["r"]), (2, vec!["r", "missing"])] {
            let mut e = editor();
            let before = e.parts().clone();
            let c = serde_json::from_value(json!({"op":"stepRepeat", "ids":ids,
                "grid":false, "count":count, "rows":1, "columns":1, "h":10, "v":5})).unwrap();
            assert!(e.apply("repeat", &[c]).is_err());
            assert_eq!(e.parts(), &before);
            assert!(e.undo_label().is_none());
        }
    }
}
