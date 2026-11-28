//! A tiny HTTP/1.1 reader and writer for the local MCP relay (and the fake
//! FeedHQ in tests). Content-Length and chunked bodies, keep-alive.

use std::io::{BufRead, BufReader, Read, Write};

pub const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn keep_alive(&self) -> bool {
        !self.header("connection").is_some_and(|c| c.eq_ignore_ascii_case("close"))
    }
}

fn read_line(r: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    let n = r.by_ref().take(16 * 1024).read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if !buf.ends_with(b"\n") {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "header line too long"));
    }
    Ok(Some(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string()))
}

/// Read one request; `Ok(None)` when the client closed the connection.
pub fn read_request(r: &mut BufReader<impl Read>) -> std::io::Result<Option<HttpRequest>> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    let line = loop {
        match read_line(r)? {
            None => return Ok(None),
            Some(l) if l.is_empty() => continue,
            Some(l) => break l,
        }
    };
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().ok_or_else(|| bad("no method"))?.to_string(), parts.next().ok_or_else(|| bad("no path"))?.to_string());
    let mut headers = Vec::new();
    loop {
        let l = read_line(r)?.ok_or_else(|| bad("headers cut off"))?;
        if l.is_empty() {
            break;
        }
        if headers.len() > 100 {
            return Err(bad("too many headers"));
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut req = HttpRequest { method, path, headers, body: Vec::new() };
    if req.header("transfer-encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked")) {
        loop {
            let size_line = read_line(r)?.ok_or_else(|| bad("chunk cut off"))?;
            let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16).map_err(|_| bad("bad chunk size"))?;
            if size == 0 {
                // trailers
                while let Some(l) = read_line(r)? {
                    if l.is_empty() {
                        break;
                    }
                }
                break;
            }
            if req.body.len() + size > MAX_BODY {
                return Err(bad("body too large"));
            }
            let start = req.body.len();
            req.body.resize(start + size, 0);
            r.read_exact(&mut req.body[start..])?;
            let mut crlf = [0u8; 2];
            r.read_exact(&mut crlf)?;
        }
    } else if let Some(len) = req.header("content-length") {
        let len: usize = len.parse().map_err(|_| bad("bad content-length"))?;
        if len > MAX_BODY {
            return Err(bad("body too large"));
        }
        req.body = vec![0u8; len];
        r.read_exact(&mut req.body)?;
    }
    Ok(Some(req))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

pub fn write_response(w: &mut impl Write, status: u16, headers: &[(&str, &str)], body: &[u8], keep_alive: bool) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nConnection: {}\r\n", reason(status), body.len(), if keep_alive { "keep-alive" } else { "close" });
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_length_and_chunked_bodies() {
        let raw = b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhelloPOST /b HTTP/1.1\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let a = read_request(&mut r).unwrap().unwrap();
        assert_eq!((a.method.as_str(), a.path.as_str(), a.body.as_slice()), ("POST", "/a", &b"hello"[..]));
        assert!(a.keep_alive());
        let b = read_request(&mut r).unwrap().unwrap();
        assert_eq!(b.body, b"abcde");
        assert!(!b.keep_alive());
        assert!(read_request(&mut r).unwrap().is_none());
        let mut out = Vec::new();
        write_response(&mut out, 405, &[("Allow", "POST")], b"", false).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("HTTP/1.1 405 Method Not Allowed\r\n") && s.contains("Allow: POST\r\n"));
    }
}
