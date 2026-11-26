//! Paragraph composer identifiers shared by editing and IDML queries.
use serde_json::Value;

pub const VALUES: &[&str] = &["HL Composer", "HL Single", "HL Composer Optyca", "HL Single Optyca"];
pub const DEFAULT: &str = "HL Composer";

pub struct Flag { pub api: &'static str, pub xml: &'static str }
pub const FLAGS: &[Flag] = &[
    Flag { api: "balanceRaggedLines", xml: "BalanceRaggedLines" },
    Flag { api: "alignToBaseline", xml: "GridAlignment" },
    Flag { api: "gridAlignFirstLineOnly", xml: "GridAlignFirstLineOnly" },
];
pub fn flag(api: &str) -> Option<&'static Flag> { FLAGS.iter().find(|p| p.api == api) }
impl Flag {
    pub fn property_type(&self) -> Option<&'static str> {
        (self.api == "balanceRaggedLines").then_some("enumeration")
    }
    pub fn encode(&self, value: &Value) -> Option<String> {
        let value=value.as_bool()?;
        Some(if self.api=="balanceRaggedLines" {if value {"VeeShape"} else {"NoBalancing"}} else if self.api=="alignToBaseline" {if value {"AlignBaseline"} else {"None"}} else if value {"true"} else {"false"}.into())
    }
    pub fn decode(&self, value: &str) -> Value {
        Value::Bool(if self.api=="balanceRaggedLines" {value=="VeeShape"} else if self.api=="alignToBaseline" { !value.is_empty() && value!="None" } else {value=="true"})
    }
}

pub fn encode(value: &Value) -> Option<String> {
    let name = value.as_str()?.trim_start_matches("$ID/");
    VALUES.contains(&name).then(|| name.to_owned())
}

pub fn decode(value: &str) -> Value {
    Value::String(if value.is_empty() { DEFAULT } else { value.trim_start_matches("$ID/") }.to_owned())
}
