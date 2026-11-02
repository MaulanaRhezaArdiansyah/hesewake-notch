//! Odoo JSON-RPC over WinINet. Blocking: called only from the network thread. The API key is never put in an error text.

use crate::cfg::{parse_url, Endpoint, OdooCfg};
use serde_json::{json, Value};
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Networking::WinInet::*;

const TIMEOUT_MS: u32 = 15_000;
const MAX_BODY: usize = 8_000_000;

/// POST `body` to `{endpoint}/jsonrpc`; the answer's text.
fn post(ep: &Endpoint, body: &str) -> Result<String, String> {
    unsafe {
        let ses = InternetOpenW(w!("HesewakeNotchOdoo"), INTERNET_OPEN_TYPE_PRECONFIG.0, None, None, 0);
        if ses.is_null() {
            return Err("Could not open a connection".to_string());
        }
        let r = post_inner(ses, ep, body);
        let _ = InternetCloseHandle(ses);
        r
    }
}

unsafe fn post_inner(ses: *mut c_void, ep: &Endpoint, body: &str) -> Result<String, String> {
    for opt in [INTERNET_OPTION_RECEIVE_TIMEOUT, INTERNET_OPTION_CONNECT_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT] {
        let ms = TIMEOUT_MS;
        let _ = InternetSetOptionW(Some(ses), opt, Some(&ms as *const u32 as *const c_void), 4);
    }
    let host: Vec<u16> = ep.host.encode_utf16().chain(Some(0)).collect();
    let conn = InternetConnectW(ses, PCWSTR(host.as_ptr()), ep.port, PCWSTR::null(), PCWSTR::null(), INTERNET_SERVICE_HTTP, 0, 0);
    if conn.is_null() {
        return Err(net_error("Could not reach the server"));
    }
    let path: Vec<u16> = format!("{}/jsonrpc", ep.prefix).encode_utf16().chain(Some(0)).collect();
    let mut flags = INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_COOKIES | INTERNET_FLAG_NO_UI;
    if ep.https {
        flags |= INTERNET_FLAG_SECURE;
    }
    let req = HttpOpenRequestW(conn, w!("POST"), PCWSTR(path.as_ptr()), PCWSTR::null(), PCWSTR::null(), None, flags, 0);
    let result = if req.is_null() {
        Err(net_error("Could not make the request"))
    } else {
        let r = send_and_read(req, body);
        let _ = InternetCloseHandle(req);
        r
    };
    let _ = InternetCloseHandle(conn);
    result
}

unsafe fn send_and_read(req: *mut c_void, body: &str) -> Result<String, String> {
    let headers: Vec<u16> = "Content-Type: application/json\r\nAccept: application/json\r\n".encode_utf16().collect();
    if HttpSendRequestW(req, Some(&headers), Some(body.as_ptr() as *const c_void), body.len() as u32).is_err() {
        return Err(net_error("The server did not answer"));
    }
    let mut status = 0u32;
    let mut len = 4u32;
    let _ = HttpQueryInfoW(req, HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER, Some(&mut status as *mut u32 as *mut c_void), &mut len, None);
    let mut out: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let mut n = 0u32;
        if InternetReadFile(req, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut n).is_err() || n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
        if out.len() > MAX_BODY {
            return Err("The answer is too large".to_string());
        }
    }
    // Odoo answers its own errors with HTTP 200 and an "error" object; another code is the server or a proxy.
    if status != 200 && status != 0 {
        return Err(format!("The server answered HTTP {status}"));
    }
    String::from_utf8(out).map_err(|_| "The answer is not UTF-8".to_string())
}

fn net_error(what: &str) -> String {
    let code = unsafe { GetLastError().0 };
    let hint = match code {
        12007 => "address not found",
        12029 | 12030 => "connection refused",
        12002 | 12152 => "timeout",
        12175 | 12157 => "certificate or TLS problem",
        _ => "",
    };
    if hint.is_empty() {
        format!("{what} (code {code})")
    } else {
        format!("{what}: {hint}")
    }
}

/// One JSON-RPC call: its `result`, or Odoo's error.
fn rpc(ep: &Endpoint, service: &str, method: &str, args: Value) -> Result<Value, String> {
    let body = json!({"jsonrpc": "2.0", "method": "call", "params": {"service": service, "method": method, "args": args}, "id": 1}).to_string();
    let text = post(ep, &body)?;
    parse_response(&text)
}

pub fn parse_response(text: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(text).map_err(|_| "The answer is not JSON (wrong URL?)".to_string())?;
    if let Some(e) = v.get("error") {
        let msg = e
            .pointer("/data/message")
            .and_then(Value::as_str)
            .or_else(|| e.get("message").and_then(Value::as_str))
            .unwrap_or("Unknown Odoo error");
        return Err(last_line(msg));
    }
    v.get("result").cloned().ok_or_else(|| "The answer has no result".to_string())
}

/// An Odoo error can be a traceback: keep its last line that says something.
fn last_line(s: &str) -> String {
    let line = s.lines().map(str::trim).rev().find(|l| !l.is_empty()).unwrap_or(s);
    line.chars().take(160).collect()
}

/// A session with one server and one key.
pub struct Session {
    ep: Endpoint,
    db: String,
    uid: u32,
    key: String,
}

impl Session {
    pub fn new(cfg: &OdooCfg, key: &str) -> Result<Session, String> {
        Ok(Session { ep: parse_url(&cfg.url)?, db: cfg.db.clone(), uid: cfg.uid, key: key.to_string() })
    }

    /// `execute_kw`; a read is tried once more when the network failed.
    pub fn call(&self, model: &str, method: &str, args: Value, kwargs: Value) -> Result<Value, String> {
        let a = json!([self.db, self.uid, self.key, model, method, args, kwargs]);
        match rpc(&self.ep, "object", "execute_kw", a.clone()) {
            Err(e) if is_read(method) && is_transport(&e) => rpc(&self.ep, "object", "execute_kw", a),
            other => other,
        }
    }

    pub fn search_read(&self, model: &str, domain: Value, fields: Value, kwargs: Value) -> Result<Vec<Value>, String> {
        let kw = if kwargs.is_object() { kwargs } else { json!({}) };
        let v = self.call(model, "search_read", json!([domain, fields]), kw)?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }
}

fn is_read(m: &str) -> bool {
    matches!(m, "search_read" | "read" | "search" | "search_count" | "fields_get")
}

fn is_transport(e: &str) -> bool {
    e.starts_with("Could not") || e.starts_with("The server did not")
}

/// Log in: the user id, or why not.
pub fn authenticate(cfg: &OdooCfg, key: &str) -> Result<u32, String> {
    let ep = parse_url(&cfg.url)?;
    let v = rpc(&ep, "common", "authenticate", json!([cfg.db, cfg.login, key, {}]))?;
    match v.as_u64() {
        Some(uid) if uid > 0 => Ok(uid as u32),
        _ => Err("Wrong login or API key".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_result_and_errors() {
        assert_eq!(parse_response(r#"{"jsonrpc":"2.0","id":1,"result":32}"#).unwrap(), json!(32));
        let e = parse_response(r#"{"error":{"message":"Odoo Server Error","data":{"message":"Access Denied"}}}"#).unwrap_err();
        assert_eq!(e, "Access Denied");
        let e = parse_response(r#"{"error":{"message":"Odoo Server Error"}}"#).unwrap_err();
        assert_eq!(e, "Odoo Server Error");
        assert!(parse_response("<html>").is_err());
        assert!(parse_response(r#"{"id":1}"#).is_err());
    }

    #[test]
    fn traceback_keeps_last_line() {
        let tb = "Traceback (most recent call last):\n  File \"x\", line 1\nValueError: Invalid field 'kanban_state'\n";
        assert_eq!(last_line(tb), "ValueError: Invalid field 'kanban_state'");
    }

    #[test]
    fn classifies_read_methods() {
        assert!(is_read("search_read") && is_read("read") && !is_read("write"));
        assert!(is_transport("Could not reach the server: timeout") && !is_transport("Access Denied"));
    }
}
