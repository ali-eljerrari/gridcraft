//! Localhost JSON-lines control server. Each connection must first authenticate with the
//! per-process token; subsequent lines `{"id":…, "method":…, "params":…}` get one reply line.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use gridcraft_ui_egui::ControlRequest;
use serde_json::{Value as Json, json};

const MAX_LINE: usize = 64 * 1024 * 1024;

fn constant_time_eq(left: &str, right: &str) -> bool {
    let mut diff = left.len() ^ right.len();
    for (a, b) in left.bytes().zip(right.bytes()) {
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

fn valid_auth(msg: &Json, token: &str) -> bool {
    msg.get("method").and_then(Json::as_str) == Some("auth")
        && msg.get("params").and_then(|p| p.get("token")).and_then(Json::as_str).is_some_and(|candidate| constant_time_eq(candidate, token))
}

pub fn start(port: u16, ctx: egui::Context, token: &str) -> std::io::Result<Receiver<ControlRequest>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (tx, rx) = channel();
    let token = token.to_owned();
    log::info!("control channel listening on 127.0.0.1:{port}; authenticate with the token printed below");
    log::info!("control channel token: {token}");
    std::thread::Builder::new().name("control".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let ctx = ctx.clone();
            let token = token.to_owned();
            let _ = std::thread::Builder::new().name("control-conn".into()).spawn(move || serve(stream, tx, ctx, &token));
        }
    })?;
    Ok(rx)
}

fn serve(stream: TcpStream, tx: Sender<ControlRequest>, ctx: egui::Context, token: &str) {
    let Ok(read) = stream.try_clone() else { return };
    let mut out = stream;
    let reader = BufReader::new(read);
    let mut authenticated = false;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_LINE {
            if !authenticated {
                return;
            }
            let _ = writeln!(out, "{}", json!({"ok": false, "error": "request too large"}));
            continue;
        }
        let msg: Json = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                if !authenticated {
                    log::warn!("control connection rejected before authentication: invalid JSON: {e}");
                    return;
                }
                let _ = writeln!(out, "{}", json!({"ok": false, "error": format!("invalid JSON: {e}")}));
                continue;
            }
        };
        if !authenticated {
            if !valid_auth(&msg, token) {
                log::warn!("control connection rejected before authentication");
                return;
            }
            let id = msg.get("id").cloned().unwrap_or(Json::Null);
            if writeln!(out, "{}", json!({"id": id, "ok": true, "result": {"authenticated": true}})).is_err() {
                return;
            }
            authenticated = true;
            continue;
        }
        let id = msg.get("id").cloned().unwrap_or(Json::Null);
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("").to_string();
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let (req, rx) = ControlRequest::new(method, params);
        if tx.send(req).is_err() {
            break;
        }
        // Wake the UI thread: egui only runs frames when something happens.
        ctx.request_repaint();
        let mut reply = rx
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| json!({"ok": false, "error": "timed out waiting for the UI (is the window minimised?)"}));
        if let Some(o) = reply.as_object_mut() {
            o.insert("id".into(), id);
        }
        if writeln!(out, "{reply}").is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, valid_auth};
    use serde_json::json;

    #[test]
    fn token_comparison_requires_equal_content_and_length() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }

    #[test]
    fn authentication_requires_the_auth_method_and_matching_token() {
        assert!(valid_auth(&json!({"method": "auth", "params": {"token": "secret"}}), "secret"));
        assert!(!valid_auth(&json!({"method": "engine.execute", "params": {"token": "secret"}}), "secret"));
        assert!(!valid_auth(&json!({"method": "auth", "params": {"token": "wrong"}}), "secret"));
    }
}
