#[cfg(any(windows,target_os="linux",target_os="macos"))]
#[path = "acp.rs"]
mod acp;
#[path = "connections.rs"]
mod connections;
#[path = "permissions.rs"]
mod permissions;
use super::*;

fn ok(app: &mut App, request: Value) -> Value {
    let reply = app.request(request);
    assert_eq!(reply["ok"], true, "{}", reply["error"]);
    reply
}

#[test]
fn title_frame_turn_uses_one_step_and_restores_the_document() {
    let data = std::env::var_os("FOLIO_USER_DATA").map(std::path::PathBuf::from).unwrap_or_else(||workspace().join("artifacts/ai-tests/userdata")).join("tests")
        .join(folio_project::util::uuid_v4());
    std::fs::create_dir_all(&data).unwrap();
    let (path, page, layer) = document(&data);
    let mut app = App::with_settings(&data,&[("FOLIO_AI_SCRIPTED_TEST","1")]);
    ok(&mut app, json!({"cmd":"open","path":path,"edit":true}));
    let before = ok(&mut app, json!({"cmd":"debugDigest"}));
    let saved_parts = |app: &mut App, name: &str| {
        let file = data.join(name);
        ok(app, json!({"cmd":"exportIdml","path":file}));
        folio_idml::read_editor_parts(&file).unwrap()
    };
    let original = saved_parts(&mut app, "before.idml");
    ok(&mut app,json!({"cmd":"aiSaveConnection","connection":{"id":"test","kind":"anthropic","name":"test"}}));
    ok(
        &mut app,
        json!({"cmd":"aiSetCredential","connectionId":"test","key":"test-key-not-real"}),
    );
    let sid = ok(
        &mut app,
        json!({"cmd":"aiNewSession","provider":"scripted","language":"ko","mode":"ask"}),
    )["sessionId"]
        .clone();
    let script = json!([
        {"kind":"thinking","text":"제목 상자 위치를 확인한다."},
        {"kind":"tool","id":"frame","name":"edit_createByDrag","args":{"page":page,"kind":"textFrame","rect":[20,20,160,65],"layer":layer,"sides":4,"inset":0}},
        {"kind":"tool","id":"objects","name":"pageItems","args":{"page":0}},
        {"kind":"tool","id":"insert","name":"edit_insertText","args":{
            "story":{"$result":"objects","pointer":"/items/items","itemId":{"$result":"frame","pointer":"/created/0"},"field":"/text/story"},
            "at":0,"text":"제목"}},
        {"kind":"text","text":"제목을 넣었습니다. test-key-not-real"},
        {"kind":"end"}
    ]);
    let turn=ok(&mut app,json!({"cmd":"aiSend","sessionId":sid,"text":"제목을 만들어 줘","attachments":[{"kind":"page","page":0}],"script":script}))["turnId"].clone();
    let first = app
        .event("aiEvent", |v| {
            v["kind"] == "permission" || v["kind"] == "end"
        })
        .unwrap();
    if first["kind"] != "permission" {
        let records = ok(&mut app, json!({"cmd":"aiSession","sessionId":sid}))["records"]
            .as_array()
            .unwrap()[1..]
            .to_vec();
        panic!("expected permission, got {records:?}");
    }
    assert_eq!(app.request(json!({"cmd":"undo"}))["code"], "aiTurnRunning");
    ok(&mut app, json!({"cmd":"setSelection","ids":[]}));
    ok(
        &mut app,
        json!({"cmd":"aiRespond","sessionId":sid,"turnId":turn,"permissionId":first["permissionId"],"decision":"allow"}),
    );
    let end = app.event("aiEvent", |v| v["kind"] == "end").unwrap();
    let records = ok(&mut app, json!({"cmd":"aiSession","sessionId":sid}))["records"]
        .as_array()
        .unwrap()
        .clone();
    let errors: Vec<_> = records.iter().filter(|v| v["isError"] == true).collect();
    assert_eq!(records[0]["model"], "scripted");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(end["status"], "complete", "{end}");
    let group = &end["documents"][0];
    assert_eq!(
        group["end"]["depth"].as_u64().unwrap(),
        group["start"]["depth"].as_u64().unwrap() + 1
    );
    assert_eq!(group["summary"]["commands"].as_array().unwrap().len(), 2);
    let frame =
        &records.iter().find(|v| v["toolUseId"] == "frame").unwrap()["content"]["created"][0];
    let objects = ok(
        &mut app,
        json!({"cmd":"aiContext","kind":"objects","page":0,"ids":[frame]}),
    );
    let object = &objects["objects"][0];
    assert_eq!(object["text"]["excerpt"], "제목");
    for q in [
        json!({"kind":"region","region":[20,20,160,65]}),
        json!({"kind":"text","story":object["text"]["story"],"start":0,"end":1}),
        json!({"kind":"page"}),
    ] {
        let mut req = q.clone();
        req["cmd"] = json!("aiContext");
        let reply = ok(&mut app, req);
        assert!(
            reply["objects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| &v["id"] == frame)
        );
        assert_image(&reply["image"]);
        assert_eq!(reply["document"], objects["document"]);
        if q["kind"] == "region" {
            assert_eq!(reply["region"], json!([20.0,20.0,160.0,65.0]));
            assert!(reply["image"]["width"].as_u64() > reply["image"]["height"].as_u64());
            let bare = ok(&mut app, json!({"cmd":"aiContext","kind":"region","page":0,
                "region":q["region"],"images":false,"document":reply["document"]}));
            assert!(bare["image"].is_null());
            assert_eq!(bare["structureHash"], reply["structureHash"]);
            assert_eq!(app.request(json!({"cmd":"aiContext","kind":"region","page":0,
                "region":q["region"],"images":false,"document":"another-document"}))["ok"], false);
            assert_eq!(app.request(json!({"cmd":"aiContext","kind":"objects","page":0,
                "ids":["missing-frame"],"images":false}))["ok"], false);
        }
        if q["kind"] == "text" {
            assert_eq!(reply["text"]["excerpt"], "제");
            assert_eq!(reply["text"]["sha256"], folio_project::hash::sha256_bytes("제".as_bytes()));
            assert_eq!(reply["objects"][0]["text"]["excerpt"], "제");
        }
    }
    assert_image(&objects["image"]);
    let style = ok(
        &mut app,
        json!({"cmd":"aiContext","kind":"style","styleKind":"paragraph","style":"ParagraphStyle/$ID/[No paragraph style]"}),
    );
    assert!(style["style"]["style"].is_object());
    let file = ok(
        &mut app,
        json!({"cmd":"aiContext","kind":"file","file":"source.idml"}),
    );
    assert_eq!(
        file["file"]["size"],
        std::fs::metadata(path.with_file_name("source.idml"))
            .unwrap()
            .len()
    );
    assert_eq!(
        app.request(json!({"cmd":"aiContext","kind":"file","file":"../secret"}))["ok"],
        false
    );
    let stored = &records.iter().find(|m| m["kind"] == "user").unwrap()["attachments"][0]["image"];
    assert!(stored["png"].is_null() && stored["pngRef"] == stored["sha256"]);
    let hydrated = ok(
        &mut app,
        json!({"cmd":"aiSession","sessionId":sid,"includeImages":true}),
    );
    let hydrated = &hydrated["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["kind"] == "user")
        .unwrap()["attachments"][0]["image"];
    assert_image(hydrated);
    assert_eq!(stored["sha256"], hydrated["sha256"]);
    for (seq, event) in records.iter().skip(1).enumerate() {
        assert_eq!(event["seq"], seq + 1);
        assert_eq!(event["turnId"], turn);
        assert_eq!(event["sessionId"], sid);
    }
    let preflight = ok(&mut app, json!({"cmd":"aiContext","kind":"preflight"}));
    assert!(preflight["preflight"].is_object());
    let markdown = ok(&mut app, json!({"cmd":"aiExportSession","sessionId":sid}));
    assert!(
        markdown["markdown"]
            .as_str()
            .unwrap()
            .contains("제목을 넣었습니다")
    );
    assert!(
        !serde_json::to_string(&records)
            .unwrap()
            .contains("test-key-not-real")
    );
    let dry = ok(
        &mut app,
        json!({"cmd":"aiRevertTurn","sessionId":sid,"turnId":turn,"dryRun":true}),
    );
    assert_eq!(dry["steps"], json!(["AI: 제목을 만들어 줘"]));
    assert_eq!(dry["applied"], false);
    let restored = ok(
        &mut app,
        json!({"cmd":"aiRevertTurn","sessionId":sid,"turnId":turn}),
    );
    assert_eq!(restored["applied"], true);
    let after = ok(&mut app, json!({"cmd":"debugDigest"}));
    assert_eq!(after["layout"], before["layout"]);
    assert!(
        original == saved_parts(&mut app, "after.idml"),
        "exported document parts differ after reverting"
    );
    ok(&mut app, json!({"cmd":"redo"}));
    let manual = |app: &mut App, label: &str| {
        ok(
            app,
            json!({"cmd":"edit","label":label,"commands":[{"op":"insertText","story":object["text"]["story"],"at":0,"text":"X"}]}),
        );
    };
    manual(&mut app, "manual");
    let restore = json!({"cmd":"aiRestoreCheckpoint","sessionId":sid,"turnId":turn,"edge":"start"});
    let warning = ok(&mut app, restore.clone());
    assert_eq!(warning["needsConfirmation"], true);
    assert_eq!(warning["steps"], json!(["manual", "AI: 제목을 만들어 줘"]));
    let mut confirmed = restore.clone();
    confirmed["confirmedRev"] = warning["rev"].clone();
    manual(&mut app, "manual2");
    let newer = ok(&mut app, confirmed.clone());
    assert_eq!(newer["applied"], false);
    assert_eq!(newer["steps"], json!(["manual2", "manual", "AI: 제목을 만들어 줘"]));
    confirmed["confirmedRev"] = newer["rev"].clone();
    assert_eq!(ok(&mut app, confirmed)["applied"], true);
    assert!(original == saved_parts(&mut app, "confirmed.idml"));
    let later = ok(
        &mut app,
        json!({"cmd":"aiSend","sessionId":sid,"text":"다시 읽기","script":[{"kind":"end"}]}),
    );
    let end = app
        .event("aiEvent", |e| {
            e["kind"] == "end" && e["turnId"] == later["turnId"]
        })
        .unwrap();
    assert_eq!(end["status"], "complete");
}
fn assert_image(image: &Value) {
    let data = folio_project::idmlparts::decode_base64(image["png"].as_str().unwrap()).unwrap();
    assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
    for (offset, key) in [(16, "width"), (20, "height")] {
        let size = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap());
        assert!((1..=1568).contains(&size));
        assert_eq!(image[key], size);
    }
    assert_eq!(image["sha256"], folio_project::hash::sha256_bytes(&data));
}
