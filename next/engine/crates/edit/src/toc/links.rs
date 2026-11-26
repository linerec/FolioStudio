use super::xml::scan;
use crate::{EditError, Work, xml::{escape, unescape}};
use std::collections::BTreeSet;

pub(super) fn clear(w: &mut Work<'_>, story: &str) -> Result<(), EditError> {
    let (part, ix, root) = w.find(story)?;
    let text = w.text(&part)?;
    let sources: BTreeSet<_> = (root+1..ix.nodes.len()).take_while(|&i| ix.nodes[i].start<ix.nodes[root].end)
        .filter(|&i| ix.local(text,i)=="HyperlinkTextSource")
        .filter_map(|i|ix.attr(text,i,"Self").map(str::to_owned)).collect();
    let text = w.text("designmap.xml")?;
    let ix = scan(text)?;
    let mut removed = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    for i in 0..ix.nodes.len() {
        if ix.local(text,i)=="Hyperlink" && ix.attr(text,i,"Source").is_some_and(|s| sources.contains(s)) {
            removed.insert(i);
            for j in i+1..ix.nodes.len() {
                if ix.nodes[j].start>=ix.nodes[i].end {break}
                if ix.local(text,j)=="Destination" {
                    destinations.insert(unescape(&text[ix.nodes[j].open_end..ix.nodes[j].close_start]));
                }
            }
        }
    }
    let mut retained=BTreeSet::new();
    for i in 0..ix.nodes.len() {
        if ix.local(text,i)=="Hyperlink" && !removed.contains(&i) {
            for j in i+1..ix.nodes.len() {
                if ix.nodes[j].start>=ix.nodes[i].end {break}
                if ix.local(text,j)=="Destination" {retained.insert(unescape(&text[ix.nodes[j].open_end..ix.nodes[j].close_start]));}
            }
        }
    }
    for i in 0..ix.nodes.len() {
        if ix.local(text,i)=="HyperlinkPageDestination" && ix.attr(text,i,"Self").is_some_and(|s|destinations.contains(s)&&!retained.contains(s)) {removed.insert(i);}
    }
    let mut ranges=Vec::new();
    for i in removed {
        let range=ix.nodes[i].start..ix.nodes[i].end;
        if !ranges.iter().any(|(r,_): &(std::ops::Range<usize>,String)| r.start<=range.start && r.end>=range.end) {ranges.push((range,String::new()));}
    }
    // Remove owned bookmark tags while preserving unrelated child bookmarks.
    for i in 0..ix.nodes.len() {
        if ix.local(text,i)=="Bookmark" && ix.attr(text,i,"Destination").is_some_and(|s|destinations.contains(s)) {
            let n=&ix.nodes[i];
            ranges.push((n.start..n.open_end,String::new()));
            if n.close_start<n.end {ranges.push((n.close_start..n.end,String::new()));}
        }
    }
    w.splice("designmap.xml",ranges)
}

pub(super) struct Links { xml: String, bookmarks: String, levels: Vec<usize> }
impl Links {
    pub fn new() -> Self {Self {xml:String::new(),bookmarks:String::new(),levels:Vec::new()}}
    pub fn entry(&mut self,w:&mut Work<'_>,page:&str,title:&str,level:usize,bookmark:bool,runs:String)->String {
        let dest=w.new_id(); let source=w.new_id(); let link=w.new_id();
        self.xml.push_str(&format!("<HyperlinkPageDestination Self=\"{dest}\" Name=\"{dest}\" NameManually=\"true\" DestinationPage=\"{}\" ViewSetting=\"FitWindow\" ViewPercentage=\"100\" Hidden=\"true\"><Properties><ViewBounds Top=\"1e+256\" Left=\"1e+256\" Bottom=\"1e+256\" Right=\"1e+256\"/></Properties></HyperlinkPageDestination><Hyperlink Self=\"{link}\" Name=\"{link}\" Source=\"{source}\" Visible=\"false\" Highlight=\"None\" Width=\"Thin\" BorderStyle=\"Solid\" Hidden=\"true\"><Properties><BorderColor type=\"enumeration\">Black</BorderColor><Destination type=\"object\">{dest}</Destination></Properties></Hyperlink>",escape(page)));
        if bookmark {
            while self.levels.last().is_some_and(|&l|l>=level) {self.levels.pop();self.bookmarks.push_str("</Bookmark>");}
            self.bookmarks.push_str(&format!("<Bookmark Self=\"{}\" Name=\"{}\" Destination=\"{dest}\">",w.new_id(),escape(title)));
            self.levels.push(level);
        }
        format!("<HyperlinkTextSource Self=\"{source}\" Name=\"{source}\" Hidden=\"true\" AppliedCharacterStyle=\"n\"><Properties><AlternativeDestination Type=\"TocTextAnchor\" IndexMarkerId=\"0\" TextAnchorName=\"\" TocEntryPageNumberString=\"\" TocEntryLevel=\"0\"/></Properties>{runs}</HyperlinkTextSource>")
    }
    pub fn finish(mut self,w:&mut Work<'_>)->Result<(),EditError> {
        for _ in &self.levels {self.bookmarks.push_str("</Bookmark>");}
        self.xml.push_str(&self.bookmarks);
        let ix=w.index("designmap.xml")?;
        let at=ix.nodes[0].close_start;
        w.splice("designmap.xml",vec![(at..at,self.xml)])
    }
}
