//! The `/` commands a chat can use (PROTOCOL.md, "Attachments, @ and /"):
//! what Claude Code listed when its latest session started, plus the
//! person's own commands and skills on disk (with their descriptions), so the
//! list is there before the first chat too. Reading only.

use std::path::Path;

use serde_json::{json, Value};

/// `description:` from a Markdown file's front matter, else its first line.
fn description(text: &str) -> String {
    let mut lines = text.lines();
    if text.starts_with("---") {
        lines.next();
        for line in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
            if let Some(d) = line.strip_prefix("description:") {
                return crate::util::one_line(d.trim().trim_matches('"').trim_matches('\''), 140);
            }
        }
    }
    lines.map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty()).map(|l| crate::util::one_line(l, 140)).unwrap_or_default()
}

/// Commands in `<dir>/commands/**/*.md` (sub/folder/x.md → `sub:folder:x`)
/// and skills in `<dir>/skills/*/SKILL.md`.
fn scan(dir: &Path, out: &mut Vec<(String, String)>) {
    let mut stack = vec![(dir.join("commands"), String::new())];
    while let Some((d, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                stack.push((path, format!("{prefix}{name}:")));
            } else if let Some(stem) = name.strip_suffix(".md") {
                out.push((format!("{prefix}{stem}"), description(&std::fs::read_to_string(&path).unwrap_or_default())));
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir.join("skills")) {
        for e in entries.flatten() {
            let skill = e.path().join("SKILL.md");
            if let Ok(text) = std::fs::read_to_string(&skill) {
                out.push((e.file_name().to_string_lossy().to_string(), description(&text)));
            }
        }
    }
}

/// `[{ name, description }]`, the folder's own first, then the person's,
/// then the rest Claude Code listed; each name once, sorted within a group.
pub fn list(home: &Path, cwd: Option<&Path>, listed: &[String]) -> Vec<Value> {
    let mut found = Vec::new();
    if let Some(cwd) = cwd {
        scan(&cwd.join(".claude"), &mut found);
    }
    scan(&home.join(".claude"), &mut found);
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Value> = Vec::new();
    for (name, desc) in found {
        if seen.insert(name.clone()) {
            out.push(json!({"name": name, "description": desc}));
        }
    }
    let mut rest: Vec<&String> = listed.iter().filter(|n| !seen.contains(*n)).collect();
    rest.sort();
    rest.dedup();
    out.extend(rest.into_iter().map(|n| json!({"name": n, "description": ""})));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folder_then_person_then_listed() {
        let home = std::env::temp_dir().join(format!("billable-cmd-{}", crate::util::new_id("h")));
        let site = home.join("site");
        std::fs::create_dir_all(home.join(".claude/commands/video")).unwrap();
        std::fs::create_dir_all(home.join(".claude/skills/color-grade")).unwrap();
        std::fs::create_dir_all(site.join(".claude/commands")).unwrap();
        std::fs::write(home.join(".claude/commands/video/rough-cut.md"), "---\ndescription: \"Make a rough cut\"\n---\nDo it").unwrap();
        std::fs::write(home.join(".claude/skills/color-grade/SKILL.md"), "---\nname: color-grade\ndescription: Grade footage\n---\n").unwrap();
        std::fs::write(site.join(".claude/commands/ship.md"), "# Ship the site\n\nSteps").unwrap();
        let got = list(&home, Some(&site), &["compact".into(), "ship".into(), "context".into()]);
        let names: Vec<&str> = got.iter().map(|c| c["name"].as_str().unwrap()).collect();
        assert_eq!(names[0], "ship");
        assert!(names.contains(&"video:rough-cut") && names.contains(&"color-grade"));
        assert_eq!(&names[names.len() - 2..], ["compact", "context"]);
        assert_eq!(got[0]["description"], "Ship the site");
        assert_eq!(got.iter().find(|c| c["name"] == "video:rough-cut").unwrap()["description"], "Make a rough cut");
    }
}
