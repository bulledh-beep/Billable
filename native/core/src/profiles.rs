//! Profiles: separate databases under one data folder (port of
//! `src/main/profiles.ts`). `profiles.json` holds the list and the active id;
//! each profile's data lives in `profiles/<id>/billable.db`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::js::{base64_encode, js_round, js_trim};
use crate::time::Clock;

pub const DEFAULT_PROFILE_ID: &str = "default";
pub const DEFAULT_PROFILE_NAME: &str = "Default";
pub const DEFAULT_PROFILE_COLOR: &str = "#F5A623";
const PROFILES_DIRNAME: &str = "profiles";
const PROFILES_INDEX_FILENAME: &str = "profiles.json";
const DB_FILENAME: &str = "billable.db";
const LEGACY_DB_BASENAME: &str = "billable.db";
const AVATAR_MAX_DIMENSION: f64 = 256.0;

/// One profile in `profiles.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub created_at: String,
    /// Base64-encoded PNG (no `data:` prefix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// Fields this version doesn't know, kept on rewrite.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// `profile:list` result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
    pub active: Option<Profile>,
}

/// The `profiles.json` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilesFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Billable's data folder: `$BILLABLE_USER_DATA` if set (the dev/test hook
/// the Electron app also honours), else `~/Library/Application Support/Billable`.
pub fn user_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BILLABLE_USER_DATA").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    home.join("Library").join("Application Support").join("Billable")
}

/// Profiles in one data folder.
#[derive(Debug, Clone)]
pub struct Profiles {
    user_data: PathBuf,
    clock: Clock,
}

impl Profiles {
    /// Profiles in `user_data` (see [`user_data_dir`]).
    pub fn new(user_data: impl Into<PathBuf>, clock: Clock) -> Self {
        Profiles { user_data: user_data.into(), clock }
    }

    /// Profiles in [`user_data_dir`] with the system clock.
    pub fn from_env() -> Self {
        Self::new(user_data_dir(), Clock::system())
    }

    pub fn user_data_dir(&self) -> &Path {
        &self.user_data
    }

    pub fn index_path(&self) -> PathBuf {
        self.user_data.join(PROFILES_INDEX_FILENAME)
    }

    fn profiles_root(&self) -> PathBuf {
        self.user_data.join(PROFILES_DIRNAME)
    }

    pub fn profile_dir(&self, id: &str) -> PathBuf {
        self.profiles_root().join(id)
    }

    pub fn profile_db_path(&self, id: &str) -> PathBuf {
        self.profile_dir(id).join(DB_FILENAME)
    }

    /// The index, or `None` if it's missing or unreadable.
    pub fn read_index(&self) -> Option<ProfilesFile> {
        let raw = fs::read_to_string(self.index_path()).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn write_index(&self, data: &ProfilesFile) -> Result<()> {
        fs::create_dir_all(&self.user_data)?;
        // JSON.stringify(data, null, 2)
        fs::write(self.index_path(), serde_json::to_string_pretty(data)?)?;
        Ok(())
    }

    fn index(&self) -> Result<ProfilesFile> {
        self.read_index().ok_or_else(|| anyhow!("Profiles index missing"))
    }

    /// One-time move of the original single-database layout into a Default
    /// profile. Run at startup before opening any database.
    pub fn migrate_legacy_if_needed(&self) -> Result<()> {
        if self.index_path().exists() {
            return Ok(());
        }
        let legacy = self.user_data.join(LEGACY_DB_BASENAME);
        fs::create_dir_all(self.profile_dir(DEFAULT_PROFILE_ID))?;
        let target = self.profile_db_path(DEFAULT_PROFILE_ID);
        if legacy.exists() && !target.exists() {
            fs::rename(&legacy, &target)?;
            for ext in ["-wal", "-shm"] {
                let sib = PathBuf::from(format!("{}{ext}", legacy.display()));
                if sib.exists() {
                    fs::rename(&sib, format!("{}{ext}", target.display()))?;
                }
            }
        }
        self.write_index(&ProfilesFile {
            version: Some(Value::from(1)),
            active: Some(DEFAULT_PROFILE_ID.into()),
            profiles: vec![Profile {
                id: DEFAULT_PROFILE_ID.into(),
                name: DEFAULT_PROFILE_NAME.into(),
                color: DEFAULT_PROFILE_COLOR.into(),
                created_at: self.clock.now_iso(),
                avatar: None,
                extra: Default::default(),
            }],
            extra: Default::default(),
        })
    }

    pub fn list_profiles(&self) -> Vec<Profile> {
        self.read_index().map(|i| i.profiles).unwrap_or_default()
    }

    pub fn get_active_profile_id(&self) -> String {
        self.read_index().and_then(|i| i.active).unwrap_or_else(|| DEFAULT_PROFILE_ID.into())
    }

    pub fn get_active_profile(&self) -> Option<Profile> {
        let idx = self.read_index()?;
        let active = idx.active.clone();
        idx.profiles.into_iter().find(|p| Some(&p.id) == active.as_ref())
    }

    /// `{ profiles, active }` (`profile:list`).
    pub fn list(&self) -> ProfileList {
        ProfileList { profiles: self.list_profiles(), active: self.get_active_profile() }
    }

    /// The `profile:switch` flow: stop a running timer in the current
    /// profile, make `id` active, open its database in place of `db`, and
    /// pick up any timer it has. Returns the now-active profile. (The UI
    /// then reloads its views, as the Electron app reloads its windows.)
    pub fn switch_profile(
        &self,
        id: &str,
        db: &mut crate::db::Db,
        timer: &mut crate::timer::TimerManager,
    ) -> Result<Option<Profile>> {
        if timer.active().is_some() {
            timer.stop(db, None)?;
        }
        self.set_active_profile_id(id)?;
        *db = crate::db::Db::open(self.profile_db_path(id), db.clock().clone())?;
        timer.sync_from_database(db)?;
        Ok(self.get_active_profile())
    }

    /// Make `id` the active profile. The caller reopens the database (and
    /// stops/re-syncs the timer) as `profile:switch` does.
    pub fn set_active_profile_id(&self, id: &str) -> Result<()> {
        let mut idx = self.index()?;
        if !idx.profiles.iter().any(|p| p.id == id) {
            bail!("Profile {id} not found");
        }
        idx.active = Some(id.into());
        self.write_index(&idx)
    }

    pub fn create_profile(&self, name: &str, color: Option<&str>) -> Result<Profile> {
        if js_trim(name).is_empty() {
            bail!("Name is required");
        }
        let mut idx = self.index()?;
        let existing: Vec<&str> = idx.profiles.iter().map(|p| p.id.as_str()).collect();
        let id = generate_profile_id(name, &existing);
        let profile = Profile {
            id: id.clone(),
            name: js_trim(name).to_string(),
            color: color.filter(|c| !c.is_empty()).unwrap_or(DEFAULT_PROFILE_COLOR).to_string(),
            created_at: self.clock.now_iso(),
            avatar: None,
            extra: Default::default(),
        };
        fs::create_dir_all(self.profile_dir(&id))?;
        idx.profiles.push(profile.clone());
        self.write_index(&idx)?;
        Ok(profile)
    }

    fn edit(&self, id: &str, f: impl FnOnce(&mut Profile) -> Result<()>) -> Result<Profile> {
        let mut idx = self.index()?;
        let profile = idx.profiles.iter_mut().find(|p| p.id == id).ok_or_else(|| anyhow!("Profile {id} not found"))?;
        f(profile)?;
        let out = profile.clone();
        self.write_index(&idx)?;
        Ok(out)
    }

    pub fn rename_profile(&self, id: &str, name: &str) -> Result<Profile> {
        if js_trim(name).is_empty() {
            bail!("Name is required");
        }
        self.edit(id, |p| {
            p.name = js_trim(name).to_string();
            Ok(())
        })
    }

    pub fn update_profile_color(&self, id: &str, color: &str) -> Result<Profile> {
        self.edit(id, |p| {
            p.color = color.to_string();
            Ok(())
        })
    }

    /// Set a profile's picture from an image file: scaled down to at most
    /// 256 px on the long edge, stored as base64 PNG in `profiles.json`.
    /// Uses macOS `sips` for decoding and scaling.
    pub fn set_profile_avatar(&self, id: &str, source_path: impl AsRef<Path>) -> Result<Profile> {
        let source = source_path.as_ref();
        // Same checks, in the same order, as the original
        let idx = self.index()?;
        if !idx.profiles.iter().any(|p| p.id == id) {
            bail!("Profile {id} not found");
        }
        if !source.exists() {
            bail!("Image not found: {}", source.display());
        }
        let png = avatar_png(source)?;
        self.set_profile_avatar_png(id, &png)
    }

    /// Set a profile's picture from PNG bytes the caller already scaled.
    pub fn set_profile_avatar_png(&self, id: &str, png: &[u8]) -> Result<Profile> {
        let encoded = base64_encode(png);
        self.edit(id, |p| {
            p.avatar = Some(encoded);
            Ok(())
        })
    }

    pub fn clear_profile_avatar(&self, id: &str) -> Result<Profile> {
        self.edit(id, |p| {
            p.avatar = None;
            Ok(())
        })
    }

    /// Remove a profile and its folder. Not the last one, and not the active one.
    pub fn delete_profile(&self, id: &str) -> Result<()> {
        let mut idx = self.index()?;
        if idx.profiles.len() <= 1 {
            bail!("Cannot delete the last profile");
        }
        if idx.active.as_deref() == Some(id) {
            bail!("Switch to another profile before deleting this one");
        }
        idx.profiles.retain(|p| p.id != id);
        self.write_index(&idx)?;
        let dir = self.profile_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

/// profiles.ts `slugify`.
pub fn slugify(name: &str) -> String {
    let lowered = if name.is_empty() { "profile".to_string() } else { name.to_lowercase() };
    let lowered = js_trim(&lowered);
    let mut out = String::new();
    let mut in_run = false;
    for c in lowered.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    let trimmed: String = out.trim_matches('-').chars().take(32).collect();
    if trimmed.is_empty() {
        "profile".into()
    } else {
        trimmed
    }
}

/// Four random base-36 characters (`Math.random().toString(36).slice(2, 6)`).
fn random_suffix() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let mut n = h.finish();
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    (0..4)
        .map(|_| {
            let c = digits[(n % 36) as usize] as char;
            n /= 36;
            c
        })
        .collect()
}

fn generate_profile_id(name: &str, existing: &[&str]) -> String {
    let base = slugify(name);
    let mut id = base.clone();
    while existing.contains(&id.as_str()) || id == DEFAULT_PROFILE_ID {
        id = format!("{base}-{}", random_suffix());
    }
    id
}

/// Decode, scale to ≤ 256 px on the long edge, and encode as PNG with `sips`.
fn avatar_png(source: &Path) -> Result<Vec<u8>> {
    let unreadable = || anyhow!("Could not read image — unsupported format or corrupt file");
    let out = Command::new("/usr/bin/sips")
        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
        .arg(source)
        .output()
        .map_err(|_| unreadable())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let dim = |key: &str| -> Option<f64> {
        text.lines().find_map(|l| l.trim().strip_prefix(key).and_then(|v| v.trim().parse::<f64>().ok()))
    };
    let (Some(w), Some(h)) = (dim("pixelWidth:"), dim("pixelHeight:")) else { return Err(unreadable()) };
    if !out.status.success() || w <= 0.0 || h <= 0.0 {
        return Err(unreadable());
    }

    let tmp = std::env::temp_dir().join(format!("billable-avatar-{}-{}.png", std::process::id(), random_suffix()));
    let mut cmd = Command::new("/usr/bin/sips");
    cmd.args(["-s", "format", "png"]);
    let longest = w.max(h);
    if longest > AVATAR_MAX_DIMENSION {
        let ratio = AVATAR_MAX_DIMENSION / longest;
        let (nw, nh) = (js_round(w * ratio) as i64, js_round(h * ratio) as i64);
        cmd.args(["-z", &nh.to_string(), &nw.to_string()]);
    }
    let status = cmd.arg(source).arg("--out").arg(&tmp).output().map_err(|_| unreadable())?;
    let bytes = fs::read(&tmp).ok().filter(|_| status.status.success());
    let _ = fs::remove_file(&tmp);
    bytes.filter(|b| !b.is_empty()).ok_or_else(unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_js() {
        assert_eq!(slugify("Captain Todd B"), "captain-todd-b");
        assert_eq!(slugify("  Hello!! World  "), "hello-world");
        assert_eq!(slugify("Café"), "caf");
        assert_eq!(slugify("!!!"), "profile");
        assert_eq!(slugify(""), "profile");
        assert_eq!(slugify("abcdefghijklmnopqrstuvwxyz abcdefghij"), "abcdefghijklmnopqrstuvwxyz-abcde");
    }
}
