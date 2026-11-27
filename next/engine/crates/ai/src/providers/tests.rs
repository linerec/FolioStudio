use super::*;
use crate::{
    agent::{Input, Provider, Step},
    connection::{Connection, Kind, templates},
    transport::{self, Cancel, Request, Response, Transport},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Cursor, Read},
    sync::{Arc, Mutex},
};

struct Fixture {
    replies: Mutex<VecDeque<(u16, String)>>,
    requests: Mutex<Vec<Value>>,
}
impl Fixture {
    fn new(replies: Vec<(u16, String)>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::default(),
        }
    }
}
impl Transport for Fixture {
    fn send(&self, r: &Request, cancel: &Cancel) -> Result<Response, String> {
        cancel.check()?;
        self.requests
            .lock()
            .unwrap()
            .push(json!({"url":r.url,"headers":r.headers,"body":r.body}));
        let (status, body) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected request");
        Ok(Response {
            status,
            retry_after: Some(0),
            body: Box::new(Cursor::new(body.into_bytes())),
        })
    }
}
fn connection(kind: &str) -> Connection {
    let mut v = json!({"id":"demo","name":"Demo","kind":kind});
    match kind {
        "local" => v["baseUrl"] = json!("http://127.0.0.1:9800/v1"),
        "custom" => v["baseUrl"] = json!("https://example.invalid/openai/v1"),
        _ => {}
    }
    if kind != "anthropic" {
        v["model"] = json!("fixture-model")
    }
    Connection::parse(&v).unwrap()
}
fn events(rows: Vec<Value>) -> String {
    rows.into_iter()
        .map(|v| format!("data: {v}\r\n\r\n"))
        .collect()
}
fn messages(reason: &str) -> String {
    let mut rows = vec![
        json!({"type":"message_start","message":{"usage":{"input_tokens":11,"cache_creation_input_tokens":6,"cache_read_input_tokens":42}}}),
        json!({"type":"ping"}),
    ];
    let blocks = [
        (
            json!({"type":"thinking","thinking":"","signature":""}),
            vec![
                json!({"type":"thinking_delta","thinking":"생각"}),
                json!({"type":"signature_delta","signature":"signed=="}),
            ],
        ),
        (
            json!({"type":"text","text":""}),
            vec![json!({"type":"text_delta","text":"읽는 중"})],
        ),
        (
            json!({"type":"tool_use","name":"foo","id":"call1","input":{}}),
            vec![
                json!({"type":"input_json_delta","partial_json":"{\"fo"}),
                json!({"type":"input_json_delta","partial_json":"o\":3}"}),
            ],
        ),
    ];
    for (index, (start, deltas)) in blocks.into_iter().enumerate() {
        rows.push(json!({"type":"content_block_start","index":index,"content_block":start}));
        for delta in deltas {
            rows.push(json!({"type":"content_block_delta","index":index,"delta":delta}));
        }
        rows.push(json!({"type":"content_block_stop","index":index}));
    }
    rows.extend([
        json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"output_tokens":9}}),
        json!({"type":"message_stop"}),
    ]);
    events(rows)
}
#[test]
fn claude_sse_survives_every_split_and_replays_signed_blocks() {
    let s = messages("tool_use");
    for i in 0..s.len() {
        let (a, b) = s.as_bytes().split_at(i);
        let reply = stream::decode(
            a.chain(b),
            &Cancel::default(),
            anthropic::Messages::default(),
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(
            reply.blocks[0],
            json!({"type":"thinking","thinking":"생각","signature":"signed=="})
        );
        assert_eq!(reply.blocks[1]["text"], "읽는 중");
        assert_eq!(reply.blocks[2]["input"], json!({"foo":3}));
        assert_eq!(reply.reason, "tool_use");
        assert_eq!(reply.usage["cache_read_input_tokens"], 42);
        assert_eq!(reply.usage["output_tokens"], 9);
        let records = [
            json!({"kind":"assistant","blocks":reply.blocks}),
            json!({"kind":"tool_result","toolUseId":"call1","content":{"value":2},"isError":false}),
        ];
        assert_eq!(
            history::messages(&records, true)[0]["content"],
            json!(reply.blocks)
        );
    }
}
#[test]
fn fixtures_verify_each_template_and_free_model_listing() {
    let records =
        [json!({"kind":"user","text":"hi","attachments":[{"image":{"png":"ZmFrZQ==","width":2}}]})];
    let fixed = [json!({"name":"foo","description":"Read","input_schema":{"type":"object"}})];
    let input = Input {
        system: "fixed",
        tools: &fixed,
        messages: &records,
    };
    assert_eq!(templates().as_array().unwrap().len(), 9);
    assert!(Connection::parse(&json!({"id":"demo","name":"Demo","kind":"acp"})).is_err());
    for (kind, url) in [
        ("anthropic", "https://api.anthropic.com/v1"),
        ("openai", "https://api.openai.com/v1"),
        (
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai",
        ),
        ("openrouter", "https://openrouter.ai/api/v1"),
        ("ollama", "http://localhost:11434/v1"),
        ("lmStudio", "http://localhost:1234/v1"),
        ("local", "http://127.0.0.1:9800/v1"),
        ("custom", "https://example.invalid/openai/v1"),
    ] {
        let mut c = connection(kind);
        let listed = if c.kind == Kind::Anthropic {
            "claude-opus-5-5"
        } else {
            "fixture-model"
        };
        let t = Fixture::new(vec![
            (200, json!({"data":[{"id":listed}]}).to_string()),
            (200, String::new()),
        ]);
        let cancel = Cancel::default();
        let key = if c.kind.needs_key() {
            "test-key-not-real"
        } else {
            ""
        };
        assert_eq!(
            network::models(&t, &c, key, &cancel).unwrap(),
            vec![listed.to_owned()]
        );
        let claude = c.kind == Kind::Anthropic;
        let (body, path) = if claude {
            (anthropic::body(&c, &input), "messages")
        } else {
            (openai_compat::body(&c, &input, true), "chat/completions")
        };
        assert_eq!(body["stream"], true);
        assert!(body.to_string().contains("ZmFrZQ=="));
        if claude {
            assert_eq!(body["tools"][0]["eager_input_streaming"], true);
            assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
            assert_eq!(body["output_config"]["effort"], "medium");
            for field in ["thinking", "budget_tokens", "tool_choice"] {
                assert!(body.get(field).is_none())
            }
        } else {
            assert_eq!(
                body["tools"][0]["function"]["parameters"],
                fixed[0]["input_schema"]
            );
            assert!(body.to_string().contains("data:image/png;base64,"));
        }
        let req = Request::new(&c, key, path, Some(body));
        t.send(&req, &cancel).unwrap();
        let requests = t.requests.lock().unwrap();
        assert_eq!(requests[0]["url"], format!("{url}/models"));
        assert!(requests[0]["body"].is_null());
        assert_eq!(requests[1]["url"], format!("{url}/{path}"));
        for i in 0..2 {
            let headers = &requests[i]["headers"].as_array().unwrap();
            if claude {
                assert!(headers.contains(&json!(["anthropic-version", "2026-10-21"])));
                assert!(headers.contains(&json!(["x-api-key", key])))
            } else if !key.is_empty() {
                assert!(headers.contains(&json!(["authorization", format!("Bearer {key}")])))
            } else {
                assert!(headers.iter().all(|v| v[0] != "authorization"))
            }
        }
        c.send_images = false;
        assert!(
            !history::messages(&records, c.send_images)[0]
                .to_string()
                .contains("ZmFrZQ==")
        );
    }
}
#[test]
fn refuses_or_truncates_without_dispatching_tools() {
    for reason in ["refusal", "max_tokens", "tool_use"] {
        let t = Arc::new(Fixture::new(vec![]));
        let mut net = network::Network::new(
            connection("anthropic"),
            String::new(),
            t,
            Arc::new(|| {}),
            false,
        );
        let r = stream::decode(
            messages(reason).as_bytes(),
            &Cancel::default(),
            anthropic::Messages::default(),
            &mut |_, _, _| {},
        )
        .unwrap();
        net.received(r, false).unwrap();
        let mut tool = false;
        let mut skipped = false;
        while net.pending_request().is_none() {
            match net
                .next(Input {
                    system: "",
                    messages: &[],
                    tools: &[],
                })
                .unwrap()
            {
                Step::Tool { args, .. } => {
                    tool = true;
                    assert_eq!(args, json!({"foo":3}))
                }
                Step::Skipped { .. } => skipped = true,
                Step::Response { record } => {
                    assert!((record["estimatedUsd"].as_f64().unwrap() - 0.0002624).abs() < 1e-12)
                }
                Step::End => break,
                _ => {}
            }
        }
        assert_eq!(tool, reason == "tool_use");
        assert_ne!(tool, skipped);
    }
    assert!(anthropic::cost("unknown", &json!({"input_tokens":1,"output_tokens":1})).is_none());
}
#[test]
fn chat_stream_reassembles_arguments_and_downgrades_on_explicit_tool_error() {
    let stream = events(vec![
        json!({"choices":[{"index":0,"delta":{"content":"한글 ","tool_calls":[{"index":0,"id":"one","function":{"name":"foo","arguments":"{\"x\":"}}]}}]}),
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"1}"}}]},"finish_reason":"tool_calls"}]}),
    ]) + "data: [DONE]\n\n";
    let t = Fixture::new(vec![
        (
            400,
            json!({"error":{"message":"model does not support tools"}}).to_string(),
        ),
        (200, stream),
    ]);
    let req = Request::new(
        &connection("local"),
        "",
        "chat/completions",
        Some(json!({"tools":[{}],"stream":true})),
    );
    let (r, questions) =
        network::complete(&t, req, false, &Cancel::default(), &mut |_, _, _| {}).unwrap();
    assert!(questions);
    assert_eq!(r.blocks[1]["input"], json!({"x":1}));
    assert!(t.requests.lock().unwrap()[1]["body"].get("tools").is_none());
    let mut net = network::Network::new(
        connection("local"),
        String::new(),
        Arc::new(t),
        Arc::new(|| {}),
        false,
    );
    net.received(r, questions).unwrap();
    loop {
        match net
            .next(Input {
                system: "",
                messages: &[],
                tools: &[],
            })
            .unwrap()
        {
            Step::Tool { .. } => panic!("unexpected tool"),
            Step::End => break,
            _ => {}
        }
    }
}
#[test]
fn retries_respects_stop_and_redacts_http_errors() {
    let c = connection("anthropic");
    let req = Request::new(&c, "private-fixture-value", "models", None);
    let cancel = Cancel::default();
    let t = Fixture::new(vec![
        (429, "{}".into()),
        (529, "{}".into()),
        (200, "{}".into()),
    ]);
    assert!(transport::response(&t, &req, &cancel).is_ok());
    for (status, msg, expected) in [
        (401, "", "키를 확인하세요"),
        (403, "", "키를 확인하세요"),
        (400, "bad private-fixture-value", "bad [redacted]"),
        (400, "credit balance too low", "크레디트가 부족합니다"),
    ] {
        let t = Fixture::new(vec![(status, json!({"error":{"message":msg}}).to_string())]);
        assert_eq!(
            transport::response(&t, &req, &cancel).err().unwrap(),
            expected
        );
    }
    cancel.stop();
    assert!(transport::response(&Fixture::new(vec![]), &req, &cancel).is_err());
    assert!(
        stream::decode(
            "data: {}\n\n".as_bytes(),
            &Cancel::default(),
            anthropic::Messages::default(),
            &mut |_, _, _| {}
        )
        .is_err()
    );
}

#[test]
fn image_choice_preserves_signed_history_when_settings_change() {
    let mut records = vec![
        json!({"kind":"tool_result","toolUseId":"t","content":{"type":"pageImage","png":"test"}}),
    ];
    records.push(history::image_policy(&records, false).unwrap().unwrap());
    let first = history::messages(&records, false);
    assert_eq!(history::messages(&records, true), first);
    records[0]["content"]["sha256"] = json!("stored");
    assert_eq!(history::messages(&records, true), first);
    records.push(json!({"kind":"user","attachments":[{"image":{"png":"second"}}]}));
    records.push(history::image_policy(&records, true).unwrap().unwrap());
    let second = history::messages(&records, true);
    assert_eq!(second[0], first[0]);
    assert!(second[1].to_string().contains("second"));
    assert!(history::image_policy(&records, true).unwrap().is_none());
    assert_eq!(
        history::image_policy(&records, false).unwrap_err(),
        "aiNewSessionRequired"
    );
}
