use super::*;

/// A temporary text view for cursor and formatting queries. Mutation readers keep
/// rejecting EndnoteRange so editing cannot discard its identity or unknown data.
pub(super) fn read(t: &str, ix: &Index, node: usize, editorial: bool) -> Result<Story, EditError> {
    let n = &ix.nodes[node];
    let mut cuts = Vec::new();
    for (i, child) in ix.nodes.iter().enumerate() {
        if child.start >= n.start && child.end <= n.end && ix.local(t,i) == "EndnoteRange" {
            cuts.push(child.start..child.open_end);
            if !child.empty { cuts.push(child.close_start..child.end); }
        }
    }
    if cuts.is_empty() { return Story::read_mode(t,ix,node,editorial); }
    cuts.sort_by_key(|r|r.start);
    let mut view = t[n.start..n.end].to_owned();
    for cut in cuts.into_iter().rev() { view.replace_range(cut.start-n.start..cut.end-n.start,""); }
    let index = Index::scan(&view).map_err(|e|EditError::new("xml",e))?;
    Story::read_mode(&view,&index,0,editorial)
}
