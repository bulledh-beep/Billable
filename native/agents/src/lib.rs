//! Billable's agent engine.

pub mod claude;
pub mod codex;
pub mod discovery;
pub mod driver;
pub mod handoff;
pub mod http;
pub mod oauth;
pub mod process;
pub mod protocol;
pub mod relay;
pub mod secrets;
pub mod store;
pub mod tools;
pub mod util;
pub mod engine;
pub mod remote;

pub use engine::{Engine, EngineConfig};
pub use oauth::{CLIENT_ID, REDIRECT_URI};
pub use remote::{RemoteEngine, RemoteLink, RemoteState};
pub use secrets::{MemorySecrets, SecretStore, KEYCHAIN_SERVICE};
