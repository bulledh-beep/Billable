// Adapted from T3 Code (https://github.com/pingdotgg/t3code), MIT, © 2026 T3 Tools Inc.
// See THIRD_PARTY_NOTICES.md.

//! The handoff a chat carries when it moves to another agent (PROTOCOL.md,
//! "Handoff when switching agents mid-chat"). Rules adapted from T3 Code's
//! portable handoffs (https://github.com/pingdotgg/t3code), MIT, © 2026 T3
//! Tools Inc.
//!
//! - Budget 16,000 tokens, estimated as 1 token per 4 bytes of UTF-8.
//! - Always keep the first user message and the most recent turns; fill from
//!   the newest backwards; drop reasoning and tool outputs first (tool titles
//!   stay).
//! - `<feedhq-handoff>` … `</feedhq-handoff>`, with `User:` /
//!   `Assistant (Claude Code, Opus):` / `Tool: <title>` lines, then
//!   `Note: earlier messages omitted` when trimmed, then the new message.

use crate::protocol::{Item, ItemKind};

pub const BUDGET_TOKENS: usize = 16_000;
pub const BYTES_PER_TOKEN: usize = 4;

pub fn estimate_tokens(s: &str) -> usize {
    s.len().div_ceil(BYTES_PER_TOKEN)
}

struct Entry {
    line: String,
    /// Reasoning, or a tool's output: the first to go.
    extra: Option<String>,
    /// A reasoning entry is all extra.
    only_extra: bool,
    is_user: bool,
}

fn entries(items: &[Item], speaker: &dyn Fn(&str, &str) -> String) -> Vec<Entry> {
    let mut out = Vec::new();
    for item in items {
        let text = item.text.as_deref().unwrap_or("").trim();
        let who = || speaker(item.agent.as_deref().unwrap_or(""), item.model.as_deref().unwrap_or(""));
        match item.kind {
            ItemKind::User if !text.is_empty() => out.push(Entry { line: format!("User: {text}"), extra: None, only_extra: false, is_user: true }),
            ItemKind::Assistant if !text.is_empty() => out.push(Entry { line: format!("Assistant ({}): {text}", who()), extra: None, only_extra: false, is_user: false }),
            ItemKind::Reasoning if !text.is_empty() => {
                out.push(Entry { line: String::new(), extra: Some(format!("Assistant reasoning ({}): {text}", who())), only_extra: true, is_user: false })
            }
            ItemKind::Tool => {
                if let Some(tool) = &item.tool {
                    let output = tool.output.as_deref().map(str::trim).filter(|o| !o.is_empty()).map(|o| format!("Tool output: {o}"));
                    out.push(Entry { line: format!("Tool: {}", tool.title), extra: output, only_extra: false, is_user: false });
                }
            }
            _ => {}
        }
    }
    out
}

fn render(entries: &[&Entry], with_extra: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for e in entries {
        if !e.only_extra {
            lines.push(e.line.clone());
        }
        if with_extra {
            if let Some(x) = &e.extra {
                lines.push(x.clone());
            }
        }
    }
    lines
}

fn size(lines: &[String]) -> usize {
    lines.iter().map(|l| l.len() + 1).sum()
}

fn truncate_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.saturating_sub(3);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// The first prompt for the new agent: the budgeted transcript, then the new
/// message. `speaker(agent, model)` names who wrote an assistant line, e.g.
/// "Claude Code, Opus 5.5".
pub fn build(items: &[Item], speaker: &dyn Fn(&str, &str) -> String, new_message: &str) -> String {
    build_with_budget(items, speaker, new_message, BUDGET_TOKENS * BYTES_PER_TOKEN)
}

pub fn build_with_budget(items: &[Item], speaker: &dyn Fn(&str, &str) -> String, new_message: &str, budget_bytes: usize) -> String {
    let all = entries(items, speaker);
    if all.is_empty() {
        return new_message.to_string();
    }
    let refs: Vec<&Entry> = all.iter().collect();
    let mut trimmed = false;
    let mut lines = render(&refs, true);
    if size(&lines) > budget_bytes {
        lines = render(&refs, false);
    }
    if size(&lines) > budget_bytes {
        trimmed = true;
        let visible: Vec<&Entry> = all.iter().filter(|e| !e.only_extra).collect();
        let first = visible.iter().position(|e| e.is_user).unwrap_or(0);
        let first_line = truncate_bytes(&visible[first].line, budget_bytes / 2);
        let mut used = first_line.len() + 1;
        let mut tail: Vec<String> = Vec::new();
        for e in visible.iter().skip(first + 1).rev() {
            let cost = e.line.len() + 1;
            if used + cost > budget_bytes {
                break;
            }
            used += cost;
            tail.push(e.line.clone());
        }
        tail.reverse();
        lines = std::iter::once(first_line).chain(tail).collect();
    }
    let mut out = String::from("<feedhq-handoff>\n");
    for l in &lines {
        out.push_str(l);
        out.push('\n');
    }
    if trimmed {
        out.push_str("Note: earlier messages omitted\n");
    }
    out.push_str("</feedhq-handoff>\n\n");
    out.push_str(new_message);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ToolInfo, ToolStatus};

    fn item(kind: ItemKind, text: &str) -> Item {
        Item {
            id: crate::util::uuid(),
            thread_id: "t".into(),
            turn_id: None,
            kind,
            created_at: String::new(),
            text: Some(text.into()),
            agent: Some("claude".into()),
            model: Some("opus".into()),
            tool: None,
            divider: None,
            decision: None,
            attachments: None,
        }
    }

    fn tool(title: &str, output: &str) -> Item {
        let mut i = item(ItemKind::Tool, "");
        i.text = None;
        i.tool = Some(ToolInfo { name: "Bash".into(), title: title.into(), detail: None, status: ToolStatus::Done, output: Some(output.into()), feedhq: false, threads: vec![] });
        i
    }

    fn speaker(agent: &str, model: &str) -> String {
        format!("{} {}", if agent == "claude" { "Claude Code," } else { "Codex," }, if model == "opus" { "Opus" } else { model })
    }

    #[test]
    fn small_chats_go_whole() {
        let items = vec![item(ItemKind::User, "Plan my week"), item(ItemKind::Reasoning, "thinking"), tool("Ran ls", "a b"), item(ItemKind::Assistant, "Here it is"), item(ItemKind::Divider, "")];
        let out = build(&items, &speaker, "Now shorter");
        assert_eq!(
            out,
            "<feedhq-handoff>\nUser: Plan my week\nAssistant reasoning (Claude Code, Opus): thinking\nTool: Ran ls\nTool output: a b\nAssistant (Claude Code, Opus): Here it is\n</feedhq-handoff>\n\nNow shorter"
        );
        assert_eq!(build(&[], &speaker, "hi"), "hi");
    }

    #[test]
    fn reasoning_and_outputs_go_first() {
        let items = vec![item(ItemKind::User, "first"), item(ItemKind::Reasoning, &"r".repeat(500)), tool("Ran x", &"o".repeat(500)), item(ItemKind::Assistant, "answer")];
        let out = build_with_budget(&items, &speaker, "next", 200);
        assert!(!out.contains("rrrr") && !out.contains("oooo"), "{out}");
        assert!(out.contains("Tool: Ran x"));
        assert!(!out.contains("Note: earlier"));
    }

    #[test]
    fn keeps_the_first_message_and_the_newest() {
        let mut items = vec![item(ItemKind::User, "the very first message")];
        for i in 0..50 {
            items.push(item(ItemKind::User, &format!("question {i} {}", "x".repeat(40))));
            items.push(item(ItemKind::Assistant, &format!("answer {i} {}", "y".repeat(40))));
        }
        let out = build_with_budget(&items, &speaker, "new", 600);
        assert!(out.contains("User: the very first message"));
        assert!(out.contains("answer 49"));
        assert!(!out.contains("question 0 "));
        assert!(out.contains("Note: earlier messages omitted\n</feedhq-handoff>\n\nnew"));
        let body = out.split("</feedhq-handoff>").next().unwrap();
        assert!(body.len() <= 600 + "<feedhq-handoff>\nNote: earlier messages omitted\n".len());
        // Newest kept in order
        let a = out.find("question 49").unwrap();
        let b = out.find("answer 49").unwrap();
        assert!(a < b);
    }

    #[test]
    fn real_budget_is_sixteen_thousand_tokens() {
        let big = "z".repeat(BUDGET_TOKENS * BYTES_PER_TOKEN);
        let items = vec![item(ItemKind::User, "start"), item(ItemKind::Assistant, &big), item(ItemKind::User, "last question")];
        let out = build(&items, &speaker, "go");
        assert!(out.contains("Note: earlier messages omitted"));
        assert!(out.contains("User: last question"));
        assert!(estimate_tokens(&out) <= BUDGET_TOKENS + 50);
        // A first message bigger than the budget is cut, not dropped
        let items = vec![item(ItemKind::User, &big), item(ItemKind::Assistant, "ok")];
        let out = build(&items, &speaker, "go");
        assert!(out.contains("User: zzz") && out.contains("Assistant (Claude Code, Opus): ok"));
        assert!(estimate_tokens(&out) <= BUDGET_TOKENS + 50);
    }
}
