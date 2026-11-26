use crate::{Options, rules};
use folio_model::preflight::{Profile, Profiles};
use serde_json::{Value, json};

pub fn schema() -> Value {
    let params: serde_json::Map<_,_> = serde_json::to_value(Options::default()).expect("default preflight options are JSON values")
        .as_object().expect("preflight options serialize as an object")
        .iter().filter(|(_,v)|v.is_number()).map(|(k,v)|(k.clone(),v.clone())).collect();
    json!({"rules":rules::ALL.iter().map(|id|json!({"id":id,"title":rules::title(id),"category":category(id)})).collect::<Vec<_>>(),"parameters":params})
}

fn category(id: &str) -> &'static str {
    if id.starts_with("LINK_") {"links"}
    else if id.starts_with("FONT_") || [rules::MISSING_GLYPH,rules::OVERSET_TEXT,rules::SMALL_TEXT_INKS,rules::BLACK_TEXT_KNOCKOUT].contains(&id) {"text"}
    else if id.starts_with("IMAGE_") || [rules::HAIRLINE,rules::EMPTY_FRAME,rules::TRANSPARENCY_USED,rules::UNSUPPORTED_EFFECT].contains(&id) {"objects"}
    else if [rules::BLEED_MISSING,rules::SAFETY_MARGIN,rules::HIDDEN_CONTENT,rules::BLANK_PAGE,rules::PAGE_SIZES].contains(&id) {"document"}
    else {"color"}
}

pub fn validate(state: &Profiles) -> Result<(), String> {
    let mut names = std::collections::BTreeSet::new();
    for p in &state.profiles {
        if p.name.trim().is_empty() || !names.insert(&p.name) {return Err("profile names must be nonempty and unique".into());}
        options(p)?;
    }
    if !state.active.is_empty() && !names.contains(&state.active) {return Err("working profile does not exist".into());}
    Ok(())
}

pub fn options(profile: &Profile) -> Result<Options, String> {
    if profile.disabled_rules.iter().any(|id|!rules::ALL.contains(&id.as_str())) {return Err("unknown preflight rule".into());}
    let mut values = serde_json::to_value(Options::default()).expect("default preflight options are JSON values");
    for (name, value) in &profile.parameters {
        if !values[name].is_number() || !value.is_finite() || *value < 0.0 {return Err(format!("invalid preflight parameter: {name}"));}
        values[name] = json!(value);
    }
    let mut options: Options = serde_json::from_value(values).map_err(|e|e.to_string())?;
    if options.image_ppi_error > options.image_ppi_warn || options.bitmap_ppi_error > options.bitmap_ppi_warn {return Err("resolution error threshold exceeds warning threshold".into());}
    options.disabled_rules = profile.disabled_rules.clone();
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_parameters_keep_the_existing_error() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let mut p = Profile::default();
            p.parameters.insert("hairline_pt".into(), value);
            assert_eq!(options(&p).err().as_deref(), Some("invalid preflight parameter: hairline_pt"));
        }
        let p = Profile { disabled_rules: vec!["missing-rule".into()], ..Default::default() };
        assert_eq!(options(&p).err().as_deref(), Some("unknown preflight rule"));
        let state = Profiles { active: "missing".into(), profiles: vec![] };
        assert_eq!(validate(&state).err().as_deref(), Some("working profile does not exist"));
    }
    #[test]
    fn profile_parameters_and_validation() {
        let mut p=Profile{name:"Press".into(),..Default::default()};
        p.disabled_rules.push(rules::OVERSET_TEXT.into());
        p.parameters.insert("hairline_pt".into(),0.5);
        let o=options(&p).unwrap();
        assert_eq!(o.hairline_pt,0.5);
        assert_eq!(o.image_ppi_warn,Options::default().image_ppi_warn);
        assert_eq!(o.disabled_rules,[rules::OVERSET_TEXT]);
        p.parameters.insert("image_ppi_error".into(),600.0);
        assert!(options(&p).is_err());
        p.parameters.remove("image_ppi_error");
        p.parameters.insert("unknown".into(),1.0);
        assert!(options(&p).is_err());
        assert!(validate(&Profiles{active:"missing".into(),profiles:vec![]}).is_err());
    }
}
