//! HTTP API server — callable from any language with plain HTTP.
//!
//! POST /v1/distill with the raw CLI output as the body (query params for
//! options), or a JSON envelope {"input": "...", ...}. No auth: bind to
//! localhost (the default) or put it behind your own proxy.

use serde_json::json;
use tiny_http::{Header, Response, Server};

const USAGE: &str = "netdistill HTTP API\n\
\n\
POST /v1/distill    body = raw CLI output (text/plain)\n\
                    query: ?full=1  ?budget=800  ?format=json  ?kind=<parser>\n\
                    or JSON body: {\"input\": \"...\", \"full\": false, \"budget\": 800, \"format\": \"text\"|\"json\", \"kind\": \"...\"}\n\
POST /v1/diff       JSON body: {\"before\": \"...\", \"after\": \"...\", \"budget\": 800, \"format\": \"text\"|\"json\", \"kind\": \"...\"}\n\
                    returns only what changed between two snapshots\n\
GET  /healthz       liveness + version\n";

pub fn serve(listen: &str) {
    let server = Server::http(listen).unwrap_or_else(|e| {
        eprintln!("netdistill: cannot bind {listen}: {e}");
        std::process::exit(1);
    });
    eprintln!("netdistill API listening on http://{listen}  (POST /v1/distill; GET / for usage)");
    let server = std::sync::Arc::new(server);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let s = server.clone();
        handles.push(std::thread::spawn(move || {
            while let Ok(req) = s.recv() {
                handle(req);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
}

fn handle(mut req: tiny_http::Request) {
    let method = req.method().to_string().to_uppercase();
    let url = req.url().to_string();
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url, String::new()),
    };
    let ctype = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("content-type"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);

    let (status, out_type, out) = route(&method, &path, &query, &ctype, &body);
    let header = Header::from_bytes(&b"Content-Type"[..], out_type.as_bytes())
        .expect("static content-type header");
    let _ = req.respond(
        Response::from_string(out)
            .with_status_code(status)
            .with_header(header),
    );
}

/// Pure request router, unit-testable without sockets.
pub fn route(
    method: &str,
    path: &str,
    query: &str,
    content_type: &str,
    body: &str,
) -> (u16, String, String) {
    match (method, path) {
        ("GET", "/healthz") => (
            200,
            "application/json".into(),
            json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}).to_string(),
        ),
        ("GET", "/") => (200, "text/plain; charset=utf-8".into(), USAGE.to_string()),
        ("POST", "/v1/distill") => {
            let (input, full, budget, as_json, kind_name) =
                if content_type.contains("application/json") {
                    let v: serde_json::Value = match serde_json::from_str(body) {
                        Ok(v) => v,
                        Err(e) => return bad_request(&format!("invalid JSON body: {e}")),
                    };
                    let Some(input) = v.get("input").and_then(|i| i.as_str()).map(str::to_string)
                    else {
                        return bad_request("missing required field: input");
                    };
                    (
                        input,
                        v.get("full").and_then(|b| b.as_bool()).unwrap_or(false),
                        v.get("budget").and_then(|b| b.as_u64()).map(|b| b as usize),
                        v.get("format").and_then(|f| f.as_str()) == Some("json"),
                        v.get("kind").and_then(|k| k.as_str()).map(str::to_string),
                    )
                } else {
                    if body.is_empty() {
                        return bad_request("empty body: POST the raw CLI output");
                    }
                    (
                        body.to_string(),
                        matches!(qparam(query, "full"), Some("1") | Some("true") | Some("")),
                        qparam(query, "budget").and_then(|b| b.parse().ok()),
                        qparam(query, "format") == Some("json"),
                        qparam(query, "kind").map(str::to_string),
                    )
                };
            let kind = match kind_name.as_deref() {
                Some(name) => match crate::detect::Kind::from_name(name) {
                    Some(k) => Some(k),
                    None => return bad_request(&format!("unknown kind: {name}")),
                },
                None => None,
            };
            let out = crate::distill_to_string(&input, full, budget, as_json, kind);
            let out_type = if as_json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            };
            (200, out_type.into(), out)
        }
        ("POST", "/v1/diff") => {
            if !content_type.contains("application/json") {
                return bad_request(
                    "POST /v1/diff takes a JSON body: {\"before\": \"...\", \"after\": \"...\"}",
                );
            }
            let v: serde_json::Value = match serde_json::from_str(body) {
                Ok(v) => v,
                Err(e) => return bad_request(&format!("invalid JSON body: {e}")),
            };
            let (Some(before), Some(after)) = (
                v.get("before").and_then(|x| x.as_str()),
                v.get("after").and_then(|x| x.as_str()),
            ) else {
                return bad_request("missing required fields: before, after");
            };
            let as_json = v.get("format").and_then(|f| f.as_str()) == Some("json");
            let budget = v.get("budget").and_then(|b| b.as_u64()).map(|b| b as usize);
            let kind = match v.get("kind").and_then(|k| k.as_str()) {
                Some(name) => match crate::detect::Kind::from_name(name) {
                    Some(k) => Some(k),
                    None => return bad_request(&format!("unknown kind: {name}")),
                },
                None => None,
            };
            let out = crate::diff_to_string(before, after, budget, as_json, kind);
            let out_type = if as_json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            };
            (200, out_type.into(), out)
        }
        _ => (
            404,
            "text/plain; charset=utf-8".into(),
            format!("not found: {method} {path}\n\n{USAGE}"),
        ),
    }
}

fn bad_request(msg: &str) -> (u16, String, String) {
    (
        400,
        "application/json".into(),
        json!({"error": msg}).to_string(),
    )
}

fn qparam<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then_some(v)
    })
}
