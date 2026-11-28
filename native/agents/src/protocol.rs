//! The protocol between FeedHQ's Agents screen and this engine
//! (`native/agents/PROTOCOL.md`, version 1). FeedHQ's
//! `src/lib/agents/protocol.ts` is the other half; the two change together.
//!
//! Field names are camelCase on the wire. Unknown fields are ignored when
//! reading (serde's default), and optional fields are left out when empty.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

// ---------- Envelopes ----------

/// Screen → engine.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Request {
    #[serde(default = "one")]
    pub v: u32,
    #[serde(rename = "type", default = "request_type")]
    pub kind: String,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

fn one() -> u32 {
    1
}

fn request_type() -> String {
    "request".into()
}

/// Engine → screen, the answer to one request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reply {
    pub v: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Reply {
    pub fn ok(id: &str, result: Value) -> Reply {
        Reply { v: PROTOCOL_VERSION, kind: "reply".into(), id: id.into(), ok: true, result: Some(result), error: None }
    }

    pub fn err(id: &str, error: ProtoError) -> Reply {
        Reply {
            v: PROTOCOL_VERSION,
            kind: "reply".into(),
            id: id.into(),
            ok: false,
            result: None,
            error: Some(ErrorBody { code: error.code, message: error.message }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

/// Engine → screen, at any time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub v: u32,
    #[serde(rename = "type")]
    pub kind: String,
    pub event: Event,
}

impl EventEnvelope {
    pub fn new(event: Event) -> EventEnvelope {
        EventEnvelope { v: PROTOCOL_VERSION, kind: "event".into(), event }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    NotFound,
    Busy,
    AgentMissing,
    AgentSignedOut,
    FeedhqSignedOut,
    Offline,
    Failed,
}

/// An error on its way to a reply: a code and a short plain sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct ProtoError {
    pub code: ErrorCode,
    pub message: String,
}

impl ProtoError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> ProtoError {
        ProtoError { code, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> ProtoError {
        ProtoError::new(ErrorCode::BadRequest, message)
    }

    pub fn not_found() -> ProtoError {
        ProtoError::new(ErrorCode::NotFound, "That chat is gone.")
    }

    pub fn failed(message: impl Into<String>) -> ProtoError {
        ProtoError::new(ErrorCode::Failed, message)
    }
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProtoError {}

// ---------- Objects ----------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// "claude" | "codex" ("feedhq" belongs to the Cloud engine).
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub signed_in: bool,
    pub version: Option<String>,
    pub models: Vec<Model>,
    pub default_model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThreadStatus {
    Idle,
    Running,
    Waiting,
    Failed,
}

impl ThreadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadStatus::Idle => "idle",
            ThreadStatus::Running => "running",
            ThreadStatus::Waiting => "waiting",
            ThreadStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> ThreadStatus {
        match s {
            "running" => ThreadStatus::Running,
            "waiting" => ThreadStatus::Waiting,
            "failed" => ThreadStatus::Failed,
            _ => ThreadStatus::Idle,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    /// Always "mac" from this engine.
    pub engine: String,
    pub title: String,
    pub agent: String,
    pub model: String,
    pub effort: Option<String>,
    pub status: ThreadStatus,
    pub pinned: bool,
    pub settled_at: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub preview: String,
    /// The folder the agent works in (`None`: the engine's default).
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub role: Option<ThreadRole>,
    /// A worker's orchestrator.
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// PROTOCOL.md, "Orchestrator".
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThreadRole {
    Orchestrator,
    Worker,
}

impl ThreadRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadRole::Orchestrator => "orchestrator",
            ThreadRole::Worker => "worker",
        }
    }

    pub fn parse(s: &str) -> Option<ThreadRole> {
        match s {
            "orchestrator" => Some(ThreadRole::Orchestrator),
            "worker" => Some(ThreadRole::Worker),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    User,
    Assistant,
    Reasoning,
    Tool,
    Divider,
    Error,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::User => "user",
            ItemKind::Assistant => "assistant",
            ItemKind::Reasoning => "reasoning",
            ItemKind::Tool => "tool",
            ItemKind::Divider => "divider",
            ItemKind::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Running,
    Done,
    Failed,
    Denied,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub title: String,
    pub detail: Option<String>,
    pub status: ToolStatus,
    pub output: Option<String>,
    pub feedhq: bool,
    /// Orchestrator tools: the worker threads this call started or read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentModel {
    pub agent: String,
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Divider {
    pub from: AgentModel,
    pub to: AgentModel,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub kind: ItemKind,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divider: Option<Divider>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    AllowAlways,
    Deny,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::AllowAlways => "allow_always",
            Decision::Deny => "deny",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub thread_id: String,
    pub item_id: Option<String>,
    pub title: String,
    pub detail: String,
    pub options: Vec<Decision>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnStatus {
    Done,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FeedhqStatus {
    pub connected: bool,
    pub account: Option<String>,
}

// ---------- Events ----------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum Event {
    #[serde(rename = "thread.updated")]
    ThreadUpdated { thread: Thread },
    #[serde(rename = "thread.deleted")]
    ThreadDeleted { thread_id: String },
    #[serde(rename = "item.added")]
    ItemAdded { item: Item },
    #[serde(rename = "item.delta")]
    ItemDelta { thread_id: String, item_id: String, delta: String },
    #[serde(rename = "item.updated")]
    ItemUpdated { item: Item },
    #[serde(rename = "turn.started")]
    TurnStarted { thread_id: String, turn_id: String },
    #[serde(rename = "turn.finished")]
    TurnFinished { thread_id: String, turn_id: String, status: TurnStatus, error: Option<String>, usage: Option<Usage> },
    #[serde(rename = "approval.requested")]
    ApprovalRequested { approval: Approval },
    #[serde(rename = "approval.resolved")]
    ApprovalResolved { thread_id: String, approval_id: String, decision: Decision },
    #[serde(rename = "agents.changed")]
    AgentsChanged { agents: Vec<Agent> },
    #[serde(rename = "feedhq.status")]
    FeedhqStatus { connected: bool, account: Option<String> },
}

// ---------- Method params ----------

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HelloParams {
    #[serde(default)]
    pub client: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsListParams {
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadIdParams {
    pub thread_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCreateParams {
    pub agent: String,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    /// Only "orchestrator" may be asked for; workers are made by orchestrators.
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadsDeleteParams {
    pub thread_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUpdateParams {
    pub thread_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub settled: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSendParams {
    pub thread_id: String,
    pub text: String,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Absent: keep the thread's effort. `null`: no effort.
    #[serde(default, deserialize_with = "present")]
    pub effort: Option<Option<String>>,
}

/// Tells an absent field (`None`) from an explicit `null` (`Some(None)`).
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalAnswerParams {
    pub thread_id: String,
    pub approval_id: String,
    pub decision: Decision,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelopes_match_the_protocol() {
        let r: Request = serde_json::from_value(json!({"v":1,"type":"request","id":"r_12","method":"turn.send","params":{"threadId":"t","text":"hi"},"extra":1})).unwrap();
        assert_eq!(r.method, "turn.send");
        assert_eq!(serde_json::to_value(Reply::ok("r_12", json!({}))).unwrap(), json!({"v":1,"type":"reply","id":"r_12","ok":true,"result":{}}));
        assert_eq!(
            serde_json::to_value(Reply::err("r_12", ProtoError::not_found())).unwrap(),
            json!({"v":1,"type":"reply","id":"r_12","ok":false,"error":{"code":"not_found","message":"That chat is gone."}})
        );
        let e = EventEnvelope::new(Event::ItemDelta { thread_id: "t_1".into(), item_id: "i_9".into(), delta: "Hel".into() });
        assert_eq!(serde_json::to_value(e).unwrap(), json!({"v":1,"type":"event","event":{"type":"item.delta","threadId":"t_1","itemId":"i_9","delta":"Hel"}}));
        let e = Event::TurnFinished { thread_id: "t".into(), turn_id: "u".into(), status: TurnStatus::Done, error: None, usage: Some(Usage { input_tokens: 1, output_tokens: 2, cost_usd: None }) };
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"type":"turn.finished","threadId":"t","turnId":"u","status":"done","error":null,"usage":{"inputTokens":1,"outputTokens":2,"costUsd":null}})
        );
        assert_eq!(serde_json::to_value(ErrorCode::FeedhqSignedOut).unwrap(), json!("feedhq_signed_out"));
        assert_eq!(serde_json::to_value(ErrorCode::AgentSignedOut).unwrap(), json!("agent_signed_out"));
    }

    #[test]
    fn items_leave_out_empty_fields() {
        let item = Item {
            id: "i".into(),
            thread_id: "t".into(),
            turn_id: None,
            kind: ItemKind::Tool,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            text: None,
            agent: Some("claude".into()),
            model: None,
            tool: Some(ToolInfo { name: "Bash".into(), title: "Ran ls".into(), detail: Some("ls".into()), status: ToolStatus::Running, output: None, feedhq: false, threads: vec![] }),
            divider: None,
        };
        assert_eq!(
            serde_json::to_value(&item).unwrap(),
            json!({"id":"i","threadId":"t","turnId":null,"kind":"tool","createdAt":"2026-01-01T00:00:00.000Z","agent":"claude",
                   "tool":{"name":"Bash","title":"Ran ls","detail":"ls","status":"running","output":null,"feedhq":false}})
        );
        let back: Item = serde_json::from_value(serde_json::to_value(&item).unwrap()).unwrap();
        assert_eq!(back, item);
        let mut with_threads = item.clone();
        with_threads.tool.as_mut().unwrap().threads = vec!["t_2".into()];
        assert_eq!(serde_json::to_value(&with_threads).unwrap()["tool"]["threads"], json!(["t_2"]));
    }

    #[test]
    fn turn_send_tells_null_from_absent() {
        let p: TurnSendParams = serde_json::from_value(json!({"threadId":"t","text":"x"})).unwrap();
        assert_eq!(p.effort, None);
        let p: TurnSendParams = serde_json::from_value(json!({"threadId":"t","text":"x","effort":null})).unwrap();
        assert_eq!(p.effort, Some(None));
        let p: TurnSendParams = serde_json::from_value(json!({"threadId":"t","text":"x","effort":"high"})).unwrap();
        assert_eq!(p.effort, Some(Some("high".into())));
    }
}
