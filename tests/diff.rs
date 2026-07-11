use netdistill::detect::Kind;
use netdistill::render::render_body;
use netdistill::{diff, distill, Options};

const FULL: Options = Options {
    full: true,
    kind: None,
};

fn fx(name: &str) -> String {
    std::fs::read_to_string(format!("fixtures/{name}")).unwrap()
}

fn diff_of(before: &str, after: &str) -> String {
    render_body(&diff::diff_docs(
        distill(before, &FULL),
        distill(after, &FULL),
    ))
}

#[test]
fn identical_snapshots_say_so() {
    let b = fx("frr_bgp_summary.txt");
    let out = diff_of(&b, &b);
    assert!(out.contains("no changes detected"), "{out}");
    // pre-existing alerts are noted as unchanged, not re-raised
    assert!(!out.contains("NEW:"), "{out}");
    assert!(out.contains("alert(s) present in both snapshots"), "{out}");
}

#[test]
fn bgp_peer_down_and_recovered() {
    let before = fx("frr_bgp_summary.txt");
    // 10.0.16.2 recovers; 10.0.12.2 goes down
    let after = before
        .replace(
            "10.0.16.2       4      64515         0         0        0    0    0 never    Active               ix-peer-2",
            "10.0.16.2       4      64515      5123      5100   812734    0    0 01:12:00         8231      512 ix-peer-2",
        )
        .replace(
            "10.0.12.2       4      64513    182341    182350   812734    0    0 5w4d           184231      512 transit-a",
            "10.0.12.2       4      64513    182341    182350   812734    0    0 00:00:41 Idle                 transit-a",
        );
    let out = diff_of(&before, &after);
    assert!(out.contains("NEW: peer 10.0.12.2"), "{out}");
    assert!(out.contains("RESOLVED: peer 10.0.16.2"), "{out}");
    // changed rows carry old→new cells
    assert!(out.contains("Established→Idle"), "{out}");
    assert!(out.contains("Active→Established"), "{out}");
}

#[test]
fn route_added_and_removed() {
    let before = fx("ios_route_vrf_all.txt");
    let after = before
        .replace(
            "B        192.168.11.0/24 [20/0] via 10.0.12.2, 5w4d\n",
            "",
        )
        .replace(
            "S        172.16.0.0/16 [1/0] via 10.0.0.9",
            "S        172.16.0.0/16 [1/0] via 10.0.0.9\nB        192.168.99.0/24 [20/0] via 10.0.13.2, 00:01:11",
        );
    let out = diff_of(&before, &after);
    assert!(out.contains("+\tdefault\t192.168.99.0/24"), "{out}");
    assert!(out.contains("-\tdefault\t192.168.11.0/24"), "{out}");
    assert!(out.contains("unchanged rows omitted"), "{out}");
    // untouched prefixes do not appear
    assert!(!out.contains("10.1.0.0/24"), "{out}");
}

#[test]
fn interface_error_counter_growth() {
    let before = fx("ios_show_interfaces.txt");
    let after = before.replace(
        "14411 input errors, 14200 CRC",
        "99411 input errors, 99200 CRC",
    );
    let out = diff_of(&before, &after);
    assert!(out.contains("14411→99411"), "{out}");
    assert!(out.contains("14200→99200"), "{out}");
    // headline/alert text changes surface as NEW/RESOLVED pair
    assert!(out.contains("NEW: GigabitEthernet0/1"), "{out}");
}

#[test]
fn mcp_diff_tool() {
    let before = fx("frr_bgp_summary.txt");
    let after = before.replace(
        "10.0.16.2       4      64515         0         0        0    0    0 never    Active               ix-peer-2",
        "10.0.16.2       4      64515      5123      5100   812734    0    0 01:12:00         8231      512 ix-peer-2",
    );
    let req = serde_json::json!({
        "jsonrpc": "2.0", "id": 9, "method": "tools/call",
        "params": {"name": "diff", "arguments": {"before": before, "after": after}}
    });
    let resp = netdistill::mcp::handle_message(&req.to_string()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("RESOLVED: peer 10.0.16.2"), "{text}");
}

#[test]
fn mcp_lists_both_tools() {
    let resp = netdistill::mcp::handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let names: Vec<&str> = v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["distill", "diff"]);
}

#[test]
fn http_diff_endpoint() {
    let before = fx("ios_show_interfaces.txt");
    let after = before.replace("Total output drops: 18233", "Total output drops: 55555");
    let env = serde_json::json!({"before": before, "after": after}).to_string();
    let (status, _, out) =
        netdistill::http::route("POST", "/v1/diff", "", "application/json", &env, None, None);
    assert_eq!(status, 200, "{out}");
    assert!(out.contains("18233→55555"), "{out}");

    let (status, _, out) =
        netdistill::http::route("POST", "/v1/diff", "", "application/json", "{}", None, None);
    assert_eq!(status, 400, "{out}");
}

#[test]
fn kind_mismatch_sections_flagged() {
    let docs = diff::diff_docs(
        distill(&fx("frr_bgp_summary.txt"), &FULL),
        distill(&fx("ios_route_vrf_all.txt"), &FULL),
    );
    let out = render_body(&docs);
    assert!(out.contains("only in AFTER"), "{out}");
    assert!(out.contains("only in BEFORE"), "{out}");
    let _ = Kind::BgpSummary; // silence unused import if asserts change
}
