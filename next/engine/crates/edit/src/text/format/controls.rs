//! Translate formatting controls to and from IDML style properties.

use super::*;

// These are the direct controls implemented in C3. Unknown properties are rejected.
pub(super) fn direct(k: &str, v: &Value) -> Result<(bool, String, Property), EditError> {
    if let Some(p) = folio_idml::paragraph_columns::property(k) {
        let value = p.encode(v).ok_or_else(|| EditError::new("invalid", format!("invalid {k}")))?;
        let encoded = p.property_type(&value).map_or_else(|| attr(value.clone()), |ty| property(p.xml, ty, &value));
        return Ok((true, p.xml.into(), encoded));
    }
    if let Some(p) = folio_idml::paragraph_options::property(k) {
        let value = p.encode(v).ok_or_else(|| EditError::new("invalid", format!("invalid paragraph property {k}: {v}")))?;
        return Ok((true, p.xml.into(), attr(value)));
    }
    if let Some(p)=folio_idml::parcomp::flag(k) {
        let value=p.encode(v).ok_or_else(||EditError::new("invalid",format!("invalid paragraph property {k}")))?;
        let encoded = p.property_type().map_or_else(|| attr(value.clone()), |ty| property(p.xml, ty, &value));
        return Ok((true,p.xml.into(),encoded));
    }
    if k == "composer" {
        let value = folio_idml::parcomp::encode(v).ok_or_else(|| EditError::new("invalid", "unknown paragraph composer"))?;
        return Ok((true, "Composer".into(), attr(value)));
    }
    if let Some(p) = folio_idml::listprops::property(k) {
        let (value,xml)=p.encode(v).ok_or_else(||EditError::new("invalid",format!("invalid list property {k}: {v}")))?;
        return Ok((true,p.xml.into(),Property {value,xml}));
    }
    if k == "tabList" {
        let inner = folio_idml::tabstops::encode(v).ok_or_else(|| EditError::new("invalid", "invalid tab stops"))?;
        return Ok((true, "TabList".into(), Property { value: inner.clone(), xml: Some(format!("<TabList type=\"list\">{inner}</TabList>")) }));
    }
    if let Some(p) = folio_idml::opentype::property(k) {
        let value = p.encode(v).ok_or_else(|| EditError::new("invalid", format!("invalid text property {k}: {v}")))?;
        return Ok((false, p.xml.into(), attr(value)));
    }
    if let Some(p) = folio_idml::paragraph_decorations::property(k) {
        let value = p.encode(v).ok_or_else(|| EditError::new("invalid", format!("invalid paragraph property {k}: {v}")))?;
        let v = if p.is_reference() { let ty = p.property_type(&value); property(p.xml, ty, &value) } else { attr(value) };
        return Ok((true, p.xml.into(), v));
    }
    if let Some(p) = folio_idml::character_lines::property(k) {
        let value = p.encode(v).ok_or_else(|| EditError::new("invalid", format!("invalid character line property {k}: {v}")))?;
        let encoded = p.property_type(&value).map_or_else(|| attr(value.clone()), |ty| property(p.xml, ty, &value));
        return Ok((false, p.xml.into(), encoded));
    }
    if let Some(xml) = paint::xml(k) {
        let value = paint::encode(k, v).ok_or_else(|| EditError::new("invalid", format!("invalid text property {k}: {v}")))?;
        return Ok((false, xml.into(), attr(value)));
    }
    let paragraph = matches!(
        k,
        "justification"
            | "leftIndent"
            | "rightIndent"
            | "firstLineIndent"
            | "spaceBefore"
            | "spaceAfter"
            | "lastLineIndent"
            | "bulletsAndNumberingListType"
            | "numberingContinue"
            // InDesign 2025 writes Rotate Single-Byte Characters on the paragraph (vtext-c1, oracle vt-rot)
            | "rotateSingleByteCharacters"
    );
    let name = format!(
        "{}{}",
        k.get(..1).unwrap_or("").to_uppercase(),
        k.get(1..).unwrap_or("")
    );
    let bad = || EditError::new("invalid", format!("invalid text property {k}: {v}"));
    let prop = match k {
        "appliedFont" | "fontStyle" => {
            let text = v
                .as_str()
                .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
                .ok_or_else(bad)?;
            if k == "appliedFont" {
                property(&name, "string", text)
            } else {
                attr(text)
            }
        }
        "underline" | "strikeThru" | "ligatures" | "noBreak" | "numberingContinue" => attr(v.as_bool().ok_or_else(bad)?.to_string()),
        // Character panel menu (vertical text): Tate-Chu-Yoko, Rotate Single-Byte Characters (vtext-c1)
        "tatechuyoko" | "rotateSingleByteCharacters" => attr(v.as_bool().ok_or_else(bad)?.to_string()),
        // Tate-Chu-Yoko Settings: up/down and left/right offsets; Character Rotation (-360° to 360°)
        "tatechuyokoXOffset" | "tatechuyokoYOffset" => attr(v.as_f64().filter(|n| n.is_finite() && n.abs() <= 1000.0).ok_or_else(bad)?.to_string()),
        "characterRotation" => attr(v.as_f64().filter(|n| n.is_finite() && n.abs() <= 360.0).ok_or_else(bad)?.to_string()),
        // Type ▸ Bulleted & Numbered Lists (oracle text-format-lists)
        "bulletsAndNumberingListType" => attr(match v.as_str().ok_or_else(bad)? {
            "NO_LIST" => "NoList",
            "BULLET_LIST" => "BulletList",
            "NUMBERED_LIST" => "NumberedList",
            _ => return Err(bad()),
        }),
        // Character panel menu: Superscript, Subscript (and the OpenType forms), back to Normal
        // (oracle text-format-char: char-superscript … char-position-normal)
        "position" => attr(match v.as_str().ok_or_else(bad)? {
            "NORMAL" => "Normal",
            "SUPERSCRIPT" => "Superscript",
            "SUBSCRIPT" => "Subscript",
            "OT_SUPERSCRIPT" => "OTSuperscript",
            "OT_SUBSCRIPT" => "OTSubscript",
            "OT_NUMERATOR" => "OTNumerator",
            "OT_DENOMINATOR" => "OTDenominator",
            _ => return Err(bad()),
        }),
        "capitalization" | "justification" => {
            let name = match v.as_str().ok_or_else(bad)? {
                "NORMAL" if k == "capitalization" => "Normal",
                "ALL_CAPS" if k == "capitalization" => "AllCaps",
                "SMALL_CAPS" if k == "capitalization" => "SmallCaps",
                "CAP_TO_SMALL_CAP" if k == "capitalization" => "CapToSmallCap",
                "LOWER_CASE" if k == "capitalization" => "LowerCase",
                "LEFT_ALIGN" if paragraph => "LeftAlign",
                "CENTER_ALIGN" if paragraph => "CenterAlign",
                "RIGHT_ALIGN" if paragraph => "RightAlign",
                "LEFT_JUSTIFIED" if paragraph => "LeftJustified",
                "FULLY_JUSTIFIED" if paragraph => "FullyJustified",
                "CENTER_JUSTIFIED" if paragraph => "CenterJustified",
                "RIGHT_JUSTIFIED" if paragraph => "RightJustified",
                "TO_BINDING_SIDE" if paragraph => "ToBindingSide",
                "AWAY_FROM_BINDING_SIDE" if paragraph => "AwayFromBindingSide",
                _ => return Err(bad()),
            };
            attr(name)
        }
        "kerningMethod" => attr(match v.as_str() {
            Some("Metrics") => "$ID/Metrics",
            // Native optical spacing differs (measurements/text-format-optical.json).
            // Do not offer an edit whose composer still substitutes metric kerning.
            Some("Optical") => {
                return Err(unsupported(
                    "optical kerning composition is not yet supported",
                ));
            }
            _ => return Err(bad()),
        }),
        "leading" if v.as_str() == Some("auto") => property(&name, "enumeration", "Auto"),
        // Character panel: Vertical Scale, Horizontal Scale (25% to 400%) and Skew (-85 to 85 degrees)
        // (oracle charpanel-c3-scale, charpanel-c3-skew)
        "horizontalScale" | "verticalScale" => {
            let num = v.as_f64().filter(|n| n.is_finite() && (25.0..=400.0).contains(n)).ok_or_else(bad)?;
            attr(num.to_string())
        }
        "skew" => {
            let num = v.as_f64().filter(|n| n.is_finite() && (-85.0..=85.0).contains(n)).ok_or_else(bad)?;
            attr(num.to_string())
        }
        // Character panel: Language (oracle charpanel-c3-language)
        "appliedLanguage" => {
            let text = v.as_str().filter(|s| super::languages::known(s)).ok_or_else(bad)?;
            // InDesign writes the name of a built-in without the $ID/ prefix: AppliedLanguage="[No Language]"
            if text.starts_with('[') { attr(text) } else { attr(format!("$ID/{text}")) }
        }
        "pointSize" | "leading" | "tracking" | "kerningValue" | "baselineShift" | "leftIndent"
        | "rightIndent" | "firstLineIndent" | "lastLineIndent" | "spaceBefore" | "spaceAfter" => {
            let num = v
                .as_f64()
                .filter(|n| n.is_finite() && n.abs() <= 10000.0)
                .ok_or_else(bad)?;
            if matches!(k, "pointSize" | "leading") && num <= 0.0 {
                return Err(bad());
            }
            if matches!(
                k,
                "leftIndent" | "rightIndent" | "spaceBefore" | "spaceAfter"
            ) && num < 0.0
            {
                return Err(bad());
            }
            if k == "leading" {
                property(&name, "unit", &num.to_string())
            } else {
                attr(num.to_string())
            }
        }
        _ => return Err(EditError::new("unsupported", format!("text property {k}"))),
    };
    Ok((paragraph, name, prop))
}

const CONTROLS: &[&str] = &[
    "appliedFont",
    "fontStyle",
    "pointSize",
    "leading",
    "tracking",
    "kerningMethod",
    "kerningValue",
    "baselineShift",
    "underline",
    "strikeThru",
    "ligatures",
    "noBreak",
    "position",
    "capitalization",
    "justification",
    "leftIndent",
    "rightIndent",
    "firstLineIndent",
    "lastLineIndent",
    "spaceBefore",
    "spaceAfter",
    "horizontalScale",
    "verticalScale",
    "skew",
    "tatechuyoko",
    "tatechuyokoXOffset",
    "tatechuyokoYOffset",
    "characterRotation",
    "rotateSingleByteCharacters",
    "appliedLanguage",
    "bulletsAndNumberingListType",
    "numberingContinue",
];
pub(super) fn controls() -> impl Iterator<Item = &'static str> {
    CONTROLS.iter().copied().chain(folio_idml::character_lines::PROPERTIES.iter().map(|p| p.api)).chain(folio_idml::opentype::PROPERTIES.iter().map(|p| p.api))
        .chain(folio_idml::paragraph_columns::PROPERTIES.iter().map(|p| p.api))
        .chain(folio_idml::paragraph_options::PROPERTIES.iter().map(|p| p.api)).chain(folio_idml::paragraph_decorations::PROPERTIES.iter().map(|p| p.api))
        .chain(["tabList", "composer"])
        .chain(folio_idml::listprops::PROPERTIES.iter().map(|p| p.api))
        .chain(folio_idml::parcomp::FLAGS.iter().map(|p| p.api))
}
pub(super) fn validate_paragraph_options(direct: &Attrs, base: &Attrs) -> Result<(), EditError> {
    if folio_idml::paragraph_options::valid_ranges(|k| direct.get(k).or_else(|| base.get(k)).map(|p| p.value.as_str())) {
        Ok(())
    } else {
        Err(EditError::new("invalid", "paragraph spacing requires minimum <= desired <= maximum"))
    }
}
pub(super) fn control_key(name: &str) -> String {
    if let Some(p) = folio_idml::paragraph_columns::property(name) { return p.xml.into(); }
    if let Some(p) = folio_idml::character_lines::property(name) { return p.xml.into(); }
    if let Some(p) = folio_idml::listprops::property(name) { return p.xml.into(); }
    if let Some(p) = folio_idml::paragraph_options::property(name) { return p.xml.into(); }
    if let Some(p) = folio_idml::paragraph_decorations::property(name) { return p.xml.into(); }
    if let Some(p) = folio_idml::parcomp::flag(name) { return p.xml.into(); }
    if let Some(p) = folio_idml::opentype::property(name) { return p.xml.into(); }
    format!("{}{}", name[..1].to_uppercase(), &name[1..])
}

pub(super) fn api_value(key: &str, v: &str) -> Value {
    if let Some(value) = paint::decode(key, v) { return value; }
    if let Some(p) = folio_idml::paragraph_columns::property(key) { return p.decode(v); }
    if let Some(p) = folio_idml::character_lines::property(key) { return p.decode(v); }
    if let Some(p) = folio_idml::paragraph_options::property(key) { return p.decode(v); }
    if let Some(p) = folio_idml::listprops::property(key) { return p.decode(v); }
    if let Some(p) = folio_idml::paragraph_decorations::property(key) { return p.decode(v); }
    if let Some(p)=folio_idml::parcomp::flag(key) {return p.decode(v);}
    if key == "composer" { return folio_idml::parcomp::decode(v); }
    if key == "tabList" { return folio_idml::tabstops::decode(v); }
    if let Some(p) = folio_idml::opentype::property(key) { return p.decode(v); }
    if v.is_empty() {
        return Value::Null;
    }
    if matches!(key, "capitalization" | "justification" | "bulletsAndNumberingListType") {
        let mut out = String::new();
        for (i, c) in v.chars().enumerate() {
            if i > 0 && c.is_ascii_uppercase() {
                out.push('_');
            }
            out.push(c.to_ascii_uppercase());
        }
        return Value::String(out);
    }
    if key == "kerningMethod" {
        return Value::String(v.trim_start_matches("$ID/").into());
    }
    if key == "appliedLanguage" {
        return Value::String(v.trim_start_matches("$ID/").into());
    }
    if key == "leading" && v == "Auto" {
        return Value::String("auto".into());
    }
    if matches!(key, "underline" | "strikeThru" | "ligatures" | "noBreak" | "numberingContinue" | "tatechuyoko" | "rotateSingleByteCharacters") {
        return Value::Bool(v == "true");
    }
    if key == "position" {
        return Value::String(match v {
            "OTSuperscript" => "OT_SUPERSCRIPT".into(),
            "OTSubscript" => "OT_SUBSCRIPT".into(),
            "OTNumerator" => "OT_NUMERATOR".into(),
            "OTDenominator" => "OT_DENOMINATOR".into(),
            other => other.to_ascii_uppercase(),
        });
    }
    if !matches!(key, "appliedFont" | "fontStyle")
        && let Ok(n) = v.parse::<f64>() {
        return serde_json::json!(n);
    }
    Value::String(v.into())
}
