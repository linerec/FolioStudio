use super::{Reply, anthropic, openai_compat, stream};
use crate::{
    agent::{Input, Provider, Step},
    connection::{Connection, Kind, claude_model},
    transport::{self, Cancel, Request, Transport},
};
use serde_json::{Value, json};
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

pub fn models(
    t: &dyn Transport,
    c: &Connection,
    key: &str,
    cancel: &Cancel,
) -> Result<Vec<String>, String> {
    let mut path = "models".to_owned();
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let request = Request::new(c, key, &path, None);
        let data = transport::bytes(transport::response(t, &request, cancel)?, 4 * 1024 * 1024)?;
        let v: Value = serde_json::from_slice(&data).map_err(|_| "invalid model list")?;
        for m in v["data"].as_array().ok_or("invalid model list")? {
            let id = m["id"].as_str().ok_or("invalid model id")?;
            if c.kind != Kind::Anthropic || claude_model(id) {
                ids.push(id.to_owned())
            }
        }
        if c.kind != Kind::Anthropic || v["has_more"] != true {
            break;
        }
        let cursor = v["last_id"]
            .as_str()
            .filter(|id| {
                id.len() <= 256
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            })
            .ok_or("invalid model cursor")?;
        if !seen.insert(cursor.to_owned()) || seen.len() > 50 {
            return Err("invalid model pagination".into());
        }
        path = format!("models?after_id={cursor}");
    }
    Ok(ids)
}
pub(super) fn complete(
    t: &dyn Transport,
    mut request: Request,
    claude: bool,
    cancel: &Cancel,
    preview: &mut impl FnMut(usize, &str, &str),
) -> Result<(Reply, bool), String> {
    let mut questions_only = false;
    for attempt in 0..4 {
        let result = transport::response(t, &request, cancel).and_then(|reader| {
            if claude {
                stream::decode(reader, cancel, anthropic::Messages::default(), preview)
            } else {
                stream::decode(reader, cancel, openai_compat::Chat::default(), preview)
            }
        });
        match result {
            Err(e)
                if !claude
                    && !questions_only
                    && openai_compat::unsupported_tools(&e)
                    && request.body.as_ref().and_then(|b| b.get("tools")).is_some() =>
            {
                request
                    .body
                    .as_mut()
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove("tools");
                questions_only = true;
            }
            Err(e) if e == "aiOverloaded" && attempt < 2 => {
                transport::retry_wait(1 << attempt, cancel)?
            }
            other => return other.map(|r| (r, questions_only)),
        }
    }
    Err("AI provider unavailable".into())
}
type Answer = Result<(Reply, bool), String>;
pub struct Network {
    connection: Connection,
    key: String,
    transport: Arc<dyn Transport>,
    wake: Arc<dyn Fn() + Send + Sync>,
    preview: Arc<Mutex<Option<Value>>>,
    cancel: Cancel,
    rx: Option<Receiver<Answer>>,
    steps: VecDeque<Step>,
    questions_only: bool,
}
impl Network {
    pub fn new(
        connection: Connection,
        key: String,
        transport: Arc<dyn Transport>,
        wake: Arc<dyn Fn() + Send + Sync>,
        questions_only: bool,
    ) -> Self {
        Self {
            connection,
            key,
            transport,
            wake,
            preview: Arc::default(),
            cancel: Cancel::default(),
            rx: None,
            steps: VecDeque::new(),
            questions_only,
        }
    }
    pub(super) fn received(&mut self, reply: Reply, downgraded: bool) -> Result<(), String> {
        self.questions_only |= downgraded;
        let mut meta = json!({"kind":"assistant","blocks":reply.blocks,"stopReason":reply.reason,"usage":reply.usage,"questionsOnly":self.questions_only});
        if self.connection.kind == Kind::Anthropic
            && let Some(cost) = anthropic::cost(&self.connection.model, &reply.usage)
        {
            meta["estimatedUsd"] = json!(cost);
        }
        if let Some(limit) = self.connection.budget_warning {
            meta["budgetLimitUsd"] = json!(limit);
        }
        let execute =
            !self.questions_only && matches!(reply.reason.as_str(), "tool_use" | "tool_calls");
        let mut ids = HashSet::new();
        let mut tools = vec![];
        for b in &reply.blocks {
            match b["type"].as_str() {
                Some("tool_use") => {
                    let id = b["id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or("invalid tool id")?;
                    if !ids.insert(id) {
                        return Err("duplicate tool id".into());
                    }
                    if execute {
                        tools.push(Step::Tool {
                            id: id.into(),
                            name: b["name"].as_str().ok_or("invalid tool name")?.into(),
                            args: b["input"].clone(),
                        });
                    } else {
                        tools.push(Step::Skipped { id: id.into() })
                    }
                }
                Some("text") => self.steps.push_back(Step::Text {
                    text: b["text"].as_str().ok_or("invalid text block")?.into(),
                }),
                Some("thinking") => self.steps.push_back(Step::Thinking {
                    text: b["thinking"].as_str().unwrap_or("").into(),
                }),
                _ => {}
            }
        }
        self.steps.push_front(Step::Response { record: meta });
        let follow = execute && !tools.is_empty();
        self.steps.extend(tools);
        if !follow {
            self.steps.push_back(Step::End)
        }
        Ok(())
    }
}
impl Provider for Network {
    fn pending_request(&self) -> Option<&Connection> {
        (self.rx.is_none() && self.steps.is_empty()).then_some(&self.connection)
    }
    fn configure(&mut self, c: Connection, key: String) {
        self.connection = c;
        self.key = key
    }
    fn next(&mut self, input: Input<'_>) -> Result<Step, String> {
        if let Some(step) = self.steps.pop_front() {
            return Ok(step);
        }
        if let Some(record) = self
            .preview
            .lock()
            .map_err(|_| "preview unavailable")?
            .take()
        {
            return Ok(Step::Preview { record });
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.rx = None;
                    let (r, d) = result?;
                    self.received(r, d)?;
                    return self.steps.pop_front().ok_or("empty AI response".into());
                }
                Err(TryRecvError::Empty) => return Ok(Step::Pending),
                Err(TryRecvError::Disconnected) => return Err("AI worker stopped".into()),
            }
        }
        let claude = self.connection.kind == Kind::Anthropic;
        let body = if claude {
            anthropic::body(&self.connection, &input)
        } else {
            openai_compat::body(&self.connection, &input, !self.questions_only)
        };
        let request = Request::new(
            &self.connection,
            &self.key,
            if claude {
                "messages"
            } else {
                "chat/completions"
            },
            Some(body),
        );
        let (send, rx) = mpsc::channel();
        self.rx = Some(rx);
        let t = self.transport.clone();
        let wake = self.wake.clone();
        let cancel = self.cancel.clone();
        let preview = self.preview.clone();
        std::thread::Builder::new().name("folio-ai-http".into()).spawn(move||{
            let mut last=None;
            let mut progress=|index:usize,kind:&str,text:&str|{
                if last.is_some_and(|i:Instant|i.elapsed()<Duration::from_millis(60)){return}
                // Keep an unfinished word private until secret redaction can see it in full.
                let Some(end)=text.rfind(char::is_whitespace) else{return};
                if let Ok(mut slot)=preview.lock(){
                    let notify=slot.is_none();
                    *slot=Some(json!({"kind":"preview","block":index,"blockType":kind,"text":&text[..end]}));
                    last=Some(Instant::now());
                    if notify{wake()}
                }
            };
            let result=complete(t.as_ref(),request,claude,&cancel,&mut progress);if send.send(result).is_ok(){wake()}
        }).map_err(|_|"could not start AI worker")?;
        Ok(Step::Pending)
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        self.cancel.stop()
    }
}
