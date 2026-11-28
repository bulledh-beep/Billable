//! The engine's own SQLite file (`<user data>/agents.db`): chats, their
//! items, approvals and each agent's session id per chat. It is never
//! Billable's `billable.db`.

use std::path::Path;

use anyhow::Context as _;
use rusqlite::{params, Connection, OptionalExtension};

use crate::protocol::{Approval, Decision, Item, Thread, ThreadRole, ThreadStatus, WorkerKind};

/// Each entry moves the schema one version up (`PRAGMA user_version`).
const MIGRATIONS: &[&str] = &[
    // 1: the first schema
    "CREATE TABLE threads (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL DEFAULT '',
        agent TEXT NOT NULL,
        model TEXT NOT NULL,
        effort TEXT,
        status TEXT NOT NULL DEFAULT 'idle',
        pinned INTEGER NOT NULL DEFAULT 0,
        settled_at TEXT,
        archived_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        preview TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX threads_updated ON threads(updated_at DESC);
    CREATE TABLE items (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        id TEXT NOT NULL UNIQUE,
        thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
        turn_id TEXT,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        body TEXT NOT NULL
    );
    CREATE INDEX items_thread ON items(thread_id, seq);
    CREATE TABLE approvals (
        id TEXT PRIMARY KEY,
        thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
        item_id TEXT,
        body TEXT NOT NULL,
        created_at TEXT NOT NULL,
        resolved_at TEXT,
        decision TEXT
    );
    CREATE INDEX approvals_thread ON approvals(thread_id);
    CREATE TABLE agent_sessions (
        thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
        agent TEXT NOT NULL,
        session_id TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (thread_id, agent)
    );
    CREATE TABLE meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
    // 2: folders and orchestrators
    "ALTER TABLE threads ADD COLUMN cwd TEXT;
    ALTER TABLE threads ADD COLUMN role TEXT;
    ALTER TABLE threads ADD COLUMN parent_id TEXT;
    CREATE INDEX threads_parent ON threads(parent_id);",
    // 3: coordinators (projects, jobs and lookups, review, memory)
    "ALTER TABLE threads ADD COLUMN project_id TEXT;
    ALTER TABLE threads ADD COLUMN worker_kind TEXT;
    ALTER TABLE threads ADD COLUMN review_at TEXT;
    CREATE TABLE project_memory (
        project_id TEXT PRIMARY KEY,
        text TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );",
];

pub struct Store {
    conn: Connection,
}

fn thread_from_row(r: &rusqlite::Row) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: r.get("id")?,
        engine: "mac".into(),
        title: r.get("title")?,
        agent: r.get("agent")?,
        model: r.get("model")?,
        effort: r.get("effort")?,
        status: ThreadStatus::parse(&r.get::<_, String>("status")?),
        pinned: r.get::<_, i64>("pinned")? != 0,
        settled_at: r.get("settled_at")?,
        archived_at: r.get("archived_at")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        preview: r.get("preview")?,
        cwd: r.get("cwd")?,
        role: r.get::<_, Option<String>>("role")?.as_deref().and_then(ThreadRole::parse),
        parent_id: r.get("parent_id")?,
        project_id: r.get("project_id")?,
        worker_kind: r.get::<_, Option<String>>("worker_kind")?.as_deref().and_then(WorkerKind::parse),
        review_at: r.get("review_at")?,
    })
}

impl Store {
    /// Open (or create) the file and bring its schema up to date.
    pub fn open(path: &Path) -> anyhow::Result<Store> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Store::init(conn)
    }

    pub fn in_memory() -> anyhow::Result<Store> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Store> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> anyhow::Result<usize> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let current = self.schema_version()?;
        if current > MIGRATIONS.len() {
            anyhow::bail!("agents.db is from a newer Billable (schema {current})");
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    // ---------- Threads ----------

    pub fn insert_thread(&self, t: &Thread) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO threads (id, title, agent, model, effort, status, pinned, settled_at, archived_at, created_at, updated_at, preview, cwd, role, parent_id, project_id, worker_kind, review_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                t.id,
                t.title,
                t.agent,
                t.model,
                t.effort,
                t.status.as_str(),
                t.pinned as i64,
                t.settled_at,
                t.archived_at,
                t.created_at,
                t.updated_at,
                t.preview,
                t.cwd,
                t.role.map(ThreadRole::as_str),
                t.parent_id,
                t.project_id,
                t.worker_kind.map(WorkerKind::as_str),
                t.review_at
            ],
        )?;
        Ok(())
    }

    pub fn save_thread(&self, t: &Thread) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE threads SET title = ?2, agent = ?3, model = ?4, effort = ?5, status = ?6, pinned = ?7, settled_at = ?8,
             archived_at = ?9, updated_at = ?10, preview = ?11, project_id = ?12, review_at = ?13 WHERE id = ?1",
            params![t.id, t.title, t.agent, t.model, t.effort, t.status.as_str(), t.pinned as i64, t.settled_at, t.archived_at, t.updated_at, t.preview, t.project_id, t.review_at],
        )?;
        Ok(())
    }

    pub fn thread(&self, id: &str) -> anyhow::Result<Option<Thread>> {
        Ok(self.conn.query_row("SELECT * FROM threads WHERE id = ?1", [id], thread_from_row).optional()?)
    }

    /// Newest activity first.
    pub fn threads(&self, include_archived: bool) -> anyhow::Result<Vec<Thread>> {
        let sql = if include_archived {
            "SELECT * FROM threads ORDER BY updated_at DESC, id"
        } else {
            "SELECT * FROM threads WHERE archived_at IS NULL ORDER BY updated_at DESC, id"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], thread_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// An orchestrator's workers, oldest first.
    pub fn workers(&self, parent_id: &str) -> anyhow::Result<Vec<Thread>> {
        let mut stmt = self.conn.prepare("SELECT * FROM threads WHERE parent_id = ?1 ORDER BY created_at, rowid")?;
        let rows = stmt.query_map([parent_id], thread_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delete_thread(&self, id: &str) -> anyhow::Result<bool> {
        Ok(self.conn.execute("DELETE FROM threads WHERE id = ?1", [id])? > 0)
    }

    // ---------- Items ----------

    pub fn insert_item(&self, item: &Item) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO items (id, thread_id, turn_id, kind, created_at, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item.id, item.thread_id, item.turn_id, item.kind.as_str(), item.created_at, serde_json::to_string(item)?],
        )?;
        Ok(())
    }

    pub fn save_item(&self, item: &Item) -> anyhow::Result<()> {
        self.conn.execute("UPDATE items SET body = ?2 WHERE id = ?1", params![item.id, serde_json::to_string(item)?])?;
        Ok(())
    }

    /// Oldest first.
    pub fn items(&self, thread_id: &str) -> anyhow::Result<Vec<Item>> {
        let mut stmt = self.conn.prepare("SELECT body FROM items WHERE thread_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map([thread_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for body in rows {
            if let Ok(item) = serde_json::from_str::<Item>(&body?) {
                out.push(item);
            }
        }
        Ok(out)
    }

    pub fn item_count(&self, thread_id: &str) -> anyhow::Result<usize> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM items WHERE thread_id = ?1", [thread_id], |r| r.get::<_, i64>(0))? as usize)
    }

    /// Decision cards still waiting for the person, oldest first.
    pub fn open_decisions(&self, thread_id: Option<&str>) -> anyhow::Result<Vec<Item>> {
        let mut stmt = self.conn.prepare("SELECT body FROM items WHERE kind = 'decision' AND (?1 IS NULL OR thread_id = ?1) ORDER BY seq")?;
        let rows = stmt.query_map([thread_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for body in rows {
            if let Ok(item) = serde_json::from_str::<Item>(&body?) {
                if item.decision.as_ref().is_some_and(|d| d.answered_at.is_none()) {
                    out.push(item);
                }
            }
        }
        Ok(out)
    }

    // ---------- Project memory ----------

    pub fn memory(&self, project_id: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(self.conn.query_row("SELECT text, updated_at FROM project_memory WHERE project_id = ?1", [project_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn set_memory(&self, project_id: &str, text: &str, at: &str) -> anyhow::Result<()> {
        if text.trim().is_empty() {
            self.conn.execute("DELETE FROM project_memory WHERE project_id = ?1", [project_id])?;
        } else {
            self.conn.execute(
                "INSERT INTO project_memory (project_id, text, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(project_id) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
                params![project_id, text, at],
            )?;
        }
        Ok(())
    }

    // ---------- Approvals ----------

    pub fn insert_approval(&self, a: &Approval, created_at: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO approvals (id, thread_id, item_id, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![a.id, a.thread_id, a.item_id, serde_json::to_string(a)?, created_at],
        )?;
        Ok(())
    }

    /// `None`: it was withdrawn or expired.
    pub fn resolve_approval(&self, id: &str, decision: Option<Decision>, at: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE approvals SET resolved_at = ?2, decision = ?3 WHERE id = ?1 AND resolved_at IS NULL",
            params![id, at, decision.map(Decision::as_str)],
        )?;
        Ok(())
    }

    pub fn open_approvals(&self, thread_id: &str) -> anyhow::Result<Vec<Approval>> {
        let mut stmt = self.conn.prepare("SELECT body FROM approvals WHERE thread_id = ?1 AND resolved_at IS NULL ORDER BY created_at")?;
        let rows = stmt.query_map([thread_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for body in rows {
            if let Ok(a) = serde_json::from_str::<Approval>(&body?) {
                out.push(a);
            }
        }
        Ok(out)
    }

    // ---------- Agent sessions ----------

    pub fn session(&self, thread_id: &str, agent: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT session_id FROM agent_sessions WHERE thread_id = ?1 AND agent = ?2", [thread_id, agent], |r| r.get(0))
            .optional()?)
    }

    pub fn set_session(&self, thread_id: &str, agent: &str, session_id: &str, at: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO agent_sessions (thread_id, agent, session_id, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (thread_id, agent) DO UPDATE SET session_id = excluded.session_id, updated_at = excluded.updated_at",
            params![thread_id, agent, session_id, at],
        )?;
        Ok(())
    }

    pub fn clear_session(&self, thread_id: &str, agent: &str) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM agent_sessions WHERE thread_id = ?1 AND agent = ?2", [thread_id, agent])?;
        Ok(())
    }

    // ---------- Meta ----------

    pub fn meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    // ---------- Start-up ----------

    /// Billable was closed (or crashed) while these chats were busy: their
    /// agents are gone, so they're idle again and their approvals expired.
    /// Returns the chats that were interrupted.
    pub fn recover_after_restart(&self, at: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT id FROM threads WHERE status IN ('running', 'waiting')")?;
        let ids: Vec<String> = stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        self.conn.execute("UPDATE threads SET status = 'idle' WHERE status IN ('running', 'waiting')", [])?;
        self.conn.execute("UPDATE approvals SET resolved_at = ?1 WHERE resolved_at IS NULL", [at])?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ItemKind, ToolInfo, ToolStatus};

    fn thread(id: &str, updated: &str) -> Thread {
        Thread {
            id: id.into(),
            engine: "mac".into(),
            title: String::new(),
            agent: "claude".into(),
            model: "opus".into(),
            effort: Some("medium".into()),
            status: ThreadStatus::Idle,
            pinned: false,
            settled_at: None,
            archived_at: None,
            created_at: updated.into(),
            updated_at: updated.into(),
            preview: String::new(),
            cwd: None,
            role: None,
            parent_id: None,
            project_id: None,
            worker_kind: None,
            review_at: None,
        }
    }

    fn item(id: &str, thread_id: &str, kind: ItemKind) -> Item {
        Item {
            id: id.into(),
            thread_id: thread_id.into(),
            turn_id: Some("u1".into()),
            kind,
            created_at: "2026-01-01T00:00:00.000Z".into(),
            text: Some("hi".into()),
            agent: None,
            model: None,
            tool: None,
            divider: None,
            decision: None,
        }
    }

    #[test]
    fn migrates_once_and_reopens() {
        let dir = std::env::temp_dir().join(format!("agents-store-{}", crate::util::uuid()));
        let path = dir.join("agents.db");
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len());
            s.insert_thread(&thread("t1", "2026-01-01T00:00:00.000Z")).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len());
        assert!(s.thread("t1").unwrap().is_some());
        drop(s);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn threads_items_approvals_sessions() {
        let s = Store::in_memory().unwrap();
        s.insert_thread(&thread("old", "2026-01-01T00:00:00.000Z")).unwrap();
        s.insert_thread(&thread("new", "2026-01-02T00:00:00.000Z")).unwrap();
        let mut t = s.thread("old").unwrap().unwrap();
        t.archived_at = Some("2026-01-03T00:00:00.000Z".into());
        t.title = "Plans".into();
        t.pinned = true;
        s.save_thread(&t).unwrap();
        assert_eq!(s.threads(false).unwrap().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["new"]);
        assert_eq!(s.threads(true).unwrap().len(), 2);
        let back = s.thread("old").unwrap().unwrap();
        assert!(back.pinned && back.title == "Plans");
        assert_eq!((back.cwd.as_deref(), back.role, back.parent_id.as_deref()), (None, None, None));

        let mut boss = thread("boss", "2026-01-04T00:00:00.000Z");
        boss.cwd = Some("/Users/me/site".into());
        boss.role = Some(ThreadRole::Orchestrator);
        s.insert_thread(&boss).unwrap();
        let mut w = thread("w1", "2026-01-05T00:00:00.000Z");
        w.role = Some(ThreadRole::Worker);
        w.parent_id = Some("boss".into());
        w.cwd = boss.cwd.clone();
        s.insert_thread(&w).unwrap();
        assert_eq!(s.thread("boss").unwrap().unwrap(), boss);
        assert_eq!(s.workers("boss").unwrap(), vec![w]);
        assert!(s.workers("new").unwrap().is_empty());

        s.insert_item(&item("i1", "new", ItemKind::User)).unwrap();
        let mut tool = item("i2", "new", ItemKind::Tool);
        tool.tool = Some(ToolInfo { name: "Bash".into(), title: "Ran ls".into(), detail: None, status: ToolStatus::Running, output: None, feedhq: false, threads: vec![] });
        s.insert_item(&tool).unwrap();
        tool.tool.as_mut().unwrap().status = ToolStatus::Done;
        s.save_item(&tool).unwrap();
        let items = s.items("new").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "i1");
        assert_eq!(items[1].tool.as_ref().unwrap().status, ToolStatus::Done);

        let a = Approval { id: "a1".into(), thread_id: "new".into(), item_id: Some("i2".into()), title: "Run a command".into(), detail: "ls".into(), options: vec![Decision::Allow, Decision::Deny] };
        s.insert_approval(&a, "2026-01-01T00:00:00.000Z").unwrap();
        assert_eq!(s.open_approvals("new").unwrap(), vec![a.clone()]);
        s.resolve_approval("a1", Some(Decision::Allow), "2026-01-01T00:00:01.000Z").unwrap();
        assert!(s.open_approvals("new").unwrap().is_empty());

        assert_eq!(s.session("new", "claude").unwrap(), None);
        s.set_session("new", "claude", "s1", "x").unwrap();
        s.set_session("new", "claude", "s2", "y").unwrap();
        assert_eq!(s.session("new", "claude").unwrap().as_deref(), Some("s2"));
        s.clear_session("new", "claude").unwrap();
        assert_eq!(s.session("new", "claude").unwrap(), None);

        s.set_meta("agents", "[]").unwrap();
        assert_eq!(s.meta("agents").unwrap().as_deref(), Some("[]"));

        // Deleting a chat takes its items with it
        assert!(s.delete_thread("new").unwrap());
        assert!(s.items("new").unwrap().is_empty());
        assert!(!s.delete_thread("new").unwrap());
    }

    #[test]
    fn upgrades_a_first_version_file() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute("INSERT INTO threads (id, agent, model, created_at, updated_at) VALUES ('t', 'claude', 'opus', 'a', 'a')", []).unwrap();
        let s = Store::init(conn).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len());
        let t = s.thread("t").unwrap().unwrap();
        assert_eq!((t.cwd, t.role, t.parent_id), (None, None, None));
    }

    #[test]
    fn restart_recovery() {
        let s = Store::in_memory().unwrap();
        let mut t = thread("t", "2026-01-01T00:00:00.000Z");
        t.status = ThreadStatus::Waiting;
        s.insert_thread(&t).unwrap();
        let a = Approval { id: "a".into(), thread_id: "t".into(), item_id: None, title: "x".into(), detail: "y".into(), options: vec![Decision::Allow] };
        s.insert_approval(&a, "now").unwrap();
        assert_eq!(s.recover_after_restart("later").unwrap(), vec!["t".to_string()]);
        assert_eq!(s.thread("t").unwrap().unwrap().status, ThreadStatus::Idle);
        assert!(s.open_approvals("t").unwrap().is_empty());
    }
}
