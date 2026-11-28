//! Sign-ins (phone sync's token, FeedHQ's agents sign-in) in a private file
//! in the data folder, readable only by this Mac account.
//!
//! They used to live in the Keychain, but Billable is ad-hoc signed, so every
//! update looks like a new app to the Keychain and macOS asked for the login
//! password at launch. Anything still in the Keychain is moved here the first
//! time it's read (one last prompt), and the Keychain is never asked again.

use std::{
    collections::BTreeMap,
    io::Write as _,
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

const FILE: &str = "sign-ins.json";

/// One read-modify-write at a time (the UI and the agents engine both write).
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    secrets: BTreeMap<String, String>,
    /// Keychain items already looked for, so the Keychain is asked at most
    /// once per item, even if that ask was denied.
    #[serde(default)]
    keychain_checked: Vec<String>,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

fn load(dir: &Path) -> Stored {
    std::fs::read_to_string(path(dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(dir: &Path, stored: &Stored) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{FILE}.tmp"));
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.write_all(serde_json::to_string_pretty(stored)?.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path(dir))?;
    Ok(())
}

/// The secret for `account`, kept in `dir`. `keychain_service` is where an
/// older Billable kept it.
pub fn get(dir: &Path, keychain_service: &str, account: &str) -> Option<String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut stored = load(dir);
    if let Some(s) = stored.secrets.get(account) {
        return Some(s.clone());
    }
    if stored.keychain_checked.iter().any(|a| a == account) {
        return None;
    }
    let old = security_framework::passwords::get_generic_password(keychain_service, account).ok().and_then(|b| String::from_utf8(b).ok());
    stored.keychain_checked.push(account.to_string());
    if let Some(s) = &old {
        stored.secrets.insert(account.to_string(), s.clone());
    }
    if save(dir, &stored).is_ok() && old.is_some() {
        let _ = security_framework::passwords::delete_generic_password(keychain_service, account);
    }
    old
}

pub fn set(dir: &Path, account: &str, secret: &str) -> anyhow::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut stored = load(dir);
    stored.secrets.insert(account.to_string(), secret.to_string());
    if !stored.keychain_checked.iter().any(|a| a == account) {
        // A fresh sign-in replaces whatever an older Billable kept.
        stored.keychain_checked.push(account.to_string());
    }
    save(dir, &stored)
}

pub fn delete(dir: &Path, account: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut stored = load(dir);
    if stored.secrets.remove(account).is_some() {
        let _ = save(dir, &stored);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_secrets_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("billable-secrets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        set(&dir, "a", "one").unwrap();
        set(&dir, "b", "two").unwrap();
        assert_eq!(load(&dir).secrets.get("a").map(String::as_str), Some("one"));
        // Set marks the account checked, so get never reaches the Keychain.
        assert_eq!(get(&dir, "Billable test", "b").as_deref(), Some("two"));
        delete(&dir, "a");
        assert!(load(&dir).secrets.get("a").is_none());
        assert_eq!(get(&dir, "Billable test", "a"), None);
        let mode = std::fs::metadata(path(&dir)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
