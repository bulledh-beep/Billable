//! The person's own MCP servers (PROTOCOL.md, "Tools"): what Claude Code
//! and Codex load on this Mac, and what the Claude app has, ready to copy
//! over. Everything is read from the agents' own config files; anything is
//! changed only when the person asks on the Tools screen, and only with the
//! agents' own `mcp add` and `mcp remove` commands, so Billable never edits
//! their files itself. Nothing here starts a model.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::process::{agent_command, run_with_timeout};

/// The agents' own `mcp add` / `remove` must answer within this.
const CLI_LIMIT: Duration = Duration::from_secs(30);
/// `claude mcp list` starts every server to check it.
const CHECK_LIMIT: Duration = Duration::from_secs(90);
/// The Claude app's own Node, for an extension written for it.
const CLAUDE_APP_BINARY: &str = "/Applications/Claude.app/Contents/MacOS/Claude";

/// One server as the Tools screen lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    /// `<agent>:<scope>:<name>`, stable while the config doesn't change.
    pub id: String,
    pub name: String,
    /// "claude", "codex", or "app" (the Claude app, to copy over).
    pub agent: String,
    /// "user" (every folder), "local" (this folder, Claude Code's own
    /// list), "project" (the folder's .mcp.json), or "app".
    pub scope: String,
    /// "stdio", "http" or "sse".
    pub transport: String,
    /// The command line or the address, one line.
    pub detail: String,
    /// "connected", "failed", "needs_auth", "off" or "unknown".
    pub status: String,
    pub error: Option<String>,
    /// App servers: the agents that already have a server by this name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copied_to: Option<Vec<String>>,
}

/// Claude Code servers' last known state: name → (status, error).
pub type Statuses = BTreeMap<String, (String, Option<String>)>;

/// How to start a server: a command, or an address.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spec {
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub url: Option<String>,
    pub transport: String,
}

impl Spec {
    fn detail(&self) -> String {
        match (&self.url, &self.command) {
            (Some(u), _) => u.clone(),
            (None, Some(c)) => std::iter::once(c.as_str()).chain(self.args.iter().map(String::as_str)).map(quote).collect::<Vec<_>>().join(" "),
            _ => String::new(),
        }
    }

    /// Claude Code's `mcp add-json` value.
    pub fn claude_json(&self) -> Value {
        match &self.url {
            Some(url) => json!({ "type": if self.transport == "sse" { "sse" } else { "http" }, "url": url }),
            None => {
                let mut v = json!({ "type": "stdio", "command": self.command.clone().unwrap_or_default(), "args": self.args });
                if !self.env.is_empty() {
                    v["env"] = json!(self.env);
                }
                v
            }
        }
    }

    /// Codex's `mcp add` arguments after the name.
    pub fn codex_args(&self) -> Vec<String> {
        match &self.url {
            Some(url) => vec!["--url".into(), url.clone()],
            None => {
                let mut a = Vec::new();
                for (k, v) in &self.env {
                    a.push("--env".into());
                    a.push(format!("{k}={v}"));
                }
                a.push("--".into());
                a.push(self.command.clone().unwrap_or_default());
                a.extend(self.args.iter().cloned());
                a
            }
        }
    }
}

fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Where everything lives. `home` is the person's home folder.
#[derive(Clone, Debug)]
pub struct Places {
    pub home: PathBuf,
    pub claude_bin: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
}

impl Places {
    pub fn from_env(claude_bin: Option<PathBuf>, codex_bin: Option<PathBuf>) -> Places {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        Places { home, claude_bin, codex_bin }
    }

    fn app_support(&self) -> PathBuf {
        self.home.join("Library/Application Support/Claude")
    }
}

/// A server name the agents accept: letters, digits, `-` and `_`.
pub fn clean_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(60).collect()
}

fn spec_from_json(v: &Value) -> Spec {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let url = s("url");
    let kind = s("type").unwrap_or_else(|| if url.is_some() { "http".into() } else { "stdio".into() });
    Spec {
        command: s("command"),
        args: v.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
        env: v.get("env").and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default(),
        transport: match kind.as_str() {
            "sse" => "sse".into(),
            "http" | "streamable-http" | "streamable_http" => "http".into(),
            _ => if url.is_some() { "http".into() } else { "stdio".into() },
        },
        url,
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn server(agent: &str, scope: &str, name: &str, spec: &Spec, status: &str) -> McpServer {
    McpServer {
        id: format!("{agent}:{scope}:{name}"),
        name: name.to_string(),
        agent: agent.to_string(),
        scope: scope.to_string(),
        transport: spec.transport.clone(),
        detail: spec.detail(),
        status: status.to_string(),
        error: None,
        copied_to: None,
    }
}

/// Claude Code's servers for `cwd`: its user list and this folder's local
/// list (both in ~/.claude.json), then the folder's .mcp.json.
pub fn claude_specs(places: &Places, cwd: Option<&Path>) -> Vec<(String, String, Spec)> {
    let mut out = Vec::new();
    let config = read_json(&places.home.join(".claude.json"));
    let add = |out: &mut Vec<(String, String, Spec)>, scope: &str, servers: Option<&Value>| {
        for (name, v) in servers.and_then(Value::as_object).into_iter().flatten() {
            out.push((scope.to_string(), name.clone(), spec_from_json(v)));
        }
    };
    add(&mut out, "user", config.as_ref().and_then(|c| c.get("mcpServers")));
    if let Some(cwd) = cwd {
        let key = cwd.to_string_lossy();
        add(&mut out, "local", config.as_ref().and_then(|c| c.pointer("/projects").and_then(|p| p.get(key.as_ref()))).and_then(|p| p.get("mcpServers")));
        add(&mut out, "project", read_json(&cwd.join(".mcp.json")).as_ref().and_then(|c| c.get("mcpServers")));
    }
    out
}

/// Codex's servers, from ~/.codex/config.toml (`$CODEX_HOME` if set).
pub fn codex_specs(places: &Places) -> Vec<(String, Spec, bool)> {
    let dir = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| places.home.join(".codex"));
    let Ok(text) = std::fs::read_to_string(dir.join("config.toml")) else { return Vec::new() };
    codex_specs_from(&text)
}

pub fn codex_specs_from(text: &str) -> Vec<(String, Spec, bool)> {
    let Ok(doc) = text.parse::<toml::Table>() else { return Vec::new() };
    let Some(servers) = doc.get("mcp_servers").and_then(toml::Value::as_table) else { return Vec::new() };
    servers
        .iter()
        .filter_map(|(name, v)| {
            let t = v.as_table()?;
            let s = |k: &str| t.get(k).and_then(toml::Value::as_str).map(str::to_string);
            let url = s("url");
            let spec = Spec {
                command: s("command"),
                args: t.get("args").and_then(toml::Value::as_array).map(|a| a.iter().filter_map(toml::Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
                env: t.get("env").and_then(toml::Value::as_table).map(|e| e.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default(),
                transport: if url.is_some() { "http".into() } else { "stdio".into() },
                url,
            };
            let enabled = t.get("enabled").and_then(toml::Value::as_bool).unwrap_or(true);
            Some((name.clone(), spec, enabled))
        })
        .collect()
}

/// The Claude app's servers: its config file's list, then its installed
/// extensions (each with its settings filled in), skipping turned-off ones.
pub fn app_specs(places: &Places) -> Vec<(String, Spec)> {
    let mut out = Vec::new();
    let base = places.app_support();
    if let Some(cfg) = read_json(&base.join("claude_desktop_config.json")) {
        for (name, v) in cfg.get("mcpServers").and_then(Value::as_object).into_iter().flatten() {
            out.push((name.clone(), spec_from_json(v)));
        }
    }
    let Ok(entries) = std::fs::read_dir(base.join("Claude Extensions")) else { return out };
    let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for dir in dirs {
        let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let Some(manifest) = read_json(&dir.join("manifest.json")) else { continue };
        let settings = read_json(&base.join("Claude Extensions Settings").join(format!("{id}.json"))).unwrap_or(json!({}));
        if settings.get("isEnabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        if let Some(spec) = extension_spec(&manifest, &settings, &dir, &places.home) {
            let name = manifest.get("display_name").or_else(|| manifest.get("name")).and_then(Value::as_str).unwrap_or(&id).to_string();
            out.push((name, spec));
        }
    }
    out
}

/// An extension's start command, with `${__dirname}`, `${HOME}` and the
/// person's settings (`${user_config.x}`) filled in.
pub fn extension_spec(manifest: &Value, settings: &Value, dir: &Path, home: &Path) -> Option<Spec> {
    let server = manifest.get("server")?;
    let mut cfg = server.get("mcp_config")?.clone();
    if let Some(over) = cfg.pointer("/platform_overrides/darwin").cloned() {
        for (k, v) in over.as_object().into_iter().flatten() {
            cfg[k] = v.clone();
        }
    }
    let user = settings.get("userConfig").cloned().unwrap_or(json!({}));
    let defaults = manifest.get("user_config").cloned().unwrap_or(json!({}));
    let value_of = |key: &str| -> Option<Value> {
        user.get(key).cloned().filter(|v| !v.is_null()).or_else(|| defaults.get(key).and_then(|d| d.get("default")).cloned())
    };
    let fill = |s: &str| -> String {
        let mut out = s
            .replace("${__dirname}", &dir.to_string_lossy())
            .replace("${HOME}", &home.to_string_lossy())
            .replace("${DESKTOP}", &home.join("Desktop").to_string_lossy())
            .replace("${DOCUMENTS}", &home.join("Documents").to_string_lossy())
            .replace("${DOWNLOADS}", &home.join("Downloads").to_string_lossy())
            .replace("${pathSeparator}", "/")
            .replace("${/}", "/");
        while let Some(start) = out.find("${user_config.") {
            let Some(end) = out[start..].find('}') else { break };
            let key = out[start + 14..start + end].to_string();
            let v = match value_of(&key) {
                Some(Value::String(s)) => s,
                Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(","),
                Some(Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            out.replace_range(start..start + end + 1, &v);
        }
        out
    };
    let mut args = Vec::new();
    for a in cfg.get("args").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        // A list setting standing alone becomes one argument per entry.
        if let Some(key) = a.strip_prefix("${user_config.").and_then(|k| k.strip_suffix('}')) {
            if let Some(Value::Array(list)) = value_of(key) {
                args.extend(list.iter().filter_map(Value::as_str).map(str::to_string));
                continue;
            }
        }
        args.push(fill(a));
    }
    let env: BTreeMap<String, String> = cfg.get("env").and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), fill(v.as_str()?)))).collect()).unwrap_or_default();
    let command = cfg.get("command").and_then(Value::as_str).map(fill)?;
    Some(Spec { command: Some(command), args, env, url: None, transport: "stdio".into() })
}

/// An app server's command as the agents should run it: an extension
/// written for the app's built-in Node runs on the app itself when this
/// Mac has no Node of its own.
pub fn runnable(mut spec: Spec, node_on_path: bool) -> Spec {
    if spec.command.as_deref() == Some("node") && !node_on_path && Path::new(CLAUDE_APP_BINARY).exists() {
        spec.command = Some(CLAUDE_APP_BINARY.into());
        spec.env.insert("ELECTRON_RUN_AS_NODE".into(), "1".into());
    }
    spec
}

/// Everything the Tools screen lists, with each Claude Code server's last
/// known state (`claude_status`: name → status, from a chat's start or a
/// check).
pub fn list(places: &Places, cwd: Option<&Path>, claude_status: &Statuses) -> Vec<McpServer> {
    let mut out: Vec<McpServer> = Vec::new();
    if places.claude_bin.is_some() {
        for (scope, name, spec) in claude_specs(places, cwd) {
            let (status, error) = claude_status.get(&name).cloned().unwrap_or(("unknown".into(), None));
            let mut s = server("claude", &scope, &name, &spec, &status);
            s.error = error;
            out.push(s);
        }
    }
    if places.codex_bin.is_some() {
        for (name, spec, enabled) in codex_specs(places) {
            out.push(server("codex", "user", &name, &spec, if enabled { "unknown" } else { "off" }));
        }
    }
    let has = |agent: &str, name: &str| out.iter().any(|s| s.agent == agent && clean_name(&s.name) == clean_name(name));
    let mut app = Vec::new();
    for (name, spec) in app_specs(places) {
        let mut s = server("app", "app", &name, &spec, "unknown");
        let copied: Vec<String> = ["claude", "codex"].iter().filter(|a| has(a, &name)).map(|a| a.to_string()).collect();
        s.copied_to = Some(copied);
        app.push(s);
    }
    out.extend(app);
    out
}

/// A Claude Code status word from its own (`connected`, `needs-auth`…).
pub fn status_word(s: &str) -> &'static str {
    match s.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
        "connected" => "connected",
        "failed" | "failed_to_connect" | "error" => "failed",
        "needs_auth" | "needs_authentication" => "needs_auth",
        "disabled" => "off",
        _ => "unknown",
    }
}

/// `claude mcp list`'s lines: `name: <command> - ✓ Connected`.
pub fn parse_claude_list(out: &str) -> Statuses {
    let mut map = BTreeMap::new();
    for line in out.lines() {
        let Some((name, rest)) = line.split_once(": ") else { continue };
        let Some((_, state)) = rest.rsplit_once(" - ") else { continue };
        let state = state.trim_start_matches(|c: char| !c.is_ascii_alphabetic()).trim();
        let word = status_word(state);
        let word = if word == "unknown" && state.to_ascii_lowercase().contains("fail") { "failed" } else if word == "unknown" && state.to_ascii_lowercase().contains("auth") { "needs_auth" } else { word };
        if word != "unknown" {
            map.insert(name.trim().to_string(), (word.to_string(), (word == "failed").then(|| state.to_string())));
        }
    }
    map
}

/// Ask Claude Code to start and check every server (`claude mcp list`).
pub fn check_claude(places: &Places, cwd: &Path) -> Result<Statuses, String> {
    let bin = places.claude_bin.as_ref().ok_or("Claude Code isn't installed on this Mac.")?;
    let out = run_with_timeout(agent_command(bin, cwd).args(["mcp", "list"]), CHECK_LIMIT).ok_or("Claude Code took too long to check its tools.")?;
    Ok(parse_claude_list(&out.stdout))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = run_with_timeout(cmd, CLI_LIMIT).ok_or("It took too long.")?;
    if out.status == Some(0) {
        Ok(())
    } else {
        let msg = if out.stderr.trim().is_empty() { out.stdout } else { out.stderr };
        Err(crate::util::one_line(msg.trim(), 300))
    }
}

/// Add a server to an agent with its own command. `scope` "local" (Claude
/// Code only) adds it for `cwd`; anything else for every folder.
pub fn add(places: &Places, agent: &str, name: &str, spec: &Spec, scope: &str, cwd: &Path) -> Result<(), String> {
    let name = clean_name(name);
    if name.is_empty() {
        return Err("Give it a name.".into());
    }
    match agent {
        "claude" => {
            let bin = places.claude_bin.as_ref().ok_or("Claude Code isn't installed on this Mac.")?;
            let scope = if scope == "local" { "local" } else { "user" };
            run(agent_command(bin, cwd).args(["mcp", "add-json", "--scope", scope, &name, &spec.claude_json().to_string()]))
        }
        "codex" => {
            let bin = places.codex_bin.as_ref().ok_or("Codex isn't installed on this Mac.")?;
            let mut args = vec!["mcp".to_string(), "add".into(), name];
            args.extend(spec.codex_args());
            run(agent_command(bin, cwd).args(args))
        }
        _ => Err("Pick Claude Code or Codex.".into()),
    }
}

/// Remove a server, by its id, with the agent's own command.
pub fn remove(places: &Places, id: &str, cwd: &Path) -> Result<(), String> {
    let mut parts = id.splitn(3, ':');
    let (agent, scope, name) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    match agent {
        "claude" => {
            let bin = places.claude_bin.as_ref().ok_or("Claude Code isn't installed on this Mac.")?;
            if scope == "project" {
                return Err("This one is in the folder's .mcp.json. Remove it there.".into());
            }
            run(agent_command(bin, cwd).args(["mcp", "remove", "--scope", scope, name]))
        }
        "codex" => {
            let bin = places.codex_bin.as_ref().ok_or("Codex isn't installed on this Mac.")?;
            run(agent_command(bin, cwd).args(["mcp", "remove", name]))
        }
        _ => Err("Servers in the Claude app are changed in the Claude app.".into()),
    }
}

/// Copy one of the Claude app's servers to the agents asked for.
pub fn import(places: &Places, id: &str, agents: &[String], cwd: &Path, node_on_path: bool) -> Result<(), String> {
    let name = id.strip_prefix("app:app:").ok_or("That isn't one of the Claude app's.")?;
    let (_, spec) = app_specs(places).into_iter().find(|(n, _)| n == name).ok_or("The Claude app doesn't have that one any more.")?;
    let spec = runnable(spec, node_on_path);
    let mut errors = Vec::new();
    for agent in agents {
        if let Err(e) = add(places, agent, name, &spec, "user", cwd) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" "))
    }
}

/// A server the person typed: an address, or a command line.
pub fn spec_from_input(command: Option<&str>, url: Option<&str>, env: &BTreeMap<String, String>) -> Result<Spec, String> {
    if let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("The address should start with https://.".into());
        }
        return Ok(Spec { url: Some(url.to_string()), transport: if url.ends_with("/sse") { "sse".into() } else { "http".into() }, ..Spec::default() });
    }
    let line = command.map(str::trim).filter(|c| !c.is_empty()).ok_or("Type the command that starts it, or its address.")?;
    let mut words = shlex::split(line).ok_or("That command has an unclosed quote.")?.into_iter();
    let first = words.next().ok_or("Type the command that starts it.")?;
    Ok(Spec { command: Some(first), args: words.collect(), env: env.clone(), url: None, transport: "stdio".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home() -> PathBuf {
        let d = std::env::temp_dir().join(format!("billable-servers-{}", crate::util::new_id("h")));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_claude_code_user_local_and_project_servers() {
        let home = temp_home();
        let site = home.join("site");
        std::fs::create_dir_all(&site).unwrap();
        let key = site.to_string_lossy().to_string();
        std::fs::write(
            home.join(".claude.json"),
            json!({"oauthAccount": {"x": 1}, "mcpServers": {"resolve": {"type": "stdio", "command": "uvx", "args": ["resolve-mcp"]}}, "projects": {key: {"mcpServers": {"linear": {"type": "http", "url": "https://mcp.linear.app/mcp"}}}}}).to_string(),
        )
        .unwrap();
        std::fs::write(site.join(".mcp.json"), json!({"mcpServers": {"db": {"command": "npx", "args": ["-y", "db mcp"]}}}).to_string()).unwrap();
        let places = Places { home: home.clone(), claude_bin: Some("/bin/true".into()), codex_bin: None };
        let mut status = BTreeMap::new();
        status.insert("resolve".to_string(), ("connected".to_string(), None));
        let list = list(&places, Some(&site), &status);
        let ids: Vec<&str> = list.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["claude:user:resolve", "claude:local:linear", "claude:project:db"]);
        assert_eq!(list[0].status, "connected");
        assert_eq!(list[0].detail, "uvx resolve-mcp");
        assert_eq!(list[1].transport, "http");
        assert_eq!(list[2].detail, "npx -y 'db mcp'");
    }

    #[test]
    fn reads_codex_servers_from_its_toml() {
        let got = codex_specs_from("model = \"x\"\n[mcp_servers.resolve]\ncommand = \"uvx\"\nargs = [\"a\"]\nenv = { K = \"v\" }\n[mcp_servers.web]\nurl = \"https://h/mcp\"\nenabled = false\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "resolve");
        assert_eq!(got[0].1.codex_args(), ["--env", "K=v", "--", "uvx", "a"]);
        assert_eq!(got[1].1.url.as_deref(), Some("https://h/mcp"));
        assert!(!got[1].2);
        assert!(codex_specs_from("not = [toml").is_empty());
    }

    #[test]
    fn fills_in_an_app_extensions_command_and_settings() {
        let home = temp_home();
        let base = home.join("Library/Application Support/Claude");
        let ext = base.join("Claude Extensions/com.example.resolve");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::create_dir_all(base.join("Claude Extensions Settings")).unwrap();
        std::fs::write(
            ext.join("manifest.json"),
            json!({"name": "resolve", "display_name": "DaVinci Resolve Studio", "server": {"type": "node", "mcp_config": {"command": "node", "args": ["${__dirname}/server/index.js", "${user_config.dirs}", "--port=${user_config.port}"], "env": {"RESOLVE": "${HOME}/Resolve"}}}, "user_config": {"port": {"type": "number", "default": 9000}}}).to_string(),
        )
        .unwrap();
        std::fs::write(base.join("Claude Extensions Settings/com.example.resolve.json"), json!({"isEnabled": true, "userConfig": {"dirs": ["/a", "/b"]}}).to_string()).unwrap();
        let off = base.join("Claude Extensions/com.example.off");
        std::fs::create_dir_all(&off).unwrap();
        std::fs::write(off.join("manifest.json"), json!({"name": "off", "server": {"mcp_config": {"command": "x"}}}).to_string()).unwrap();
        std::fs::write(base.join("Claude Extensions Settings/com.example.off.json"), json!({"isEnabled": false}).to_string()).unwrap();
        std::fs::write(base.join("claude_desktop_config.json"), json!({"mcpServers": {"files": {"command": "npx", "args": ["fs"]}}}).to_string()).unwrap();

        let specs = app_specs(&Places { home: home.clone(), claude_bin: None, codex_bin: None });
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].0, "files");
        let (name, spec) = &specs[1];
        assert_eq!(name, "DaVinci Resolve Studio");
        assert_eq!(spec.args, [format!("{}/server/index.js", ext.display()), "/a".into(), "/b".into(), "--port=9000".into()]);
        assert_eq!(spec.env["RESOLVE"], format!("{}/Resolve", home.display()));
        assert_eq!(clean_name(name), "davinci-resolve-studio");
        let claude = spec.claude_json();
        assert_eq!(claude["type"], "stdio");
        assert_eq!(claude["command"], "node");
    }

    #[test]
    fn app_servers_say_where_they_were_copied() {
        let home = temp_home();
        let base = home.join("Library/Application Support/Claude");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("claude_desktop_config.json"), json!({"mcpServers": {"Control Chrome": {"command": "node", "args": ["c.js"]}}}).to_string()).unwrap();
        std::fs::write(home.join(".claude.json"), json!({"mcpServers": {"control-chrome": {"command": "node", "args": ["c.js"]}}}).to_string()).unwrap();
        let places = Places { home, claude_bin: Some("/bin/true".into()), codex_bin: None };
        let list = list(&places, None, &BTreeMap::new());
        let app = list.iter().find(|s| s.agent == "app").unwrap();
        assert_eq!(app.id, "app:app:Control Chrome");
        assert_eq!(app.copied_to.as_deref(), Some(&["claude".to_string()][..]));
    }

    #[test]
    fn parses_claude_mcp_list() {
        let out = "Checking MCP server health...\n\nresolve: uvx resolve-mcp - ✓ Connected\nlinear: https://mcp.linear.app/mcp (HTTP) - ⚠ Needs authentication\nbroken: node x.js - ✗ Failed to connect\n";
        let m = parse_claude_list(out);
        assert_eq!(m["resolve"].0, "connected");
        assert_eq!(m["linear"].0, "needs_auth");
        assert_eq!(m["broken"].0, "failed");
        assert!(m["broken"].1.is_some());
    }

    #[test]
    fn typed_servers() {
        let env = BTreeMap::new();
        let s = spec_from_input(Some("uvx  davinci-resolve-mcp --flag 'a b'"), None, &env).unwrap();
        assert_eq!(s.command.as_deref(), Some("uvx"));
        assert_eq!(s.args, ["davinci-resolve-mcp", "--flag", "a b"]);
        let u = spec_from_input(None, Some(" https://h/sse "), &env).unwrap();
        assert_eq!(u.transport, "sse");
        assert_eq!(u.claude_json(), json!({"type": "sse", "url": "https://h/sse"}));
        assert!(spec_from_input(None, Some("ftp://x"), &env).is_err());
        assert!(spec_from_input(Some("a 'b"), None, &env).is_err());
        assert!(spec_from_input(None, None, &env).is_err());
    }

    #[test]
    fn app_node_extensions_run_on_the_app_without_node() {
        let spec = Spec { command: Some("node".into()), ..Spec::default() };
        assert_eq!(runnable(spec.clone(), true).command.as_deref(), Some("node"));
        // Only on a Mac with the Claude app does it switch.
        let swapped = runnable(spec, false);
        if Path::new(CLAUDE_APP_BINARY).exists() {
            assert_eq!(swapped.env["ELECTRON_RUN_AS_NODE"], "1");
        } else {
            assert_eq!(swapped.command.as_deref(), Some("node"));
        }
    }
}
