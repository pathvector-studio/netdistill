use netdistill::mcp::handle_message;

/// Unauthenticated shim matching the pre-token route signature.
fn route(m: &str, p: &str, q: &str, c: &str, b: &str) -> (u16, String, String) {
    netdistill::http::route(m, p, q, c, b, None, None)
}

fn fx(name: &str) -> String {
    std::fs::read_to_string(format!("fixtures/{name}")).unwrap()
}

// ---- MCP (stdio JSON-RPC) ----

#[test]
fn mcp_initialize_handshake() {
    let resp = handle_message(
        r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["id"], 0);
    assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(v["result"]["serverInfo"]["name"], "netdistill");
    assert!(v["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn mcp_notifications_get_no_response() {
    assert!(handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
}

#[test]
fn mcp_tools_list_exposes_distill() {
    let resp = handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["tools"][0]["name"], "distill");
    assert_eq!(
        v["result"]["tools"][0]["inputSchema"]["required"][0],
        "input"
    );
}

#[test]
fn mcp_tools_call_distills() {
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "distill", "arguments": {"input": fx("frr_bgp_summary.txt")}}
    });
    let resp = handle_message(&req.to_string()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["result"]["isError"], false);
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("bgp-summary"), "{text}");
    assert!(text.contains("10.0.16.2"), "{text}");
}

#[test]
fn mcp_errors_are_jsonrpc_errors() {
    let resp = handle_message(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["error"]["code"], -32602);

    let resp = handle_message(r#"{"jsonrpc":"2.0","id":4,"method":"bogus/method"}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["error"]["code"], -32601);

    // missing required argument
    let resp = handle_message(
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"distill","arguments":{}}}"#,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert!(v["error"]["message"].as_str().unwrap().contains("input"));
}

// ---- HTTP API ----

#[test]
fn http_healthz() {
    let (status, ctype, body) = route("GET", "/healthz", "", "", "");
    assert_eq!(status, 200);
    assert!(ctype.contains("json"));
    assert!(body.contains("\"ok\":true"));
}

#[test]
fn http_distill_raw_body_with_query() {
    let (status, ctype, body) = route(
        "POST",
        "/v1/distill",
        "format=json&budget=500",
        "text/plain",
        &fx("ios_route_vrf_all.txt"),
    );
    assert_eq!(status, 200);
    assert!(ctype.contains("json"));
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["docs"][0]["kind"] == "route-table", "{body}");
    assert!(v["input_tokens_est"].as_u64().unwrap() > 0);
}

#[test]
fn http_distill_json_envelope() {
    let env = serde_json::json!({"input": fx("frr_bgp_summary.txt"), "budget": 300}).to_string();
    let (status, ctype, body) = route("POST", "/v1/distill", "", "application/json", &env);
    assert_eq!(status, 200);
    assert!(ctype.contains("text/plain"));
    assert!(body.contains("netdistill — input"), "{body}");
    assert!(body.contains("10.0.16.2"), "{body}");
}

#[test]
fn http_bad_requests() {
    let (status, _, body) = route("POST", "/v1/distill", "", "text/plain", "");
    assert_eq!(status, 400);
    assert!(body.contains("empty body"));

    let (status, _, body) = route("POST", "/v1/distill", "", "application/json", "{}");
    assert_eq!(status, 400);
    assert!(body.contains("input"));

    let (status, _, _) = route("GET", "/v2/nope", "", "", "");
    assert_eq!(status, 404);
}
