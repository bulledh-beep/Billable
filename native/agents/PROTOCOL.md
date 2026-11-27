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
  status: "idle" | "running" | "waiting" | "failed";  // waiting = an approval is open
  pinned: boolean;
  settledAt: string | null;      // set = in "Done"
  archivedAt: string | null;     // set = hidden from the list (Undo clears it)
  createdAt: string;
  updatedAt: string;             // last activity, sorts the list
  preview: string;               // last assistant or user text, at most 140 chars, one line
}
```

**Item** (one row in a chat; a turn makes several)

```ts
{
  id: string;
  threadId: string;
  turnId: string | null;
  kind: "user" | "assistant" | "reasoning" | "tool" | "divider" | "error";
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
  };
  divider?: { from: { agent: string; model: string }; to: { agent: string; model: string } };
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

## Methods

| Method | Params | Result |
| --- | --- | --- |
| `hello` | `{ client: "billable" \| "web" \| "phone" }` | `{ engine: "mac" \| "cloud", engineVersion, protocol: 1, agents: Agent[], feedhq: { connected: boolean, account: string \| null }, device: string \| null }` |
| `threads.list` | `{ includeArchived?: boolean }` | `{ threads: Thread[] }` (newest `updatedAt` first) |
| `thread.get` | `{ threadId }` | `{ thread: Thread, items: Item[], approvals: Approval[] }` (items oldest first) |
| `thread.create` | `{ agent, model, effort?: string \| null }` | `{ thread: Thread }` |
| `thread.update` | `{ threadId, title?, pinned?, settled?: boolean, archived?: boolean }` | `{ thread: Thread }` |
| `thread.delete` | `{ threadId }` | `{}` |
| `thread.retitle` | `{ threadId }` | `{ thread: Thread }` (asks the agent's cheapest model for a 2–5 word title) |
| `turn.send` | `{ threadId, text, agent?, model?, effort? }` | `{ turnId, userItem: Item }` |
| `turn.stop` | `{ threadId }` | `{}` |
| `approval.answer` | `{ threadId, approvalId, decision: "allow" \| "allow_always" \| "deny" }` | `{}` |
| `agents.refresh` | `{}` | `{ agents: Agent[] }` (re-detect installs and sign-ins) |
| `feedhq.connect` | `{}` | `{ authorizeUrl: string }` (Mac engine only; the screen navigates there) |
| `feedhq.disconnect` | `{}` | `{}` |

`turn.send` with an `agent`/`model` different from the thread's switches the
thread: the engine adds a `divider` item first, then starts the new agent with
a **handoff** (below). Sending while `status` is `running` fails with `busy`.

The first message of an untitled thread gets a title automatically after the
first reply (`thread.updated`).

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
