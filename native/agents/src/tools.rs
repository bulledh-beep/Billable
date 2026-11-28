//! Human lines for tool calls and approvals ("Ran npm test", "Edit a file",
//! "Change FeedHQ"), from a tool's raw name and input.

use serde_json::Value;

use crate::util::{cut, one_line};

pub const FEEDHQ_PREFIX: &str = "mcp__feedhq__";

pub fn is_feedhq(name: &str) -> bool {
    name.starts_with(FEEDHQ_PREFIX)
}

fn s<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

fn file_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

fn host(url: &str) -> &str {
    url.split_once("://").map(|(_, r)| r).unwrap_or(url).split(['/', '?', '#']).next().unwrap_or(url)
}

/// `createContentItem` / `create_content_item` → ["create", "content", "item"].
fn words(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else if c.is_uppercase() && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            cur.extend(c.to_lowercase());
        } else {
            cur.extend(c.to_lowercase());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// "Created a task in FeedHQ", "Looked up posts in FeedHQ".
fn feedhq_title(tool: &str) -> String {
    let w = words(tool);
    let Some((verb, rest)) = w.split_first() else { return "Used FeedHQ".into() };
    let rest = rest.join(" ");
    let past = match verb.as_str() {
        "create" => "Created",
        "update" => "Updated",
        "add" => "Added",
        "delete" | "remove" => "Removed",
        "schedule" => "Scheduled",
        "reschedule" => "Rescheduled",
        "send" => "Sent",
        "attach" => "Attached",
        "link" => "Linked",
        "import" => "Imported",
        "approve" => "Approved",
        "plan" => "Planned",
        "set" => "Set",
        "save" => "Saved",
        "convert" => "Converted",
        "remember" => "Remembered",
        "forget" => "Forgot",
        "undo" => "Undid",
        "retry" => "Retried",
        "get" | "list" | "search" | "find" | "read" | "show" | "lookup" => "Looked up",
        _ => return format!("Used FeedHQ: {}", w.join(" ")),
    };
    if rest.is_empty() {
        format!("{past} in FeedHQ")
    } else {
        format!("{past} {rest} in FeedHQ")
    }
}

/// "Started 3 agents", "Messaged an agent": the orchestrator's own tools.
fn orchestrator_title(tool: &str, input: &Value) -> (String, Option<String>) {
    let count = |key: &str| input.get(key).and_then(Value::as_array).map(Vec::len);
    let agents = |n: usize| if n == 1 { "an agent".to_string() } else { format!("{n} agents") };
    match tool {
        "start_agents" => {
            let titles: Vec<&str> = input.get("agents").and_then(Value::as_array).map(|a| a.iter().filter_map(|w| s(w, "title")).collect()).unwrap_or_default();
            let title = match count("agents") {
                Some(n) if n > 0 => format!("Started {}", agents(n)),
                _ => "Started agents".into(),
            };
            (title, (!titles.is_empty()).then(|| one_line(&titles.join(", "), 200)))
        }
        "wait_for_agents" => match count("ids") {
            Some(n) if n > 0 => (format!("Waited for {}", agents(n)), None),
            _ => ("Waited for the agents".into(), None),
        },
        "message_agent" => ("Messaged an agent".into(), s(input, "text").map(|t| one_line(t, 200))),
        "list_agents" => ("Checked on agents".into(), None),
        "stop_agents" => ("Stopped agents".into(), None),
        "ask_person" => ("Asked you".into(), s(input, "question").map(|t| one_line(t, 200))),
        "computer_screenshot" => ("Looked at the screen".into(), None),
        "computer_click" => {
            let double = input.get("count").and_then(Value::as_u64).unwrap_or(1) > 1;
            (if double { "Double-clicked" } else { "Clicked" }.into(), None)
        }
        "computer_move" => ("Moved the pointer".into(), None),
        "computer_drag" => ("Dragged".into(), None),
        "computer_scroll" => (format!("Scrolled {}", s(input, "direction").unwrap_or("down")), None),
        "computer_type" => ("Typed".into(), s(input, "text").map(|t| one_line(t, 200))),
        "computer_key" => ("Pressed keys".into(), s(input, "keys").map(str::to_string)),
        "computer_open_app" => (format!("Opened {}", s(input, "name").unwrap_or("an app")), None),
        "computer_cursor_position" => ("Found the pointer".into(), None),
        "computer_wait" => ("Waited".into(), None),
        other => (format!("Used {}", words(other).join(" ")), None),
    }
}

/// `(title, detail)` for a tool call.
pub fn describe(name: &str, input: &Value) -> (String, Option<String>) {
    if let Some(tool) = name.strip_prefix(crate::orchestrator::TOOL_PREFIX) {
        return orchestrator_title(tool, input);
    }
    if let Some(tool) = name.strip_prefix(FEEDHQ_PREFIX) {
        let detail = input.as_object().filter(|o| !o.is_empty()).map(|_| one_line(&input.to_string(), 200));
        return (feedhq_title(tool), detail);
    }
    if let Some(rest) = name.strip_prefix("mcp__") {
        let (server, tool) = rest.split_once("__").unwrap_or(("", rest));
        let title = if server.is_empty() { format!("Used {}", words(tool).join(" ")) } else { format!("Used {} ({server})", words(tool).join(" ")) };
        let detail = input.as_object().filter(|o| !o.is_empty()).map(|_| one_line(&input.to_string(), 200));
        return (title, detail);
    }
    let path = s(input, "file_path").or_else(|| s(input, "path")).or_else(|| s(input, "notebook_path"));
    match name {
        "Bash" | "shell" | "commandExecution" => {
            let command = s(input, "command").map(|c| one_line(c, 200));
            let title = match (s(input, "description"), command.as_deref()) {
                (Some(d), _) => one_line(d, 80),
                (None, Some(c)) => format!("Ran {}", cut(c, 60)),
                (None, None) => "Ran a command".into(),
            };
            (title, command)
        }
        "Read" => (path.map(|p| format!("Read {}", file_name(p))).unwrap_or_else(|| "Read a file".into()), path.map(str::to_string)),
        "Write" => (path.map(|p| format!("Wrote {}", file_name(p))).unwrap_or_else(|| "Wrote a file".into()), path.map(str::to_string)),
        "Edit" | "MultiEdit" | "NotebookEdit" => (path.map(|p| format!("Edited {}", file_name(p))).unwrap_or_else(|| "Edited a file".into()), path.map(str::to_string)),
        "apply_patch" | "fileChange" => {
            let paths: Vec<&str> = input.get("changes").and_then(Value::as_array).map(|c| c.iter().filter_map(|ch| ch.get("path").and_then(Value::as_str)).collect()).unwrap_or_default();
            let title = match paths.as_slice() {
                [] => "Edited files".to_string(),
                [one] => format!("Edited {}", file_name(one)),
                many => format!("Edited {} files", many.len()),
            };
            (title, (!paths.is_empty()).then(|| one_line(&paths.join(", "), 200)))
        }
        "Glob" => ("Looked for files".into(), s(input, "pattern").map(str::to_string)),
        "Grep" => (s(input, "pattern").map(|p| format!("Searched for {}", cut(p, 50))).unwrap_or_else(|| "Searched files".into()), path.map(str::to_string)),
        "LS" => (path.map(|p| format!("Listed {}", file_name(p))).unwrap_or_else(|| "Listed a folder".into()), path.map(str::to_string)),
        "WebFetch" => {
            let url = s(input, "url");
            (url.map(|u| format!("Read {}", host(u))).unwrap_or_else(|| "Read a web page".into()), url.map(str::to_string))
        }
        "WebSearch" | "web_search" | "webSearch" => ("Searched the web".into(), s(input, "query").map(|q| one_line(q, 200))),
        "Task" | "Agent" => ("Started a helper agent".into(), s(input, "description").map(|d| one_line(d, 200))),
        "TodoWrite" | "update_plan" => ("Updated the plan".into(), None),
        "AskUserQuestion" => ("Asked a question".into(), None),
        "ExitPlanMode" => ("Finished planning".into(), None),
        other => (format!("Used {other}"), input.as_object().filter(|o| !o.is_empty()).map(|_| one_line(&input.to_string(), 200))),
    }
}

/// `(title, detail)` for an approval card.
pub fn approval(name: &str, input: &Value) -> (String, String) {
    let detail = |v: Option<&str>| cut(v.unwrap_or(""), 2000);
    let pretty = || cut(&serde_json::to_string_pretty(input).unwrap_or_default(), 2000);
    if is_feedhq(name) {
        return ("Change FeedHQ".into(), format!("{}\n{}", feedhq_title(&name[FEEDHQ_PREFIX.len()..]), pretty()));
    }
    if let Some(tool) = name.strip_prefix(crate::orchestrator::TOOL_PREFIX) {
        let title = match tool {
            "start_agents" => "Start helper agents",
            "message_agent" => "Message a helper agent",
            "stop_agents" => "Stop helper agents",
            _ => "Check on helper agents",
        };
        return (title.into(), pretty());
    }
    if let Some(rest) = name.strip_prefix("mcp__") {
        let tool = rest.split_once("__").map(|(_, t)| t).unwrap_or(rest);
        return (format!("Use {}", words(tool).join(" ")), pretty());
    }
    match name {
        "Bash" | "shell" | "commandExecution" => ("Run a command".into(), detail(s(input, "command"))),
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "apply_patch" | "fileChange" => {
            let path = s(input, "file_path").or_else(|| s(input, "path")).or_else(|| s(input, "notebook_path"));
            ("Edit a file".into(), path.map(|p| cut(p, 2000)).unwrap_or_else(pretty))
        }
        "WebFetch" => ("Open a web page".into(), detail(s(input, "url"))),
        "WebSearch" => ("Search the web".into(), detail(s(input, "query"))),
        other => (format!("Use {other}"), pretty()),
    }
}

/// Text of a tool result (a string, or MCP-style content blocks), trimmed
/// to 4,000 characters.
pub fn output_text(v: &Value) -> Option<String> {
    let text = match v {
        Value::Null => return None,
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                Value::String(s) => Some(s.clone()),
                other => other.get("text").and_then(Value::as_str).map(str::to_string),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.get("text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| other.to_string()),
    };
    Some(cut(&text, 4000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn titles() {
        assert_eq!(describe("Bash", &json!({"command":"npm test"})), ("Ran npm test".into(), Some("npm test".into())));
        assert_eq!(describe("Bash", &json!({"command":"ls","description":"List the folder"})).0, "List the folder");
        assert_eq!(describe("Read", &json!({"file_path":"/a/b/notes.md"})), ("Read notes.md".into(), Some("/a/b/notes.md".into())));
        assert_eq!(describe("Edit", &json!({"file_path":"/x/y.rs"})).0, "Edited y.rs");
        assert_eq!(describe("WebFetch", &json!({"url":"https://example.com/a?b"})).0, "Read example.com");
        assert_eq!(describe("mcp__feedhq__createTask", &json!({"title":"Film"})).0, "Created task in FeedHQ");
        assert_eq!(describe("mcp__feedhq__scheduleSocialPost", &json!({})).0, "Scheduled social post in FeedHQ");
        assert_eq!(describe("mcp__feedhq__listContentItems", &json!({})).0, "Looked up content items in FeedHQ");
        assert_eq!(describe("mcp__feedhq__whatever_thing", &json!({})).0, "Used FeedHQ: whatever thing");
        assert_eq!(describe("mcp__github__create_issue", &json!({})).0, "Used create issue (github)");
        assert_eq!(describe("apply_patch", &json!({"changes":[{"path":"a.txt"},{"path":"b.txt"}]})).0, "Edited 2 files");
        assert_eq!(describe("Frobnicate", &json!({})), ("Used Frobnicate".into(), None));
        let start = json!({"agents":[{"title":"Rename","task":"x"},{"title":"Fix test","task":"y"},{"title":"Design","task":"z"}]});
        assert_eq!(describe("mcp__billable__start_agents", &start), ("Started 3 agents".into(), Some("Rename, Fix test, Design".into())));
        assert_eq!(describe("mcp__billable__start_agents", &json!({"agents":[{"title":"A","task":"x"}]})).0, "Started an agent");
        assert_eq!(describe("mcp__billable__wait_for_agents", &json!({"ids":["a","b"]})).0, "Waited for 2 agents");
        assert_eq!(describe("mcp__billable__wait_for_agents", &json!({})).0, "Waited for the agents");
        assert_eq!(describe("mcp__billable__message_agent", &json!({"id":"a","text":"And tests"})), ("Messaged an agent".into(), Some("And tests".into())));
        assert_eq!(describe("mcp__billable__list_agents", &json!({})).0, "Checked on agents");
        assert_eq!(describe("mcp__billable__computer_open_app", &json!({"name":"DaVinci Resolve"})).0, "Opened DaVinci Resolve");
        assert_eq!(describe("mcp__billable__computer_key", &json!({"keys":"cmd+s"})), ("Pressed keys".into(), Some("cmd+s".into())));
        assert_eq!(describe("mcp__billable__stop_agents", &json!({})).0, "Stopped agents");
        assert!(!is_feedhq("mcp__billable__start_agents"));
    }

    #[test]
    fn approvals_and_outputs() {
        assert_eq!(approval("Bash", &json!({"command":"rm -rf x"})), ("Run a command".into(), "rm -rf x".into()));
        assert_eq!(approval("Write", &json!({"file_path":"/a"})).0, "Edit a file");
        assert_eq!(approval("mcp__feedhq__createTask", &json!({})).0, "Change FeedHQ");
        assert_eq!(output_text(&json!([{"type":"text","text":"a"},{"type":"text","text":"b"}])).as_deref(), Some("a\nb"));
        assert_eq!(output_text(&json!("x".repeat(5000))).unwrap().chars().count(), 4000);
        assert_eq!(output_text(&Value::Null), None);
    }
}
