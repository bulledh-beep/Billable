#!/usr/bin/env python3
# A fake Codex CLI for the engine tests: --version, a failing `exec` (so
# titles fall back to the first words), and `app-server` JSON-RPC. Every
# line it reads is logged to codex.log next to this script.
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
LOG = os.path.join(HERE, "codex.log")
args = sys.argv[1:]


def log(kind, value):
    with open(LOG, "a") as f:
        f.write(json.dumps({"kind": kind, "value": value}) + "\n")


def out(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


if args == ["--version"]:
    print("codex-cli 0.130.0")
    sys.exit(0)
if args[:1] == ["exec"]:
    sys.exit(1)
if args != ["app-server"]:
    sys.exit(2)

log("launch", args)
backlog = []


def read():
    if backlog:
        return backlog.pop(0)
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    msg = json.loads(line)
    log("in", msg)
    return msg


def wait_result(request_id):
    while True:
        msg = read()
        if "method" not in msg and msg.get("id") == request_id:
            return msg.get("result")
        backlog.append(msg)


threads = 0
turns = 0
while True:
    msg = read()
    method, rid, p = msg.get("method"), msg.get("id"), msg.get("params") or {}
    if method == "initialize":
        out({"id": rid, "result": {"userAgent": "fake"}})
    elif method == "thread/start":
        threads += 1
        out({"id": rid, "result": {"thread": {"id": "cx-%d" % threads}}})
    elif method == "thread/resume":
        out({"id": rid, "error": {"code": -32600, "message": "no rollout"}})
    elif method == "turn/start":
        turns += 1
        ct, tid = p["threadId"], "turn-%d" % turns
        text = p["input"][0]["text"]
        out({"id": rid, "result": {"turn": {"id": tid}}})
        out({"method": "turn/started", "params": {"threadId": ct, "turn": {"id": tid}}})
        if "tool" in text.split("</feedhq-handoff>")[-1]:
            item = {"type": "commandExecution", "id": "call_1", "command": "rm -rf build", "cwd": "/w", "status": "inProgress"}
            out({"method": "item/started", "params": {"threadId": ct, "turnId": tid, "item": item}})
            out({"id": 900, "method": "item/commandExecution/requestApproval", "params": {"threadId": ct, "turnId": tid, "itemId": "call_1", "command": "rm -rf build", "cwd": "/w"}})
            decision = wait_result(900)["decision"]
            done = dict(item, status="completed" if decision != "decline" else "declined", exitCode=0, aggregatedOutput="")
            out({"method": "item/completed", "params": {"threadId": ct, "turnId": tid, "item": done}})
        reply = "Codex took over" if "<feedhq-handoff>" in text else "Codex here"
        out({"method": "item/agentMessage/delta", "params": {"threadId": ct, "turnId": tid, "itemId": "m%d" % turns, "delta": reply}})
        out({"method": "item/completed", "params": {"threadId": ct, "turnId": tid, "item": {"type": "agentMessage", "id": "m%d" % turns, "text": reply}}})
        out({"method": "thread/tokenUsage/updated", "params": {"threadId": ct, "turnId": tid, "tokenUsage": {"last": {"inputTokens": 3, "outputTokens": 4}}}})
        out({"method": "turn/completed", "params": {"threadId": ct, "turn": {"id": tid, "status": "completed"}}})
    elif method == "turn/interrupt":
        out({"id": rid, "result": {}})
    elif rid is not None and method is not None:
        out({"id": rid, "error": {"code": -32601, "message": "unknown"}})
