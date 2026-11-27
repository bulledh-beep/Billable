//! Finding the agent CLIs and running them.
//!
//! Apps opened from the Finder or the Dock don't get the shell's PATH, so
//! `claude` and `codex` (and the `node` an npm install of them needs) are
//! looked for in the usual install folders too, and once in the user's login
//! shell. Every child gets the widened PATH.

use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Install folders worth trying, in order (after PATH).
pub fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(h) = home() {
        for rel in [".local/bin", ".claude/local", ".npm-global/bin", ".bun/bin", ".volta/bin", ".cargo/bin", "Library/pnpm", ".yarn/bin"] {
            dirs.push(h.join(rel));
        }
        // nvm: every installed Node, newest first
        if let Ok(entries) = std::fs::read_dir(h.join(".nvm/versions/node")) {
            let mut versions: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            versions.sort_by_key(|p| std::cmp::Reverse(version_key(p.file_name().and_then(|n| n.to_str()).unwrap_or(""))));
            dirs.extend(versions.into_iter().map(|v| v.join("bin")));
        }
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        dirs.push(PathBuf::from(d));
    }
    dirs
}

/// `v22.11.0` → (22, 11, 0), for sorting.
fn version_key(name: &str) -> (u64, u64, u64) {
    let mut parts = name.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// The login shell's PATH: `None` until asked, then what it said.
static SHELL_PATH: Mutex<Option<Option<String>>> = Mutex::new(None);

/// The PATH of the user's login shell (`$SHELL -lic`), asked once.
fn login_shell_path() -> Option<String> {
    let mut cached = SHELL_PATH.lock().ok()?;
    if let Some(v) = cached.as_ref() {
        return v.clone();
    }
    let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| "/bin/zsh".into());
    // Markers, because interactive shells may print greetings around it.
    let script = "printf '__BILLABLE_PATH__%s__END__' \"$PATH\"";
    let found = run_with_timeout(Command::new(&shell).args(["-lic", script]), Duration::from_secs(5)).and_then(|out| {
        let start = out.stdout.find("__BILLABLE_PATH__")? + "__BILLABLE_PATH__".len();
        let end = out.stdout[start..].find("__END__")? + start;
        Some(out.stdout[start..end].to_string()).filter(|p| !p.is_empty())
    });
    *cached = Some(found.clone());
    found
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Where a program is: PATH, the install folders, then the login shell's
/// PATH (when `login_shell` is allowed).
pub fn find_binary(name: &str, login_shell: bool) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(candidate_dirs());
    if let Some(found) = dirs.iter().map(|d| d.join(name)).find(|p| is_executable(p)) {
        return Some(found);
    }
    if login_shell {
        let shell_path = login_shell_path()?;
        return std::env::split_paths(&OsString::from(shell_path)).map(|d| d.join(name)).find(|p| is_executable(p));
    }
    None
}

/// PATH for children: the binary's own folder, ours, the login shell's (if
/// it was already asked) and the install folders, without repeats.
pub fn child_path(binary: Option<&Path>) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = binary.and_then(Path::parent) {
        dirs.push(dir.to_path_buf());
    }
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    if let Some(p) = SHELL_PATH.lock().ok().and_then(|g| g.clone().flatten()) {
        dirs.extend(std::env::split_paths(&OsString::from(p)));
    }
    dirs.extend(candidate_dirs());
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    std::env::join_paths(dirs).unwrap_or_default()
}

pub struct Output {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run to completion, killing the program if it takes longer than `timeout`.
pub fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
    Some(Output { status, stdout, stderr })
}

fn drain(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// A command for an agent CLI with the widened PATH, in `cwd`.
pub fn agent_command(binary: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(binary);
    cmd.current_dir(cwd).env("PATH", child_path(Some(binary)));
    cmd
}

/// Read a child's stdout line by line on its own thread, calling `on_line`
/// for each and `on_end` once it closes.
pub fn read_lines(
    stdout: impl Read + Send + 'static,
    mut on_line: impl FnMut(String) + Send + 'static,
    on_end: impl FnOnce() + Send + 'static,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) if l.trim().is_empty() => {}
                Ok(l) => on_line(l),
                Err(_) => break,
            }
        }
        on_end();
    });
}

/// Keep the last few KB of a child's stderr (for error messages).
pub fn tail_stderr(stderr: impl Read + Send + 'static) -> std::sync::Arc<Mutex<String>> {
    let tail = std::sync::Arc::new(Mutex::new(String::new()));
    let keep = tail.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(mut t) = keep.lock() {
                t.push_str(&line);
                t.push('\n');
                if t.len() > 8192 {
                    let cut = t.len() - 8192;
                    let cut = (cut..t.len()).find(|i| t.is_char_boundary(*i)).unwrap_or(t.len());
                    t.drain(..cut);
                }
            }
        }
    });
    tail
}

/// The last meaningful line of a stderr tail, for a short error message.
pub fn last_error_line(tail: &str) -> Option<String> {
    tail.lines().rev().map(str::trim).find(|l| !l.is_empty()).map(|l| crate::util::cut(l, 300))
}

/// Stop a child: kill it and reap it so it doesn't linger.
pub fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_programs_and_runs_them() {
        assert!(find_binary("sh", false).is_some());
        assert!(find_binary("definitely-not-a-program-xyz", false).is_none());
        let out = run_with_timeout(Command::new("sh").args(["-c", "echo out; echo err >&2; exit 3"]), Duration::from_secs(5)).unwrap();
        assert_eq!(out.status, Some(3));
        assert_eq!(out.stdout.trim(), "out");
        assert_eq!(out.stderr.trim(), "err");
        let started = Instant::now();
        assert!(run_with_timeout(Command::new("sh").args(["-c", "sleep 5"]), Duration::from_millis(200)).is_none());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn widened_path_has_the_binary_folder_first() {
        let p = child_path(Some(Path::new("/some/where/claude")));
        let dirs: Vec<PathBuf> = std::env::split_paths(&p).collect();
        assert_eq!(dirs[0], PathBuf::from("/some/where"));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert_eq!(version_key("v22.11.3"), (22, 11, 3));
        assert_eq!(last_error_line("a\nError: boom\n\n").as_deref(), Some("Error: boom"));
    }
}
