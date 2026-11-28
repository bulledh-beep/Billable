//! Billable's agent engine.

pub mod attachments;
pub mod claude;
pub mod codex;
pub mod commands;
pub mod computer;
pub mod discovery;
pub mod driver;
pub mod engine;
pub mod guard;
pub mod handoff;
pub mod http;
pub mod oauth;
pub mod orchestrator;
pub mod process;
pub mod protocol;
pub mod relay;
pub mod remote;
pub mod secrets;
pub mod servers;
pub mod store;
pub mod tools;
pub mod util;

pub use engine::{Engine, EngineConfig, FolderAnswer, FolderPicker, PickKind};
pub use oauth::{CLIENT_ID, REDIRECT_URI};
pub use remote::{RemoteEngine, RemoteLink, RemoteState};
pub use secrets::{MemorySecrets, SecretStore, KEYCHAIN_SERVICE};
