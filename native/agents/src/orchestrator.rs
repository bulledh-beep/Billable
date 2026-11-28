//! The `billable` MCP server an orchestrator thread's session gets
//! (PROTOCOL.md, "Orchestrator"): the tool list, the guidance its agent is
//! given, and MCP's JSON-RPC over HTTP (plain JSON answers, no sessions).
//! The relay serves it at `/mcp/<secret>/orchestrate/<threadId>` and hands
//! each tool call to the engine thread, which does the work.

use serde_json::{json, Value};

/// The MCP server's name in the agent's config (tools are `mcp__billable__*`).
pub const SERVER_NAME: &str = "billable";
pub const TOOL_PREFIX: &str = "mcp__billable__";

/// At most this many workers run at once per orchestrator.
pub const MAX_RUNNING: usize = 4;
/// At most this many workers per orchestrator, ever.
pub const MAX_WORKERS: usize = 24;
/// At most this many workers per start_agents call.
pub const MAX_PER_CALL: usize = 8;
pub const DEFAULT_WAIT_SECS: u64 = 600;
pub const MAX_WAIT_SECS: u64 = 1800;
/// A worker's answer, as wait_for_agents returns it.
pub const ANSWER_CHARS: usize = 8000;
/// A queued worker's preview.
pub const WAITING_PREVIEW: &str = "Waiting to start";

/// Said to an orchestrator's agent after the usual note.
pub const GUIDANCE: &str = "You are an orchestrator. You can split the person's request across helper agents with the mcp__billable__* tools: \
start_agents, wait_for_agents, message_agent, list_agents and stop_agents. Helpers work in the same folder as you, can't see this chat, \
and can't start helpers of their own, so give each one a complete, self-contained task.\n\
Choose each helper's model and effort for its job:\n\
- low effort and the fastest model for lookups, renames, searches and mechanical edits;\n\
- medium effort for everyday features and fixes;\n\
- high effort and the strongest model for hard debugging, design and anything risky.\n\
Start every task with a one-line reason for the model and effort you chose.\n\
Start independent pieces of work in parallel with one start_agents call, then call wait_for_agents. \
Review the answers, follow up with message_agent when something is missing or wrong, and finish with a short summary for the person \
of what was done and by whom. Don't split work that is quicker to do yourself, and don't give two helpers the same files to change.";

/// What the engine is asked.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// A line about the agents and models a worker can use (for the tool list).
    Describe,
    Tool { name: String, args: Value },
}

/// The tool definitions; `models` (from [`Call::Describe`]) goes into
/// start_agents' description.
pub fn tools(models: &str) -> Value {
    let ids = json!({"type": "array", "items": {"type": "string"}, "description": "Helper ids from start_agents. Leave out for all of them."});
    let start = format!(
        "Start 1 to {MAX_PER_CALL} helper agents, each on its own task, in the background. Returns their ids at once. \
At most {MAX_RUNNING} run at a time (the rest wait their turn) and at most {MAX_WORKERS} per chat. \
Pick agent, model and effort per helper (left out: your own agent, then that agent's default model and the model's default effort).\n{models}"
    );
    json!([
        {
            "name": "start_agents",
            "description": start,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agents": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_PER_CALL,
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string", "description": "A short name for the helper's chat, e.g. \"Fix the login test\"."},
                                "task": {"type": "string", "description": "The complete task. Start with a one-line reason for the model and effort."},
                                "agent": {"type": "string", "enum": ["claude", "codex"]},
                                "model": {"type": "string", "description": "A model id from the list above."},
                                "effort": {"type": "string", "description": "An effort level the model allows, e.g. low, medium, high."}
                            },
                            "required": ["title", "task"]
                        }
                    }
                },
                "required": ["agents"]
            }
        },
        {
            "name": "wait_for_agents",
            "description": format!("Wait until the helpers finish (or the timeout passes) and get each one's status and final answer. Default timeout {DEFAULT_WAIT_SECS} seconds, at most {MAX_WAIT_SECS}."),
            "inputSchema": {
                "type": "object",
                "properties": {"ids": ids, "timeoutSeconds": {"type": "number", "minimum": 1, "maximum": MAX_WAIT_SECS}}
            }
        },
        {
            "name": "message_agent",
            "description": "Send a follow-up message to a helper that has finished; it works on it in the background (wait_for_agents for the answer). Fails while the helper is still busy.",
            "inputSchema": {
                "type": "object",
                "properties": {"id": {"type": "string"}, "text": {"type": "string"}},
                "required": ["id", "text"]
            }
        },
        {
            "name": "list_agents",
            "description": "List this chat's helpers with their agent, model, effort and status.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "stop_agents",
            "description": "Stop helpers that are running or waiting to start.",
            "inputSchema": {"type": "object", "properties": {"ids": ids}}
        }
    ])
}

/// Answer one JSON-RPC message (or a batch). `None`: only notifications,
/// nothing to send back (HTTP 202).
pub fn rpc(body: &[u8], engine: &dyn Fn(Call) -> Result<Value, String>) -> Option<Value> {
    let Ok(msg) = serde_json::from_slice::<Value>(body) else {
        return Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "That isn't JSON."}}));
    };
    match msg {
        Value::Array(batch) => {
            let out: Vec<Value> = batch.iter().filter_map(|m| one(m, engine)).collect();
            (!out.is_empty()).then_some(Value::Array(out))
        }
        m => one(&m, engine),
    }
}

fn one(msg: &Value, engine: &dyn Fn(Call) -> Result<Value, String>) -> Option<Value> {
    let id = msg.get("id").cloned().filter(|i| !i.is_null())?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let ok = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    match method {
        "initialize" => {
            let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18");
            ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": SERVER_NAME, "title": "Billable", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Start, wait for, message and stop helper agents in Billable."
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => {
            let models = engine(Call::Describe).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            ok(json!({ "tools": tools(&models) }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().filter(|a| a.is_object()).unwrap_or(json!({}));
            let (text, is_error) = match engine(Call::Tool { name, args }) {
                Ok(v) => (v.to_string(), false),
                Err(e) => (e, true),
            };
            ok(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        _ => Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("Billable doesn't know {method}.")}})),
    }
}

/// The worker ids a billable tool call started or read, from its input
/// (`id`, `ids`) and its output (a JSON list of `{ id }`).
pub fn threads_of(input: &Value, output: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        if !s.is_empty() && !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    };
    if let Some(id) = input.get("id").and_then(Value::as_str) {
        add(id);
    }
    for id in input.get("ids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        add(id);
    }
    if let Some(list) = output.and_then(|o| serde_json::from_str::<Value>(o.trim()).ok()) {
        for id in list.as_array().into_iter().flatten().filter_map(|e| e.get("id").and_then(Value::as_str)) {
            add(id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_mcp() {
        let calls = std::cell::RefCell::new(Vec::new());
        let engine = |c: Call| {
            calls.borrow_mut().push(c.clone());
            match c {
                Call::Describe => Ok(json!("- claude: opus, haiku")),
                Call::Tool { name, .. } if name == "list_agents" => Ok(json!([{"id": "t_1"}])),
                Call::Tool { .. } => Err("No.".to_string()),
            }
        };
        let init = rpc(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, &engine).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "billable");
        assert!(rpc(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &engine).is_none());
        let list = rpc(br#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &engine).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["start_agents", "wait_for_agents", "message_agent", "list_agents", "stop_agents"]);
        assert!(list["result"]["tools"][0]["description"].as_str().unwrap().contains("- claude: opus, haiku"));
        let r = rpc(br#"{"jsonrpc":"2.0","id":"x","method":"tools/call","params":{"name":"list_agents","arguments":{}}}"#, &engine).unwrap();
        assert_eq!(r["id"], "x");
        assert_eq!(r["result"], json!({"content":[{"type":"text","text":"[{\"id\":\"t_1\"}]"}],"isError":false}));
        let r = rpc(br#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"stop_agents"}}"#, &engine).unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(calls.borrow().last(), Some(&Call::Tool { name: "stop_agents".into(), args: json!({}) }));
        assert_eq!(rpc(br#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#, &engine).unwrap()["error"]["code"], -32601);
        assert_eq!(rpc(b"nope", &engine).unwrap()["error"]["code"], -32700);
        let batch = rpc(br#"[{"jsonrpc":"2.0","id":5,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/x"}]"#, &engine).unwrap();
        assert_eq!(batch, json!([{"jsonrpc":"2.0","id":5,"result":{}}]));
    }

    #[test]
    fn worker_ids_of_a_call() {
        assert_eq!(threads_of(&json!({"agents": [{}]}), Some(r#"[{"id":"t_1"},{"id":"t_2"}]"#)), vec!["t_1", "t_2"]);
        assert_eq!(threads_of(&json!({"id": "t_3", "text": "more"}), Some(r#"{"ok":true}"#)), vec!["t_3"]);
        assert_eq!(threads_of(&json!({"ids": ["t_1"]}), Some(r#"[{"id":"t_1","status":"done"}]"#)), vec!["t_1"]);
        assert!(threads_of(&json!({}), None).is_empty());
    }
}
