#!/usr/bin/env python3
# A fake Claude Code CLI for the engine tests: answers --version, auth status,
# the one-shot title call, and the stream-json protocol. Every launch and
# every line it reads is logged to claude.log next to this script.
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
LOG = os.path.join(HERE, "claude.log")
args = sys.argv[1:]


def log(kind, value):
    with open(LOG, "a") as f:
        f.write(json.dumps({"kind": kind, "value": value}) + "\n")


def out(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


if args == ["--version"]:
    print("2.1.332 (Claude Code)")
    sys.exit(0)
if args[:2] == ["auth", "status"]:
    print(json.dumps({"loggedIn": True, "authMethod": "claude.ai"}))
    sys.exit(0)
if "--output-format" in args and args[args.index("--output-format") + 1] == "json":
    log("title", args)
    print(json.dumps({"type": "result", "subtype": "success", "is_error": False, "result": "\"Pottery Launch Plan.\""}))
    sys.exit(0)

log("launch", args)
session = "sess-1"
if "--resume" in args:
    session = args[args.index("--resume") + 1]
turns = 0
announced = False


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    msg = json.loads(line)
    log("in", msg)
    return msg


def wait_response(request_id):
    while True:
        msg = read()
        if msg.get("type") == "control_response" and msg["response"].get("request_id") == request_id:
            return msg["response"]


def finish(ok=True):
    if ok:
        out({"type": "result", "subtype": "success", "is_error": False, "result": "", "total_cost_usd": 0.0123,
             "usage": {"input_tokens": 10, "cache_read_input_tokens": 5, "output_tokens": 7}})
    else:
        out({"type": "result", "subtype": "error_during_execution", "is_error": True, "errors": []})


def say(msg_id, text):
    out({"type": "stream_event", "event": {"type": "message_start", "message": {"id": msg_id}}})
    out({"type": "stream_event", "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}})
    half = len(text) // 2
    for part in (text[:half], text[half:]):
        out({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": part}}})
    out({"type": "assistant", "parent_tool_use_id": None, "message": {"id": msg_id, "content": [{"type": "text", "text": text}]}})


while True:
    msg = read()
    if msg.get("type") == "control_request":
        sub = msg["request"]["subtype"]
        if sub == "initialize":
            out({"type": "control_response", "response": {"subtype": "success", "request_id": msg["request_id"], "response": {
                "models": [{"value": "opus", "displayName": "Opus", "supportsEffort": True, "supportedEffortLevels": ["low", "high"]}]}}})
        elif sub == "interrupt":
            out({"type": "control_response", "response": {"subtype": "success", "request_id": msg["request_id"], "response": {}}})
            finish(ok=False)
        continue
    if msg.get("type") != "user":
        continue
    turns += 1
    text = msg["message"]["content"]
    # Only the new message decides what to do, not the handoff before it.
    asked = text.split("</feedhq-handoff>")[-1]
    if not announced:
        out({"type": "system", "subtype": "init", "session_id": session})
        announced = True
    mid = "msg_%d" % turns
    if "wait forever" in asked:
        say(mid, "Working on it")
        continue
    if "use a tool" in asked:
        out({"type": "assistant", "parent_tool_use_id": None, "message": {"id": mid + "a", "content": [
            {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "ls", "description": "List files"}}]}})
        out({"type": "control_request", "request_id": "perm-1", "request": {"subtype": "can_use_tool", "tool_name": "Bash",
             "input": {"command": "ls"}, "tool_use_id": "toolu_1", "permission_suggestions": []}})
        answer = wait_response("perm-1")
        allowed = answer["response"]["behavior"] == "allow"
        out({"type": "user", "parent_tool_use_id": None, "message": {"content": [
            {"type": "tool_result", "tool_use_id": "toolu_1", "content": "a.txt" if allowed else "declined", "is_error": not allowed}]}})
    reply = "Handoff received" if "<feedhq-handoff>" in text else "Hello there"
    say(mid, reply)
    finish()
