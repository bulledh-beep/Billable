//! Which agents are installed and signed in, and their models.
//!
//! Only `--version` and the CLIs' own status commands (`claude auth status`,
//! `codex login status`) run here; nothing is sent to a model and no
//! credential file is opened. Models come from what a session the user
//! started reported last time, else a built-in list.

use std::path::{Path, PathBuf};

use crate::{claude, codex, process::find_binary, protocol::Agent};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Binaries {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Fixed paths (tests, or a user override); otherwise searched for.
    pub claude_bin: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
    /// Also ask the login shell for its PATH.
    pub login_shell: bool,
}

fn locate(fixed: &Option<PathBuf>, name: &str, login_shell: bool) -> Option<PathBuf> {
    match fixed {
        Some(p) => p.exists().then(|| p.clone()),
        None => find_binary(name, login_shell),
    }
}

fn cached_models(cached: &[Agent], id: &str) -> Option<(Vec<crate::protocol::Model>, String)> {
    cached.iter().find(|a| a.id == id && !a.models.is_empty()).map(|a| (a.models.clone(), a.default_model.clone()))
}

fn models_or_fallback(cached: &[Agent], id: &str, fallback: fn() -> Vec<crate::protocol::Model>) -> (Vec<crate::protocol::Model>, String) {
    cached_models(cached, id).unwrap_or_else(|| {
        let m = fallback();
        let d = m[0].id.clone();
        (m, d)
    })
}

/// Sign-in as the CLI's own status command says; unknown (an older CLI):
/// what was known before, else assumed, and a turn's error tells.
fn sign_in(said: Option<bool>, cached: &[Agent], id: &str) -> bool {
    said.or(cached.iter().find(|a| a.id == id && a.installed).map(|a| a.signed_in)).unwrap_or(true)
}

fn claude_agent(bin: Option<&Path>, cached: &[Agent]) -> Agent {
    let Some(bin) = bin else {
        let models = claude::fallback_models();
        return Agent { id: claude::AGENT_ID.into(), name: claude::AGENT_NAME.into(), installed: false, signed_in: false, version: None, default_model: models[0].id.clone(), models };
    };
    let version = claude::version(bin);
    let signed_in = sign_in(claude::signed_in(bin), cached, claude::AGENT_ID);
    let (models, default_model) = models_or_fallback(cached, claude::AGENT_ID, claude::fallback_models);
    Agent { id: claude::AGENT_ID.into(), name: claude::AGENT_NAME.into(), installed: true, signed_in, version, models, default_model }
}

fn codex_agent(bin: Option<&Path>, cached: &[Agent]) -> Agent {
    let Some(bin) = bin else {
        let models = codex::fallback_models();
        return Agent { id: codex::AGENT_ID.into(), name: codex::AGENT_NAME.into(), installed: false, signed_in: false, version: None, default_model: models[0].id.clone(), models };
    };
    let version = codex::version(bin);
    let signed_in = sign_in(codex::signed_in(bin), cached, codex::AGENT_ID);
    let (models, default_model) = models_or_fallback(cached, codex::AGENT_ID, codex::fallback_models);
    Agent { id: codex::AGENT_ID.into(), name: codex::AGENT_NAME.into(), installed: true, signed_in, version, models, default_model }
}

/// The agents, in picker order (Claude Code, Codex), and where they are.
pub fn detect(opts: &Options, cached: &[Agent]) -> (Vec<Agent>, Binaries) {
    let bins = Binaries { claude: locate(&opts.claude_bin, "claude", opts.login_shell), codex: locate(&opts.codex_bin, "codex", opts.login_shell) };
    let (c, x) = std::thread::scope(|s| {
        let c = s.spawn(|| claude_agent(bins.claude.as_deref(), cached));
        let x = s.spawn(|| codex_agent(bins.codex.as_deref(), cached));
        (c.join(), x.join())
    });
    let mut agents = Vec::new();
    agents.extend(c.ok());
    agents.extend(x.ok());
    (agents, bins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_agents_are_listed_as_not_installed() {
        let opts = Options { claude_bin: Some("/nope/claude".into()), codex_bin: Some("/nope/codex".into()), login_shell: false };
        let (agents, bins) = detect(&opts, &[]);
        assert_eq!(bins, Binaries::default());
        assert_eq!(agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["claude", "codex"]);
        assert!(agents.iter().all(|a| !a.installed && !a.signed_in && !a.models.is_empty()));
        assert_eq!(agents[0].default_model, "opus");
    }
}
