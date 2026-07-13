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
                    query: ?full=1  ?budget=800  ?format=text|json|toon|wire  ?kind=<parser>\n\
                    or JSON body: {\"input\": \"...\", \"full\": false, \"budget\": 800, \"format\": \"text\"|\"json\"|\"toon\"|\"wire\", \"kind\": \"...\"}\n\
POST /v1/diff       JSON body: {\"before\": \"...\", \"after\": \"...\", \"budget\": 800, \"format\": \"text\"|\"json\"|\"toon\"|\"wire\", \"kind\": \"...\"}\n\
                    returns only what changed between two snapshots\n\
GET  /healthz       liveness + version\n";

pub fn serve(listen: &str, token: Option<String>) {
    let server = Server::http(listen).unwrap_or_else(|e| {
        eprintln!("netdistill: cannot bind {listen}: {e}");
        std::process::exit(1);
    });
    eprintln!(
        "netdistill API listening on http://{listen}  (POST /v1/distill; GET / for usage; auth: {})",
        if token.is_some() { "bearer token required" } else { "none" }
    );
    let server = std::sync::Arc::new(server);
    let token = std::sync::Arc::new(token);
    let mut handles = Vec::new();
    for _ in 0..4 {
        let s = server.clone();
        let t = token.clone();
        handles.push(std::thread::spawn(move || {
            while let Ok(req) = s.recv() {
                handle(req, t.as_deref());
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
}

fn handle(mut req: tiny_http::Request, expected_token: Option<&str>) {
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
    let auth = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("authorization"))
        .map(|h| h.value.as_str().to_string());
    let provided = auth
        .as_deref()
        .and_then(|a| a.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| qparam(&query, "token").map(str::to_string));
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);

    let (status, out_type, out) = route(
        &method,
        &path,
        &query,
        &ctype,
        &body,
        expected_token,
        provided.as_deref(),
    );
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
    expected_token: Option<&str>,
    provided_token: Option<&str>,
) -> (u16, String, String) {
    // /healthz stays open for liveness probes; everything else needs the token.
    if let Some(expected) = expected_token {
        if path != "/healthz" && provided_token != Some(expected) {
            return (
                401,
                "application/json".into(),
                json!({"error": "unauthorized: pass Authorization: Bearer <token> or ?token="})
                    .to_string(),
            );
        }
    }
    match (method, path) {
        ("GET", "/healthz") => (
            200,
            "application/json".into(),
            json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}).to_string(),
        ),
        ("GET", "/") => (200, "text/plain; charset=utf-8".into(), USAGE.to_string()),
        ("POST", "/v1/distill") => {
            let (input, full, budget, format_name, kind_name, section) =
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
                        v.get("format").and_then(|f| f.as_str()).map(str::to_string),
                        v.get("kind").and_then(|k| k.as_str()).map(str::to_string),
                        v.get("section")
                            .and_then(|s| s.as_str())
                            .map(str::to_string),
                    )
                } else {
                    if body.is_empty() {
                        return bad_request("empty body: POST the raw CLI output");
                    }
                    (
                        body.to_string(),
                        matches!(qparam(query, "full"), Some("1") | Some("true") | Some("")),
                        qparam(query, "budget").and_then(|b| b.parse().ok()),
                        qparam(query, "format").map(str::to_string),
                        qparam(query, "kind").map(str::to_string),
                        qparam(query, "section").map(str::to_string),
                    )
                };
            let format = match parse_format(format_name.as_deref()) {
                Ok(f) => f,
                Err(e) => return bad_request(&e),
            };
            let kind = match kind_name.as_deref() {
                Some(name) => match crate::detect::Kind::from_name(name) {
                    Some(k) => Some(k),
                    None => return bad_request(&format!("unknown kind: {name}")),
                },
                None => None,
            };
            let out =
                crate::distill_to_string(&input, full, budget, format, kind, section.as_deref());
            (200, content_type_for(format), out)
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
            let format = match parse_format(v.get("format").and_then(|f| f.as_str())) {
                Ok(f) => f,
                Err(e) => return bad_request(&e),
            };
            let budget = v.get("budget").and_then(|b| b.as_u64()).map(|b| b as usize);
            let drift = v
                .get("drift")
                .and_then(|d| d.as_u64())
                .map(|d| d as u32)
                .unwrap_or(crate::diff::DEFAULT_DRIFT_PCT);
            let kind = match v.get("kind").and_then(|k| k.as_str()) {
                Some(name) => match crate::detect::Kind::from_name(name) {
                    Some(k) => Some(k),
                    None => return bad_request(&format!("unknown kind: {name}")),
                },
                None => None,
            };
            let out = crate::diff_to_string(before, after, budget, format, kind, drift);
            (200, content_type_for(format), out)
        }
        _ => (
            404,
            "text/plain; charset=utf-8".into(),
            format!("not found: {method} {path}\n\n{USAGE}"),
        ),
    }
}

/// Resolve an optional `format` string to a `Format` (default text).
fn parse_format(name: Option<&str>) -> Result<crate::Format, String> {
    match name {
        None => Ok(crate::Format::Text),
        Some(n) => crate::Format::from_name(n)
            .ok_or_else(|| format!("unknown format: {n} (expected text, json, toon, or wire)")),
    }
}

fn content_type_for(format: crate::Format) -> String {
    if format == crate::Format::Json {
        "application/json".into()
    } else {
        "text/plain; charset=utf-8".into()
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
