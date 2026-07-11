use netdistill::detect::{detect, Kind};
use netdistill::render::{render_body, render_text};
use netdistill::{distill, split_sessions, token, Options};

const OPTS: Options = Options {
    full: false,
    kind: None,
};

fn fx(name: &str) -> String {
    std::fs::read_to_string(format!("fixtures/{name}")).unwrap()
}

fn digest(name: &str) -> String {
    render_body(&distill(&fx(name), &OPTS))
}

#[test]
fn detects_kinds() {
    assert_eq!(detect(&fx("frr_bgp_summary.txt")), Kind::BgpSummary);
    assert_eq!(detect(&fx("junos_bgp_summary.txt")), Kind::BgpSummary);
    assert_eq!(detect(&fx("ios_route_vrf_all.txt")), Kind::RouteTable);
    assert_eq!(detect(&fx("junos_show_route.txt")), Kind::RouteTable);
    assert_eq!(detect(&fx("ios_show_interfaces.txt")), Kind::Interfaces);
    assert_eq!(detect(&fx("ios_running_config.txt")), Kind::Config);
    assert_eq!(detect(&fx("generic_log.txt")), Kind::Generic);
}

#[test]
fn frr_bgp_alerts_and_elision() {
    let out = digest("frr_bgp_summary.txt");
    // down peers surfaced
    assert!(out.contains("10.0.16.2"), "{out}");
    assert!(out.contains("10.0.17.2"), "{out}");
    // established-but-empty peer surfaced
    assert!(out.contains("0 prefixes received"), "{out}");
    // healthy peers elided, and the elision is explicit
    assert!(out.contains("healthy Established peers omitted"), "{out}");
    // healthy peer rows not listed
    assert!(!out.contains("10.0.13.2\t"), "{out}");
    // router context preserved
    assert!(out.contains("10.255.0.1"), "{out}");
    assert!(out.contains("64512"), "{out}");
}

#[test]
fn junos_bgp_flaps_and_states() {
    let out = digest("junos_bgp_summary.txt");
    assert!(out.contains("10.20.0.9"), "{out}");
    assert!(out.contains("9 flaps"), "{out}");
    assert!(out.contains("4 flaps"), "{out}");
    // stable established peer should not raise a session alert line
    assert!(!out.contains("peer 10.20.0.1 (AS64520): session"), "{out}");
}

#[test]
fn ios_route_vrf_matrix() {
    let out = digest("ios_route_vrf_all.txt");
    assert!(out.contains("MGMT"), "{out}");
    assert!(out.contains("CUST-A"), "{out}");
    assert!(out.contains("no default route in: MGMT"), "{out}");
    assert!(out.contains("TOP NEXT-HOPS"), "{out}");
    // individual prefixes elided but explicitly noted
    assert!(out.contains("individual prefixes omitted"), "{out}");
}

#[test]
fn junos_route_hidden_alert() {
    let out = digest("junos_show_route.txt");
    assert!(out.contains("hidden route"), "{out}");
    assert!(out.contains("CUST-A.inet.0"), "{out}");
}

#[test]
fn interfaces_alerts() {
    let out = digest("ios_show_interfaces.txt");
    assert!(out.contains("14411 input errors"), "{out}");
    assert!(out.contains("18233 output drops"), "{out}");
    assert!(out.contains("cust-a-handoff"), "{out}");
    assert!(out.contains("DOWN"), "{out}");
    // healthy interfaces elided
    assert!(!out.contains("Loopback0\t"), "{out}");
    assert!(out.contains("healthy interfaces omitted"), "{out}");
}

#[test]
fn config_inventory() {
    let out = digest("ios_running_config.txt");
    assert!(out.contains("edge-tokyo-1"), "{out}");
    assert!(out.contains("AS 64512"), "{out}");
    assert!(out.contains("10.0.13.2"), "{out}");
    assert!(out.contains("att-transit"), "{out}");
    assert!(out.contains("CUST-A"), "{out}");
    // mask converted to prefix length
    assert!(out.contains("10.255.0.1/32"), "{out}");
    // secrets are not echoed
    assert!(!out.contains("$9$"), "{out}");
}

#[test]
fn generic_collapses_duplicates() {
    let out = digest("generic_log.txt");
    assert!(out.contains("[x5]"), "{out}");
}

#[test]
fn session_log_splits_per_command() {
    let mut log = String::new();
    log.push_str("edge-tokyo-1# show ip bgp summary\n");
    log.push_str(&fx("frr_bgp_summary.txt"));
    log.push_str("edge-tokyo-1# show ip route vrf all\n");
    log.push_str(&fx("ios_route_vrf_all.txt"));
    let chunks = split_sessions(&log);
    assert_eq!(chunks.len(), 2, "{chunks:?}");
    let docs = distill(&log, &OPTS);
    assert_eq!(docs.len(), 2);
    assert!(docs[0].headline.contains("show ip bgp summary"));
    assert!(docs[1].headline.contains("show ip route vrf all"));
}

#[test]
fn big_bgp_summary_compresses_hard() {
    // Synthesize a 2000-peer summary: 1997 healthy, 3 broken.
    let mut input = String::from(
        "BGP router identifier 10.255.0.1, local AS number 64512\n\nNeighbor        V         AS   MsgRcvd   MsgSent   TblVer  InQ OutQ  Up/Down State/PfxRcd\n",
    );
    for i in 0..2000u32 {
        let addr = format!("10.{}.{}.{}", i / 65536, (i / 256) % 256, i % 256 + 1);
        let line = if i == 500 {
            format!("{addr}       4      65001         0         0        0    0    0 never    Active\n")
        } else if i == 900 {
            format!(
                "{addr}       4      65002        42        40        0    0    0 1d03h    Idle\n"
            )
        } else if i == 1500 {
            format!("{addr}       4      65003      1211      1209   812734    0    0 00:42:11            0\n")
        } else {
            format!("{addr}       4      65{:03}    182341    182350   812734    0    0 5w4d           1823\n", i % 1000)
        };
        input.push_str(&line);
    }
    let in_tok = token::estimate(&input);
    let out = render_text(distill(&input, &OPTS), in_tok, None);
    let out_tok = token::estimate(&out);
    assert!(
        out_tok * 10 < in_tok,
        "expected >90% reduction, got {in_tok} -> {out_tok}\n{out}"
    );
    // the three broken peers all survive distillation
    assert!(out.contains("10.0.1.245"), "{out}");
    assert!(out.contains("10.0.3.133"), "{out}");
    assert!(out.contains("10.0.5.221"), "{out}");
}

#[test]
fn budget_is_enforced() {
    let out = render_text(
        distill(&fx("ios_route_vrf_all.txt"), &OPTS),
        1000,
        Some(120),
    );
    assert!(token::estimate(&out) <= 200, "{out}");
    assert!(out.contains("trimmed for --budget"), "{out}");
}
