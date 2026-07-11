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
fn detects_new_kinds() {
    assert_eq!(detect(&fx("ios_show_ip_bgp.txt")), Kind::BgpTable);
    assert_eq!(detect(&fx("ios_ospf_neighbors.txt")), Kind::OspfNeighbors);
    assert_eq!(detect(&fx("ios_isis_neighbors.txt")), Kind::IsisNeighbors);
    assert_eq!(detect(&fx("ios_lldp_neighbors.txt")), Kind::LldpNeighbors);
    // and the old ones still win where they should
    assert_eq!(detect(&fx("frr_bgp_summary.txt")), Kind::BgpSummary);
    assert_eq!(detect(&fx("ios_route_vrf_all.txt")), Kind::RouteTable);
}

#[test]
fn bgp_table_aggregates() {
    let out = digest("ios_show_ip_bgp.txt");
    assert!(out.contains("networks 10, paths 11, best 10"), "{out}");
    assert!(out.contains("2 path(s) suppressed"), "{out}");
    assert!(out.contains("1 path(s) RIB-failure"), "{out}");
    // origin AS histogram (best paths): 65010 x4, 7018 x2, 3356 x1, 64512 (AS_SET) x1
    assert!(out.contains("AS65010\t4"), "{out}");
    assert!(out.contains("AS7018\t2"), "{out}");
    assert!(out.contains("AS64512\t1"), "{out}");
    // locally originated = 0.0.0.0 route + empty-path iBGP route
    assert!(out.contains("locally originated best paths: 2"), "{out}");
    // never dumps prefixes
    assert!(out.contains("per-prefix rows are never expanded"), "{out}");
    assert!(!out.contains("10.11.0.0/24"), "{out}");
    // router context kept
    assert!(out.contains("10.255.0.1"), "{out}");
}

#[test]
fn ospf_neighbors_alerts() {
    let out = digest("ios_ospf_neighbors.txt");
    // stuck adjacency surfaces
    assert!(out.contains("10.255.0.3"), "{out}");
    assert!(out.contains("EXSTART"), "{out}");
    // 2WAY and FULL/- (p2p) are healthy, not alerted
    assert!(!out.contains("10.255.0.4: "), "{out}");
    assert!(out.contains("ALERTS (1)"), "{out}");
    assert!(
        out.contains("healthy (FULL/2WAY) neighbors omitted"),
        "{out}"
    );
    assert!(out.contains("2WAY/DROTHER is normal"), "{out}");
}

#[test]
fn isis_neighbors_alerts() {
    let out = digest("ios_isis_neighbors.txt");
    assert!(out.contains("edge-1"), "{out}");
    assert!(out.contains("state INIT"), "{out}");
    assert!(out.contains("ALERTS (1)"), "{out}");
    assert!(out.contains("L1L2 1"), "{out}");
    assert!(!out.contains("core-2\t"), "{out}");
}

#[test]
fn lldp_inventory_kept_whole() {
    let out = digest("ios_lldp_neighbors.txt");
    assert!(out.contains("4 LLDP entries"), "{out}");
    // inventory: every device listed
    for d in [
        "sw-access-01",
        "sw-access-02",
        "core-1.example.net",
        "pdu-rack3",
    ] {
        assert!(out.contains(d), "{out}");
    }
    assert!(out.contains("ALERTS: none"), "{out}");
}

#[test]
fn lldp_truncation_mismatch_alerts() {
    let mut input = fx("ios_lldp_neighbors.txt");
    input = input.replace("Total entries displayed: 4", "Total entries displayed: 9");
    let out = render_body(&distill(&input, &OPTS));
    assert!(
        out.contains("declared 9 entries but 4 rows parsed"),
        "{out}"
    );
}

#[test]
fn big_bgp_table_compresses_hard() {
    // 20k networks, 2 paths each, from a small set of origin ASes.
    let mut input = String::from(
        "BGP table version is 99, local router ID is 10.255.0.1\nStatus codes: s suppressed, d damped, h history, * valid, > best, i - internal,\nOrigin codes: i - IGP, e - EGP, ? - incomplete\n\n     Network          Next Hop            Metric LocPrf Weight Path\n",
    );
    for i in 0..20000u32 {
        let net = format!("10.{}.{}.0/24", (i / 250) % 250, i % 250);
        let origin = 65000 + (i % 7);
        input.push_str(&format!(
            " *>  {net:<17}10.0.12.2                0             0 64513 {origin} i\n"
        ));
        input.push_str(" *                    10.0.13.2                              0 7018 i\n");
    }
    let in_tok = netdistill::token::estimate(&input);
    let out = digest_str(&input);
    let out_tok = netdistill::token::estimate(&out);
    assert!(
        out_tok * 50 < in_tok,
        "expected >98% reduction, got {in_tok} -> {out_tok}"
    );
    assert!(out.contains("networks 20000, paths 40000"), "{out}");
}

fn digest_str(input: &str) -> String {
    render_body(&distill(input, &OPTS))
}
