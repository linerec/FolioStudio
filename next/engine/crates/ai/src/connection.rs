use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ureq::http::Uri;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Anthropic,
    Openai,
    Gemini,
    Openrouter,
    Ollama,
    LmStudio,
    Local,
    Custom,
    Acp,
}

impl Kind {
    pub fn base(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Openai => "https://api.openai.com/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Openrouter => "https://openrouter.ai/api/v1",
            Self::Ollama => "http://localhost:11434/v1",
            Self::LmStudio => "http://localhost:1234/v1",
            _ => "",
        }
    }
    pub fn needs_key(self) -> bool {
        matches!(
            self,
            Self::Anthropic | Self::Openai | Self::Gemini | Self::Openrouter
        )
    }
}
pub fn templates() -> Value {
    use Kind::*;
    json!([Anthropic,Openai,Gemini,Openrouter,Ollama,LmStudio,Local,Custom,Acp].map(|kind|
        json!({"kind":kind,"baseUrl":kind.base(),"model":if kind==Anthropic {"claude-opus-5-5"}else{""},
        "requiresKey":kind.needs_key(),"available":kind!=Acp || cfg!(any(windows,target_os="linux",target_os="macos"))})))
}
fn images() -> bool {
    true
}
fn effort() -> String {
    "medium".into()
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub model: String,
    #[serde(default = "effort")]
    pub effort: String,
    #[serde(default = "images")]
    pub send_images: bool,
    #[serde(default)]
    pub budget_warning: Option<f64>,
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
pub fn is_loopback(url: &str) -> bool {
    url.parse::<Uri>()
        .ok()
        .and_then(|u| u.host().map(str::to_owned))
        .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]"))
}
impl Connection {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let mut value = value.clone();
        for (field, empty) in [("args", json!([])), ("env", json!({}))] {
            if let Some(text) = value[field].as_str() {
                value[field] = if text.trim().is_empty() {
                    empty
                } else {
                    serde_json::from_str(text)
                        .map_err(|_| "invalid agent arguments or environment")?
                };
            }
        }
        let mut c: Self =
            serde_json::from_value(value.clone()).map_err(|_| "invalid connection fields")?;
        if !valid_id(&c.id)
            || c.name.trim().is_empty()
            || c.name.len() > 256
            || !matches!(
                c.effort.as_str(),
                "low" | "medium" | "high" | "xhigh" | "max"
            )
            || c.budget_warning.is_some_and(|n| !n.is_finite() || n < 0.)
        {
            return Err("invalid connection".into());
        }
        if c.base_url.is_empty() {
            c.base_url = c.kind.base().into()
        }
        c.base_url = c.base_url.trim_end_matches('/').into();
        if c.kind == Kind::Acp {
            if !c.base_url.is_empty() {
                return Err("invalid agent address".into());
            }
            if c.command.trim().is_empty()
                || c.command.len() > 4096
                || c.command.chars().any(char::is_control)
                || c.args.len() > 128
                || c.args
                    .iter()
                    .any(|s| s.len() > 8192 || s.contains(char::from(0)))
                || c.env.len() > 64
                || c.env.iter().any(|(name, v)| {
                    name.is_empty()
                        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                        || !(v == "${credential}"
                            || matches!(name.as_str(), "LANG" | "LC_ALL" | "TZ")
                                && v.len() <= 128
                                && !v.chars().any(char::is_control))
                })
            {
                return Err("invalid agent command or environment".into());
            }
            c.model = "ACP".into();
            return Ok(c);
        }
        let uri = c
            .base_url
            .parse::<Uri>()
            .map_err(|_| "invalid API address")?;
        if uri.host().is_none()
            || uri.authority().is_some_and(|a| a.as_str().contains('@'))
            || c.base_url.contains(['?', '#', '\\'])
            || !(uri.scheme_str() == Some("https")
                || (uri.scheme_str() == Some("http") && is_loopback(&c.base_url)))
            || (c.kind.needs_key() && c.base_url != c.kind.base())
            || (matches!(c.kind, Kind::Local | Kind::Ollama | Kind::LmStudio)
                && !is_loopback(&c.base_url))
        {
            return Err("invalid API address".into());
        }
        if c.model.is_empty() && c.kind == Kind::Anthropic {
            c.model = "claude-opus-5-5".into()
        }
        if c.model.len() > 256 || c.model.chars().any(char::is_control) {
            return Err("invalid model".into());
        }
        if c.kind == Kind::Anthropic && !claude_model(&c.model) {
            return Err("unknown Claude model".into());
        }
        Ok(c)
    }
    pub fn ready(&self, key: &str, local_only: bool) -> Result<(), String> {
        if self.kind == Kind::Acp
            && (!cfg!(any(windows, target_os = "linux", target_os = "macos"))
                || self.env.values().any(|v| v == "${credential}") && key.is_empty())
            || self.kind.needs_key() && key.is_empty()
        {
            return Err("aiNotConfigured".into());
        }
        if local_only && !is_loopback(&self.base_url) {
            return Err("aiLocalOnly".into());
        }
        Ok(())
    }
}
pub fn claude_model(id: &str) -> bool {
    matches!(
        id,
        "claude-opus-5-5"
            | "claude-sonnet-5-5"
            | "claude-haiku-5-5"
            | "claude-fable-5-2"
            | "claude-mythos-5-2"
            | "claude-fable-5-1"
            | "claude-mythos-5-1"
            | "claude-fable-5"
            | "claude-mythos-5"
            | "claude-opus-5"
            | "claude-opus-4-8"
            | "claude-opus-4-7"
            | "claude-opus-4-6"
            | "claude-sonnet-5"
            | "claude-sonnet-4-6"
            | "claude-haiku-4-5"
            | "claude-haiku-4-5-20251001"
    )
}
