//! The local MCP relay: agents talk to `http://127.0.0.1:<port>/mcp/<secret>`
//! and the relay forwards each POST to `<FeedHQ>/api/mcp` with a fresh
//! access token, so no agent ever sees a FeedHQ token. Any other path is
//! refused; GET and DELETE answer 405 like FeedHQ does.

use std::{
    io::BufReader,
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use serde_json::{json, Value};

use crate::{
    http::{read_request, write_response, HttpRequest},
    oauth::{FeedhqAuth, TokenError},
    util::{b64url, random_bytes},
};

/// Request headers passed on to FeedHQ.
const FORWARD: &[&str] = &["content-type", "accept", "mcp-session-id", "mcp-protocol-version", "last-event-id"];
/// Response headers passed back.
const BACK: &[&str] = &["content-type", "mcp-session-id"];

pub struct Relay {
    url: String,
    addr: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
}

impl Relay {
    pub fn start(auth: Arc<FeedhqAuth>) -> std::io::Result<Relay> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let path = format!("/mcp/{}", b64url(&random_bytes(32)));
        let url = format!("http://{addr}{path}");
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                let (auth, path, stopping) = (auth.clone(), path.clone(), stopping.clone());
                std::thread::spawn(move || serve(conn, &auth, &path, &stopping));
            }
        });
        Ok(Relay { url, addr, stop })
    }

    /// The URL agents are given.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn stop(&self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            // Wake the accept loop so it sees the flag.
            let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve(conn: TcpStream, auth: &FeedhqAuth, path: &str, stopping: &AtomicBool) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(300)));
    let Ok(read_half) = conn.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let mut writer = conn;
    loop {
        let req = match read_request(&mut reader) {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(_) => {
                let _ = write_response(&mut writer, 400, &[], b"", false);
                return;
            }
        };
        if stopping.load(Ordering::SeqCst) {
            return;
        }
        let keep = req.keep_alive();
        let (status, headers, body) = answer(&req, auth, path);
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        if write_response(&mut writer, status, &headers, &body, keep).is_err() || !keep {
            return;
        }
    }
}

type Answer = (u16, Vec<(String, String)>, Vec<u8>);

/// A JSON-RPC error for the request's id (200), or 202 for a notification:
/// a 401 would send the agent off on its own OAuth discovery against us.
fn rpc_error(body: &[u8], message: &str) -> Answer {
    let id = serde_json::from_slice::<Value>(body).ok().and_then(|v| v.get("id").cloned());
    match id {
        Some(id) => {
            let out = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32001, "message": message}});
            (200, vec![("Content-Type".into(), "application/json".into())], out.to_string().into_bytes())
        }
        None => (202, vec![], Vec::new()),
    }
}

fn answer(req: &HttpRequest, auth: &FeedhqAuth, path: &str) -> Answer {
    if req.path != path {
        return (404, vec![], Vec::new());
    }
    // Browsers send Origin; agents don't. A page must never reach FeedHQ through us.
    if req.header("origin").is_some() {
        return (403, vec![], Vec::new());
    }
    if req.method != "POST" {
        return (405, vec![("Allow".into(), "POST".into())], Vec::new());
    }
    let token = match auth.access_token() {
        Ok(t) => t,
        Err(TokenError::SignedOut(_)) => return rpc_error(&req.body, "FeedHQ isn't connected in Billable. Connect it on the Agents page."),
        Err(TokenError::Unavailable(m)) => return rpc_error(&req.body, &m),
    };
    match forward(req, auth.origin(), &token) {
        Ok(a) if a.0 == 401 => match auth.refresh_after_rejection(&token) {
            Ok(fresh) => forward(req, auth.origin(), &fresh).unwrap_or_else(|e| rpc_error(&req.body, &e)),
            Err(e) => rpc_error(&req.body, &e.to_string()),
        },
        Ok(a) => a,
        Err(e) => rpc_error(&req.body, &e),
    }
}

fn forward(req: &HttpRequest, origin: &str, token: &str) -> Result<Answer, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(format!("Billable/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut r = agent.post(format!("{origin}/api/mcp")).header("Authorization", format!("Bearer {token}"));
    for name in FORWARD {
        if let Some(v) = req.header(name) {
            r = r.header(*name, v);
        }
    }
    if req.header("content-type").is_none() {
        r = r.header("Content-Type", "application/json");
    }
    let mut res = r.send(&req.body[..]).map_err(|e| format!("Couldn't reach FeedHQ ({e})."))?;
    let status = res.status().as_u16();
    let headers: Vec<(String, String)> = BACK
        .iter()
        .filter_map(|name| res.headers().get(*name).and_then(|v| v.to_str().ok()).map(|v| (name.to_string(), v.to_string())))
        .collect();
    let body = res.body_mut().read_to_vec().map_err(|e| format!("FeedHQ's answer was cut off ({e})."))?;
    Ok((status, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{refresh_account, MemorySecrets, SecretStore};
    use std::{io::{Read, Write}, sync::Mutex};

    /// A fake FeedHQ: the token endpoint hands out fhq_at_1, fhq_at_2…; the
    /// MCP endpoint answers 401 to `fhq_at_1` and echoes the request
    /// otherwise.
    fn fake_feedhq() -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let keep = log.clone();
        std::thread::spawn(move || {
            let mut issued = 0;
            for s in l.incoming() {
                let s = s.unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut w = s;
                while let Ok(Some(req)) = read_request(&mut r) {
                    let auth = req.header("authorization").unwrap_or("").to_string();
                    keep.lock().unwrap().push(format!("{} {} {}", req.method, req.path, auth));
                    if req.path == "/api/oauth/token" {
                        issued += 1;
                        let body = format!(r#"{{"access_token":"fhq_at_{issued}","expires_in":3600,"refresh_token":"fhq_rt_{issued}"}}"#);
                        write_response(&mut w, 200, &[("Content-Type", "application/json")], body.as_bytes(), true).unwrap();
                    } else if auth == "Bearer fhq_at_1" {
                        write_response(&mut w, 401, &[("Content-Type", "application/json")], br#"{"error":"expired"}"#, true).unwrap();
                    } else {
                        let echo = json!({"jsonrpc":"2.0","id":1,"result":{"saw": String::from_utf8_lossy(&req.body), "session": req.header("mcp-session-id")}});
                        write_response(&mut w, 200, &[("Content-Type", "application/json"), ("Mcp-Session-Id", "sess-1")], echo.to_string().as_bytes(), true).unwrap();
                    }
                }
            }
        });
        (origin, log)
    }

    fn call(url: &str, method: &str, path_override: Option<&str>, body: &str, extra: &str) -> (u16, String) {
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let path = path_override.map(str::to_string).unwrap_or(format!("/{path}"));
        let mut s = TcpStream::connect(host).unwrap();
        write!(s, "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let status = out[9..12].parse().unwrap();
        let body = out.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
        (status, body)
    }

    #[test]
    fn forwards_with_fresh_tokens_and_refuses_the_rest() {
        let (origin, log) = fake_feedhq();
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        secrets.set(&refresh_account(&origin), "fhq_rt_0").unwrap();
        let auth = Arc::new(FeedhqAuth::new(&origin, secrets.clone()));
        let relay = Relay::start(auth).unwrap();
        let url = relay.url().to_string();
        assert!(url.starts_with("http://127.0.0.1:") && url.contains("/mcp/"));

        // First token (fhq_at_1) is refused upstream: refresh once and retry
        let (status, body) = call(&url, "POST", None, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#, "Mcp-Session-Id: abc\r\n");
        assert_eq!(status, 200, "{body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["result"]["saw"], json!(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#));
        assert_eq!(v["result"]["session"], json!("abc"));
        assert_eq!(secrets.get(&refresh_account(&origin)).as_deref(), Some("fhq_rt_2"));
        let seen = log.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![
                "POST /api/oauth/token ".to_string(),
                "POST /api/mcp Bearer fhq_at_1".to_string(),
                "POST /api/oauth/token ".to_string(),
                "POST /api/mcp Bearer fhq_at_2".to_string(),
            ]
        );

        assert_eq!(call(&url, "GET", None, "", "").0, 405);
        assert_eq!(call(&url, "POST", Some("/mcp/wrong"), "{}", "").0, 404);
        assert_eq!(call(&url, "POST", Some("/api/mcp"), "{}", "").0, 404);
        assert_eq!(call(&url, "POST", None, "{}", "Origin: https://evil.example\r\n").0, 403);
        relay.stop();
    }

    #[test]
    fn not_connected_is_a_json_rpc_error() {
        let auth = Arc::new(FeedhqAuth::new("http://127.0.0.1:9", Arc::new(MemorySecrets::default())));
        let relay = Relay::start(auth).unwrap();
        let (status, body) = call(relay.url(), "POST", None, r#"{"jsonrpc":"2.0","id":"x","method":"initialize"}"#, "");
        assert_eq!(status, 200);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["id"], json!("x"));
        assert!(v["error"]["message"].as_str().unwrap().contains("isn't connected"));
        assert_eq!(call(relay.url(), "POST", None, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "").0, 202);
    }
}
