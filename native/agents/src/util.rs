//! Small helpers: times, ids, trimming text, URL encoding.

use base64::Engine as _;

/// Now as ISO 8601 UTC with milliseconds (`2026-11-27T22:17:00.123Z`).
pub fn now_iso() -> String {
    iso(chrono::Utc::now())
}

pub fn iso(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// `n` random bytes from the system's generator.
pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    if getrandom::fill(&mut b).is_err() {
        // The system generator is always there on macOS and Linux; this only
        // keeps ids unique if it somehow isn't.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        for (i, x) in b.iter_mut().enumerate() {
            *x = (t >> ((i % 16) * 8)) as u8 ^ (i as u8).wrapping_mul(151);
        }
    }
    b
}

/// base64url without padding.
pub fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A random UUID v4.
pub fn uuid() -> String {
    let mut b = random_bytes(16);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// An engine id like `t_<uuid>`.
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid())
}

/// At most `n` characters, with an ellipsis when cut.
pub fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// One line (whitespace runs become one space), at most `n` characters.
pub fn one_line(s: &str, n: usize) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    cut(&joined, n)
}

/// The first `n` words of a text, for a fallback title.
pub fn first_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

/// Percent-encode for a URL query value (RFC 3986 unreserved kept).
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Decode a query value (`+` is a space).
pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The decoded `key=value` pairs of a URL's query.
pub fn query_pairs(url: &str) -> Vec<(String, String)> {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    let query = query.split('#').next().unwrap_or("");
    query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

/// A form body (`application/x-www-form-urlencoded`).
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v))).collect::<Vec<_>>().join("&")
}

/// `scheme://host[:port]` of an http(s) address, lowercased, without a path.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("").to_ascii_lowercase();
    if authority.is_empty() {
        return None;
    }
    let authority = match (scheme.as_str(), authority.rsplit_once(':')) {
        ("https", Some((h, "443"))) | ("http", Some((h, "80"))) => h.to_string(),
        _ => authority,
    };
    Some(format!("{scheme}://{authority}"))
}

/// Text of a JSON value: strings as they are, anything else as compact JSON.
pub fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_times() {
        let a = uuid();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert_ne!(uuid(), a);
        assert!(new_id("t").starts_with("t_"));
        let t = now_iso();
        assert_eq!(t.len(), 24, "{t}");
        assert!(t.ends_with('Z'));
    }

    #[test]
    fn text_helpers() {
        assert_eq!(cut("hello", 5), "hello");
        assert_eq!(cut("hello!", 5), "hell…");
        assert_eq!(one_line("a\n  b\tc ", 10), "a b c");
        assert_eq!(first_words("one two three four five six seven", 6), "one two three four five six");
    }

    #[test]
    fn urls() {
        assert_eq!(url_encode("billable://oauth"), "billable%3A%2F%2Foauth");
        assert_eq!(url_decode("a%20b+c%2Fd%zz"), "a b c/d%zz");
        assert_eq!(url_decode("abc%"), "abc%");
        assert_eq!(url_decode("abc%4"), "abc%4");
        let q = query_pairs("billable://oauth?code=abc%2D1&state=x+y#frag");
        assert_eq!(q, vec![("code".to_string(), "abc-1".to_string()), ("state".to_string(), "x y".to_string())]);
        assert_eq!(form(&[("a", "b c"), ("redirect_uri", "billable://oauth")]), "a=b%20c&redirect_uri=billable%3A%2F%2Foauth");
        assert_eq!(origin_of("https://FeedHQ.app:443/home?x").as_deref(), Some("https://feedhq.app"));
        assert_eq!(origin_of("http://127.0.0.1:3000/").as_deref(), Some("http://127.0.0.1:3000"));
        assert_eq!(origin_of("ftp://x"), None);
    }
}
