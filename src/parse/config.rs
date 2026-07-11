use crate::model::{Doc, Table};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct CfgIface {
    name: String,
    desc: String,
    ip: String,
    vrf: String,
    shutdown: bool,
    vlan: String,
}

#[derive(Default)]
struct Neighbor {
    remote_as: String,
    desc: String,
    group: String,
}

fn ip_with_len(d: &str) -> String {
    let mut it = d.split_whitespace();
    let ip = it.next().unwrap_or("");
    match it.next() {
        Some(mask) if mask.contains('.') => {
            let bits: u32 = mask
                .split('.')
                .filter_map(|o| o.parse::<u32>().ok())
                .map(|o| o.count_ones())
                .sum();
            format!("{ip}/{bits}")
        }
        Some(x) => format!("{ip} {x}"),
        None => ip.to_string(),
    }
}

/// Parse an IOS-style running/startup config into an inventory digest.
pub fn parse(input: &str, full: bool) -> Doc {
    let lines: Vec<&str> = input.lines().collect();
    let mut hostname = String::new();
    let mut ifaces: Vec<CfgIface> = Vec::new();
    let mut bgp_as = String::new();
    let mut neighbors: BTreeMap<String, Neighbor> = BTreeMap::new();
    let mut afs: BTreeSet<String> = BTreeSet::new();
    let mut vrfs: Vec<String> = Vec::new();
    let mut static_routes = 0usize;
    let mut acls: BTreeSet<String> = BTreeSet::new();
    let mut prefix_lists: BTreeSet<String> = BTreeSet::new();
    let mut route_maps: BTreeSet<String> = BTreeSet::new();
    let mut other_blocks: BTreeSet<String> = BTreeSet::new();
    let (mut ntp, mut log_hosts, mut users) = (0usize, 0usize, 0usize);
    let (mut snmp, mut aaa) = (false, false);

    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_end();
        if let Some(h) = t.strip_prefix("hostname ") {
            hostname = h.trim().to_string();
            i += 1;
            continue;
        }
        if let Some(name) = t.strip_prefix("interface ") {
            let mut f = CfgIface {
                name: name.trim().to_string(),
                ..Default::default()
            };
            i += 1;
            while i < lines.len() && lines[i].starts_with(' ') {
                let c = lines[i].trim();
                if let Some(d) = c.strip_prefix("description ") {
                    f.desc = d.to_string();
                } else if let Some(d) = c.strip_prefix("ip address ") {
                    f.ip = ip_with_len(d);
                } else if let Some(d) = c.strip_prefix("ipv6 address ") {
                    if f.ip.is_empty() {
                        f.ip = d.to_string();
                    }
                } else if let Some(d) = c.strip_prefix("vrf forwarding ") {
                    f.vrf = d.to_string();
                } else if let Some(d) = c.strip_prefix("ip vrf forwarding ") {
                    f.vrf = d.to_string();
                } else if c == "shutdown" {
                    f.shutdown = true;
                } else if let Some(d) = c.strip_prefix("switchport access vlan ") {
                    f.vlan = d.to_string();
                }
                i += 1;
            }
            ifaces.push(f);
            continue;
        }
        if let Some(asn) = t.strip_prefix("router bgp ") {
            bgp_as = asn.trim().to_string();
            i += 1;
            while i < lines.len() && lines[i].starts_with(' ') {
                let c = lines[i].trim();
                if let Some(rest) = c.strip_prefix("neighbor ") {
                    let mut it = rest.split_whitespace();
                    if let Some(addr) = it.next() {
                        let e = neighbors.entry(addr.to_string()).or_default();
                        let kw: Vec<&str> = it.collect();
                        match kw.first() {
                            Some(&"remote-as") => {
                                e.remote_as = kw.get(1).unwrap_or(&"").to_string()
                            }
                            Some(&"description") => e.desc = kw[1..].join(" "),
                            Some(&"peer-group") if kw.len() > 1 => e.group = kw[1].to_string(),
                            _ => {}
                        }
                    }
                } else if let Some(af) = c.strip_prefix("address-family ") {
                    afs.insert(af.trim().to_string());
                }
                i += 1;
            }
            continue;
        }

        if t.starts_with("ip route ") || t.starts_with("ipv6 route ") {
            static_routes += 1;
        } else if let Some(r) = t.strip_prefix("vrf definition ") {
            vrfs.push(r.trim().to_string());
        } else if let Some(r) = t.strip_prefix("ip vrf ") {
            vrfs.push(r.trim().to_string());
        } else if t.starts_with("access-list ") {
            if let Some(id) = t.split_whitespace().nth(1) {
                acls.insert(id.to_string());
            }
        } else if t.starts_with("ip access-list ") {
            let toks: Vec<&str> = t.split_whitespace().collect();
            if let Some(id) = toks.get(3).or_else(|| toks.get(2)) {
                acls.insert(id.to_string());
            }
        } else if t.starts_with("ip prefix-list ") {
            if let Some(id) = t.split_whitespace().nth(2) {
                prefix_lists.insert(id.to_string());
            }
        } else if t.starts_with("route-map ") {
            if let Some(id) = t.split_whitespace().nth(1) {
                route_maps.insert(id.to_string());
            }
        } else if t.starts_with("ntp server") {
            ntp += 1;
        } else if t.starts_with("snmp-server") {
            snmp = true;
        } else if t.starts_with("aaa ") {
            aaa = true;
        } else if t.starts_with("logging host") {
            log_hosts += 1;
        } else if t.starts_with("username ") {
            users += 1;
        } else if !t.is_empty() && !t.starts_with(' ') && !t.starts_with('!') {
            if let Some(w) = t.split_whitespace().next() {
                other_blocks.insert(w.to_string());
            }
        }
        i += 1;
    }

    let host = if hostname.is_empty() {
        "(no hostname)".to_string()
    } else {
        hostname
    };
    let mut doc = Doc::new("config", format!("{host} — {} config lines", lines.len()));

    let shut = ifaces.iter().filter(|f| f.shutdown).count();
    let nodesc = ifaces
        .iter()
        .filter(|f| !f.shutdown && f.desc.is_empty())
        .count();
    doc.summary.push(format!(
        "interfaces: {} ({shut} shutdown, {nodesc} active without description)",
        ifaces.len()
    ));
    if !bgp_as.is_empty() {
        let af = if afs.is_empty() {
            "-".to_string()
        } else {
            afs.iter().cloned().collect::<Vec<_>>().join(", ")
        };
        doc.summary.push(format!(
            "bgp: AS {bgp_as}, {} neighbor statements; address-families: {af}",
            neighbors.len()
        ));
    }
    if !vrfs.is_empty() {
        doc.summary.push(format!("vrfs: {}", vrfs.join(", ")));
    }
    doc.summary.push(format!(
        "policy: {} ACLs, {} prefix-lists, {} route-maps, {static_routes} static routes",
        acls.len(),
        prefix_lists.len(),
        route_maps.len()
    ));
    doc.summary.push(format!(
        "mgmt: ntp servers {ntp}, snmp {}, aaa {}, logging hosts {log_hosts}, local users {users}",
        if snmp { "yes" } else { "no" },
        if aaa { "yes" } else { "no" }
    ));

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut skipped = 0usize;
    for f in &ifaces {
        let configured = !f.ip.is_empty()
            || !f.desc.is_empty()
            || f.shutdown
            || !f.vrf.is_empty()
            || !f.vlan.is_empty();
        if !configured && !full {
            skipped += 1;
            continue;
        }
        rows.push(vec![
            f.name.clone(),
            if f.ip.is_empty() {
                "-".into()
            } else {
                f.ip.clone()
            },
            if f.vrf.is_empty() {
                "-".into()
            } else {
                f.vrf.clone()
            },
            if f.vlan.is_empty() {
                "-".into()
            } else {
                f.vlan.clone()
            },
            if f.shutdown {
                "shut".into()
            } else {
                "up".into()
            },
            f.desc.clone(),
        ]);
    }
    if !rows.is_empty() {
        doc.tables.push(Table {
            title: "INTERFACES".into(),
            header: ["interface", "ip", "vrf", "vlan", "admin", "description"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            omitted: (skipped > 0).then(|| {
                format!(
                    "{skipped} interfaces with no L3/desc/vlan config omitted (--full to include)"
                )
            }),
        });
    }
    if !neighbors.is_empty() {
        doc.tables.push(Table {
            title: "BGP NEIGHBORS".into(),
            header: ["neighbor", "remote-as", "peer-group", "description"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows: neighbors
                .iter()
                .map(|(a, nb)| {
                    vec![
                        a.clone(),
                        if nb.remote_as.is_empty() {
                            "-".into()
                        } else {
                            nb.remote_as.clone()
                        },
                        if nb.group.is_empty() {
                            "-".into()
                        } else {
                            nb.group.clone()
                        },
                        nb.desc.clone(),
                    ]
                })
                .collect(),
            omitted: None,
        });
    }
    if !other_blocks.is_empty() {
        let mut list: Vec<&String> = other_blocks.iter().collect();
        list.truncate(15);
        doc.notes.push(format!(
            "top-level sections present but not summarized: {}{}",
            list.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            if other_blocks.len() > 15 { ", ..." } else { "" }
        ));
    }
    doc.notes.push(
        "secrets/crypto/aaa details are not extracted — consult the raw config if needed".into(),
    );
    doc
}
