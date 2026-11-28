# Agents: the protocol between the chat screen and an engine

FeedHQ's **Agents** page (`/agents`) is one chat screen with up to three
engines behind it:

| Engine | Where it runs | Agents it offers | Transport |
| --- | --- | --- | --- |
| **Mac** | Billable (Rust, `native/agents` crate) on Todd's Mac | Claude Code, Codex, on his own subscriptions | WebKit bridge, only inside Billable's window |
| **Remote Mac** | the same Billable engine, reached from the website or phone | the same | FeedHQ relay (`/api/agents/remote/*`) |
| **Cloud** | FeedHQ's server (`/api/ai/*`, Anthropic API credits) | FeedHQ assistant: Haiku, Sonnet, Opus, Fable | HTTP + stream |

The screen speaks one protocol to every engine. This file is the contract:
`src/lib/agents/protocol.ts` (FeedHQ) and `native/agents/src/protocol.rs`
(Billable) implement it and must change together. Version: **1**.

All messages are JSON objects. Field names are camelCase. Times are ISO 8601
UTC strings with milliseconds. IDs are strings (engine-chosen; UUID v4 is
fine). Unknown fields must be ignored; unknown event types must be ignored.

## Envelopes

Screen → engine, a **request**:

```json
{ "v": 1, "type": "request", "id": "r_12", "method": "turn.send", "params": { } }
```

Engine → screen, the **reply** to one request (exactly one per request):

```json
{ "v": 1, "type": "reply", "id": "r_12", "ok": true, "result": { } }
{ "v": 1, "type": "reply", "id": "r_12", "ok": false, "error": { "code": "not_found", "message": "That chat is gone." } }
```

Error codes: `bad_request`, `not_found`, `busy` (a turn is already running in
that thread), `agent_missing` (not installed), `agent_signed_out`,
`feedhq_signed_out`, `offline` (remote Mac not reachable), `failed`.
`message` is a short plain sentence fit to show.

Engine → screen, an **event** (any time, not tied to a request):

```json
{ "v": 1, "type": "event", "event": { "type": "item.delta", "threadId": "t_1", "itemId": "i_9", "delta": "Hel" } }
```

## Objects

**Agent**

```ts
{
  id: "claude" | "codex" | "feedhq";   // feedhq = the Cloud engine's assistant
  name: string;                        // "Claude Code", "Codex", "FeedHQ"
  installed: boolean;
  signedIn: boolean;
  version: string | null;
  models: Model[];                     // order = picker order
  defaultModel: string;                // a Model.id
}
```

**Model**

```ts
{
  id: string;            // what the agent CLI takes, e.g. "opus", "sonnet", "gpt-5.5"; for feedhq the route: "fast" | "strong" | "opus" | "fable"
  name: string;          // "Opus 4.8", "Sonnet 5.5", "GPT-5.5"
  efforts: string[];     // allowed effort levels in order, e.g. ["low","medium","high"]; [] = no effort control
  defaultEffort: string | null;
}
```

**Thread**

```ts
{
  id: string;
  engine: "mac" | "cloud";
  title: string;                 // "" until named; the screen shows the first message then
  agent: Agent["id"];            // the agent of the latest turn
  model: string;
  effort: string | null;
  status: "idle" | "running" | "waiting" | "failed";  // waiting = an approval or a decision card is open
  pinned: boolean;
  settledAt: string | null;      // set = in "Done"
  archivedAt: string | null;     // set = hidden from the list (Undo clears it)
  createdAt: string;
  updatedAt: string;             // last activity, sorts the list
  preview: string;               // last assistant or user text, at most 140 chars, one line
  cwd: string | null;            // Mac only: the folder the agent works in (null = the engine's default)
  role: "orchestrator" | "worker" | null;  // Mac only, see "Orchestrator"
  parentId: string | null;       // a worker's orchestrator thread
  projectId?: string | null;     // Mac only: the FeedHQ project it's filed in (its memory applies)
  workerKind?: "job" | "lookup" | null;  // workers only, see "Coordinator"
  reviewAt?: string | null;      // a job handed its work back for the person to check
}
```

**Item** (one row in a chat; a turn makes several)

```ts
{
  id: string;
  threadId: string;
  turnId: string | null;
  kind: "user" | "assistant" | "reasoning" | "tool" | "divider" | "error" | "decision";
  createdAt: string;
  text?: string;                 // user, assistant, reasoning (markdown), error (plain)
  agent?: Agent["id"];           // assistant / reasoning / tool: who wrote it
  model?: string;
  tool?: {
    name: string;                // raw tool name, e.g. "Bash", "mcp__feedhq__create_post"
    title: string;               // short human line: "Ran npm test", "Created a post in FeedHQ"
    detail: string | null;       // one-line input summary (command, path, query)
    status: "running" | "done" | "failed" | "denied";
    output: string | null;       // trimmed to 4,000 chars
    feedhq: boolean;             // true when it's a FeedHQ connector tool
    threads?: string[];          // orchestrator tools: the worker threads this call started or read
  };
  divider?: { from: { agent: string; model: string }; to: { agent: string; model: string } };
  decision?: Decision;           // kind "decision"
}
```

**Approval** (an agent asking before a step)

```ts
{
  id: string;
  threadId: string;
  itemId: string | null;         // the tool item it belongs to, when known
  title: string;                 // "Run a command", "Edit a file", "Change FeedHQ"
  detail: string;                // the command / path / tool input, trimmed to 2,000 chars
  options: ("allow" | "allow_always" | "deny")[];
}
```

**Decision** (a card an agent put to the person: a question, or a plan of jobs)

```ts
{
  question: string;
  detail?: string;
  options: { label: string; detail?: string }[];   // 2 to 4; a plan: ["Start 3", "Not now"]
  recommended?: number;          // index into options
  plan?: { title, task, agent, model, effort?, kind: "job" | "lookup", reason? }[];  // a plan card: option 0 starts these
  chosen?: number;               // set once answered
  answer?: string;               // what the person wrote with their pick
  kept?: number[];               // a started plan: the jobs the person kept
  answeredAt?: string;           // unset = still open
}
```

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `hello` | `{ client: "billable" \| "web" \| "phone" }` | `{ engine: "mac" \| "cloud", engineVersion, protocol: 1, agents: Agent[], feedhq: { connected: boolean, account: string \| null }, device: string \| null, features?: string[] }` (`features` lists what this engine adds to v1: `"folders"` (cwd + system.pickFolder), `"orchestrator"`, `"bulk-delete"`, `"coordinator"`, `"decisions"`, `"memory"`, `"lookups"`, `"review"`, `"tools"`, `"computer"`; missing = none) |
| `threads.list` | `{ includeArchived?: boolean }` | `{ threads: Thread[] }` (newest `updatedAt` first) |
| `thread.get` | `{ threadId }` | `{ thread: Thread, items: Item[], approvals: Approval[] }` (items oldest first) |
| `thread.create` | `{ agent, model, effort?: string \| null, cwd?: string \| null, role?: "orchestrator" \| null, projectId?: string \| null }` | `{ thread: Thread }` |
| `thread.update` | `{ threadId, title?, pinned?, settled?: boolean, archived?: boolean, projectId?: string \| null }` | `{ thread: Thread }` (`settled: true` also clears `reviewAt`; a coordinator's `projectId` carries to its jobs) |
| `thread.delete` | `{ threadId }` | `{}` (also deletes its workers) |
| `threads.delete` | `{ threadIds: string[] }` | `{ deleted: string[] }` (at most 200; unknown ids are skipped; one `thread.deleted` event each) |
| `thread.retitle` | `{ threadId }` | always `bad_request` "Rename it instead." (titles never use a model; rename with `thread.update`) |
| `turn.send` | `{ threadId, text, agent?, model?, effort? }` | `{ turnId, userItem: Item }` |
| `turn.stop` | `{ threadId }` | `{}` |
| `approval.answer` | `{ threadId, approvalId, decision: "allow" \| "allow_always" \| "deny" }` | `{}` |
| `decision.answer` | `{ threadId, itemId, choice: number, text?: string, kept?: number[] }` | `{ item: Item }` (`not_found` once answered; a plan needs at least one kept job) |
| `memory.get` | `{ projectId }` | `{ projectId, text, updatedAt: string \| null }` |
| `memory.set` | `{ projectId, text }` | `{ projectId, text, updatedAt }` (at most 6,000 chars; "" clears it) |
| `agents.refresh` | `{}` | `{ agents: Agent[] }` (re-detect installs and sign-ins) |
| `feedhq.connect` | `{}` | `{ authorizeUrl: string }` (Mac engine only; the screen navigates there) |
| `feedhq.disconnect` | `{}` | `{}` |
| `system.pickFolder` | `{}` | `{ path: string \| null }` (Mac engine, inside Billable only: shows the Mac's folder picker; null = cancelled. From the relay: `bad_request` "Pick the folder on your Mac.") |
| `tools.list` | `{ cwd?: string \| null }` | `{ servers: McpServer[], computer: ComputerAccess }` (see "Tools") |
| `tools.check` | `{ cwd? }` | the same, after Claude Code started and checked each server |
| `tools.add` | `{ agents: ("claude" \| "codex")[], name, command?: string, url?: string, env?: Record<string,string>, scope?: "user" \| "local", cwd? }` | the same list |
| `tools.remove` | `{ id, cwd? }` | the same list |
| `tools.import` | `{ id, agents: ("claude" \| "codex")[] }` (an `app` server) | the same list |
| `computer.allow` | `{ which: "screen" \| "control" }` | `{ computer: ComputerAccess }` (asks macOS, and opens its settings pane) |

`cwd` must be an existing folder (`bad_request` "That folder isn't on this Mac." otherwise). It's fixed when the thread is made; every turn of the thread, and every
worker it starts, runs there.

`turn.send` with an `agent`/`model` different from the thread's switches the
thread: the engine adds a `divider` item first, then starts the new agent with
a **handoff** (below). Sending while `status` is `running` fails with `busy`.

An untitled thread is titled from its first message when it's sent: the
first 6 words, trimmed (`thread.updated`). No model is asked. Nothing runs
on the person's Claude or ChatGPT subscription unless they started it: no
background titles, probes or polling, and a failed turn is never resent on
their behalf (a resumed session that failed straight away is forgotten, so
their next message starts fresh with a handoff).

## Events

| Event | Fields |
| --- | --- |
| `thread.updated` | `thread: Thread` (also sent for a new thread) |
| `thread.deleted` | `threadId` |
| `item.added` | `item: Item` |
| `item.delta` | `threadId, itemId, delta: string` (append to `text`) |
| `item.updated` | `item: Item` (full replacement, e.g. a tool finishing) |
| `turn.started` | `threadId, turnId` |
| `turn.finished` | `threadId, turnId, status: "done" \| "stopped" \| "failed", error: string \| null, usage: { inputTokens, outputTokens, costUsd: number \| null } \| null` |
| `approval.requested` | `approval: Approval` |
| `approval.resolved` | `threadId, approvalId, decision` |
| `agents.changed` | `agents: Agent[]` |
| `feedhq.status` | `connected: boolean, account: string \| null` |
| `memory.updated` | `projectId, text, updatedAt: string \| null` |

## Orchestrator (Mac engine)

A thread made with `role: "orchestrator"` runs Claude Code or Codex as a lead
that splits work across **workers**: ordinary threads with `role: "worker"`
and `parentId` set to the orchestrator. The engine gives the orchestrator's
session a second MCP server, `billable` (same local relay, path
`/mcp/<secret>/orchestrate/<threadId>`), with these tools:

| Tool | Input | Returns |
| --- | --- | --- |
| `start_agents` | `{ agents: [{ title, task, agent?: "claude" \| "codex", model?, effort? }] }` (1 to 8) | `[{ id, title, agent, model, effort }]` at once; the workers run in the background |
| `wait_for_agents` | `{ ids?: string[], timeoutSeconds?: number }` (default all, 600 s, max 1,800) | `[{ id, title, status, answer }]`; `answer` = the worker's last assistant text, trimmed to 8,000 chars |
| `message_agent` | `{ id, text }` | `{ ok }` (a follow-up turn in that worker; `busy` while it runs) |
| `list_agents` | `{}` | `[{ id, title, agent, model, effort, status }]` |
| `stop_agents` | `{ ids?: string[] }` | `{ ok }` |

- The orchestrator is told (appended system prompt) to choose each worker's
  model and effort for the job: low effort and the fastest model for lookups,
  renames and mechanical edits; medium for everyday features and fixes; high
  and the strongest model for hard debugging, design and anything risky. It
  writes a one-line reason in each worker's `task`.
- At most 4 workers run at once per orchestrator; the rest wait their turn
  (status `idle` with preview "Waiting to start"). At most 24 workers per
  orchestrator.
- Workers run in the orchestrator's `cwd`, with the FeedHQ connector, and ask
  for approval like any thread (their approvals show on the worker and as a
  badge on the orchestrator). A worker can't start workers.
- Tool items for these calls carry `tool.threads` with the worker ids.
- Nothing starts on its own: workers exist only because the person sent the
  orchestrator a message. `turn.stop` on the orchestrator stops its workers.
- Deleting the orchestrator deletes its workers.

## Coordinator (Mac engine, `features` "coordinator")

An orchestrator is the chat's **coordinator**, in the way Claude Code
projects work, and every Mac chat gets the `billable` server with the tools
for its seat:

| Seat | Tools |
| --- | --- |
| coordinator (`role: "orchestrator"`) | the five above, plus `propose_plan`, `ask_person`, and `remember` in a project |
| job (`role: "worker"`, `workerKind: "job"`) | `ask_person`, `ready_for_review`, and `remember` in a project |
| lookup (`workerKind: "lookup"`) | none: it reads, never changes anything (Claude Code: no Edit, Write or Bash; Codex: read-only sandbox, never asks), and anything that would need an approval is turned down |
| any other chat | `ask_person`, and `remember` in a project |

- `start_agents` items take `kind: "job" | "lookup"` (default job). Lookups
  have their own 4 slots, so a quick read never waits behind jobs; 40 jobs
  and lookups per coordinator in all. Results carry `kind`.
- `propose_plan { summary?, agents: [... same items, plus reason?] }` shows a
  plan card (a `decision` item with `plan`) and holds the call up to 25
  minutes. `decision.answer` with choice 0 starts the kept jobs, whether or
  not the coordinator is still waiting (the person asked for them); the call
  answers `{ approved, started, note }`.
- `ask_person { question, detail?, options: [{label, detail?}] (2 to 4),
  recommended }` shows a decision card and holds the call up to 25 minutes;
  it answers `{ choice, label, note }`, or `{ answered: false, note }` at the
  timeout. Stop, or the turn ending, lets go of the call; the card stays open
  and the chat stays `waiting`. Answered later, the answer is sent as the
  person's next message: "My answer to “…”: Label." Sending a message while a
  card is open is fine.
- `ready_for_review { summary }` sets the job's `reviewAt` (wait_for_agents
  and list_agents say `review`). A new message in the job or `settled: true`
  clears it.
- `remember { note }` appends "- note" to the project's memory (`memory.updated`).
  Every chat in the project gets the memory after its guidance; a changed
  memory starts a fresh agent process on the next turn (the session resumes).
- Worker statuses: queued, running, waiting (an approval or a card is open),
  review, done, stopped, failed, not_started, deleted.

## Tools (Mac engine, `features` "tools")

The person's own MCP servers, as the agents load them. Billable never edits
an agent's files itself: it reads `~/.claude.json` (user and this folder's
local list), the folder's `.mcp.json` and `~/.codex/config.toml`, and changes
them only when the person asks, with the agents' own `claude mcp add-json` /
`mcp remove` and `codex mcp add` / `mcp remove`. Nothing here starts a model.

```ts
McpServer = {
  id: string;            // "<agent>:<scope>:<name>"
  name: string;
  agent: "claude" | "codex" | "app";        // app = the Claude app, to copy over
  scope: "user" | "local" | "project" | "app";
  transport: "stdio" | "http" | "sse";
  detail: string;        // the command line or address
  status: "connected" | "failed" | "needs_auth" | "off" | "unknown";
  error: string | null;
  copiedTo?: ("claude" | "codex")[];        // app servers: already copied
}
ComputerAccess = { supported: boolean, screen: boolean, control: boolean }
```

- Claude Code's status comes from each chat's start (`system/init`'s
  `mcp_servers`) and from `tools.check` (`claude mcp list`).
- The Claude app's servers are its `claude_desktop_config.json` list and its
  installed extensions (`Claude Extensions/*/manifest.json`, with the
  person's settings filled in; turned-off ones are left out). `tools.import`
  copies one under a cleaned name (`DaVinci Resolve Studio` →
  `davinci-resolve-studio`), for every folder. An extension written for the
  app's built-in Node runs on the Claude app itself
  (`ELECTRON_RUN_AS_NODE=1`) when the Mac has no Node of its own.

## Computer (Mac engine, `features` "computer")

Every chat but a lookup has these on the `billable` server. Billable does the
work itself, so it's the same for Claude Code and Codex, and only Billable
needs Screen Recording and Accessibility.

| Tool | Does |
| --- | --- |
| `computer_screenshot` | an image of the main display, at most 1280 px on its long side; coordinates for the rest are in its pixels |
| `computer_click` | `{ x, y, button?, count?, modifiers? }` |
| `computer_move`, `computer_drag`, `computer_scroll` | pointer |
| `computer_type`, `computer_key` | keyboard (`"cmd+s"`, `"return"`) |
| `computer_open_app` | `{ name }` |
| `computer_cursor_position`, `computer_wait` | |

- One chat at a time: the first to act holds the screen until its turn ends.
- The first screenshot asks "Let this chat see your screen?", and acting in
  an app asks "Let this chat use <App>?", as decision cards. Allow holds for
  that chat until Billable quits; Don't allow fails the call.
- Billable itself is never clicked or typed into.
- Missing macOS access fails the call with a plain line, and asks macOS.

## Handoff when switching agents mid-chat

(Rules adapted from T3 Code's portable handoffs, MIT, © T3 Tools Inc.)

When a thread moves to a different agent, or the agent can't resume its own
session, the new session's first prompt carries a budgeted transcript:

- Budget 16,000 tokens, estimated as 1 token per 4 bytes of UTF-8.
- Always keep the first user message and the most recent turns; fill from the
  newest backwards; drop reasoning and tool outputs first (keep tool titles).
- Wrap it as: `<feedhq-handoff>` … `</feedhq-handoff>` with lines
  `User:` / `Assistant (Claude Code, Opus):` / `Tool: <title>`, then
  `Note: earlier messages omitted` when trimmed, then the new message after the
  closing tag.

Switching model within the same agent resumes the agent's own session with the
new model (Claude Code `--resume <session> --model`, Codex `turn/start` with a
model override) and needs no handoff.

## Transports

### WebKit bridge (inside Billable)

- Present when `window.ipc?.postMessage` exists and the page is FeedHQ inside
  Billable (`billable_shell=1` cookie). The page calls
  `window.ipc.postMessage(JSON.stringify(request))`.
- Billable delivers replies and events by running
  `window.__feedhqAgents && window.__feedhqAgents.receive(<json string>)`.
  Before the page has registered, Billable queues up to 500 messages and
  flushes them when the page sends `hello`.
- Billable accepts IPC only when the web view's current URL is on FeedHQ's
  configured origin. Anything else is dropped.

### FeedHQ relay (website and phone → the Mac)

For when the screen is not inside Billable. Billable authenticates with its
existing phone-sync token (`Authorization: Bearer bhq_…`); the page uses the
signed-in user's session. Both are scoped to the one user.

- `POST /api/agents/remote/requests` (user session) body = a request envelope.
  Stored in `agent_remote_requests`. `202 { ok: true }`. `409 offline` if the
  Mac hasn't polled in 45 s.
- `GET /api/agents/remote/inbox?after=<cursor>` (Bearer bhq_) long-polls up to
  25 s; returns `{ requests: Envelope[], cursor }` and marks them delivered;
  also records the Mac as online (`last_seen_at`).
- `POST /api/agents/remote/outbox` (Bearer bhq_) body
  `{ messages: Envelope[] }` (replies and events, at most 200 per call, batched
  every ~250 ms while streaming). Stored in `agent_remote_messages`.
- The page receives replies and events through Supabase Realtime
  (`postgres_changes` INSERT on `agent_remote_messages`, filtered to its user by
  RLS), and catches up with `GET /api/agents/remote/messages?after=<seq>`.
- `GET /api/agents/remote/status` (user session) → `{ online: boolean,
  lastSeenAt, device }`.
- Rows older than one day are removed by the daily cleanup.
- While the Mac is offline the page offers only the Cloud engine and shows
  the Mac's chats as unavailable.

### Cloud

`src/lib/agents/cloud-transport.ts` maps the protocol onto FeedHQ's existing
assistant (`ai_conversations`, `POST /api/ai/chat`, its approvals and Undo).
Its only agent is `feedhq`, and its models are the assistant routes.

## FeedHQ sign-in for the Mac engine (auto-connect)

- Billable is a built-in OAuth client of FeedHQ's connector (PR #58):
  `client_id` **`billable-mac`**, no secret (public client, PKCE S256),
  redirect URI **`billable://oauth`**. It is created by migration, not by
  dynamic registration.
- `feedhq.connect` makes a PKCE verifier and `state`, keeps them in memory,
  and returns `https://<feedhq>/oauth/authorize?response_type=code&client_id=billable-mac&redirect_uri=billable%3A%2F%2Foauth&code_challenge=…&code_challenge_method=S256&state=…&resource=https%3A%2F%2F<feedhq>%2Fapi%2Fmcp`.
  The screen navigates there inside Billable's window, the member taps
  Allow, and FeedHQ redirects to `billable://oauth?code=…&state=…`, which
  Billable intercepts (like `billable://connect`). Billable swaps the code at
  `/api/oauth/token`, keeps the refresh token in the Keychain (service
  `app.billable.feedhq-agents`), and sends `feedhq.status`.
- Billable runs an MCP relay on `127.0.0.1:<random port>` at path
  `/mcp/<random 32-byte secret>`. Each agent session is started with that URL
  as an HTTP MCP server named `feedhq` (Claude Code via `--mcp-config` JSON,
  Codex via `-c mcp_servers.feedhq.url=…`). The relay refreshes the access
  token when it has under 5 minutes left and forwards each request to
  `https://<feedhq>/api/mcp` with `Authorization: Bearer fhq_at_…`. Agents
  never see a FeedHQ token, and the user's own Claude Code and Codex settings
  are never edited.
