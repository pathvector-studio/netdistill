use netdistill::render::render_body;
use netdistill::{diff, distill, filter_section, Options};

const FULL: Options = Options {
    full: true,
    kind: None,
};

fn fx(name: &str) -> String {
    std::fs::read_to_string(format!("fixtures/{name}")).unwrap()
}

// ---- diff drift damping ----

#[test]
fn drift_only_rows_are_suppressed() {
    let before = fx("frr_bgp_summary.txt");
    // prefix count drifts 0.06%, uptime column identical — pure noise
    let after = before.replace(
        "5w4d           184231      512 transit-a",
        "5w4d           184340      512 transit-a",
    );
    let docs = diff::diff_docs(distill(&before, &FULL), distill(&after, &FULL));
    let out = render_body(&docs);
    assert!(!out.contains("184231→184340"), "{out}");
    assert!(
        out.contains("changed only by counter/uptime drift"),
        "{out}"
    );
    // ... but --drift 0 reports it
    let docs = diff::diff_docs_with(distill(&before, &FULL), distill(&after, &FULL), 0);
    let out = render_body(&docs);
    assert!(out.contains("184231→184340"), "{out}");
}

#[test]
fn big_counter_jumps_survive_damping() {
    let before = fx("frr_bgp_summary.txt");
    // 184231 -> 92000: a 50% prefix drop is signal, not drift
    let after = before.replace(
        "5w4d           184231      512 transit-a",
        "5w4d            92000      512 transit-a",
    );
    let out = render_body(&diff::diff_docs(
        distill(&before, &FULL),
        distill(&after, &FULL),
    ));
    assert!(out.contains("184231→92000"), "{out}");
}

#[test]
fn timestamp_only_alert_changes_do_not_flap() {
    let before = fx("frr_bgp_summary.txt");
    // the recently-reset peer's uptime ticks forward; alert text changes but
    // it is the same condition — must not produce a NEW/RESOLVED pair
    let after = before.replace("00:42:11", "00:55:02");
    let out = render_body(&diff::diff_docs(
        distill(&before, &FULL),
        distill(&after, &FULL),
    ));
    assert!(!out.contains("NEW:"), "{out}");
    assert!(!out.contains("RESOLVED:"), "{out}");
    assert!(out.contains("no changes detected"), "{out}");
}

// ---- --section drill-down ----

#[test]
fn section_reemits_one_table_in_full() {
    let mut docs = distill(&fx("ios_route_vrf_all.txt"), &FULL);
    filter_section(&mut docs, "next-hops");
    let out = render_body(&docs);
    assert!(out.contains("TOP NEXT-HOPS"), "{out}");
    // other tables and prose are gone
    assert!(!out.contains("TABLES/VRFs"), "{out}");
    assert!(!out.contains("SUMMARY"), "{out}");
}

#[test]
fn section_miss_lists_available_tables() {
    let mut docs = distill(&fx("ios_route_vrf_all.txt"), &FULL);
    filter_section(&mut docs, "nonexistent");
    let out = render_body(&docs);
    assert!(out.contains("no table matching 'nonexistent'"), "{out}");
    assert!(out.contains("available tables:"), "{out}");
    assert!(out.contains("TOP NEXT-HOPS"), "{out}");
}

#[test]
fn http_section_param() {
    let (status, _, out) = netdistill::http::route(
        "POST",
        "/v1/distill",
        "section=next-hops",
        "text/plain",
        &fx("ios_route_vrf_all.txt"),
        None,
        None,
    );
    assert_eq!(status, 200);
    assert!(out.contains("TOP NEXT-HOPS"), "{out}");
    assert!(!out.contains("TABLES/VRFs"), "{out}");
}

// ---- serve auth token ----

#[test]
fn token_required_when_configured() {
    let body = fx("frr_bgp_summary.txt");
    // no token -> 401
    let (status, _, out) = netdistill::http::route(
        "POST",
        "/v1/distill",
        "",
        "text/plain",
        &body,
        Some("s3cret"),
        None,
    );
    assert_eq!(status, 401, "{out}");
    // wrong token -> 401
    let (status, _, _) = netdistill::http::route(
        "POST",
        "/v1/distill",
        "",
        "text/plain",
        &body,
        Some("s3cret"),
        Some("nope"),
    );
    assert_eq!(status, 401);
    // right token -> 200
    let (status, _, out) = netdistill::http::route(
        "POST",
        "/v1/distill",
        "",
        "text/plain",
        &body,
        Some("s3cret"),
        Some("s3cret"),
    );
    assert_eq!(status, 200, "{out}");
    // healthz stays open for liveness probes
    let (status, _, _) =
        netdistill::http::route("GET", "/healthz", "", "", "", Some("s3cret"), None);
    assert_eq!(status, 200);
}
