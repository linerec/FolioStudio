"""Offline ACP peer. Its only child is the Folio MCP supplied by the client."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1])
page, layer = sys.argv[2:4]
sid = "fixture-session"


def receive(stream=sys.stdin):
    return json.loads(stream.readline())


def send(value, stream=sys.stdout):
    stream.write(json.dumps({"jsonrpc": "2.0", **value}) + "\n")
    stream.flush()


def update(kind, **data):
    send({"method": "session/update", "params": {"sessionId": sid,
          "update": {"sessionUpdate": kind, **data}}})


def permission(number, choices, tool=None):
    options = [{"optionId": k + "-id", "kind": k, "name": k} for k in choices]
    send({"id": number, "method": "session/request_permission", "params": {
        "sessionId": sid, "toolCall": tool or {"toolCallId": "shell", "title": "Shell", "kind": "execute"},
        "options": options}})


def run():
    init = receive()
    assert init["method"] == "initialize"
    caps = init["params"]["clientCapabilities"]
    assert not caps["terminal"] and not any(caps["fs"].values())
    send({"id": init["id"], "result": {"protocolVersion": 1, "agentCapabilities": {
        "loadSession": True, "promptCapabilities": {"image": True}, "sessionCapabilities": {"systemPrompt": {}}}}})
    setup = receive()
    params = setup["params"]
    assert Path(params["cwd"]).samefile(root / "project")
    assert Path.cwd().samefile(root / "project")
    mcp, = params["mcpServers"]
    attach, flag, file = mcp["args"]
    assert attach == "--attach" and flag == "--connection" and Path(file).is_file()
    assert params["systemPrompt"]["mode"] == "append"
    (root / "endpoint").write_text(file, encoding="utf-8")
    loaded = setup["method"] == "session/load"
    if loaded:
        assert params["sessionId"] == sid
        update("agent_message_chunk", content={"type":"text","text":"replayed duplicate"})
    else:
        assert setup["method"] == "session/new"
    send({"id":setup["id"],"result":{} if loaded else {"sessionId":sid}})
    prompt = receive()
    assert prompt["method"] == "session/prompt" and prompt["params"]["sessionId"] == sid
    blocks = prompt["params"]["prompt"]
    if loaded:
        assert blocks == [{"type":"text","text":"stop"}]
        permission(12, ["allow_once", "reject_once"])
        assert receive()["method"] == "session/cancel"
        assert receive()["result"]["outcome"]["outcome"] == "cancelled"
        (root / "cancelled").touch()
        return
    assert any(b["type"] == "resource_link" and b["uri"].startswith("folio://") for b in blocks)
    assert any(b["type"] == "image" and b["mimeType"] == "image/png" for b in blocks)
    key = os.environ["FIXTURE_KEY"]
    update("agent_message_chunk", content={"type":"text","text":"hello " + key[:4]})
    update("agent_message_chunk", content={"type":"text","text":key[4:]})
    update("agent_thought_chunk", content={"type":"text","text":"thinking"})
    update("plan", entries=[{"content":"two frames","status":"pending","priority":"high"}])
    update("tool_call", toolCallId="shell", title="Shell", status="pending")
    permission(8, ["allow_always","reject_once"])
    assert receive()["result"]["outcome"]["optionId"] == "reject_once-id"
    for i, decision in enumerate(["reject_once", "allow_once"]):
        permission(9+i, ["reject_once","allow_once","allow_always"])
        assert receive()["result"]["outcome"]["optionId"] == decision + "-id"
    frame = {"page":page,"layer":layer,"kind":"textFrame","rect":[20,20,160,65],"sides":4,"inset":0}
    permission(11, ["allow_once","allow_always","reject_once"],
               {"toolCallId":"frame","title":"Create frame","name":"edit.createByDrag","kind":"edit","rawInput":frame})
    assert receive()["result"]["outcome"]["optionId"] == "allow_once-id"
    env = os.environ.copy()
    env.update({item["name"]:item["value"] for item in mcp["env"]})
    with subprocess.Popen([mcp["command"], *mcp["args"]], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, text=True, encoding="utf-8", env=env) as child:
        def rpc(method, params):
            send({"id":1,"method":method,"params":params},child.stdin)
            return receive(child.stdout)["result"]
        rpc("initialize", {"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}})
        send({"method":"notifications/initialized"},child.stdin)
        assert rpc("tools/call", {"name":"appConnection","arguments":{}})["structuredContent"]["attached"]
        for i in range(2):
            frame = {"page":page,"layer":layer,"kind":"textFrame","rect":[20+i,20,160,65],"sides":4,"inset":0}
            result = rpc("tools/call", {"name":"edit.createByDrag","arguments":frame})
            assert not result["isError"], result
        child.stdin.close()
    update("tool_call_update",toolCallId="shell",status="completed")
    send({"id":prompt["id"],"result":{"stopReason":"end_turn"}})


if __name__ == "__main__":
    try:
        run()
    except Exception as error:
        import traceback
        (root / "failure").write_text(traceback.format_exc(),encoding="utf-8")
        raise
    finally:
        (root / "exited").touch()
