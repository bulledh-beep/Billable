//! Which agents are installed and signed in, and their models.
//!
//! Two passes: a quick one (where the CLIs are, versions, sign-in) whose
//! models come from the last full pass or a built-in list, and a full one
//! that also asks each CLI for its models.

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
    /// Where probes run.
    pub cwd: PathBuf,
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

fn claude_agent(bin: Option<&Path>, opts: &Options, probe: bool, cached: &[Agent]) -> Agent {
    let Some(bin) = bin else {
        let models = claude::fallback_models();
        return Agent { id: claude::AGENT_ID.into(), name: claude::AGENT_NAME.into(), installed: false, signed_in: false, version: None, default_model: models[0].id.clone(), models };
    };
    let version = claude::version(bin);
    // Unknown (an older CLI without `auth status`): let a turn find out.
    let signed_in = claude::signed_in(bin).unwrap_or(true);
    let (models, default_model) = probe
        .then(|| claude::probe_models(bin, &opts.cwd))
        .flatten()
        .or_else(|| cached_models(cached, claude::AGENT_ID))
        .unwrap_or_else(|| {
            let m = claude::fallback_models();
            let d = m[0].id.clone();
            (m, d)
        });
    Agent { id: claude::AGENT_ID.into(), name: claude::AGENT_NAME.into(), installed: true, signed_in, version, models, default_model }
}

fn codex_agent(bin: Option<&Path>, opts: &Options, probe: bool, cached: &[Agent]) -> Agent {
    let Some(bin) = bin else {
        let models = codex::fallback_models();
        return Agent { id: codex::AGENT_ID.into(), name: codex::AGENT_NAME.into(), installed: false, signed_in: false, version: None, default_model: models[0].id.clone(), models };
    };
    let version = codex::version(bin);
    let (signed_in, probed) = if probe {
        let p = codex::probe(bin, &opts.cwd);
        (p.signed_in, p.models)
    } else {
        (None, None)
    };
    let previous = cached.iter().find(|a| a.id == codex::AGENT_ID);
    let signed_in = signed_in.or(previous.filter(|a| a.installed).map(|a| a.signed_in)).unwrap_or(true);
    let (models, default_model) = probed.or_else(|| cached_models(cached, codex::AGENT_ID)).unwrap_or_else(|| {
        let m = codex::fallback_models();
        let d = m[0].id.clone();
        (m, d)
    });
    Agent { id: codex::AGENT_ID.into(), name: codex::AGENT_NAME.into(), installed: true, signed_in, version, models, default_model }
}

/// The agents, in picker order (Claude Code, Codex), and where they are.
pub fn detect(opts: &Options, probe: bool, cached: &[Agent]) -> (Vec<Agent>, Binaries) {
    let bins = Binaries { claude: locate(&opts.claude_bin, "claude", opts.login_shell), codex: locate(&opts.codex_bin, "codex", opts.login_shell) };
    let (c, x) = std::thread::scope(|s| {
        let c = s.spawn(|| claude_agent(bins.claude.as_deref(), opts, probe, cached));
        let x = s.spawn(|| codex_agent(bins.codex.as_deref(), opts, probe, cached));
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
        let opts = Options { claude_bin: Some("/nope/claude".into()), codex_bin: Some("/nope/codex".into()), login_shell: false, cwd: std::env::temp_dir() };
        let (agents, bins) = detect(&opts, true, &[]);
        assert_eq!(bins, Binaries::default());
        assert_eq!(agents.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["claude", "codex"]);
        assert!(agents.iter().all(|a| !a.installed && !a.signed_in && !a.models.is_empty()));
        assert_eq!(agents[0].default_model, "opus");
    }
}
