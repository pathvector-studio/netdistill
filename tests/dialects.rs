use netdistill::detect::{detect, Kind};
use netdistill::render::render_body;
use netdistill::{distill, Options};

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
fn detects_dialects() {
    assert_eq!(detect(&fx("eos_bgp_summary.txt")), Kind::BgpSummary);
    assert_eq!(detect(&fx("vrp_display_bgp_peer.txt")), Kind::BgpSummary);
    assert_eq!(detect(&fx("xr_bgp_summary.txt")), Kind::BgpSummary);
    assert_eq!(detect(&fx("nxos_show_ip_route.txt")), Kind::RouteTable);
    assert_eq!(detect(&fx("junos_config_set.txt")), Kind::JunosConfig);
    assert_eq!(detect(&fx("junos_config_hier.txt")), Kind::JunosConfig);
}

#[test]
fn eos_bgp_summary_word_states() {
    let out = digest("eos_bgp_summary.txt");
    assert!(out.contains("4 peers"), "{out}");
    // Estab normalized to Established; healthy peers not alerted
    assert!(out.contains("Established 2"), "{out}");
    assert!(!out.contains("peer 10.0.12.2 (AS64513): session"), "{out}");
    // down peers alerted
    assert!(out.contains("10.0.16.2"), "{out}");
    assert!(out.contains("session Active"), "{out}");
    assert!(out.contains("session Idle"), "{out}");
    // prefix counts picked from the PfxRcd column
    assert!(out.contains("1096575"), "{out}"); // 184231 + 912344
                                               // router id from EOS "Router identifier" banner
    assert!(out.contains("10.255.0.1"), "{out}");
}

#[test]
fn vrp_bgp_peer_word_states() {
    let out = digest("vrp_display_bgp_peer.txt");
    assert!(out.contains("3 peers"), "{out}");
    assert!(out.contains("Established 2"), "{out}");
    assert!(
        out.contains("peer 10.0.16.2 (AS64515): session Active"),
        "{out}"
    );
    // VRP banner: router-id and local AS on separate lines
    assert!(out.contains("router-id 10.255.0.1"), "{out}");
    assert!(out.contains("local AS 64512"), "{out}");
    assert!(out.contains("394562"), "{out}"); // 184231 + 210331
}

#[test]
fn xr_bgp_summary_spk_column() {
    let out = digest("xr_bgp_summary.txt");
    assert!(out.contains("3 peers"), "{out}");
    assert!(
        out.contains("peer 10.0.16.2 (AS64515): session Idle"),
        "{out}"
    );
    assert!(out.contains("394562"), "{out}");
}

#[test]
fn nxos_route_table() {
    let out = digest("nxos_show_ip_route.txt");
    assert!(out.contains("management"), "{out}");
    // protocol census: bgp 1, ospf 1 (ECMP counted once), connected 2, local 1, static 1
    assert!(out.contains("connected 2"), "{out}");
    assert!(out.contains("ospf 1"), "{out}");
    assert!(out.contains("bgp 1"), "{out}");
    assert!(out.contains("static 1"), "{out}");
    // default route via the static entry
    assert!(out.contains("172.16.0.1"), "{out}");
    assert!(out.contains("no default route in: management"), "{out}");
    // ECMP next-hops both counted
    assert!(out.contains("10.0.0.1"), "{out}");
    assert!(out.contains("10.0.0.5"), "{out}");
}

#[test]
fn junos_config_set_style() {
    let out = digest("junos_config_set.txt");
    assert!(out.contains("edge-osaka-1"), "{out}");
    assert!(out.contains("interfaces: 4 (1 disabled)"), "{out}");
    assert!(out.contains("10.0.12.2\t64513\tTRANSIT"), "{out}");
    assert!(out.contains("transit-a"), "{out}");
    // instance-scoped neighbor found and attributed
    assert!(out.contains("10.50.0.1\t65100\tCUST\tCUST-A"), "{out}");
    assert!(out.contains("routing-instances: CUST-A"), "{out}");
    // secrets never echoed
    assert!(!out.contains("$6$"), "{out}");
    assert!(out.contains("ntp servers 2"), "{out}");
}

#[test]
fn junos_config_hierarchical() {
    let out = digest("junos_config_hier.txt");
    assert!(out.contains("edge-nagoya-1"), "{out}");
    assert!(out.contains("10.0.12.2\t64513\tTRANSIT"), "{out}");
    assert!(out.contains("ge-0/0/0\t10.0.0.2/30"), "{out}");
    assert!(out.contains("ge-0/0/2\t-\tdisable"), "{out}");
    assert!(!out.contains("$6$"), "{out}");
    assert!(out.contains("1 static routes"), "{out}");
}

#[test]
fn kind_override_forces_parser() {
    let forced = Options {
        full: false,
        kind: Some(Kind::Generic),
    };
    let docs = distill(&fx("frr_bgp_summary.txt"), &forced);
    assert_eq!(docs[0].kind, "generic");
    // and without the override it auto-detects as bgp-summary
    let docs = distill(&fx("frr_bgp_summary.txt"), &OPTS);
    assert_eq!(docs[0].kind, "bgp-summary");
}
