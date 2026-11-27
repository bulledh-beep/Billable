//! Where the FeedHQ refresh token is kept. The app stores it in the macOS
//! Keychain (service `app.billable.feedhq-agents`); tests keep it in memory.

use std::{collections::HashMap, sync::Mutex};

/// The Keychain service the app uses for FeedHQ agent secrets.
pub const KEYCHAIN_SERVICE: &str = "app.billable.feedhq-agents";

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Option<String>;
    fn set(&self, account: &str, secret: &str) -> anyhow::Result<()>;
    fn delete(&self, account: &str);
}

/// Secrets in memory only (tests, and a fallback when there's no Keychain).
#[derive(Default)]
pub struct MemorySecrets {
    items: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecrets {
    fn get(&self, account: &str) -> Option<String> {
        self.items.lock().ok()?.get(account).cloned()
    }

    fn set(&self, account: &str, secret: &str) -> anyhow::Result<()> {
        self.items.lock().map_err(|_| anyhow::anyhow!("secret store poisoned"))?.insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) {
        if let Ok(mut items) = self.items.lock() {
            items.remove(account);
        }
    }
}

/// The Keychain account for the refresh token of one FeedHQ origin, so a dev
/// FeedHQ never picks up the live one's connection.
pub fn refresh_account(origin: &str) -> String {
    format!("refresh {origin}")
}
