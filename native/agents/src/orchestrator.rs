//! The `billable` MCP server every Mac chat's session gets (PROTOCOL.md,
//! "Coordinator"): the tools for its seat, the guidance its agent is given,
//! and MCP's JSON-RPC over HTTP (plain JSON answers, no sessions). The relay
//! serves it at `/mcp/<secret>/orchestrate/<threadId>` and hands each tool
//! call to the engine thread, which does the work.
//!
//! A coordinator (role "orchestrator") starts jobs and lookups, proposes
//! plans and waits for answers; a job reports back with ready_for_review;
//! any chat can put a decision card to the person (ask_person) and, in a
//! project, save a standing preference to the project's memory (remember).
//! Lookups get no server at all: they only read.

use serde_json::{json, Value};

/// The MCP server's name in the agent's config (tools are `mcp__billable__*`).
pub const SERVER_NAME: &str = "billable";
pub const TOOL_PREFIX: &str = "mcp__billable__";

/// At most this many jobs run at once per coordinator.
pub const MAX_RUNNING: usize = 4;
/// Lookups have their own slots, so a quick read never waits behind a job.
pub const MAX_LOOKUPS_RUNNING: usize = 4;
/// At most this many jobs and lookups per coordinator, ever.
pub const MAX_WORKERS: usize = 40;
/// At most this many per start_agents (or propose_plan) call.
pub const MAX_PER_CALL: usize = 8;
pub const DEFAULT_WAIT_SECS: u64 = 600;
pub const MAX_WAIT_SECS: u64 = 1800;
/// How long ask_person and propose_plan hold the call open for an answer.
/// The card stays open after; a later answer arrives as a new message.
pub const ASK_WAIT_SECS: u64 = 1500;
/// A worker's answer, as wait_for_agents returns it.
pub const ANSWER_CHARS: usize = 8000;
/// Project memory is kept short: it goes into every chat's instructions.
pub const MEMORY_CHARS: usize = 6000;
/// A queued worker's preview.
pub const WAITING_PREVIEW: &str = "Waiting to start";

/// Where a chat sits, which decides its tools and guidance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Coordinator,
    Job,
    Chat,
}

/// Said to a coordinator's agent after the usual note.
pub const COORDINATOR: &str = "You are this chat's coordinator. The person talks to you; jobs do the heavy work. \
Your tools are mcp__billable__*.\n\
- Do quick things yourself: answer questions, make a small edit, read a file or two.\n\
- Send bigger work to jobs with start_agents. A job is its own chat that the person can open and steer. It can't see this chat, \
so give it a complete, self-contained task. Run independent pieces as parallel jobs, and never give two jobs the same files.\n\
- For quick read-only research (where is X, what does this file do, check a fact), start a lookup (kind \"lookup\"). \
It can read but never change anything, and never needs the person.\n\
- Choose each one's model and effort: low effort and the fastest model for lookups, renames and mechanical edits; \
medium for everyday features and fixes; high effort and the strongest model for hard debugging, design and anything risky. \
Start each task with a one-line reason for your choice.\n\
- Before a bigger push (more than three jobs, or any high-effort job), call propose_plan instead, so the person can check \
the plan; it starts the jobs they keep.\n\
- When a choice changes what the person gets, or a step can't be undone, call ask_person with 2 to 4 options and your \
recommendation instead of guessing or asking in prose.\n\
- Then call wait_for_agents. Check what comes back, follow up with message_agent when something is missing or wrong, \
and finish with one short update: what's done, what's ready for the person to review, and anything that needs them.\n\
- When the person states a standing preference (always, never, how they like things), save it with remember.";

/// Said to a job's agent.
pub const JOB: &str = "You are a job, started by this project's coordinator. The person can open this chat and steer you \
directly; follow them over the original task. Your tools are mcp__billable__*.\n\
- If you reach a choice only the person can make, call ask_person with 2 to 4 options and your recommendation.\n\
- When your work is ready for the person to check, call ready_for_review with a one-line summary, then end with a short \
report of what you changed and anything left open.\n\
- When the person states a standing preference, save it with remember.";

/// Said to any other Mac chat's agent.
pub const CHAT: &str = "If you reach a choice only the person can make, call mcp__billable__ask_person with 2 to 4 options \
and your recommendation instead of asking in prose.";

/// Said to a lookup's agent (it gets no billable server).
pub const LOOKUP: &str = "You are a quick read-only lookup for a coordinator. Read, search and answer; never change a file \
or anything in FeedHQ, and don't ask the person anything. Answer in a few short lines.";

/// A chat's instructions after the usual note: its seat's guidance and
/// the project's memory.
pub fn notes(seat: Option<Seat>, memory: Option<&str>) -> String {
    let mut out = match seat {
        Some(Seat::Coordinator) => COORDINATOR.to_string(),
        Some(Seat::Job) => JOB.to_string(),
        Some(Seat::Chat) => CHAT.to_string(),
        None => LOOKUP.to_string(),
    };
    if let Some(m) = memory.map(str::trim).filter(|m| !m.is_empty()) {
        out.push_str("\n\nThis project's memory (the person's standing preferences; follow them):\n");
        out.push_str(m);
    }
    out
}

/// What the engine is asked.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// The tool list for this chat's seat.
    List,
    Tool { name: String, args: Value },
}

fn job_items(with_reason: bool) -> Value {
    let mut props = json!({
        "title": {"type": "string", "description": "A short name for the job's chat, e.g. \"Fix the login test\"."},
        "task": {"type": "string", "description": "The complete task. Start with a one-line reason for the model and effort."},
        "kind": {"type": "string", "enum": ["job", "lookup"], "description": "job (default): its own chat, can change things. lookup: quick and read-only."},
        "agent": {"type": "string", "enum": ["claude", "codex"]},
        "model": {"type": "string", "description": "A model id from the list above."},
        "effort": {"type": "string", "description": "An effort level the model allows, e.g. low, medium, high."}
    });
    if with_reason {
        props["reason"] = json!({"type": "string", "description": "Why this model and effort, in a few words (shown to the person)."});
    }
    json!({"type": "object", "properties": props, "required": ["title", "task"]})
}

/// The tool definitions for a seat. `models` goes into start_agents'
/// description; `memory`: the chat is in a project.
pub fn tools(seat: Seat, models: &str, memory: bool) -> Value {
    let ids = json!({"type": "array", "items": {"type": "string"}, "description": "Ids from start_agents. Leave out for all of them."});
    let start = format!(
        "Start 1 to {MAX_PER_CALL} jobs or lookups, each on its own task, in the background. Returns their ids at once. \
At most {MAX_RUNNING} jobs and {MAX_LOOKUPS_RUNNING} lookups run at a time (the rest wait their turn), {MAX_WORKERS} per chat in all. \
Pick agent, model and effort for each (left out: your own agent, then that agent's default model and the model's default effort).\n{models}"
    );
    let ask = json!({
        "name": "ask_person",
        "description": format!("Put a decision to the person as a card with 2 to 4 options and your recommendation, and wait up to {} minutes for their pick. \
If they haven't answered by then, finish what you can without it and say what you're waiting on; their answer will arrive as a new message.", ASK_WAIT_SECS / 60),
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "One short question, verb first."},
                "detail": {"type": "string", "description": "Optional: a line or two of context."},
                "options": {"type": "array", "minItems": 2, "maxItems": 4, "items": {"type": "object", "properties": {
                    "label": {"type": "string", "description": "A few words."},
                    "detail": {"type": "string", "description": "One line on what happens if they pick it."}
                }, "required": ["label"]}},
                "recommended": {"type": "integer", "minimum": 0, "description": "The index of the option you recommend."}
            },
            "required": ["question", "options", "recommended"]
        }
    });
    let remember = json!({
        "name": "remember",
        "description": format!("Save one of the person's standing preferences to this project's memory, which every chat in the project follows. \
One short line each; at most {MEMORY_CHARS} characters in all. Returns the memory as it now stands."),
        "inputSchema": {"type": "object", "properties": {"note": {"type": "string"}}, "required": ["note"]}
    });
    let mut out: Vec<Value> = Vec::new();
    match seat {
        Seat::Coordinator => {
            out.push(json!({
                "name": "start_agents",
                "description": start,
                "inputSchema": {"type": "object", "properties": {"agents": {"type": "array", "minItems": 1, "maxItems": MAX_PER_CALL, "items": job_items(false)}}, "required": ["agents"]}
            }));
            out.push(json!({
                "name": "propose_plan",
                "description": format!("Show the person a plan of 1 to {MAX_PER_CALL} jobs and lookups (same fields as start_agents, plus a short reason each) and wait up to {} minutes. \
They can drop jobs before starting it. Returns what they decided and the ids of the jobs it started; then call wait_for_agents.", ASK_WAIT_SECS / 60),
                "inputSchema": {"type": "object", "properties": {
                    "summary": {"type": "string", "description": "One line on the plan as a whole."},
                    "agents": {"type": "array", "minItems": 1, "maxItems": MAX_PER_CALL, "items": job_items(true)}
                }, "required": ["agents"]}
            }));
            out.push(json!({
                "name": "wait_for_agents",
                "description": format!("Wait until the jobs finish (or the timeout passes) and get each one's status and final answer. Status is queued, running, waiting (needs the person), review (ready for the person to check), done, stopped, failed or not_started. Default timeout {DEFAULT_WAIT_SECS} seconds, at most {MAX_WAIT_SECS}."),
                "inputSchema": {"type": "object", "properties": {"ids": ids, "timeoutSeconds": {"type": "number", "minimum": 1, "maximum": MAX_WAIT_SECS}}}
            }));
            out.push(json!({
                "name": "message_agent",
                "description": "Send a follow-up to a job that has finished; it works on it in the background (wait_for_agents for the answer). Fails while the job is still busy.",
                "inputSchema": {"type": "object", "properties": {"id": {"type": "string"}, "text": {"type": "string"}}, "required": ["id", "text"]}
            }));
            out.push(json!({
                "name": "list_agents",
                "description": "List this chat's jobs and lookups with their kind, agent, model, effort and status.",
                "inputSchema": {"type": "object", "properties": {}}
            }));
            out.push(json!({
                "name": "stop_agents",
                "description": "Stop jobs and lookups that are running or waiting to start.",
                "inputSchema": {"type": "object", "properties": {"ids": ids}}
            }));
            out.push(ask);
        }
        Seat::Job => {
            out.push(ask);
            out.push(json!({
                "name": "ready_for_review",
                "description": "Tell the person (and the coordinator) your work is ready for them to check. The chat shows Review ready until they approve it or send you a message.",
                "inputSchema": {"type": "object", "properties": {"summary": {"type": "string", "description": "One line on what's ready."}}, "required": ["summary"]}
            }));
        }
        Seat::Chat => out.push(ask),
    }
    if memory {
        out.push(remember);
    }
    Value::Array(out)
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
                "instructions": "Coordinate jobs, ask the person and keep the project's memory, in Billable."
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({ "tools": engine(Call::List).unwrap_or_else(|_| json!([])) })),
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
                Call::List => Ok(tools(Seat::Coordinator, "- claude: opus, haiku", false)),
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
        assert_eq!(names, vec!["start_agents", "propose_plan", "wait_for_agents", "message_agent", "list_agents", "stop_agents", "ask_person"]);
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
    fn each_seat_gets_its_tools_and_guidance() {
        let names = |seat, memory| -> Vec<String> { tools(seat, "", memory).as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect() };
        assert_eq!(names(Seat::Job, true), vec!["ask_person", "ready_for_review", "remember"]);
        assert_eq!(names(Seat::Chat, false), vec!["ask_person"]);
        assert!(names(Seat::Coordinator, true).ends_with(&["ask_person".to_string(), "remember".to_string()]));
        let start = &tools(Seat::Coordinator, "", false)[0]["inputSchema"]["properties"]["agents"]["items"]["properties"];
        assert_eq!(start["kind"]["enum"], json!(["job", "lookup"]));
        assert!(start.get("reason").is_none());
        assert!(tools(Seat::Coordinator, "", false)[1]["inputSchema"]["properties"]["agents"]["items"]["properties"].get("reason").is_some());
        let n = notes(Some(Seat::Coordinator), Some("- Always use British spelling"));
        assert!(n.starts_with("You are this chat's coordinator"));
        assert!(n.ends_with("follow them):\n- Always use British spelling"));
        assert!(!notes(Some(Seat::Job), Some("  ")).contains("memory"));
        assert!(notes(None, None).contains("read-only"));
    }

    #[test]
    fn worker_ids_of_a_call() {
        assert_eq!(threads_of(&json!({"agents": [{}]}), Some(r#"[{"id":"t_1"},{"id":"t_2"}]"#)), vec!["t_1", "t_2"]);
        assert_eq!(threads_of(&json!({"id": "t_3", "text": "more"}), Some(r#"{"ok":true}"#)), vec!["t_3"]);
        assert_eq!(threads_of(&json!({"ids": ["t_1"]}), Some(r#"[{"id":"t_1","status":"done"}]"#)), vec!["t_1"]);
        assert!(threads_of(&json!({}), None).is_empty());
    }
}
