//! Store page sections and compute page names and numbers.

use super::*;

impl Book {
    pub(super) fn check_numbering_styles(&self) -> Result<(), EditError> {
        if self.sections.iter().any(|section| !supports_numbering_style(&section_style(section))) {
            return Err(EditError::new("unsupported", "this document uses a page-numbering style not supported by the page panel"));
        }
        Ok(())
    }

    pub(super) fn set_section(
        &mut self,
        w: &mut Work<'_>,
        page: &str,
        numbering: (Option<u32>, &str),
        prefix: &str,
        include_prefix: bool,
        marker: &str,
    ) -> Result<(), EditError> {
        let (start, style) = numbering;
        self.locate(page)?;
        if prefix.encode_utf16().count() > 8
            || start.is_some_and(|n| n == 0 || n > 999999)
            || !supports_numbering_style(style)
        {
            return Err(EditError::new("invalid", "invalid page numbering"));
        }
        let at = self.sections.iter().position(|s| get(s, "PageStart") == page);
        let i = if let Some(i) = at {
            i
        } else {
            let id = w.new_id();
            self.sections.push(format!("<Section Self=\"{id}\" Length=\"1\" AlternateLayoutLength=\"1\" AlternateLayout=\"\" Name=\"\" ContinueNumbering=\"true\" IncludeSectionPrefix=\"false\" Marker=\"\" PageStart=\"{}\" SectionPrefix=\"\"><Properties><PageNumberStyle type=\"enumeration\">Arabic</PageNumberStyle></Properties></Section>",escape(page)));
            self.sections.len() - 1
        };
        let s = &mut self.sections[i];
        set(
            s,
            "ContinueNumbering",
            if start.is_some() { "false" } else { "true" },
        );
        if let Some(n) = start {
            set(s, "PageNumberStart", &n.to_string())
        } else {
            let ix = scan(s)?;
            if let Some(a) = ix
                .attrs(s, 0)
                .into_iter()
                .find(|a| &s[a.name.0..a.name.1] == "PageNumberStart")
            {
                s.replace_range(a.name.0..a.value.1 + 1, "");
            }
        }
        for (k, v) in [
            ("Name", prefix),
            ("SectionPrefix", prefix),
            (
                "IncludeSectionPrefix",
                if include_prefix { "true" } else { "false" },
            ),
            ("Marker", marker),
        ] {
            set(s, k, v)
        }
        let ix = scan(s)?;
        let n = ix.path(s, 0, &["Properties", "PageNumberStyle"]).unwrap();
        s.replace_range(ix.nodes[n].open_end..ix.nodes[n].close_start, style);
        self.shuffle_and_position(w)?;
        if at.is_none() {
            let (_, pos) = self.locate(page)?;
            set(
                &mut self.sections[i],
                "AlternateLayout",
                if pos == 0 { "$ID/" } else { "" },
            );
        }
        Ok(())
    }

    fn first_number(&self) -> u32 {
        self.sections
            .first()
            .filter(|s| get(s, "ContinueNumbering") == "false")
            .and_then(|s| get(s, "PageNumberStart").parse().ok())
            .unwrap_or(1)
    }
    pub(super) fn numbers(&self) -> std::collections::BTreeMap<String, u32> {
        let order = self.order();
        let mut n = self.first_number();
        let mut out = std::collections::BTreeMap::new();
        for (i, id) in order.iter().enumerate() {
            if i > 0
                && let Some(s) = self
                    .sections
                    .iter()
                    .skip(1)
                    .find(|s| get(s, "PageStart") == *id)
                && get(s, "ContinueNumbering") == "false"
            {
                n = get(s, "PageNumberStart").parse().unwrap_or(n);
            }
            out.insert(id.clone(), n);
            n += 1;
        }
        out
    }
    pub(super) fn number(&mut self) {
        let order = self.order();
        if order.is_empty() {
            return;
        }
        if let Some(s) = self.sections.first_mut() {
            set(s, "PageStart", &order[0]);
        }
        self.sections
            .retain(|s| order.contains(&get(s, "PageStart")));
        self.sections.sort_by_key(|s| {
            order
                .iter()
                .position(|id| *id == get(s, "PageStart"))
                .unwrap()
        });
        // A book gives its documents' first pages a number (`set_book_start`): it lives on the first section while that section
        // continues numbering, as InDesign's DOM reports it (its IDML spells it only in the page names).
        let mut number = self
            .sections
            .first()
            .filter(|s| get(s, "ContinueNumbering") != "false")
            .and_then(|s| get(s, "PageNumberStart").parse::<u32>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(1);
        // pages of hidden spreads before this one in the numbering run: InDesign counts the pages that remain
        let mut hidden_before = 0u32;
        for k in 0..self.sections.len() {
            let start = order
                .iter()
                .position(|id| *id == get(&self.sections[k], "PageStart"))
                .unwrap();
            let end = if k + 1 < self.sections.len() {
                order
                    .iter()
                    .position(|id| *id == get(&self.sections[k + 1], "PageStart"))
                    .unwrap()
            } else {
                order.len()
            };
            let s = &mut self.sections[k];
            set(s, "Length", &(end - start).to_string());
            set(
                s,
                "AlternateLayoutLength",
                &(order.len() - start).to_string(),
            );
            if get(s, "ContinueNumbering") == "false" {
                number = get(s, "PageNumberStart").parse().unwrap_or(1);
                hidden_before = 0;
            }
            let style = section_style(s);
            let prefix = get(s, "SectionPrefix");
            let include = get(s, "IncludeSectionPrefix") == "true";
            let marker = get(s, "Marker");
            let continuing = get(s, "ContinueNumbering");
            for id in &order[start..end] {
                let (s, p) = self.locate(id).unwrap();
                let hidden = self.spreads[s].hidden();
                let pg = &mut self.spreads[s].pages[p].xml;
                let label = number_string(number, &style);
                set(
                    pg,
                    "Name",
                    &format!("{}{}", if include { &prefix } else { "" }, label),
                );
                let ix = Index::scan(pg).unwrap();
                if let Some(i) = ix.path(pg, 0, &["Properties", "Descriptor"]) {
                    let values = [
                        ("string", prefix.clone()),
                        ("enumeration", style.clone()),
                        ("boolean", continuing.clone()),
                        ("boolean", include.to_string()),
                        ("long", number.to_string()),
                        // the second number counts the pages that are output: a hidden page has none
                        ("long", if hidden { "999999".to_owned() } else { (number - hidden_before).to_string() }),
                        ("string", marker.clone()),
                    ];
                    let body = values
                        .iter()
                        .map(|(ty, v)| {
                            if v.is_empty() {
                                // as InDesign spells an empty item
                                format!("<ListItem type=\"{ty}\"></ListItem>")
                            } else {
                                format!("<ListItem type=\"{ty}\">{}</ListItem>", escape(v))
                            }
                        })
                        .collect::<Vec<_>>();
                    // the items keep the line breaks and tabs InDesign wrote between them
                    let inner = &pg[ix.nodes[i].open_end..ix.nodes[i].close_start];
                    let lead = inner.find('<').map_or("", |k| &inner[..k]).to_owned();
                    let tail = inner.rfind('>').map_or("", |k| &inner[k + 1..]).to_owned();
                    let body = format!("{lead}{}{tail}", body.join(&lead));
                    pg.replace_range(ix.nodes[i].open_end..ix.nodes[i].close_start, &body);
                }
                if hidden {
                    hidden_before += 1;
                }
                number += 1;
            }
        }
    }
}

fn section_style(t: &str) -> String {
    let ix = Index::scan(t).unwrap();
    ix.path(t, 0, &["Properties", "PageNumberStyle"])
        .map(|i| {
            t[ix.nodes[i].open_end..ix.nodes[i].close_start]
                .trim()
                .to_owned()
        })
        .unwrap_or_else(|| "Arabic".into())
}

fn supports_numbering_style(style: &str) -> bool {
    ["Arabic", "UpperRoman", "LowerRoman", "UpperLetters", "LowerLetters"].contains(&style)
}

fn number_string(mut n: u32, style: &str) -> String {
    match style {
        "UpperRoman" | "LowerRoman" => {
            let mut s = String::new();
            for (v, c) in [
                (1000, "M"),
                (900, "CM"),
                (500, "D"),
                (400, "CD"),
                (100, "C"),
                (90, "XC"),
                (50, "L"),
                (40, "XL"),
                (10, "X"),
                (9, "IX"),
                (5, "V"),
                (4, "IV"),
                (1, "I"),
            ] {
                while n >= v {
                    s.push_str(c);
                    n -= v
                }
            }
            if style == "LowerRoman" {
                s.to_lowercase()
            } else {
                s
            }
        }
        "UpperLetters" | "LowerLetters" => {
            let mut letters = Vec::new();
            while n > 0 {
                n -= 1;
                letters.push((b'A' + (n % 26) as u8) as char);
                n /= 26
            }
            let s = letters.into_iter().rev().collect::<String>();
            if style == "LowerLetters" {
                s.to_lowercase()
            } else {
                s
            }
        }
        _ => n.to_string(),
    }
}

/// Book ▸ Repaginate: the number the document's first page takes in its book. A first section that has a number of its own
/// (`ContinueNumbering` false) keeps it, as InDesign's book does (oracle probe `bookx-c1-repaginate2`, cases e1-e3); otherwise the
/// number goes on the section as `PageNumberStart` and the pages are named again.
pub(crate) fn set_book_start(w: &mut Work<'_>, start: &u32) -> Result<(), EditError> {
    if *start == 0 || *start > 999_999 {
        return Err(EditError::new("invalid", "invalid page numbering"));
    }
    let mut b = Book::read(w)?;
    b.number();
    let Some(first) = b.sections.first_mut() else {
        return Err(EditError::new("notFound", "section"));
    };
    if get(first, "ContinueNumbering") != "false" {
        set(first, "PageNumberStart", &start.to_string());
    }
    b.write(w)
}

/// Pages panel ▸ Section menu ▸ Delete Section. InDesign 2025 (DOM probe oracle/measured/delete-section-2025.json): the section that starts on
/// `page` is removed and its pages number on from the section before it (its prefix and style); the first section is the document's default
/// one and cannot be deleted ("Cannot delete the default section in a document", error 19462).
pub(crate) fn delete_section(w: &mut Work<'_>, page: &String) -> Result<(), EditError> {
    let mut b = Book::read(w)?;
    b.check_numbering_styles()?;
    b.locate(page)?;
    match b.sections.iter().position(|s| get(s, "PageStart") == *page) {
        Some(0) => return Err(EditError::new("invalid", "cannot delete the default section")),
        Some(i) => {
            b.sections.remove(i);
        }
        None => return Err(EditError::new("notFound", "section")),
    }
    b.shuffle_and_position(w)?;
    b.write(w)
}
