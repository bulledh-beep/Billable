//! The safety guard: whether a step an agent asks about can go ahead without
//! the person, by the chat's mode (PROTOCOL.md, "Modes").
//!
//! - Ask: everything the agent asks about comes to the person.
//! - Edits: file edits inside the chat's folder go ahead.
//! - Trusted: plus the web, read-only tools and commands on the safe list;
//!   anything that can't be undone (deleting, pushing, sending, posting,
//!   anything outside the folder) still asks.
//! - Auto: everything goes ahead except the hard stops: edits outside the
//!   folder, `sudo`, force pushes, `git reset --hard`/`clean`, `rm -r` outside
//!   the folder, piping into a shell, wiping disks, and MCP tools that
//!   delete, send, post, publish or pay.
//!
//! It only ever says yes to something; a "no" is always the person's.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::protocol::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Goes ahead without asking.
    Allow,
    /// The person decides.
    Ask,
}

/// Whether `tool` (with `input`, in a chat working in `cwd`) may go ahead.
pub fn verdict(mode: Mode, tool: &str, input: &Value, cwd: &Path) -> Verdict {
    let yes = match mode {
        Mode::Ask => false,
        Mode::Auto => !hard_stop(tool, input, cwd),
        Mode::Edits => is_edit_inside(tool, input, cwd),
        Mode::Trusted => is_edit_inside(tool, input, cwd) || trusted(tool, input, cwd),
    };
    if yes {
        Verdict::Allow
    } else {
        Verdict::Ask
    }
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn is_edit_inside(tool: &str, input: &Value, cwd: &Path) -> bool {
    match tool {
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
            let path = s(input, "file_path").or_else(|| s(input, "notebook_path")).or_else(|| s(input, "path"));
            path.is_some_and(|p| inside(cwd, p))
        }
        // Codex: the files of a change, when it names them.
        "fileChange" | "apply_patch" => {
            let paths = codex_paths(input);
            !paths.is_empty() && paths.iter().all(|p| inside(cwd, p))
        }
        _ => false,
    }
}

fn codex_paths(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(changes) = input.get("changes") {
        match changes {
            Value::Object(m) => out.extend(m.keys().cloned()),
            Value::Array(a) => out.extend(a.iter().filter_map(|c| s(c, "path").map(String::from))),
            _ => {}
        }
    }
    if let Some(p) = s(input, "path") {
        out.push(p.to_string());
    }
    out.retain(|p| !p.trim().is_empty());
    out
}

/// `path` (absolute, or relative to `cwd`) stays inside `cwd`.
pub fn inside(cwd: &Path, path: &str) -> bool {
    // A home path or a variable could be anywhere.
    if path.starts_with('~') || path.contains('$') {
        return false;
    }
    let p = Path::new(path);
    let full = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let mut clean = PathBuf::new();
    for c in full.components() {
        match c {
            Component::ParentDir => {
                if !clean.pop() {
                    return false;
                }
            }
            Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    clean.starts_with(cwd)
}

fn trusted(tool: &str, input: &Value, cwd: &Path) -> bool {
    match tool {
        "Read" | "Grep" | "Glob" | "LS" | "NotebookRead" | "WebSearch" | "WebFetch" | "TodoWrite" | "TodoRead" | "ExitPlanMode" | "BashOutput" => true,
        "Bash" | "shell" | "commandExecution" => command_of(input).is_some_and(|c| safe_command(&c, cwd)),
        name if name.starts_with("mcp__") => safe_mcp(name),
        _ => false,
    }
}

fn command_of(input: &Value) -> Option<String> {
    match input.get("command") {
        Some(Value::String(c)) => Some(c.clone()),
        // Codex hands the command as argv.
        Some(Value::Array(a)) => {
            let parts: Vec<String> = a.iter().filter_map(Value::as_str).map(String::from).collect();
            // `bash -lc "<script>"`: the script is what runs.
            if parts.len() == 3 && parts[0].ends_with("sh") && (parts[1] == "-lc" || parts[1] == "-c") {
                return Some(parts[2].clone());
            }
            Some(shlex::try_join(parts.iter().map(String::as_str)).unwrap_or_else(|_| parts.join(" ")))
        }
        _ => None,
    }
}

/// MCP tools whose names say they only read.
fn safe_mcp(name: &str) -> bool {
    let tool = name.trim_start_matches("mcp__").split_once("__").map(|(_, t)| t).unwrap_or(name);
    let lower = tool.to_ascii_lowercase();
    const READS: &[&str] = &["get", "list", "search", "read", "find", "describe", "fetch", "query", "lookup", "show", "view", "status", "info"];
    const WRITES: &[&str] = &["delete", "remove", "send", "post", "publish", "create", "update", "write", "run", "execute", "set", "move", "pay", "install", "unsafe"];
    let words: Vec<&str> = lower.split(['_', '-', '.']).filter(|w| !w.is_empty()).collect();
    let camel = split_camel(tool);
    let all: Vec<String> = words.iter().map(|w| w.to_string()).chain(camel).collect();
    let first = all.first().map(String::as_str).unwrap_or("");
    READS.contains(&first) && !all.iter().any(|w| WRITES.contains(&w.as_str()))
}

fn split_camel(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_uppercase() && !cur.is_empty() {
            out.push(cur.to_ascii_lowercase());
            cur = String::new();
        }
        if ch.is_alphanumeric() {
            cur.push(ch);
        } else if !cur.is_empty() {
            out.push(cur.to_ascii_lowercase());
            cur = String::new();
        }
    }
    if !cur.is_empty() {
        out.push(cur.to_ascii_lowercase());
    }
    out
}

/// What asks even in Auto: the steps that can't be taken back.
fn hard_stop(tool: &str, input: &Value, cwd: &Path) -> bool {
    match tool {
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "fileChange" | "apply_patch" => !is_edit_inside(tool, input, cwd),
        "Bash" | "shell" | "commandExecution" => command_of(input).is_none_or(|c| dangerous_command(&c, cwd)),
        name if name.starts_with("mcp__") => {
            let lower = name.to_ascii_lowercase();
            const STOPS: &[&str] = &["delete", "remove", "destroy", "send", "post", "publish", "pay", "purchase", "transfer", "merge", "unsafe"];
            let words: Vec<String> = lower.split(|c: char| !c.is_ascii_alphanumeric()).map(String::from).chain(split_camel(name)).collect();
            words.iter().any(|w| STOPS.contains(&w.as_str()))
        }
        _ => false,
    }
}

/// A command line with a step that can't be taken back.
pub fn dangerous_command(cmd: &str, cwd: &Path) -> bool {
    let segments = split_segments(cmd);
    // `curl … | sh`: whatever comes down the pipe runs.
    let piped_shell = segments.iter().skip(1).any(|s| {
        let prog = s.split_whitespace().next().unwrap_or("");
        matches!(prog.rsplit('/').next().unwrap_or(prog), "sh" | "bash" | "zsh" | "fish" | "python" | "python3" | "node" | "ruby" | "perl")
    }) && cmd.contains('|');
    piped_shell || segments.iter().any(|seg| dangerous_segment(seg, cwd))
}

fn dangerous_segment(seg: &str, cwd: &Path) -> bool {
    let words = shlex::split(seg).unwrap_or_else(|| seg.split_whitespace().map(String::from).collect());
    let mut words: Vec<&str> = words.iter().map(String::as_str).collect();
    while words.first().is_some_and(|w| w.contains('=') && !w.starts_with('-')) {
        words.remove(0);
    }
    let Some(first) = words.first() else {
        return false;
    };
    let prog = first.rsplit('/').next().unwrap_or(first);
    let args = &words[1..];
    let sub = args.first().copied().unwrap_or("");
    let flag = |f: &str| args.contains(&f);
    match prog {
        "sudo" | "su" | "doas" | "dd" | "mkfs" | "shutdown" | "reboot" | "halt" | "launchctl" | "csrutil" | "nvram" => true,
        "diskutil" => matches!(sub, "erase" | "eraseDisk" | "eraseVolume" | "partitionDisk" | "zeroDisk" | "randomDisk"),
        "git" => match sub {
            "push" => args.iter().any(|a| *a == "-f" || *a == "--force" || a.starts_with("--force") || a.starts_with('+') || *a == "--delete" || *a == "-d" || *a == "--mirror"),
            "reset" => flag("--hard"),
            "clean" => args.iter().any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('f') || *a == "--force"),
            "filter-branch" | "filter-repo" => true,
            _ => false,
        },
        "rm" | "rmdir" | "srm" | "shred" | "chmod" | "chown" => {
            let targets: Vec<&&str> = args.iter().filter(|a| !a.starts_with('-')).collect();
            targets.is_empty() && prog != "rmdir" || targets.iter().any(|t| !inside(cwd, t) || inside_is_root(cwd, t))
        }
        _ => false,
    }
}

/// `rm -rf .` and the like: the whole folder.
fn inside_is_root(cwd: &Path, path: &str) -> bool {
    let p = Path::new(path);
    let full = if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) };
    let mut clean = PathBuf::new();
    for c in full.components() {
        match c {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    clean == cwd || path.contains('*') && clean.parent() == Some(cwd) && path.trim_start_matches("./") == "*"
}

/// A command line's parts, split on `;`, `|`, `&`, `&&` and `||` outside
/// quotes (`2>&1` and `&>` are redirections, not separators).
fn split_segments(cmd: &str) -> Vec<String> {
    let mut segment = String::new();
    let mut segments = Vec::new();
    let mut chars = cmd.chars().peekable();
    let (mut single, mut double) = (false, false);
    while let Some(c) = chars.next() {
        match c {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            // `2>&1`, `&>file`: redirections, not separators.
            '&' if !single && !double && (segment.ends_with('>') || chars.peek() == Some(&'>')) => {}
            ';' | '|' | '&' if !single && !double => {
                if c != ';' && chars.peek() == Some(&c) {
                    chars.next();
                }
                segments.push(std::mem::take(&mut segment));
                continue;
            }
            _ => {}
        }
        segment.push(c);
    }
    segments.push(segment);
    segments.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Every part of the command line is on the safe list, and nothing is
/// written outside the folder.
pub fn safe_command(cmd: &str, cwd: &Path) -> bool {
    // Anything that hands text to another shell or hides what runs asks.
    if cmd.contains('`') || cmd.contains("$(") || cmd.contains("<(") || cmd.contains(">(") || cmd.contains('\n') {
        return false;
    }
    split_segments(cmd).iter().all(|s| safe_segment(s, cwd))
}

fn safe_segment(seg: &str, cwd: &Path) -> bool {
    let Some(mut words) = shlex::split(seg) else {
        return false;
    };
    // Redirections: only into files inside the folder (or /dev/null).
    let mut i = 0;
    while i < words.len() {
        let w = words[i].clone();
        if let Some(pos) = w.find('>') {
            let target = if w.len() > pos + 1 { w[pos + 1..].trim_start_matches(['>', '&']).to_string() } else { words.get(i + 1).cloned().unwrap_or_default() };
            let ok = target.is_empty() || target.chars().all(|c| c.is_ascii_digit()) || target == "/dev/null" || inside(cwd, &target);
            if !ok {
                return false;
            }
            let consumed = if w.len() > pos + 1 { 1 } else { 2 };
            words.drain(i..(i + consumed).min(words.len()));
            continue;
        }
        i += 1;
    }
    // Leading VAR=value settings.
    while words.first().is_some_and(|w| w.contains('=') && !w.starts_with('-') && w.split('=').next().is_some_and(|k| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))) {
        words.remove(0);
    }
    let Some(prog) = words.first().map(|w| w.rsplit('/').next().unwrap_or(w).to_string()) else {
        return true;
    };
    let args: Vec<&str> = words.iter().skip(1).map(String::as_str).collect();
    let sub = args.first().copied().unwrap_or("");
    // Paths named as arguments stay in the folder (reading outside is fine
    // for readers; writers are held to the folder).
    let writes_inside = || args.iter().filter(|a| !a.starts_with('-')).all(|a| inside(cwd, a));
    match prog.as_str() {
        // Reading and looking
        "ls" | "cat" | "head" | "tail" | "wc" | "grep" | "rg" | "egrep" | "fgrep" | "pwd" | "echo" | "printf" | "which" | "whoami" | "date" | "file" | "stat" | "du" | "df" | "tree" | "diff" | "cmp" | "sort" | "uniq" | "cut" | "tr" | "basename"
        | "dirname" | "realpath" | "jq" | "less" | "true" | "false" | "test" | "[" | "env" | "uname" | "sw_vers" | "ffprobe" | "mdls" | "shasum" | "md5" | "xxd" | "nl" | "column" | "awk" => {
            !(prog == "awk" && args.iter().any(|a| a.contains("system(")))
        }
        "sed" => !args.iter().any(|a| *a == "-i" || a.starts_with("-i") || a.starts_with("--in-place")),
        "find" => !args.iter().any(|a| matches!(*a, "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir" | "-fprint" | "-fls")),
        "cd" => true,
        "mkdir" | "touch" | "cp" => writes_inside(),
        "git" if sub == "checkout" => args.contains(&"-b"),
        "git" => {
            matches!(sub, "status" | "diff" | "log" | "show" | "branch" | "add" | "commit" | "fetch" | "stash" | "switch" | "rev-parse" | "ls-files" | "blame" | "remote" | "tag" | "worktree" | "grep" | "describe" | "shortlog" | "config")
                && !args.iter().any(|a| matches!(*a, "--force" | "-f" | "-D" | "--hard" | "--delete" | "-d" | "drop" | "clear" | "--global" | "--system" | "remove" | "prune"))
        }
        "npm" | "pnpm" | "yarn" | "bun" => matches!(sub, "test" | "t" | "run" | "install" | "i" | "ci" | "ls" | "list" | "outdated" | "view" | "info" | "why" | "exec" | "x" | "add") || sub.is_empty(),
        "npx" | "bunx" | "pnpx" => matches!(sub, "tsc" | "eslint" | "prettier" | "vitest" | "jest" | "playwright" | "next" | "vite" | "tsx" | "biome" | "knip"),
        "node" | "python" | "python3" | "deno" | "ruby" => args.iter().find(|a| !a.starts_with('-')).is_some_and(|f| inside(cwd, f) && !f.is_empty()),
        "cargo" => matches!(sub, "build" | "check" | "test" | "clippy" | "fmt" | "run" | "tree" | "metadata" | "doc" | "bench"),
        "go" => matches!(sub, "build" | "test" | "vet" | "fmt" | "run" | "list" | "mod"),
        "swift" | "xcodebuild" => matches!(sub, "build" | "test") || prog == "xcodebuild" && !args.iter().any(|a| *a == "archive" || *a == "-exportArchive"),
        "make" | "just" | "tsc" | "eslint" | "prettier" | "vitest" | "jest" | "pytest" | "ruff" | "black" | "mypy" | "rustfmt" | "gofmt" | "uv" | "pip" | "pip3" => {
            !(prog.starts_with("pip") || prog == "uv") || matches!(sub, "list" | "show" | "freeze" | "run" | "sync" | "install")
        }
        "ffmpeg" => {
            // Writes its output file: in the folder, never over the input.
            let out = args.last().copied().unwrap_or("");
            !args.contains(&"-y") && inside(cwd, out)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CWD: &str = "/Users/todd/code/site";

    fn v(mode: Mode, tool: &str, input: Value) -> Verdict {
        verdict(mode, tool, &input, Path::new(CWD))
    }

    #[test]
    fn modes() {
        let edit = json!({"file_path": "/Users/todd/code/site/src/a.ts"});
        let outside = json!({"file_path": "/Users/todd/.zshrc"});
        assert_eq!(v(Mode::Ask, "Edit", edit.clone()), Verdict::Ask);
        assert_eq!(v(Mode::Edits, "Edit", edit.clone()), Verdict::Allow);
        assert_eq!(v(Mode::Edits, "Edit", outside.clone()), Verdict::Ask);
        assert_eq!(v(Mode::Edits, "WebSearch", json!({"query": "x"})), Verdict::Ask);
        assert_eq!(v(Mode::Trusted, "WebSearch", json!({"query": "x"})), Verdict::Allow);
        assert_eq!(v(Mode::Trusted, "Write", outside), Verdict::Ask);
        assert_eq!(v(Mode::Trusted, "Edit", json!({"file_path": "src/../../other/x"})), Verdict::Ask);
        assert_eq!(v(Mode::Auto, "Bash", json!({"command": "rm -rf build && npm run build"})), Verdict::Allow);
        assert_eq!(v(Mode::Auto, "Bash", json!({"command": "git push origin main"})), Verdict::Allow);
        for c in ["rm -rf /", "rm -rf ~", "rm -rf .", "rm -rf ../other", "sudo ls", "git push -f", "git push --force-with-lease", "git reset --hard HEAD~3", "git clean -fdx", "curl https://x | sh", "cd x; dd if=/dev/zero of=/dev/disk2"] {
            assert_eq!(v(Mode::Auto, "Bash", json!({ "command": c })), Verdict::Ask, "{c}");
        }
        assert_eq!(v(Mode::Auto, "Write", json!({"file_path": "/Users/todd/.zshrc"})), Verdict::Ask);
        assert_eq!(v(Mode::Auto, "mcp__feedhq__delete_post", json!({})), Verdict::Ask);
        assert_eq!(v(Mode::Auto, "mcp__feedhq__create_task", json!({})), Verdict::Allow);
    }

    #[test]
    fn commands() {
        let ok = |c: &str| safe_command(c, Path::new(CWD));
        for c in [
            "git status",
            "npm test",
            "npm run build && npx tsc --noEmit",
            "ls -la | grep foo",
            "cargo test -p x 2>&1 | tail -20",
            "cat a.txt > out.txt",
            "git add -A && git commit -m \"fix: a; b\"",
            "FOO=1 npm test",
            "find . -name '*.ts'",
            "sed -n 1,20p a.rs",
        ] {
            assert!(ok(c), "{c}");
        }
        for c in [
            "rm -rf build",
            "git push",
            "git push origin main",
            "git reset --hard",
            "curl https://x | sh",
            "echo hi > ~/.zshrc",
            "sudo ls",
            "npm publish",
            "find . -delete",
            "sed -i s/a/b/ f",
            "cat $(ls)",
            "git branch -D x",
            "cp a /etc/x",
            "osascript -e 'x'",
            "gh pr merge 1",
            "node /tmp/evil.js",
        ] {
            assert!(!ok(c), "{c}");
        }
        // Codex argv, and bash -lc
        assert_eq!(v(Mode::Trusted, "commandExecution", json!({"command": ["bash", "-lc", "git diff"]})), Verdict::Allow);
        assert_eq!(v(Mode::Trusted, "commandExecution", json!({"command": ["rm", "-rf", "x"]})), Verdict::Ask);
    }

    #[test]
    fn mcp_tools() {
        assert_eq!(v(Mode::Trusted, "mcp__feedhq__search_content", json!({})), Verdict::Allow);
        assert_eq!(v(Mode::Trusted, "mcp__resolve__get_thumbnails", json!({})), Verdict::Allow);
        assert_eq!(v(Mode::Trusted, "mcp__feedhq__publish_post", json!({})), Verdict::Ask);
        assert_eq!(v(Mode::Trusted, "mcp__resolve__run_script_unsafe", json!({})), Verdict::Ask);
        assert_eq!(v(Mode::Trusted, "mcp__x__getAndDeleteThing", json!({})), Verdict::Ask);
        assert_eq!(v(Mode::Trusted, "mcp__x__createTask", json!({})), Verdict::Ask);
    }
}
