use crate::detect::junos_rib_header;
use crate::model::{Doc, Table};
use std::collections::BTreeMap;

#[derive(Default)]
struct Rib {
    total: u64,
    proto: BTreeMap<String, u64>,
    default_via: Option<String>,
    hidden: u64,
}

const SUBCODES: [&str; 14] = [
    "IA", "E1", "E2", "EX", "N1", "N2", "L1", "L2", "su", "ND", "NDp", "M", "l", "a",
];

fn proto_name(code: &str) -> String {
    let c = code.trim_matches(|c| c == '*' || c == '%' || c == '+');
    match c {
        "C" => "connected".into(),
        "L" => "local".into(),
        "S" => "static".into(),
        "O" => "ospf".into(),
        "B" => "bgp".into(),
        "D" => "eigrp".into(),
        "R" => "rip".into(),
        "i" => "isis".into(),
        "K" => "kernel".into(),
        _ => c.to_lowercase(),
    }
}

fn is_code(tok: &str) -> bool {
    !tok.is_empty()
        && tok.len() <= 3
        && tok
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '*' || c == '%' || c == '+')
}

fn via_of(t: &str) -> Option<String> {
    if let Some(p) = t.find(" via ") {
        let rest = &t[p + 5..];
        let tok = rest.split(|c: char| c == ',' || c.is_whitespace()).next()?;
        if tok.is_empty() {
            None
        } else {
            Some(tok.to_string())
        }
    } else if t.contains("is directly connected") {
        t.rsplit(", ").next().map(|s| format!("direct:{s}"))
    } else {
        None
    }
}

/// NX-OS via-line protocol token: "ospf-1", "bgp-65000", "direct", "static", ...
fn nxos_proto(tok: &str) -> Option<String> {
    let base = tok.trim().split('-').next().unwrap_or("");
    match base {
        "direct" => Some("connected".into()),
        "local" => Some("local".into()),
        "static" | "ospf" | "bgp" | "eigrp" | "rip" | "isis" | "kernel" => Some(base.into()),
        "am" => Some("arp".into()),
        "hsrp" | "vrrp" | "broadcast" => Some(base.into()),
        _ => None,
    }
}

fn num_before_word(s: &str, word: &str) -> Option<u64> {
    let p = s.find(word)?;
    s[..p]
        .split_whitespace()
        .last()?
        .trim_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()
}

/// Parse IOS/IOS-XR `show ip route [vrf all]` and Junos `show route` output.
pub fn parse(input: &str, full: bool) -> Doc {
    let mut ribs: BTreeMap<String, Rib> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut cur = String::from("default");
    let mut nexthops: BTreeMap<String, u64> = BTreeMap::new();
    let mut entries: Vec<[String; 4]> = Vec::new(); // rib, prefix, proto, via
    let mut junos = false;
    let mut nxos = false;
    let mut pending_default = false;
    // NX-OS: the prefix line carries no protocol; the first "*via" line does.
    let mut nx_pending: Option<String> = None;

    fn rib_of<'a>(
        ribs: &'a mut BTreeMap<String, Rib>,
        order: &mut Vec<String>,
        name: &str,
    ) -> &'a mut Rib {
        if !ribs.contains_key(name) {
            order.push(name.to_string());
        }
        ribs.entry(name.to_string()).or_default()
    }

    for raw in input.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.is_empty() {
            continue;
        }

        if let Some((name, rest)) = junos_rib_header(line) {
            junos = true;
            cur = name;
            let r = rib_of(&mut ribs, &mut order, &cur);
            r.hidden = num_before_word(&rest, "hidden").unwrap_or(0);
            continue;
        }
        // NX-OS: IP Route Table for VRF "default"
        if let Some(rest) = t.strip_prefix("IP Route Table for VRF ") {
            nxos = true;
            cur = rest.trim().trim_matches('"').to_string();
            rib_of(&mut ribs, &mut order, &cur);
            continue;
        }
        if let Some(rest) = t.strip_prefix("Routing Table: ") {
            cur = rest.trim().to_string();
            rib_of(&mut ribs, &mut order, &cur);
            continue;
        }
        if let Some(rest) = t.strip_prefix("VRF: ") {
            cur = rest.trim().to_string();
            rib_of(&mut ribs, &mut order, &cur);
            continue;
        }
        if t.starts_with("Codes:") {
            continue;
        }
        if let Some(rest) = t.strip_prefix("Gateway of last resort is ") {
            if !rest.starts_with("not set") {
                let via = rest.split_whitespace().next().unwrap_or("?").to_string();
                let r = rib_of(&mut ribs, &mut order, &cur);
                r.default_via = Some(via);
            } else {
                rib_of(&mut ribs, &mut order, &cur);
            }
            continue;
        }

        if nxos {
            // "10.1.0.0/24, ubest/mbest: 1/0"
            if let Some((pfx, _)) = t.split_once(", ubest/mbest") {
                nx_pending = Some(pfx.trim().to_string());
                continue;
            }
            // "    *via 10.0.0.2, Eth1/1, [110/41], 3d02h, ospf-1, intra"
            let vt = t.trim_start_matches('*');
            if let Some(rest) = vt.strip_prefix("via ") {
                let parts: Vec<&str> = rest.split(", ").collect();
                let nh = parts[0].trim().to_string();
                if nh.contains('.') || nh.contains(':') {
                    *nexthops.entry(nh.clone()).or_insert(0) += 1;
                }
                if let Some(pfx) = nx_pending.take() {
                    let proto = parts
                        .iter()
                        .find_map(|p| nxos_proto(p))
                        .unwrap_or_else(|| "?".to_string());
                    let r = rib_of(&mut ribs, &mut order, &cur);
                    r.total += 1;
                    *r.proto.entry(proto.clone()).or_insert(0) += 1;
                    if (pfx == "0.0.0.0/0" || pfx == "::/0") && r.default_via.is_none() {
                        r.default_via = Some(nh.clone());
                    }
                    entries.push([cur.clone(), pfx, proto, nh]);
                }
            }
            continue;
        }

        if junos {
            let f: Vec<&str> = t.split_whitespace().collect();
            if f.len() >= 2
                && (f[0].contains('/') || f[0].contains(':'))
                && (f[1].starts_with("*[") || f[1].starts_with('['))
            {
                let proto = f[1]
                    .trim_start_matches("*[")
                    .trim_start_matches('[')
                    .split('/')
                    .next()
                    .unwrap_or("?")
                    .to_lowercase();
                let r = rib_of(&mut ribs, &mut order, &cur);
                r.total += 1;
                *r.proto.entry(proto.clone()).or_insert(0) += 1;
                pending_default = f[0] == "0.0.0.0/0" || f[0] == "::/0";
                entries.push([cur.clone(), f[0].to_string(), proto, String::new()]);
            } else if let Some(pos) = f.iter().position(|w| *w == "to") {
                if f.first() == Some(&">") || pos == 0 {
                    if let Some(nh) = f.get(pos + 1) {
                        let nh = nh.to_string();
                        *nexthops.entry(nh.clone()).or_insert(0) += 1;
                        if pending_default {
                            let r = rib_of(&mut ribs, &mut order, &cur);
                            if r.default_via.is_none() {
                                r.default_via = Some(nh.clone());
                            }
                            pending_default = false;
                        }
                        if let Some(e) = entries.last_mut() {
                            if e[3].is_empty() {
                                e[3] = nh;
                            }
                        }
                    }
                }
            }
            continue;
        }

        // IOS-style route lines start at column 0 with protocol code tokens.
        if !raw.starts_with(' ') {
            let f: Vec<&str> = t.split_whitespace().collect();
            let mut idx = 0;
            if !f.is_empty() && is_code(f[0]) {
                idx = 1;
                if f.len() > 1 && SUBCODES.contains(&f[1]) {
                    idx = 2;
                }
            }
            if idx > 0 && idx < f.len() {
                let target = f[idx];
                if (target.contains('.') || target.contains(':')) && !t.contains("subnetted") {
                    let proto = proto_name(f[0]);
                    let via = via_of(t);
                    let r = rib_of(&mut ribs, &mut order, &cur);
                    r.total += 1;
                    *r.proto.entry(proto.clone()).or_insert(0) += 1;
                    if (target == "0.0.0.0/0" || target == "::/0") && r.default_via.is_none() {
                        r.default_via = via.clone().or(Some("present".into()));
                    }
                    if let Some(nh) = &via {
                        if nh.contains('.') || nh.contains(':') {
                            *nexthops.entry(nh.clone()).or_insert(0) += 1;
                        }
                    }
                    entries.push([
                        cur.clone(),
                        target.to_string(),
                        proto,
                        via.unwrap_or_default(),
                    ]);
                }
            }
        } else if t.starts_with('[') && t.contains(" via ") {
            // ECMP / wrapped continuation line — counts as an extra next-hop.
            if let Some(nh) = via_of(t) {
                if nh.contains('.') || nh.contains(':') {
                    *nexthops.entry(nh).or_insert(0) += 1;
                }
            }
        }
    }

    let total: u64 = ribs.values().map(|r| r.total).sum();
    let mut doc = Doc::new(
        "route-table",
        format!("{} routing table(s), {} routes", ribs.len(), total),
    );

    for name in &order {
        let r = &ribs[name];
        if r.total == 0 {
            doc.alerts.push(format!("table {name}: 0 routes"));
        }
        if r.hidden > 0 {
            doc.alerts
                .push(format!("table {name}: {} hidden route(s)", r.hidden));
        }
    }

    let mut proto_tot: BTreeMap<String, u64> = BTreeMap::new();
    for r in ribs.values() {
        for (k, v) in &r.proto {
            *proto_tot.entry(k.clone()).or_insert(0) += v;
        }
    }
    let mut protos: Vec<(String, u64)> = proto_tot.into_iter().collect();
    protos.sort_by_key(|(_, v)| std::cmp::Reverse(*v));
    doc.summary.push(format!(
        "routes by protocol: {}",
        protos
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let no_default: Vec<&str> = order
        .iter()
        .filter(|n| ribs[*n].default_via.is_none())
        .map(|s| s.as_str())
        .collect();
    if !no_default.is_empty() {
        if no_default.len() == order.len() {
            doc.summary.push("no default route in any table".into());
        } else {
            doc.summary
                .push(format!("no default route in: {}", no_default.join(", ")));
        }
    }

    // Per-table protocol matrix.
    let cols: Vec<String> = protos.iter().take(6).map(|(k, _)| k.clone()).collect();
    let extra = protos.len() > 6;
    let mut header: Vec<String> = vec!["table".into(), "routes".into()];
    header.extend(cols.iter().cloned());
    if extra {
        header.push("other".into());
    }
    header.push("default_via".into());
    let rows: Vec<Vec<String>> = order
        .iter()
        .map(|name| {
            let r = &ribs[name];
            let mut row = vec![name.clone(), r.total.to_string()];
            let mut counted = 0u64;
            for c in &cols {
                let v = r.proto.get(c).copied().unwrap_or(0);
                counted += v;
                row.push(v.to_string());
            }
            if extra {
                row.push(r.total.saturating_sub(counted).to_string());
            }
            row.push(r.default_via.clone().unwrap_or_else(|| "-".into()));
            row
        })
        .collect();
    doc.tables.push(Table {
        title: "TABLES/VRFs".into(),
        header,
        rows,
        omitted: None,
    });

    if !nexthops.is_empty() {
        let mut nh: Vec<(&String, &u64)> = nexthops.iter().collect();
        nh.sort_by_key(|(_, v)| std::cmp::Reverse(**v));
        let shown: Vec<Vec<String>> = nh
            .iter()
            .take(8)
            .map(|(k, v)| vec![(*k).clone(), v.to_string()])
            .collect();
        let omitted = (nh.len() > 8).then(|| format!("{} more next-hops omitted", nh.len() - 8));
        doc.tables.push(Table {
            title: "TOP NEXT-HOPS".into(),
            header: vec!["next-hop".into(), "routes".into()],
            rows: shown,
            omitted,
        });
    }

    if full && !entries.is_empty() {
        doc.tables.push(Table {
            title: "ALL ROUTES".into(),
            header: ["table", "prefix", "proto", "via"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows: entries.iter().map(|e| e.to_vec()).collect(),
            omitted: None,
        });
    } else {
        doc.notes.push(
            "individual prefixes omitted — aggregated by table/protocol (--full to list every route)"
                .into(),
        );
    }
    doc
}
