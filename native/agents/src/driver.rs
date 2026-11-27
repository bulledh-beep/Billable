//! What the agent drivers tell the engine, the same for Claude Code and
//! Codex. Keys are the agent's own ids for a text block or tool call; the
//! engine maps them to protocol items.

use serde_json::Value;

use crate::protocol::{Decision, TurnStatus, Usage};

#[derive(Clone, Debug, PartialEq)]
pub enum DriverEvent {
    /// The agent's own session (Claude Code session id, Codex thread id).
    Session(String),
    /// The models the CLI offers, as it said when a session the user
    /// started came up (and the default).
    Models(Vec<crate::protocol::Model>, String),
    TextDelta { key: String, text: String },
    /// The whole text of a block, once it's finished.
    TextDone { key: String, text: String },
    ReasoningDelta { key: String, text: String },
    ReasoningDone { key: String, text: String },
    ToolStarted { key: String, name: String, input: Value },
    ToolFinished { key: String, output: Option<String>, failed: bool, declined: bool },
    /// The agent asks before a step. `native` is what the driver needs to
    /// answer it.
    Approval { native: String, tool_key: Option<String>, name: String, input: Value, options: Vec<Decision> },
    /// The agent withdrew a question it asked.
    ApprovalWithdrawn { native: String },
    Usage(Usage),
    /// A problem worth showing that doesn't end the turn by itself.
    Error(String),
    TurnDone { status: TurnStatus, error: Option<String>, usage: Option<Usage> },
    /// The process ended.
    Exited { error: Option<String> },
}

/// The engine's answer to an approval, as the driver sends it on.
#[derive(Clone, Debug)]
pub struct Answer {
    pub native: String,
    pub decision: Decision,
}
