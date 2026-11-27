use super::*;

#[test]
fn connections_pin_models_and_keep_credentials_out_of_replies_and_files() {
    let data = std::env::var_os("FOLIO_USER_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace().join("artifacts/ai-tests/userdata"))
        .join("tests")
        .join(folio_project::util::uuid_v4());
    std::fs::create_dir_all(&data).unwrap();
    allow(&data, true);
    let key = "test-key-not-real";
    let mut app = App::with_settings(
        &data,
        &[
            ("ANTHROPIC_API_KEY", "test-env-not-real"),
            ("OPENAI_API_KEY", "test-env-not-real"),
        ],
    );
    assert_eq!(
        app.request(json!({"cmd":"aiSend","text":"test"}))["code"],
        "aiNotConfigured"
    );
    assert_eq!(
        app.request(json!({"cmd":"aiNewSession"}))["code"],
        "aiNotConfigured"
    );
    let mut c = json!({"id":"a","kind":"anthropic","name":"Claude"});
    ok(&mut app, json!({"cmd":"aiSaveConnection","connection":c}));
    ok(
        &mut app,
        json!({"cmd":"aiSaveConnection","connection":{"id":"b","kind":"ollama","name":"test","model":"fixture"}}),
    );
    ok(
        &mut app,
        json!({"cmd":"aiSetDefaultConnection","connectionId":"b"}),
    );
    assert_eq!(
        ok(&mut app, json!({"cmd":"aiConnections"}))["defaultConnection"],
        "b"
    );
    ok(
        &mut app,
        json!({"cmd":"aiSetDefaultConnection","connectionId":"a"}),
    );
    let sid = ok(&mut app, json!({"cmd":"aiNewSession"}))["sessionId"].clone();
    let send = json!({"cmd":"aiSend","sessionId":sid,"text":key});
    assert_eq!(app.request(send.clone())["code"], "aiNotConfigured");
    let reply = ok(
        &mut app,
        json!({"cmd":"aiSetCredential","connectionId":"a","key":key}),
    );
    assert_eq!(reply["hasKey"], true);
    assert!(!reply.to_string().contains(key));
    // Exercise the public MCP surface with the credential present in the running engine.
    assert!(wait_for(|| connection_file(&data).exists()));
    {
        let mut mcp = Mcp::start(&data);
        let names = mcp.names();
        for name in [
            "aiConnections",
            "aiSettings",
            "aiSetCredential",
            "aiSaveConnection",
            "aiModels",
            "aiDeleteConnection",
            "aiSetDefaultConnection",
            "aiTestConnection",
        ] {
            assert!(!names.iter().any(|n| n == name));
            let reply = mcp.rpc(
                "tools/call",
                json!({"name":name,"arguments":{"connectionId":"a","key":key}}),
            );
            assert!(reply["result"]["isError"] == true || reply.get("error").is_some());
            assert!(!reply.to_string().contains(key));
        }
        assert!(!mcp.tool("dialogState", json!({})).to_string().contains(key));
    }
    c["model"] = json!("claude-sonnet-5-5");
    ok(&mut app, json!({"cmd":"aiSaveConnection","connection":c}));
    let header = |app: &mut App, s: &Value| {
        ok(app, json!({"cmd":"aiSession","sessionId":s}))["records"][0].clone()
    };
    assert_eq!(header(&mut app, &sid)["model"], "claude-opus-5-5");
    let later = ok(&mut app, json!({"cmd":"aiNewSession","summary":"요약"}))["sessionId"].clone();
    let h = header(&mut app, &later);
    assert_eq!(h["model"], "claude-sonnet-5-5");
    assert_eq!(h["connectionId"], "a");
    assert_eq!(h["summary"], "요약");
    let mut switched = send.clone();
    switched["model"] = c["model"].clone();
    assert_eq!(app.request(switched)["code"], "aiNewSessionRequired");
    ok(
        &mut app,
        json!({"cmd":"aiSetSettings","values":{"folio.ai.localOnly":true}}),
    );
    assert_eq!(app.request(send.clone())["code"], "aiLocalOnly");
    assert_eq!(
        app.request(json!({"cmd":"aiModels","connectionId":"a"}))["code"],
        "aiLocalOnly"
    );
    ok(
        &mut app,
        json!({"cmd":"aiSetSettings","values":{"folio.ai.localOnly":false}}),
    );
    ok(&mut app, send.clone());
    let card = app.event("aiEvent", |e| e["kind"] == "permission").unwrap();
    assert_eq!(card["risk"], "external");
    ok(
        &mut app,
        json!({"cmd":"aiRespond","permissionId":card["permissionId"],"decision":"deny"}),
    );
    assert_eq!(
        app.event("aiEvent", |e| e["kind"] == "end").unwrap()["status"],
        "stopped"
    );
    for req in [
        json!({"cmd":"aiConnections"}),
        json!({"cmd":"aiSettings"}),
        json!({"cmd":"userSettings"}),
        json!({"cmd":"dialogState"}),
        json!({"cmd":"aiSession","sessionId":sid}),
        json!({"cmd":"aiExportSession","sessionId":sid}),
        json!({"cmd":"aiSaveConnection","connection":{"id":"a","kind":"anthropic","name":key}}),
        json!({"cmd":"userSettings","values":{"folio.ai.connections":[{"key":key}]}}),
    ] {
        assert!(!app.request(req).to_string().contains(key));
    }
    fn clean_files(p: &Path, key: &str) {
        for e in std::fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                clean_files(&p, key)
            } else {
                assert!(
                    !std::fs::read(&p)
                        .unwrap()
                        .windows(key.len())
                        .any(|b| b == key.as_bytes()),
                    "{}",
                    p.display()
                );
            }
        }
    }
    clean_files(&data.join("app"), key);
    ok(
        &mut app,
        json!({"cmd":"aiDeleteConnection","connectionId":"a"}),
    );
    assert!(ok(&mut app, json!({"cmd":"aiConnections"}))["defaultConnection"].is_null());
    assert_eq!(app.request(send)["code"], "aiNotConfigured");
    ok(&mut app, json!({"cmd":"aiSaveConnection","connection":c}));
    assert_eq!(
        ok(&mut app, json!({"cmd":"aiConnections"}))["connections"][1]["hasKey"],
        false
    );
}
