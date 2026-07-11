//! MCP (Model Context Protocol) server over stdio.
//!
//! Newline-delimited JSON-RPC 2.0: one message per line on stdin, one
//! response per line on stdout. Exposes a single `distill` tool so agents
//! (Claude Code, etc.) can compress network CLI output before reading it.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub fn serve_stdio() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle_message(&line) {
            let _ = writeln!(stdout, "{resp}");
            let _ = stdout.flush();
        }
    }
}

/// Handle one JSON-RPC message. Returns None for notifications (no id),
/// which must not be answered.
pub fn handle_message(line: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            ))
        }
    };
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = match msg.get("id") {
        Some(v) if !v.is_null() => v.clone(),
        _ => return None, // notification (e.g. notifications/initialized) or response
    };

    let result = match method {
        "initialize" => {
            let requested = msg
                .pointer("/params/protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            json!({
                "protocolVersion": requested,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "netdistill", "version": env!("CARGO_PKG_VERSION")}
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": [distill_spec(), diff_spec()]}),
        "tools/call" => {
            let name = msg
                .pointer("/params/name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = msg
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let budget = args
                .get("budget")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let as_json = args.get("format").and_then(|v| v.as_str()) == Some("json");
            let kind = match args.get("kind").and_then(|v| v.as_str()) {
                Some(name) => match crate::detect::Kind::from_name(name) {
                    Some(k) => Some(k),
                    None => {
                        return Some(error_response(id, -32602, &format!("unknown kind: {name}")))
                    }
                },
                None => None,
            };
            let out = match name {
                "distill" => {
                    let Some(input) = args.get("input").and_then(|v| v.as_str()) else {
                        return Some(error_response(
                            id,
                            -32602,
                            "missing required argument: input",
                        ));
                    };
                    let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);
                    crate::distill_to_string(input, full, budget, as_json, kind)
                }
                "diff" => {
                    let (Some(before), Some(after)) = (
                        args.get("before").and_then(|v| v.as_str()),
                        args.get("after").and_then(|v| v.as_str()),
                    ) else {
                        return Some(error_response(
                            id,
                            -32602,
                            "missing required arguments: before, after",
                        ));
                    };
                    crate::diff_to_string(before, after, budget, as_json, kind)
                }
                _ => return Some(error_response(id, -32602, &format!("unknown tool: {name}"))),
            };
            json!({"content": [{"type": "text", "text": out}], "isError": false})
        }
        _ => {
            return Some(error_response(
                id,
                -32601,
                &format!("method not found: {method}"),
            ))
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string())
}

fn diff_spec() -> Value {
    json!({
        "name": "diff",
        "description": "Diff two snapshots of the same network command output (or config) and return only what changed: new/resolved alerts, added/removed/changed entries. Two healthy identical snapshots collapse to 'no changes detected'. Use for before/after comparisons around maintenance or incidents.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "before": {"type": "string", "description": "Earlier snapshot (raw CLI output or config)"},
                "after": {"type": "string", "description": "Later snapshot of the same command"},
                "budget": {"type": "integer", "description": "Approximate output token budget"},
                "format": {"type": "string", "enum": ["text", "json"], "description": "Output format (default text)"},
                "kind": {"type": "string", "description": "Force a specific parser instead of auto-detection"}
            },
            "required": ["before", "after"]
        }
    })
}

fn distill_spec() -> Value {
    json!({
        "name": "distill",
        "description": "Distill network CLI output (show commands, configs, whole terminal session logs) into a compact LLM-friendly digest: anomalies first, aggregates instead of dumps, explicit elision notes. Typically reduces tokens by 90-99%. Use this before reading any large router/switch output.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Raw network CLI output, config, or terminal session log"
                },
                "full": {
                    "type": "boolean",
                    "description": "Do not elide healthy/normal entries (default false)"
                },
                "budget": {
                    "type": "integer",
                    "description": "Approximate output token budget; tables are trimmed to fit"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "json"],
                    "description": "Output format (default text)"
                },
                "kind": {
                    "type": "string",
                    "enum": ["bgp-summary", "bgp-table", "route-table", "interfaces", "ospf-neighbors", "isis-neighbors", "lldp-neighbors", "config", "junos-config", "generic"],
                    "description": "Force a specific parser instead of auto-detection"
                }
            },
            "required": ["input"]
        }
    })
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}
