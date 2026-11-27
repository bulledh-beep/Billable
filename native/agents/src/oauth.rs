//! Billable's sign-in to FeedHQ's connector: a public OAuth client
//! (`billable-mac`, PKCE S256, redirect `billable://oauth`). The refresh
//! token lives in the [`SecretStore`] (the Keychain in the app); access
//! tokens stay in memory and are refreshed when under 5 minutes are left.
//! FeedHQ replaces the refresh token on every refresh, so refreshes are
//! serialised.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    secrets::{refresh_account, SecretStore},
    util::{b64url, form, query_pairs, random_bytes, url_encode},
};

pub const CLIENT_ID: &str = "billable-mac";
pub const REDIRECT_URI: &str = "billable://oauth";
/// Refresh when an access token has less than this left.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(5 * 60);

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn challenge_for(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

pub fn pkce() -> Pkce {
    let verifier = b64url(&random_bytes(32));
    let challenge = challenge_for(&verifier);
    Pkce { verifier, challenge }
}

pub fn new_state() -> String {
    b64url(&random_bytes(24))
}

/// `<origin>/api/mcp`, the resource the tokens are for.
pub fn resource(origin: &str) -> String {
    format!("{}/api/mcp", origin.trim_end_matches('/'))
}

/// FeedHQ's own name for itself (its OAuth `issuer`), read from
/// `<origin>/.well-known/oauth-authorization-server`. The site can be
/// reached at more than one address, but tokens are only granted for the
/// issuer's `/api/mcp`, so sign-in, refresh and the relay all use it. Only
/// a bare https origin (or localhost) is accepted.
pub fn discover_issuer(origin: &str) -> Option<String> {
    let origin = origin.trim_end_matches('/');
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(8))).user_agent(format!("Billable/{}", env!("CARGO_PKG_VERSION"))).build().into();
    let mut res = agent.get(format!("{origin}/.well-known/oauth-authorization-server")).header("Accept", "application/json").call().ok()?;
    let body: Value = serde_json::from_str(&res.body_mut().read_to_string().ok()?).ok()?;
    issuer_from_metadata(&body)
}

pub fn issuer_from_metadata(body: &Value) -> Option<String> {
    let issuer = body.get("issuer")?.as_str()?.trim_end_matches('/');
    let bare = crate::util::origin_of(issuer)?;
    let local = bare.starts_with("http://localhost") || bare.starts_with("http://127.0.0.1");
    (bare == issuer && (bare.starts_with("https://") || local)).then_some(bare)
}

/// The Allow page opens on `page_origin`, the address the person is signed in
/// to inside Billable's window (it may be an alias such as
/// content-hq-live.vercel.app); the token is for the issuer's connector.
pub fn authorize_url(page_origin: &str, issuer: &str, challenge: &str, state: &str) -> String {
    let origin = page_origin.trim_end_matches('/');
    let issuer = issuer.trim_end_matches('/');
    format!(
        "{origin}/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}&resource={}",
        url_encode(CLIENT_ID),
        url_encode(REDIRECT_URI),
        url_encode(challenge),
        url_encode(state),
        url_encode(&resource(issuer))
    )
}

#[derive(Debug, Default, PartialEq)]
pub struct Redirect {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
    pub iss: Option<String>,
}

/// `billable://oauth?code=…&state=…` (or `?error=…`).
pub fn parse_redirect(url: &str) -> Option<Redirect> {
    let rest = url.strip_prefix("billable://")?;
    let path = rest.split(['?', '#']).next().unwrap_or("");
    if path.trim_end_matches('/') != "oauth" {
        return None;
    }
    let mut r = Redirect::default();
    for (k, v) in query_pairs(url) {
        let v = Some(v).filter(|v| !v.is_empty());
        match k.as_str() {
            "code" => r.code = v,
            "state" => r.state = v,
            "error" => r.error = v,
            "error_description" => r.error_description = v,
            "iss" => r.iss = v,
            _ => {}
        }
    }
    Some(r)
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenError {
    /// No connection, or FeedHQ ended it: connect again.
    SignedOut(String),
    /// Couldn't reach FeedHQ, or it had a problem; try again later.
    Unavailable(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::SignedOut(m) | TokenError::Unavailable(m) => f.write_str(m),
        }
    }
}

struct Access {
    token: String,
    expires: Instant,
}

pub type OnChange = Arc<dyn Fn(bool) + Send + Sync>;

/// FeedHQ's tokens for one origin.
pub struct FeedhqAuth {
    origin: String,
    secrets: Arc<dyn SecretStore>,
    access: Mutex<Option<Access>>,
    on_change: Mutex<Option<OnChange>>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(format!("Billable/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn account_key(origin: &str) -> String {
    format!("account {origin}")
}

impl FeedhqAuth {
    pub fn new(origin: &str, secrets: Arc<dyn SecretStore>) -> FeedhqAuth {
        FeedhqAuth { origin: origin.trim_end_matches('/').to_string(), secrets, access: Mutex::new(None), on_change: Mutex::new(None) }
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Hear when the connection starts or ends (also when FeedHQ ends it).
    pub fn set_on_change(&self, f: OnChange) {
        if let Ok(mut c) = self.on_change.lock() {
            *c = Some(f);
        }
    }

    fn changed(&self, connected: bool) {
        let f = self.on_change.lock().ok().and_then(|c| c.clone());
        if let Some(f) = f {
            f(connected);
        }
    }

    pub fn connected(&self) -> bool {
        self.secrets.get(&refresh_account(&self.origin)).is_some()
    }

    pub fn account(&self) -> Option<String> {
        self.secrets.get(&account_key(&self.origin)).filter(|a| !a.is_empty())
    }

    fn token_call(&self, pairs: &[(&str, &str)]) -> Result<Value, TokenError> {
        let res = agent()
            .post(format!("{}/api/oauth/token", self.origin))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .send(form(pairs).as_bytes());
        let mut res = res.map_err(|e| TokenError::Unavailable(format!("Couldn't reach FeedHQ ({e}).")))?;
        let status = res.status().as_u16();
        let body: Value = serde_json::from_str(&res.body_mut().read_to_string().unwrap_or_default()).unwrap_or(Value::Null);
        let desc = body.get("error_description").and_then(Value::as_str).map(str::to_string);
        match status {
            200..=299 if body.get("access_token").and_then(Value::as_str).is_some() => Ok(body),
            400 | 401 => {
                let code = body.get("error").and_then(Value::as_str).unwrap_or("");
                if matches!(code, "invalid_grant" | "invalid_client" | "unauthorized_client") || status == 401 {
                    Err(TokenError::SignedOut(desc.unwrap_or_else(|| "FeedHQ ended this connection. Connect again.".into())))
                } else {
                    Err(TokenError::Unavailable(desc.unwrap_or_else(|| format!("FeedHQ refused the sign-in ({code})."))))
                }
            }
            _ => Err(TokenError::Unavailable(desc.unwrap_or_else(|| format!("FeedHQ had a problem (error {status})."))))
        }
    }

    fn keep(&self, body: &Value) -> Result<String, TokenError> {
        let access = body.get("access_token").and_then(Value::as_str).unwrap_or("").to_string();
        let refresh = body.get("refresh_token").and_then(Value::as_str).unwrap_or("");
        let secs = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
        if !refresh.is_empty() {
            self.secrets
                .set(&refresh_account(&self.origin), refresh)
                .map_err(|e| TokenError::Unavailable(format!("Billable couldn't keep the FeedHQ sign-in in the Keychain ({e}).")))?;
        }
        if let Some(account) = body.get("account").or_else(|| body.get("email")).and_then(Value::as_str) {
            let _ = self.secrets.set(&account_key(&self.origin), account);
        }
        if let Ok(mut a) = self.access.lock() {
            *a = Some(Access { token: access.clone(), expires: Instant::now() + Duration::from_secs(secs) });
        }
        Ok(access)
    }

    /// Swap the sign-in code for tokens.
    pub fn exchange_code(&self, code: &str, verifier: &str) -> Result<(), TokenError> {
        let resource = resource(&self.origin);
        let body = self.token_call(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("client_id", CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("resource", &resource),
        ])?;
        self.keep(&body)?;
        self.changed(true);
        Ok(())
    }

    fn refresh_locked(&self, slot: &mut Option<Access>) -> Result<String, TokenError> {
        let Some(refresh) = self.secrets.get(&refresh_account(&self.origin)) else {
            return Err(TokenError::SignedOut("FeedHQ isn't connected in Billable.".into()));
        };
        let resource = resource(&self.origin);
        match self.token_call(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", CLIENT_ID), ("resource", &resource)]) {
            Ok(body) => {
                *slot = None;
                // `keep` takes the lock itself; hand the new token back through it.
                let access = body.get("access_token").and_then(Value::as_str).unwrap_or("").to_string();
                let secs = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
                if let Some(r) = body.get("refresh_token").and_then(Value::as_str).filter(|r| !r.is_empty()) {
                    self.secrets
                        .set(&refresh_account(&self.origin), r)
                        .map_err(|e| TokenError::Unavailable(format!("Billable couldn't keep the FeedHQ sign-in in the Keychain ({e}).")))?;
                }
                *slot = Some(Access { token: access.clone(), expires: Instant::now() + Duration::from_secs(secs) });
                Ok(access)
            }
            Err(TokenError::SignedOut(m)) => {
                *slot = None;
                self.secrets.delete(&refresh_account(&self.origin));
                self.secrets.delete(&account_key(&self.origin));
                self.changed(false);
                Err(TokenError::SignedOut(m))
            }
            Err(e) => Err(e),
        }
    }

    /// A usable access token, refreshed when it's close to expiring.
    pub fn access_token(&self) -> Result<String, TokenError> {
        let mut slot = self.access.lock().map_err(|_| TokenError::Unavailable("token lock poisoned".into()))?;
        if let Some(a) = slot.as_ref() {
            if a.expires.saturating_duration_since(Instant::now()) > REFRESH_MARGIN {
                return Ok(a.token.clone());
            }
        }
        self.refresh_locked(&mut slot)
    }

    /// A new access token even if the one held looks fine (FeedHQ said 401).
    /// `rejected` is the token that failed: if another request already
    /// replaced it, that one is used instead of refreshing again.
    pub fn refresh_after_rejection(&self, rejected: &str) -> Result<String, TokenError> {
        let mut slot = self.access.lock().map_err(|_| TokenError::Unavailable("token lock poisoned".into()))?;
        if let Some(a) = slot.as_ref() {
            if a.token != rejected && a.expires.saturating_duration_since(Instant::now()) > REFRESH_MARGIN {
                return Ok(a.token.clone());
            }
        }
        self.refresh_locked(&mut slot)
    }

    /// Forget the connection here and tell FeedHQ to revoke it (best effort).
    pub fn disconnect(&self) {
        let refresh = self.secrets.get(&refresh_account(&self.origin));
        self.secrets.delete(&refresh_account(&self.origin));
        self.secrets.delete(&account_key(&self.origin));
        if let Ok(mut a) = self.access.lock() {
            *a = None;
        }
        if let Some(token) = refresh {
            let _ = agent()
                .post(format!("{}/api/oauth/revoke", self.origin))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .send(form(&[("token", &token), ("client_id", CLIENT_ID)]).as_bytes());
        }
        self.changed(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        http::{read_request, write_response},
        secrets::MemorySecrets,
    };
    use std::{io::BufReader, net::TcpListener, sync::atomic::{AtomicUsize, Ordering}};

    #[test]
    fn issuer_metadata() {
        use serde_json::json;
        assert_eq!(issuer_from_metadata(&json!({"issuer": "https://feedhq.app/"})).as_deref(), Some("https://feedhq.app"));
        assert_eq!(issuer_from_metadata(&json!({"issuer": "http://localhost:3000"})).as_deref(), Some("http://localhost:3000"));
        assert_eq!(issuer_from_metadata(&json!({"issuer": "http://feedhq.app"})), None);
        assert_eq!(issuer_from_metadata(&json!({"issuer": "https://feedhq.app/tenant"})), None);
        assert_eq!(issuer_from_metadata(&json!({})), None);
        assert_eq!(discover_issuer("http://127.0.0.1:9"), None);
    }

    #[test]
    fn pkce_s256() {
        // RFC 7636 appendix B
        assert_eq!(challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let p = pkce();
        assert_eq!(p.verifier.len(), 43);
        assert_eq!(p.challenge.len(), 43);
        assert_ne!(pkce().verifier, p.verifier);
    }

    #[test]
    fn authorize_and_redirect() {
        let url = authorize_url("https://feedhq.app/", "https://feedhq.app", "CH", "ST");
        assert_eq!(
            url,
            "https://feedhq.app/oauth/authorize?response_type=code&client_id=billable-mac&redirect_uri=billable%3A%2F%2Foauth&code_challenge=CH&code_challenge_method=S256&state=ST&resource=https%3A%2F%2Ffeedhq.app%2Fapi%2Fmcp"
        );
        let alias = authorize_url("https://content-hq-live.vercel.app", "https://feedhq.app", "CH", "ST");
        assert!(alias.starts_with("https://content-hq-live.vercel.app/oauth/authorize?"));
        assert!(alias.ends_with("&resource=https%3A%2F%2Ffeedhq.app%2Fapi%2Fmcp"));
        let r = parse_redirect("billable://oauth?code=fhq_code_1&state=ST&iss=https%3A%2F%2Ffeedhq.app").unwrap();
        assert_eq!((r.code.as_deref(), r.state.as_deref(), r.iss.as_deref()), (Some("fhq_code_1"), Some("ST"), Some("https://feedhq.app")));
        let r = parse_redirect("billable://oauth/?error=access_denied&state=ST").unwrap();
        assert_eq!(r.error.as_deref(), Some("access_denied"));
        assert!(parse_redirect("billable://connect?token=x").is_none());
    }

    /// A fake token endpoint: answers each POST with the next reply and
    /// records the form bodies.
    fn fake(replies: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let keep = seen.clone();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let (s, _) = l.accept().unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let req = read_request(&mut r).unwrap().unwrap();
                keep.lock().unwrap().push(format!("{} {}", req.path, String::from_utf8_lossy(&req.body)));
                let mut w = s;
                write_response(&mut w, status, &[("Content-Type", "application/json")], body.as_bytes(), false).unwrap();
            }
        });
        (origin, seen)
    }

    fn tokens(n: u32, expires: u64) -> (u16, String) {
        (200, format!(r#"{{"access_token":"fhq_at_{n}","token_type":"Bearer","expires_in":{expires},"refresh_token":"fhq_rt_{n}","scope":"feedhq"}}"#))
    }

    #[test]
    fn exchange_refresh_rotation_and_sign_out() {
        let (origin, seen) = fake(vec![tokens(1, 120), tokens(2, 3600), (400, r#"{"error":"invalid_grant","error_description":"That refresh token isn't valid any more. Connect again."}"#.into())]);
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let auth = FeedhqAuth::new(&origin, secrets.clone());
        let changes = Arc::new(AtomicUsize::new(0));
        let c = changes.clone();
        auth.set_on_change(Arc::new(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(!auth.connected());
        auth.exchange_code("fhq_code_x", "verifier").unwrap();
        assert!(auth.connected());
        assert_eq!(secrets.get(&refresh_account(&origin)).as_deref(), Some("fhq_rt_1"));
        // 120 s left is under the 5-minute margin: refresh, and the refresh token rotates
        assert_eq!(auth.access_token().unwrap(), "fhq_at_2");
        assert_eq!(secrets.get(&refresh_account(&origin)).as_deref(), Some("fhq_rt_2"));
        // Fresh enough now: no call
        assert_eq!(auth.access_token().unwrap(), "fhq_at_2");
        // Another request already refreshed past the rejected token
        assert_eq!(auth.refresh_after_rejection("fhq_at_1").unwrap(), "fhq_at_2");
        // FeedHQ ended it
        assert!(matches!(auth.refresh_after_rejection("fhq_at_2"), Err(TokenError::SignedOut(_))));
        assert!(!auth.connected());
        assert_eq!(changes.load(Ordering::SeqCst), 2);
        let seen = seen.lock().unwrap();
        assert!(seen[0].starts_with("/api/oauth/token grant_type=authorization_code&code=fhq_code_x&code_verifier=verifier&client_id=billable-mac&redirect_uri=billable%3A%2F%2Foauth&resource="), "{}", seen[0]);
        assert!(seen[1].contains("grant_type=refresh_token&refresh_token=fhq_rt_1&client_id=billable-mac"), "{}", seen[1]);
        assert!(seen[2].contains("refresh_token=fhq_rt_2"));
    }

    #[test]
    fn unreachable_is_not_signed_out() {
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        secrets.set(&refresh_account("http://127.0.0.1:9"), "fhq_rt_x").unwrap();
        let auth = FeedhqAuth::new("http://127.0.0.1:9", secrets);
        assert!(matches!(auth.access_token(), Err(TokenError::Unavailable(_))));
        assert!(auth.connected());
    }
}
