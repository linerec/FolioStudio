use super::*;
use folio_ai::{
    agent::{Agent, Effect, Mode},
    providers::scripted::Scripted,
    tools::{self, Gate, Risk},
};
use folio_engine::mcp::gate::{self, Attached};
use std::collections::BTreeSet;

#[test]
fn only_attached_engine_tools_can_be_discovered_and_executed() {
    let published = super::super::mcp::catalog::attached_tools();
    let exposed = published
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .filter(|name| !matches!(*name, "coverage" | "appConnection"))
        .collect::<BTreeSet<_>>();
    let names = Attached
        .catalog()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(names, exposed);
    assert!(gate::prepare("exportIdml", &json!({"path":"out.idml"}), false, false).is_err());
    assert!(gate::prepare("dataMergeSourceStatus", &json!({"check":true}), false, true).is_err());
    let fixed = tools::fixed(&Attached);
    assert_eq!(fixed.len(), 22);
    for name in folio_ai::COMMANDS {
        assert_eq!(Attached.risk(name, &json!({})), Risk::Withheld);
        assert!(gate::prepare(name, &json!({}), true, true).is_err());
        assert!(
            tools::resolve(
                &Attached,
                &fixed,
                "folio_call",
                &json!({"name":name,"args":{}})
            )
            .is_err()
        );
    }
    for tool in Attached.catalog() {
        assert_eq!(
            json!(Attached.risk(tool["name"].as_str().unwrap(), &Value::Null)),
            tool["_meta"]["risk"]
        );
    }
    let tools::Action::Found(found) = tools::resolve(
        &Attached,
        &fixed,
        "folio_find_tools",
        &json!({"query":"insertText edit"}),
    )
    .unwrap() else {
        panic!("missing search result");
    };
    let entry = found["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "edit.insertText")
        .unwrap();
    assert_eq!(
        entry["inputSchema"],
        Attached
            .catalog()
            .iter()
            .find(|t| t["name"] == entry["name"])
            .unwrap()["inputSchema"]
    );
}

#[test]
fn invalid_calls_and_denials_return_results_and_external_actions_always_wait() {
    let fixed = tools::fixed(&Attached);
    let file = json!({"kind":"tool","id":"out","name":"folio_call","args":{"name":"exportIdml","args":{"path":"out.idml"}}});
    let db = json!({"kind":"tool","id":"db","name":"folio_call","args":{"name":"dataMergeSourceStatus","args":{"check":true}}});
    for mode in [Mode::ReadOnly, Mode::Ask, Mode::Direct] {
        let mut agent = Agent::new(mode, vec![]);
        let mut provider=Scripted::from_json(&json!([
            {"kind":"tool","id":"bad","name":"edit_insertText","args":{"text":42}},
            {"kind":"tool","id":"private","name":"folio_call","args":{"name":"aiSettings","args":{}}},
            file,file,db,{"kind":"end"}
        ])).unwrap();
        for _ in 0..2 {
            let Effect::Event(result) =
                agent.advance(&Attached, &mut provider, "", &fixed).unwrap()
            else {
                panic!("expected error result");
            };
            assert_eq!(result["isError"], true);
        }
        for (index, risk) in ["file", "file", "database"].into_iter().enumerate() {
            let effect = agent.advance(&Attached, &mut provider, "", &fixed).unwrap();
            if mode == Mode::ReadOnly {
                let Effect::Event(result) = effect else {
                    panic!("read only mode");
                };
                assert_eq!(result["content"]["error"], "readOnly");
                continue;
            }
            let Effect::Permission(card) = effect else {
                panic!("missing card for {risk}: {:?}", agent.messages.last());
            };
            assert_eq!(card["risk"], risk);
            assert!(agent.respond("stale", true).is_err());
            let allow = index == 0;
            let response = agent
                .respond(card["permissionId"].as_str().unwrap(), allow)
                .unwrap();
            if allow {
                let Effect::Execute(call) = response else {
                    panic!("approval missing");
                };
                agent.result(&call.id, json!({"ok":true}), false);
            } else {
                let Effect::Event(result) = response else {
                    panic!("denial missing");
                };
                assert_eq!(result["isError"], true);
                assert_eq!(result["content"]["error"], "denied");
            }
        }
        assert!(matches!(
            agent.advance(&Attached, &mut provider, "", &fixed).unwrap(),
            Effect::End
        ));
    }
}

#[test]
fn stopping_a_permission_card_preserves_the_call_and_writes_no_file() {
    let data = std::env::var_os("FOLIO_USER_DATA").map(std::path::PathBuf::from).unwrap_or_else(||workspace().join("artifacts/ai-tests/userdata")).join("tests")
        .join(folio_project::util::uuid_v4());
    std::fs::create_dir_all(&data).unwrap();
    let (path, _, _) = document(&data);
    let mut app = App::with_settings(&data,&[("FOLIO_AI_SCRIPTED_TEST","1")]);
    ok(&mut app, json!({"cmd":"open","path":path,"edit":true}));
    let sid = ok(
        &mut app,
        json!({"cmd":"aiNewSession","provider":"scripted","mode":"direct","language":"en"}),
    )["sessionId"]
        .clone();
    let turn=ok(&mut app,json!({"cmd":"aiSend","sessionId":sid,"text":"export","script":[
        {"kind":"tool","id":"out","name":"folio_call","args":{"name":"exportIdml","args":{"path":"stopped.idml"}}},
        {"kind":"end"}
    ]}))["turnId"].clone();
    let card = app
        .event("aiEvent", |e| {
            e["kind"] == "permission" || e["kind"] == "end"
        })
        .unwrap();
    assert_eq!(card["risk"], "file");
    ok(
        &mut app,
        json!({"cmd":"aiStop","sessionId":sid,"turnId":turn}),
    );
    let end = app.event("aiEvent", |e| e["kind"] == "end").unwrap();
    assert_eq!(end["status"], "stopped");
    let history = ok(&mut app, json!({"cmd":"aiSession","sessionId":sid}));
    assert!(
        history["records"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["toolUseId"] == "out" && r["isError"] == true)
    );
    assert!(!path.parent().unwrap().join("stopped.idml").exists());
}

#[test]
fn session_grant_follows_one_reviewed_edit_across_turns() {
    let fixed = tools::fixed(&Attached);
    let edit = json!({"kind":"tool","id":"one","name":"edit_insertText","args":{"story":"s","at":0,"text":"a"}});
    let other = json!({"kind":"tool","id":"two","name":"edit_replaceTextRange","args":{"story":"s","start":0,"end":1,"text":"b"}});
    let file = json!({"kind":"tool","id":"file","name":"folio_call","args":{"name":"exportIdml","args":{"path":"out.idml"}}});
    let mut agent = Agent::new(Mode::Ask,vec![]);
    let mut provider = Scripted::from_json(&json!([edit])).unwrap();
    let Effect::Permission(card) = agent.advance(&Attached,&mut provider,"",&fixed).unwrap() else { panic!() };
    assert!(matches!(agent.grant_session(card["permissionId"].as_str().unwrap()),Ok(Effect::Execute(_))));
    let mut next = Agent::new(Mode::Ask,vec![]);
    next.restore_grants(&agent.messages);
    let mut provider = Scripted::from_json(&json!([edit,other,file])).unwrap();
    assert!(matches!(next.advance(&Attached,&mut provider,"",&fixed),Ok(Effect::Execute(_))));
    for risk in ["edit","file"] {
        let Effect::Permission(card) = next.advance(&Attached,&mut provider,"",&fixed).unwrap() else { panic!("{:?}",next.messages.last()) };
        assert_eq!(card["risk"],risk);
        let id=card["permissionId"].as_str().unwrap();
        if risk=="file" { assert!(next.grant_session(id).is_err()); }
        next.respond(id,false).unwrap();
    }
}
