//! App-wide state: the open profile's database, a revision number that
//! bumps whenever data changes (screens reload when it moves, like the
//! renderer's `billing-changed` events), the timer, and UI preferences.

use std::path::PathBuf;

use billable_core::{
    db::Db,
    models::TimerRow,
    profiles::{Profile, Profiles},
    time::Clock,
};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};

/// Preferences the Electron app kept in the renderer's localStorage. They
/// live in `ui-prefs.json` beside profiles.json, shared by all profiles.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Prefs {
    /// "dark" | "light" | "auto" (billable.theme)
    #[serde(default)]
    pub theme: Option<String>,
    /// "billable" | "content" (billable.suite.app)
    #[serde(default)]
    pub suite_app: Option<String>,
    /// billable.lastSeenVersion
    #[serde(default)]
    pub last_seen_version: Option<String>,
    /// billable.update.dismissed
    #[serde(default)]
    pub update_dismissed: Option<String>,
    /// billable.lineStyle (the invoice builder's line style)
    #[serde(default)]
    pub line_style: Option<String>,
}

/// The timer as the toolbar sees it: one running entry, or one paused.
#[derive(Clone, Default)]
pub struct Timer {
    pub active: Option<TimerRow>,
    pub paused: Option<TimerRow>,
}

pub struct Store {
    pub dir: PathBuf,
    pub profiles: Profiles,
    pub db: Db,
    pub rev: u64,
    pub timer: Timer,
    pub prefs: Prefs,
    /// profiles.json as last read, with its modification time: the sidebar
    /// asks for the profile on every frame, so it's only re-read when the
    /// file changes.
    profile_cache: std::cell::RefCell<Option<(Option<std::time::SystemTime>, Option<billable_core::profiles::ProfilesFile>)>>,
}

impl Global for Store {}

impl Store {
    pub fn open() -> anyhow::Result<Store> {
        let dir = billable_core::profiles::user_data_dir();
        std::fs::create_dir_all(&dir)?;
        let profiles = Profiles::new(dir.clone(), Clock::system());
        profiles.migrate_legacy_if_needed()?;
        let active = profiles.get_active_profile_id();
        let db = Db::open_profile(&profiles, &active, Clock::system())?;
        let timer = Timer { active: db.get_active_timer()?, paused: db.get_paused_timer()? };
        let prefs_path = dir.join("ui-prefs.json");
        let prefs = match std::fs::read_to_string(&prefs_path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            // First native launch: carry over what the Electron app kept in localStorage.
            Err(_) => {
                let p = electron_prefs(&dir);
                if let Ok(json) = serde_json::to_string_pretty(&p) {
                    let _ = std::fs::write(&prefs_path, json);
                }
                p
            }
        };
        Ok(Store { dir, profiles, db, rev: 1, timer, prefs, profile_cache: Default::default() })
    }

    /// profiles.json, read again only when it has changed on disk.
    fn profiles_file(&self) -> Option<billable_core::profiles::ProfilesFile> {
        let mtime = std::fs::metadata(self.profiles.index_path()).and_then(|m| m.modified()).ok();
        let mut cache = self.profile_cache.borrow_mut();
        if let Some((at, file)) = cache.as_ref() {
            if *at == mtime && mtime.is_some() {
                return file.clone();
            }
        }
        let file = self.profiles.read_index();
        *cache = Some((mtime, file.clone()));
        file
    }

    pub fn active_profile(&self) -> Profile {
        let file = self.profiles_file();
        let list = file.as_ref().map(|f| f.profiles.clone()).unwrap_or_default();
        let active = file.as_ref().and_then(|f| f.active.clone());
        list.iter()
            .find(|p| Some(&p.id) == active.as_ref())
            .cloned()
            .or_else(|| list.into_iter().next())
            .unwrap_or_else(|| serde_json::from_value(serde_json::json!({"id": "default", "name": "Profile", "color": "#F5A623", "created_at": ""})).expect("profile"))
    }

    pub fn profile_list(&self) -> Vec<Profile> {
        self.profiles_file().map(|f| f.profiles).unwrap_or_default()
    }

    pub fn save_prefs(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.prefs) {
            let _ = std::fs::write(self.dir.join("ui-prefs.json"), json);
        }
    }

    /// The timer entry showing in the toolbar: running, else paused.
    pub fn timer_entry(&self) -> Option<&TimerRow> {
        self.timer.active.as_ref().or(self.timer.paused.as_ref())
    }

    pub fn timer_running(&self) -> bool {
        self.timer.active.as_ref().is_some_and(|e| e.entry.paused_at.is_none())
    }

    pub fn timer_paused(&self) -> bool {
        self.timer.active.is_none() && self.timer.paused.is_some()
    }

    fn refresh_timer(&mut self) {
        if let (Ok(active), Ok(paused)) = (self.db.get_active_timer(), self.db.get_paused_timer()) {
            self.timer = Timer { active, paused };
        }
    }
}

pub fn store(cx: &App) -> &Store {
    cx.global::<Store>()
}

pub fn db(cx: &App) -> &Db {
    &cx.global::<Store>().db
}

pub fn rev(cx: &App) -> u64 {
    cx.global::<Store>().rev
}

/// Something changed in the data: screens reload, the timer is reread, and
/// the window redraws.
pub fn data_changed(cx: &mut App) {
    let s = cx.global_mut::<Store>();
    s.rev += 1;
    s.refresh_timer();
    cx.refresh_windows();
}

/// Run a write against the database; on success, everything reloads. On
/// failure the error comes back for the caller to show.
pub fn write<T>(cx: &mut App, f: impl FnOnce(&Db) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let result = f(&cx.global::<Store>().db);
    if result.is_ok() {
        data_changed(cx);
    }
    result
}

/// Billing items that need a look (the sidebar's Billing badge leaves out
/// duplicate-client hints). Cached per data revision.
pub fn attention_count(cx: &mut App) -> usize {
    thread_local! {
        static CACHE: std::cell::Cell<(u64, usize)> = const { std::cell::Cell::new((0, 0)) };
    }
    let rev = rev(cx);
    let (at, n) = CACHE.with(|c| c.get());
    if at == rev {
        return n;
    }
    let n = db(cx).get_billing_overview().map(|o| o.attention.iter().filter(|a| a.kind != "duplicate_clients").count()).unwrap_or(0);
    CACHE.with(|c| c.set((rev, n)));
    n
}

/// Open another profile's database and make it active.
pub fn switch_profile(id: &str, cx: &mut App) -> anyhow::Result<()> {
    let s = cx.global_mut::<Store>();
    s.profiles.set_active_profile_id(id)?;
    let db = Db::open_profile(&s.profiles, id, Clock::system())?;
    s.db = db;
    data_changed(cx);
    Ok(())
}

/// Read the Electron app's localStorage prefs (Chromium keeps them in a
/// LevelDB log under "Local Storage/leveldb"). Each write appends the key,
/// a length byte, 0x01 (Latin-1) and the value, so the last write wins.
/// Best effort: anything unreadable is simply left at its default.
fn electron_prefs(dir: &std::path::Path) -> Prefs {
    let mut bytes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir.join("Local Storage").join("leveldb")) {
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "log" || x == "ldb")).collect();
        // Older tables first, the live log last, so later writes win.
        files.sort_by_key(|p| (p.extension().is_some_and(|x| x == "log"), p.file_name().map(|n| n.to_os_string())));
        for f in files {
            if let Ok(b) = std::fs::read(f) {
                bytes.extend_from_slice(&b);
            }
        }
    }
    let get = |key: &str| -> Option<String> {
        let needle = format!("billable.{key}");
        let needle = needle.as_bytes();
        let mut found = None;
        let mut i = 0;
        while let Some(pos) = bytes[i..].windows(needle.len()).position(|w| w == needle) {
            let at = i + pos + needle.len();
            i = i + pos + 1;
            // The key must end here (not a longer key with this prefix).
            let (Some(&len), Some(&1)) = (bytes.get(at), bytes.get(at + 1)) else { continue };
            let len = len as usize;
            if len < 2 || len > 64 {
                continue;
            }
            let Some(v) = bytes.get(at + 2..at + 1 + len) else { continue };
            if v.iter().all(|c| c.is_ascii_graphic()) {
                found = Some(String::from_utf8_lossy(v).into_owned());
            }
        }
        found
    };
    Prefs {
        theme: get("theme").filter(|v| ["dark", "light", "auto"].contains(&v.as_str())),
        suite_app: get("suite.app").filter(|v| v == "billable" || v == "content"),
        last_seen_version: get("lastSeenVersion"),
        update_dismissed: get("update.dismissed"),
        line_style: get("lineStyle"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_electron_local_storage() {
        let dir = std::env::temp_dir().join(format!("billable-ls-test-{}", std::process::id()));
        let ldb = dir.join("Local Storage").join("leveldb");
        std::fs::create_dir_all(&ldb).unwrap();
        let mut log = Vec::new();
        for (k, v) in [("lastSeenVersion", "1.4.0"), ("suite.app", "content"), ("lastSeenVersion", "1.5.3"), ("theme", "dark"), ("suite.app", "billable")] {
            log.extend_from_slice(b"\x1c_file://\x00\x01billable.");
            log.extend_from_slice(k.as_bytes());
            log.push(v.len() as u8 + 1);
            log.push(1);
            log.extend_from_slice(v.as_bytes());
            log.extend_from_slice(b"\xc9\xd3META");
        }
        std::fs::write(ldb.join("000003.log"), log).unwrap();
        let p = super::electron_prefs(&dir);
        assert_eq!(p.last_seen_version.as_deref(), Some("1.5.3"));
        assert_eq!(p.suite_app.as_deref(), Some("billable"));
        assert_eq!(p.theme.as_deref(), Some("dark"));
        assert_eq!(p.line_style, None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Run by hand against a copied user data folder:
    /// BILLABLE_LS_DIR=<dir> cargo test -p billable --bin billable electron_prefs_from_dir -- --ignored --nocapture
    #[test]
    #[ignore]
    fn electron_prefs_from_dir() {
        let dir = std::path::PathBuf::from(std::env::var("BILLABLE_LS_DIR").unwrap());
        println!("{:?}", super::electron_prefs(&dir));
    }
}
