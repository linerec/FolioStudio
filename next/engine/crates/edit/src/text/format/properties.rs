//! Read style properties and resolve their inheritance.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Property {
    pub(super) value: String,
    pub(super) xml: Option<String>,
}
pub(super) type Attrs = BTreeMap<String, Property>;
pub(super) fn attr(v: impl Into<String>) -> Property {
    Property {
        value: v.into(),
        xml: None,
    }
}
pub(super) fn property(k: &str, ty: &str, v: &str) -> Property {
    Property {
        value: v.into(),
        xml: Some(format!("<{k} type=\"{ty}\">{}</{k}>", escape(v))),
    }
}
pub(super) fn read_attrs(t: &str, ix: &Index, i: usize) -> Attrs {
    let mut a: Attrs = ix
        .attrs(t, i)
        .iter()
        .map(|a| {
            (
                t[a.name.0..a.name.1].into(),
                attr(unescape(&t[a.value.0..a.value.1])),
            )
        })
        .collect();
    if let Some(p) = ix.path(t, i, &["Properties"]) {
        for &c in ix.children(p) {
            let n = &ix.nodes[c];
            a.insert(
                ix.local(t, c).into(),
                Property {
                    value: if ix.local(t, c) == "BulletChar" {t[n.start..n.end].into()} else if ix.local(t, c) == "TabList" { t[n.open_end..n.close_start].trim().into() } else { unescape(t[n.open_end..n.close_start].trim()) },
                    xml: Some(t[n.start..n.end].into()),
                },
            );
        }
    }
    a
}
pub(super) fn style_attrs(s: &Style) -> Attrs {
    let t = format!(
        "{}{}</{}>",
        s.open,
        s.properties,
        if s.open.starts_with("<Paragraph") {
            "ParagraphStyleRange"
        } else {
            "CharacterStyleRange"
        }
    );
    let ix = Index::scan(&t).expect("previously parsed style");
    read_attrs(&t, &ix, 0)
}
pub(super) fn make_style(old: &Style, a: &Attrs) -> Style {
    if &style_attrs(old) == a {
        return old.clone();
    }
    let paragraph = old.open.starts_with("<Paragraph");
    let tag = if paragraph {
        "ParagraphStyleRange"
    } else {
        "CharacterStyleRange"
    };
    let indent = if paragraph { "\t\t\t" } else { "\t\t\t\t" };
    let mut t = format!("<{tag}");
    for (k, v) in a.iter().filter(|(_, v)| v.xml.is_none()) {
        t.push_str(&format!(" {k}=\"{}\"", escape(&v.value)));
    }
    t.push('>');
    let mut props: Vec<_> = a
        .iter()
        .filter_map(|(k, v)| v.xml.as_ref().map(|x| (k, x)))
        .collect();
    // IDML's common character property order is Leading, AppliedFont.
    props.sort_by_key(|(k, _)| (if k.as_str() == "Leading" { 0 } else { folio_idml::character_lines::xml_order(k).or_else(|| folio_idml::paragraph_decorations::xml_order(k)).unwrap_or(1) }, *k));
    if !props.is_empty() {
        t.push_str(&format!("\n{indent}<Properties>"));
        for (_, p) in props {
            t.push_str(&format!("\n{indent}\t{p}"));
        }
        t.push_str(&format!("\n{indent}</Properties>"));
    }
    // A dummy child supplies the same whitespace wrappers as InDesign.
    t.push_str(&format!(
        "\n{indent}<Content></Content>\n{}</{tag}>",
        &indent[..indent.len() - 1]
    ));
    let ix = Index::scan(&t).unwrap();
    Style::read(&t, &ix, 0)
}
pub(super) fn val<'a>(a: &'a Attrs, k: &str) -> &'a str {
    a.get(k).map(|v| v.value.as_str()).unwrap_or("")
}
pub(super) fn equal(a: &Property, b: &Property) -> bool {
    a.value == b.value
        || a.value
            .parse::<f64>()
            .ok()
            .zip(b.value.parse::<f64>().ok())
            .is_some_and(|(a, b)| (a - b).abs() < 1e-9)
}
pub(super) fn normalize(a: &mut Attrs, base: &Attrs) {
    a.retain(|k, v| {
        k.starts_with("AppliedParagraphStyle")
            || k.starts_with("AppliedCharacterStyle")
            || !base.get(k).is_some_and(|b| equal(v, b))
    });
}

pub(in crate::text) fn note_styles(w: &Work<'_>, p: &Style, c: &Style) -> Result<(Style, Style), EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let base = catalog.resolved("ParagraphStyle/$ID/[No paragraph style]")?;
    let t = w.text("Resources/Preferences.xml")?;
    let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
    let mut pa = style_attrs(p);
    let mut ca = style_attrs(c);
    if let Some(i) = (0..ix.nodes.len()).find(|&i| ix.local(t,i)=="TextDefault") {
        for (k,v) in read_attrs(t,&ix,i) {
            if matches!(k.as_str(), "Self" | "AppliedParagraphStyle" | "AppliedCharacterStyle" | "AppliedNamedGrid")
                || base.get(&k).is_some_and(|b| equal(&v,b)) { continue; }
            if crate::ops::paragraph_attr(&k) {pa.insert(k,v);} else {ca.insert(k,v);}
        }
    }
    Ok((make_style(p,&pa),make_style(c,&ca)))
}

pub(in crate::text) fn rebase_character(w: &Work<'_>, p: &Style, c: &Style) -> Result<Style, EditError> {
    let catalog = Catalog::read(&w.ed.parts)?;
    let mut ca=style_attrs(c);
    let base=catalog.character(&style_attrs(p),&ca)?;
    normalize(&mut ca,&base);
    Ok(make_style(c,&ca))
}
pub(super) const META: &[&str] = &[
    "Self",
    "Name",
    "Imported",
    "NextStyle",
    "SplitDocument",
    "EmitCss",
    "StyleUniqueId",
    "IncludeClass",
    "ExtendedKeyboardShortcut",
    "KeyboardShortcut",
    "EmptyNestedStyles",
    "EmptyLineStyles",
    "EmptyGrepStyles",
    "BasedOn",
    "PreviewColor",
];
pub(super) struct Catalog {
    pub(super) lists: Vec<folio_model::numbering::NumberingList>,
    pub(super) styles: BTreeMap<String, Attrs>,
}
impl Catalog {
    pub(super) fn read(parts: &BTreeMap<String, String>) -> Result<Self, EditError> {
        let mut styles = BTreeMap::new();
        if let Some(t) = parts.get("Resources/Styles.xml") {
            let ix = Index::scan(t).map_err(|e| EditError::new("xml", e))?;
            for i in 0..ix.nodes.len() {
                if matches!(ix.local(t, i), "ParagraphStyle" | "CharacterStyle")
                    && let Some(id) = ix.value(t, i, "Self") {
                    styles.insert(id, read_attrs(t, &ix, i));
                }
            }
        }
        let lists=parts.get("designmap.xml").map(|t|folio_idml::numbering::read(t)).transpose().map_err(|e|EditError::new("xml",e))?.unwrap_or_default();
        Ok(Self { styles, lists })
    }
    pub(super) fn list_ref(&self,props:&BTreeMap<String,Value>)->Result<(),EditError>{
        if let Some(value)=props.get("appliedNumberingList")
            && !self.lists.iter().any(|l|Some(l.id.as_str())==value.as_str()) {
            return Err(EditError::new("invalid","numbering list not found"));
        }
        Ok(())
    }
    pub(super) fn id(&self, kind: &str, id: &str) -> Result<String, EditError> {
        // the panel names of the built-in styles (the DOM's `[Basic Paragraph]`, `[No Paragraph Style]`, `[None]`)
        let built_in = match id {
            "[Basic Paragraph]" => "$ID/NormalParagraphStyle",
            "[No Paragraph Style]" | "[No paragraph style]" => "$ID/[No paragraph style]",
            "[None]" | "[No Character Style]" | "[No character style]" => "$ID/[No character style]",
            _ => "",
        };
        let found = self.styles.iter().find(|(k, a)| {
            k.starts_with(kind)
                && (k.as_str() == id
                    || val(a, "Name") == id
                    || !built_in.is_empty() && val(a, "Name") == built_in)
        });
        found
            .map(|(id, _)| id.clone())
            .ok_or_else(|| EditError::new("notFound", format!("{kind} {id}")))
    }
    pub(super) fn resolved(&self, id: &str) -> Result<Attrs, EditError> {
        fn visit(c: &Catalog, id: &str, seen: &mut Vec<String>) -> Result<Attrs, EditError> {
            if seen.iter().any(|s| s == id) {
                return Err(EditError::new("invalid", "cyclic style inheritance"));
            }
            let Some(a) = c.styles.get(id) else {
                return Ok(Attrs::new());
            };
            seen.push(id.into());
            let based = val(a, "BasedOn");
            let based = if based.starts_with("$ID/") {
                format!("{}/{}", id.split('/').next().unwrap(), based)
            } else {
                based.into()
            };
            let mut out = if based.is_empty() {
                Attrs::new()
            } else {
                visit(c, &based, seen)?
            };
            out.extend(
                a.iter()
                    .filter(|(k, _)| !META.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            Ok(out)
        }
        visit(self, id, &mut Vec::new())
    }
    pub(super) fn para(&self, p: &Attrs) -> Result<Attrs, EditError> {
        self.resolved(val(p, "AppliedParagraphStyle"))
    }
    pub(super) fn character(&self, p: &Attrs, c: &Attrs) -> Result<Attrs, EditError> {
        let mut base = self.para(p)?;
        base.extend(p.clone());
        base.extend(self.resolved(val(c, "AppliedCharacterStyle"))?);
        Ok(base)
    }
}

/// The style with one more attribute (`PageNumberType`, `ParagraphBreakType`…).
pub(in crate::text) fn with_attr(s: &Style, key: &str, value: &str) -> Style {
    let mut attrs = style_attrs(s);
    attrs.insert(key.to_owned(), attr(value.to_owned()));
    make_style(s, &attrs)
}

/// The style without an attribute.
pub(in crate::text) fn without_attr(s: &Style, key: &str) -> Style {
    let mut attrs = style_attrs(s);
    attrs.remove(key);
    make_style(s, &attrs)
}

/// The format of the object character Create Outlines leaves in a story (rules: outlines-c2): the replaced run's
/// character format with an explicit Leading (kept when the run already has one) and, when the object is taller than
/// the leading, the baseline shift InDesign adds.
pub(in crate::text) fn object_style(s: &Style, leading: f64, shift: f64) -> Style {
    let mut a = style_attrs(s);
    // a manual kerning value belongs to the pair of characters it was set between, not to the object (range-kern)
    a.remove("KerningValue");
    if !a.contains_key("Leading") {
        a.insert("Leading".into(), property("Leading", "unit", &crate::geom::num(leading)));
    }
    if shift > 0.0 {
        a.insert("BaselineShift".into(), attr(crate::geom::num(shift)));
    }
    make_style(s, &a)
}

pub(in crate::text) fn typing_style(s: &Style) -> Style {
    let mut attrs = style_attrs(s);
    attrs.remove("KerningValue");
    make_style(s, &attrs)
}
