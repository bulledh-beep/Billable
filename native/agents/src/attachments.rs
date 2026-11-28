//! Files, folders and images a person attaches to a message (PROTOCOL.md,
//! "Attachments"), and the `@` file search.
//!
//! Every attachment ends up as a path on this Mac: one the person picked
//! here, bytes the page sent (saved under the chat's attachments folder), or
//! a FeedHQ upload fetched from its signed link before the turn starts. The
//! agent gets the paths in the message, and images as images too (Claude
//! Code: image blocks; Codex: `localImage` inputs).

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::process::run_with_timeout;

/// At most this many attachments per message.
pub const MAX_ATTACHMENTS: usize = 20;
/// Bytes sent with the message itself (inside Billable): at most this.
pub const MAX_INLINE: usize = 40 * 1024 * 1024;
/// A FeedHQ upload: at most this.
pub const MAX_DOWNLOAD: u64 = 100 * 1024 * 1024;
/// Images bigger than this go to the agent by path only.
pub const MAX_IMAGE_BLOCK: u64 = 5 * 1024 * 1024;
/// A preview the page made (a data: URL): at most this long.
pub const MAX_THUMB: usize = 64 * 1024;

/// An attachment as the page sends it with `turn.send`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentIn {
    pub name: String,
    /// "image", "file" or "folder" (worked out from the path when left out).
    #[serde(default)]
    pub kind: Option<String>,
    /// A path on this Mac.
    #[serde(default)]
    pub path: Option<String>,
    /// The file itself, base64.
    #[serde(default)]
    pub data: Option<String>,
    /// A FeedHQ upload's signed link.
    #[serde(default)]
    pub url: Option<String>,
    /// A small `data:image/…` preview for the chat.
    #[serde(default)]
    pub thumb: Option<String>,
}

/// An attachment as the chat shows it (the user item's `attachments`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    pub kind: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
}

/// What a message's attachments came to: what the chat shows, and the
/// uploads still to fetch (link → where it goes).
#[derive(Debug, Default)]
pub struct Prepared {
    pub shown: Vec<Attachment>,
    pub downloads: Vec<(String, PathBuf)>,
}

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

fn mime_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

/// A file name that's safe to save: no folders, no hidden-file dot, short.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base.chars().map(|c| if c.is_control() || c == ':' { '-' } else { c }).collect();
    let trimmed = cleaned.trim().trim_start_matches('.').to_string();
    let mut out: String = trimmed.chars().take(120).collect();
    if out.is_empty() {
        out = "attachment".into();
    }
    out
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
        None => PathBuf::from(path),
    }
}

/// A path in `dir` for `name` that isn't taken yet.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

fn thumb_of(a: &AttachmentIn) -> Option<String> {
    a.thumb.clone().filter(|t| t.starts_with("data:image/") && t.len() <= MAX_THUMB)
}

/// Check a message's attachments and save the bytes that came with it into
/// `dir` (the chat's attachments folder). Uploads are only planned here.
pub fn prepare(list: &[AttachmentIn], dir: &Path) -> Result<Prepared, String> {
    if list.len() > MAX_ATTACHMENTS {
        return Err(format!("Attach at most {MAX_ATTACHMENTS} at a time."));
    }
    let mut out = Prepared::default();
    for a in list {
        let name = safe_name(&a.name);
        if let Some(p) = a.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            let path = expand(p);
            let meta = std::fs::metadata(&path).map_err(|_| format!("{name} isn't on this Mac any more."))?;
            let kind = if meta.is_dir() { "folder" } else if is_image(&path) { "image" } else { "file" };
            out.shown.push(Attachment { name, kind: kind.into(), path: path.to_string_lossy().into(), size: meta.is_file().then_some(meta.len()), thumb: thumb_of(a) });
            continue;
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't save {name} ({e})."))?;
        let path = free_path(dir, &name);
        let kind = if is_image(&path) { "image" } else { "file" };
        if let Some(data) = &a.data {
            let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).map_err(|_| format!("{name} didn't come through."))?;
            if bytes.len() > MAX_INLINE {
                return Err(format!("{name} is too big to attach."));
            }
            std::fs::write(&path, &bytes).map_err(|e| format!("Couldn't save {name} ({e})."))?;
            out.shown.push(Attachment { name, kind: kind.into(), path: path.to_string_lossy().into(), size: Some(bytes.len() as u64), thumb: thumb_of(a) });
        } else if let Some(url) = a.url.as_deref().filter(|u| u.starts_with("https://") || u.starts_with("http://127.0.0.1")) {
            // Reserve the name so two uploads called the same don't collide.
            std::fs::write(&path, b"").map_err(|e| format!("Couldn't save {name} ({e})."))?;
            out.downloads.push((url.to_string(), path.clone()));
            out.shown.push(Attachment { name, kind: kind.into(), path: path.to_string_lossy().into(), size: None, thumb: thumb_of(a) });
        } else {
            return Err(format!("{name} has nothing in it."));
        }
    }
    Ok(out)
}

/// Fetch an upload to `dest` (at most [`MAX_DOWNLOAD`]).
pub fn download(url: &str, dest: &Path) -> Result<u64, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(300))).http_status_as_error(false).build().into();
    let mut res = agent.get(url).call().map_err(|e| format!("Couldn't fetch the attachment ({e})."))?;
    if !res.status().is_success() {
        return Err(format!("Couldn't fetch the attachment (HTTP {}). Attach it again.", res.status().as_u16()));
    }
    let bytes = res.body_mut().with_config().limit(MAX_DOWNLOAD).read_to_vec().map_err(|_| "The attachment is too big, or the download broke off.".to_string())?;
    std::fs::write(dest, &bytes).map_err(|e| format!("Couldn't save the attachment ({e})."))?;
    Ok(bytes.len() as u64)
}

/// The message as the agent gets it: the person's words, then the paths.
pub fn prompt_with(text: &str, shown: &[Attachment]) -> String {
    if shown.is_empty() {
        return text.to_string();
    }
    let mut out = text.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str("Attached:");
    for a in shown {
        out.push_str(&format!("\n- {}{}", a.path, if a.kind == "folder" { " (folder)" } else { "" }));
    }
    out
}

/// The images among them, as paths, for the agent's own image inputs.
pub fn images(shown: &[Attachment]) -> Vec<PathBuf> {
    shown.iter().filter(|a| a.kind == "image").map(|a| PathBuf::from(&a.path)).collect()
}

/// Claude Code's user message content: the text, then each image small
/// enough to send inline (the rest it reads from the path).
pub fn claude_content(text: &str, images: &[PathBuf]) -> Value {
    let mut blocks = vec![json!({"type": "text", "text": text})];
    for path in images {
        let small = std::fs::metadata(path).map(|m| m.len() <= MAX_IMAGE_BLOCK).unwrap_or(false);
        let Some(bytes) = small.then(|| std::fs::read(path).ok()).flatten() else { continue };
        blocks.push(json!({"type": "image", "source": {"type": "base64", "media_type": mime_of(path), "data": base64::engine::general_purpose::STANDARD.encode(bytes)}}));
    }
    Value::Array(blocks)
}

// ---------- @ files ----------

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", ".next", "dist", "build", ".venv", "__pycache__", ".turbo", "Pods", "DerivedData"];
const MAX_LISTED: usize = 30_000;

/// Every file under `root`, relative: git's list when it's a repository
/// (tracked and untracked, ignored left out), else a walk.
pub fn project_files(root: &Path) -> Vec<String> {
    let git = run_with_timeout(Command::new("git").args(["-C", &root.to_string_lossy(), "ls-files", "-co", "--exclude-standard"]), Duration::from_secs(5));
    if let Some(out) = git.filter(|o| o.status == Some(0)) {
        return out.stdout.lines().filter(|l| !l.is_empty()).take(MAX_LISTED).map(str::to_string).collect();
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let path = e.path();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(path);
                }
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
                if out.len() >= MAX_LISTED {
                    return out;
                }
            }
        }
    }
    out
}

/// How well `query` matches `path` (higher is better); `None` = no match.
/// Every character of the query must appear in order; a match in the file
/// name, at a word start, or in a run counts more; shorter paths win ties.
pub fn score(path: &str, query: &str) -> Option<i64> {
    let q: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(-(path.len() as i64));
    }
    let p: Vec<char> = path.to_lowercase().chars().collect();
    let name_start = path.rfind('/').map(|i| path[..=i].chars().count()).unwrap_or(0);
    let mut score = 0i64;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, c) in p.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            score += 1;
            if i >= name_start {
                score += 3;
            }
            if i == 0 || matches!(p[i - 1], '/' | '-' | '_' | '.' | ' ') {
                score += 4;
            }
            if last == Some(i.wrapping_sub(1)) {
                score += 5;
            }
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| score * 100 - path.len() as i64)
}

/// The best matches for `query` among `files` (and their folders), as
/// `{ path, rel, folder }`.
pub fn search(root: &Path, files: &[String], query: &str, limit: usize) -> Vec<Value> {
    let mut folders: Vec<String> = Vec::new();
    for f in files {
        let mut p = f.as_str();
        while let Some(i) = p.rfind('/') {
            p = &p[..i];
            folders.push(p.to_string());
        }
    }
    folders.sort();
    folders.dedup();
    let mut hits: Vec<(i64, &str, bool)> = files
        .iter()
        .map(|f| (f.as_str(), false))
        .chain(folders.iter().map(|f| (f.as_str(), true)))
        .filter_map(|(rel, folder)| score(rel, query).map(|s| (s, rel, folder)))
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    hits.into_iter()
        .take(limit)
        .map(|(_, rel, folder)| json!({ "path": root.join(rel).to_string_lossy(), "rel": rel, "folder": folder }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("billable-att-{}", crate::util::new_id("t")));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn saves_sent_bytes_keeps_paths_and_plans_uploads() {
        let dir = temp();
        let chat = dir.join("chat");
        let mine = dir.join("cut.mov");
        std::fs::write(&mine, b"x").unwrap();
        let list = vec![
            AttachmentIn { name: "../shot.png".into(), data: Some(base64::engine::general_purpose::STANDARD.encode(b"png")), thumb: Some("data:image/png;base64,AA".into()), ..Default::default() },
            AttachmentIn { name: "shot.png".into(), data: Some(base64::engine::general_purpose::STANDARD.encode(b"png2")), ..Default::default() },
            AttachmentIn { name: "cut.mov".into(), path: Some(mine.to_string_lossy().into()), ..Default::default() },
            AttachmentIn { name: "footage".into(), path: Some(dir.to_string_lossy().into()), ..Default::default() },
            AttachmentIn { name: "brief.pdf".into(), url: Some("https://h/signed?token=1".into()), thumb: Some("javascript:x".into()), ..Default::default() },
        ];
        let p = prepare(&list, &chat).unwrap();
        let kinds: Vec<&str> = p.shown.iter().map(|a| a.kind.as_str()).collect();
        assert_eq!(kinds, ["image", "image", "file", "folder", "file"]);
        assert_eq!(p.shown[0].path, chat.join("shot.png").to_string_lossy());
        assert_eq!(p.shown[1].path, chat.join("shot 2.png").to_string_lossy());
        assert_eq!(std::fs::read(chat.join("shot 2.png")).unwrap(), b"png2");
        assert_eq!(p.shown[0].thumb.as_deref(), Some("data:image/png;base64,AA"));
        assert!(p.shown[4].thumb.is_none());
        assert_eq!(p.downloads, vec![("https://h/signed?token=1".to_string(), chat.join("brief.pdf"))]);

        let prompt = prompt_with("Cut this down", &p.shown);
        assert!(prompt.starts_with("Cut this down\n\nAttached:\n- "));
        assert!(prompt.contains(&format!("- {} (folder)", dir.display())));
        assert_eq!(images(&p.shown).len(), 2);
        let content = claude_content("hi", &images(&p.shown));
        assert_eq!(content.as_array().unwrap().len(), 3);
        assert_eq!(content[1]["source"]["media_type"], "image/png");

        assert!(prepare(&[AttachmentIn { name: "gone".into(), path: Some("/nope/x".into()), ..Default::default() }], &chat).is_err());
        assert!(prepare(&[AttachmentIn { name: "empty".into(), ..Default::default() }], &chat).is_err());
        assert!(prepare(&[AttachmentIn { name: "x".into(), url: Some("ftp://h".into()), ..Default::default() }], &chat).is_err());
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name(".env"), "env");
        assert_eq!(safe_name("a:b.txt"), "a-b.txt");
        assert_eq!(safe_name(""), "attachment");
    }

    #[test]
    fn fuzzy_search_prefers_file_names() {
        let files: Vec<String> = ["src/components/agent-tools.tsx", "src/lib/agents/protocol.ts", "docs/agents/protocol.md", "README.md"].iter().map(|s| s.to_string()).collect();
        let hits = search(Path::new("/w"), &files, "proto", 10);
        let rels: Vec<&str> = hits.iter().map(|h| h["rel"].as_str().unwrap()).collect();
        assert_eq!(&rels[..2], ["docs/agents/protocol.md", "src/lib/agents/protocol.ts"]);
        assert_eq!(hits[0]["path"], "/w/docs/agents/protocol.md");
        let tools = search(Path::new("/w"), &files, "agtools", 10);
        assert_eq!(tools[0]["rel"], "src/components/agent-tools.tsx");
        let folders = search(Path::new("/w"), &files, "src/lib", 10);
        assert!(folders.iter().any(|h| h["rel"] == "src/lib" && h["folder"] == true));
        assert!(score("abc", "xyz").is_none());
    }

    #[test]
    fn lists_project_files_without_git() {
        let dir = temp();
        std::fs::create_dir_all(dir.join("src/node_modules/x")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("src/a.rs"), "").unwrap();
        std::fs::write(dir.join("src/node_modules/x/b.js"), "").unwrap();
        std::fs::write(dir.join(".hidden/c"), "").unwrap();
        std::fs::write(dir.join("top.md"), "").unwrap();
        let mut files = project_files(&dir);
        files.sort();
        assert_eq!(files, ["src/a.rs", "top.md"]);
    }
}
