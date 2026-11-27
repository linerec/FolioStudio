use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Read,
    Edit,
    File,
    Database,
    Withheld,
}

/// The host supplies the reviewed attached catalog, validator and wire mapping.
pub trait Gate {
    fn catalog(&self) -> &[Value];
    fn risk(&self, name: &str, args: &Value) -> Risk;
    fn validate(&self, args: &Value, schema: &Value) -> Result<(), String>;
}

const CORE: &[&str] = &[
    "documentInfo",
    "pageItems",
    "viewState",
    "editState",
    "render",
    "links",
    "textFind",
    "pageRange",
    "undo",
    "redo",
    "setSelection",
    "goToPage",
    "edit.createByDrag",
    "edit.insertText",
    "edit.replaceTextRange",
    "edit.textFormat",
    "edit.paragraphStyle",
    "edit.characterStyle",
    "edit.setObjectAttributes",
    "edit.textFrameOptions",
];

fn schema(name: &str, description: &str, input: Value) -> Value {
    json!({"name": name, "description": description, "input_schema": input})
}

pub fn fixed(gate: &impl Gate) -> Vec<Value> {
    let mut out: Vec<_> = CORE
        .iter()
        .filter_map(|name| {
            let tool = gate.catalog().iter().find(|t| t["name"] == *name)?;
            Some(schema(
                &name.replace('.', "_"),
                tool["description"].as_str()?,
                tool["inputSchema"].clone(),
            ))
        })
        .collect();
    out.push(schema("folio_find_tools", "Find reviewed Folio tools by name, description or risk class.",
        json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false})));
    out.push(schema("folio_call", "Invoke a reviewed Folio tool with its catalog name and arguments.",
        json!({"type":"object","properties":{"name":{"type":"string"},"args":{"type":"object"}},"required":["name","args"],"additionalProperties":false})));
    out
}

pub enum Action {
    Found(Value),
    Call {
        name: String,
        args: Value,
        risk: Risk,
    },
}

pub fn resolve(
    gate: &impl Gate,
    fixed: &[Value],
    name: &str,
    args: &Value,
) -> Result<Action, String> {
    let schema = fixed
        .iter()
        .find(|s| s["name"] == name)
        .ok_or("unknown model tool")?;
    gate.validate(args, &schema["input_schema"])?;
    if name == "folio_find_tools" {
        let query = args["query"]
            .as_str()
            .ok_or("query must be text")?
            .to_lowercase();
        let matches: Vec<_> = gate.catalog().iter().filter_map(|t| {
            let name = t["name"].as_str()?;
            let risk = gate.risk(name, &Value::Null);
            if risk == Risk::Withheld { return None; }
            let hay = format!("{name} {} {risk:?}", t["description"].as_str().unwrap_or("")).to_lowercase();
            query.split_whitespace().all(|word| hay.contains(word)).then(|| json!({
                "name":name, "risk":risk, "description":t["description"], "inputSchema":t["inputSchema"]
            }))
        }).collect();
        return Ok(Action::Found(json!({"tools": matches})));
    }
    let (name, args) = if name == "folio_call" {
        (
            args["name"].as_str().ok_or("missing tool name")?,
            &args["args"],
        )
    } else {
        (
            *CORE
                .iter()
                .find(|n| n.replace('.', "_") == name)
                .ok_or("unknown tool")?,
            args,
        )
    };
    let risk = gate.risk(name, args);
    if risk == Risk::Withheld {
        return Err("tool is withheld".into());
    }
    let tool = gate
        .catalog()
        .iter()
        .find(|t| t["name"] == name)
        .ok_or("tool is withheld")?;
    gate.validate(args, &tool["inputSchema"])?;
    Ok(Action::Call {
        name: name.into(),
        args: args.clone(),
        risk,
    })
}
