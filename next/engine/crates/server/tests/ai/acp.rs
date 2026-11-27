use super::*;

fn next_card(app: &mut App, data: &Path, risk: &str) -> Value {
    let event = app
        .event("aiEvent", |e| {
            e["kind"] == "permission" || e["kind"] == "end"
        })
        .expect("agent permission");
    assert_eq!(
        event["risk"],
        risk,
        "{event} {:?}",
        std::fs::read_to_string(data.join("failure"))
    );
    event
}
fn respond(app: &mut App, card: &Value, decision: &str) -> Value {
    app.request(
        json!({"cmd":"aiRespond","sessionId":card["sessionId"],"turnId":card["turnId"],
        "permissionId":card["permissionId"],"decision":decision}),
    )
}
#[test]
fn acp_scoped_edits_permissions_load_and_cancel() {
    let data = std::env::var_os("FOLIO_USER_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace().join("artifacts/ai-tests/userdata"))
        .join("tests")
        .join(folio_project::util::uuid_v4());
    let data = std::path::absolute(data).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    allow(&data, true);
    let (path, page, layer) = document(&data);
    let mut app = App::start(&data);
    ok(
        &mut app,
        json!({"cmd":"projectInfo","path":data.join("project")}),
    );
    ok(&mut app, json!({"cmd":"open","path":path,"edit":true}));
    let mut outside = Mcp::start(&data);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ai/acp_fixture.py");
    let cmd = if cfg!(windows) { "python" } else { "python3" };
    ok(
        &mut app,
        json!({"cmd":"aiSaveConnection","connection":{"id":"fixture","name":"offline","kind":"acp","command":cmd,
       "args":["-I","-u",fixture,data,page,layer],"env":{"FIXTURE_KEY":"${credential}"}}}),
    );
    ok(
        &mut app,
        json!({"cmd":"aiSetCredential","connectionId":"fixture","key":"test-key-not-real"}),
    );
    let sid = ok(&mut app, json!({"cmd":"aiNewSession","mode":"ask"}))["sessionId"].clone();
    let original = ok(&mut app, json!({"cmd":"debugDigest"}));
    let turn=ok(&mut app,json!({"cmd":"aiSend","sessionId":sid,"text":"draw","attachments":[{"kind":"page","page":0}]}))["turnId"].clone();
    let launch = next_card(&mut app, &data, "external");
    assert!(!data.join("endpoint").exists());
    assert_eq!(respond(&mut app, &launch, "allow")["ok"], true);
    for answer in ["deny", "allow"] {
        let card = next_card(&mut app, &data, "external");
        assert_eq!(card["outside"], true);
        assert_eq!(respond(&mut app, &card, "session")["ok"], false);
        assert_eq!(respond(&mut app, &card, answer)["ok"], true);
    }
    let card = next_card(&mut app, &data, "external");
    assert_eq!(card["sessionTool"], "edit.createByDrag");
    let other = outside.tool("edit.createByDrag", card["args"].clone());
    assert_eq!(other["isError"], true, "{other}");
    assert_eq!(respond(&mut app, &card, "session")["ok"], true);
    let end = app
        .event("aiEvent", |e| {
            e["kind"] == "end" || e["kind"] == "permission"
        })
        .unwrap();
    assert_eq!(
        end["status"],
        "complete",
        "{end} {:?}",
        std::fs::read_to_string(data.join("failure"))
    );
    assert_eq!(
        end["documents"][0]["summary"]["commands"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(wait_for(|| data.join("exited").exists()));
    let endpoint = std::fs::read_to_string(data.join("endpoint")).unwrap();
    assert!(wait_for(|| !Path::new(&endpoint).exists()));
    let history = ok(&mut app, json!({"cmd":"aiSession","sessionId":sid}));
    for kind in ["text", "thinking", "plan", "agentTool", "permissionGrant"] {
        assert!(
            history["records"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["kind"] == kind)
        );
    }
    assert!(!history.to_string().contains("test-key-not-real"));
    assert!(!history.to_string().contains("key-not-real"));
    assert!(history.to_string().contains("[redacted]"));
    assert_eq!(
        ok(
            &mut app,
            json!({"cmd":"aiRevertTurn","sessionId":sid,"turnId":turn})
        )["applied"],
        true
    );
    assert_eq!(
        ok(&mut app, json!({"cmd":"debugDigest"}))["layout"],
        original["layout"]
    );
    let turn = ok(
        &mut app,
        json!({"cmd":"aiSend","sessionId":sid,"text":"stop"}),
    )["turnId"]
        .clone();
    let launch = next_card(&mut app, &data, "external");
    assert_eq!(respond(&mut app, &launch, "allow")["ok"], true);
    let card = next_card(&mut app, &data, "external");
    assert_eq!(card["outside"], true);
    ok(
        &mut app,
        json!({"cmd":"aiStop","sessionId":sid,"turnId":turn}),
    );
    assert_eq!(
        app.event("aiEvent", |e| e["kind"] == "end").unwrap()["status"],
        "stopped"
    );
    assert!(wait_for(|| data.join("cancelled").exists()));
    let history = ok(&mut app, json!({"cmd":"aiSession","sessionId":sid})).to_string();
    assert!(!history.contains("replayed duplicate"));
}
