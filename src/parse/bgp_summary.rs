use crate::model::{Doc, Table};
use std::collections::BTreeMap;

struct Peer {
    addr: String,
    asn: String,
    state: String,
    updown: String,
    pfx: Option<u64>,
    flaps: Option<u64>,
    section: String,
}

fn is_addr(s: &str) -> bool {
    let s = s.trim_start_matches('*');
    if s.is_empty() {
        return false;
    }
    (s.contains('.') && s.chars().all(|c| c.is_ascii_digit() || c == '.'))
        || (s.contains(':')
            && s.chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.'))
}

fn norm_state(s: &str) -> String {
    if s.starts_with("Estab") {
        "Established".to_string()
    } else {
        s.to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// IOS / IOS-XR / FRR / NX-OS: State/PfxRcd share one column.
    IosFrr,
    /// Junos: established peers show a/b/c/d counts, others a state word.
    Junos,
    /// Word-state dialects (Arista EOS, Huawei VRP): up/down, state, and
    /// received-prefix count each in their own column.
    StateCol {
        updown: usize,
        state: usize,
        pfx: usize,
    },
}

/// Parse `show bgp summary` in IOS / FRR / EOS style and Junos style.
pub fn parse(input: &str, full: bool) -> Doc {
    let mut router_id: Option<String> = None;
    let mut local_as: Option<String> = None;
    let mut peers: Vec<Peer> = Vec::new();
    let mut section = String::new();
    let mut mode = Mode::IosFrr;

    for raw in input.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        // "BGP router identifier ..." (IOS/FRR/XR; NX-OS says "local AS" without
        // "number") and "Router identifier ..." (EOS).
        if let Some(rest) = t
            .strip_prefix("BGP router identifier ")
            .or_else(|| t.strip_prefix("Router identifier "))
        {
            router_id = rest.split(',').next().map(|s| s.trim().to_string());
            for tag in ["local AS number ", "local AS "] {
                if let Some(p) = rest.find(tag) {
                    local_as = rest[p + tag.len()..]
                        .split_whitespace()
                        .next()
                        .map(str::to_string);
                    break;
                }
            }
            continue;
        }
        // Huawei VRP banner lines.
        if let Some(rest) = t.strip_prefix("BGP local router ID : ") {
            router_id = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = t.strip_prefix("Local AS number : ") {
            local_as = rest.split_whitespace().next().map(str::to_string);
            continue;
        }
        // NX-OS / EOS: "BGP summary information for VRF default[, address family ...]"
        if let Some(rest) = t.strip_prefix("BGP summary information for ") {
            section = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = t.strip_prefix("For address family: ") {
            section = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = t.strip_prefix("Instance ") {
            section = rest.trim().trim_end_matches(':').to_string();
            continue;
        }
        if let Some(rest) = t.strip_prefix("VRF: ") {
            section = rest.trim().to_string();
            continue;
        }
        if t.contains("InPkt") && t.contains("OutPkt") {
            mode = Mode::Junos;
            continue;
        }
        if t.contains("PfxAcc") {
            // EOS: Neighbor V AS MsgRcvd MsgSent InQ OutQ Up/Down State PfxRcd PfxAcc
            mode = Mode::StateCol {
                updown: 7,
                state: 8,
                pfx: 9,
            };
            continue;
        }
        if t.contains("PrefRcv") {
            // VRP: Peer V AS MsgRcvd MsgSent OutQ Up/Down State PrefRcv
            mode = Mode::StateCol {
                updown: 6,
                state: 7,
                pfx: 8,
            };
            continue;
        }
        if t.starts_with("Neighbor") && t.contains("MsgRcvd") {
            mode = Mode::IosFrr;
            continue;
        }

        let f: Vec<&str> = t.split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        if !is_addr(f[0]) {
            // Junos per-RIB continuation for an established peer: "inet.0: 2/4/4/0"
            if mode == Mode::Junos
                && raw.starts_with(' ')
                && f.len() == 2
                && f[0].ends_with(':')
                && f[1].contains('/')
            {
                if let Some(p) = peers.last_mut() {
                    if p.pfx.is_none() {
                        p.pfx = f[1].split('/').next().and_then(|n| n.parse().ok());
                    }
                    if p.state != "Established" && !p.state.chars().any(|c| c.is_ascii_alphabetic())
                    {
                        p.state = "Established".to_string();
                    }
                }
            }
            continue;
        }
        if let Mode::StateCol { updown, state, pfx } = mode {
            if f.len() <= state {
                continue;
            }
            let st = norm_state(f[state]);
            let p = if st == "Established" {
                f.get(pfx).and_then(|v| v.parse().ok())
            } else {
                None
            };
            peers.push(Peer {
                addr: f[0].to_string(),
                asn: f[2].to_string(),
                state: st,
                updown: f[updown].to_string(),
                pfx: p,
                flaps: None,
                section: section.clone(),
            });
        } else if mode == Mode::Junos {
            if f.len() < 8 {
                continue;
            }
            let last = *f.last().unwrap();
            let (state, pfx) =
                if last.contains('/') && last.starts_with(|c: char| c.is_ascii_digit()) {
                    (
                        "Established".to_string(),
                        last.split('/').next().and_then(|n| n.parse().ok()),
                    )
                } else {
                    (norm_state(last), None)
                };
            peers.push(Peer {
                addr: f[0].to_string(),
                asn: f[1].to_string(),
                state,
                updown: f[6..f.len() - 1].join(" "),
                pfx,
                flaps: f[5].parse().ok(),
                section: section.clone(),
            });
        } else {
            // IOS/FRR: Neighbor V AS MsgRcvd MsgSent TblVer InQ OutQ Up/Down State/PfxRcd [PfxSnt] [Desc]
            if f.len() < 10 {
                continue;
            }
            let (state, pfx) = match f[9].parse::<u64>() {
                Ok(n) => ("Established".to_string(), Some(n)),
                Err(_) => {
                    let mut s = f[9].to_string();
                    if let Some(nx) = f.get(10) {
                        if nx.starts_with('(') {
                            s.push(' ');
                            s.push_str(nx);
                        }
                    }
                    (norm_state(&s), None)
                }
            };
            peers.push(Peer {
                addr: f[0].trim_start_matches('*').to_string(),
                asn: f[2].to_string(),
                state,
                updown: f[8].to_string(),
                pfx,
                flaps: None,
                section: section.clone(),
            });
        }
    }

    let n = peers.len();
    let mut by_state: BTreeMap<String, usize> = BTreeMap::new();
    for p in &peers {
        *by_state.entry(p.state.clone()).or_insert(0) += 1;
    }
    let est: Vec<&Peer> = peers.iter().filter(|p| p.state == "Established").collect();
    let total_pfx: u64 = est.iter().filter_map(|p| p.pfx).sum();

    let mut headline = String::new();
    if let Some(r) = &router_id {
        headline.push_str(&format!("router-id {r}"));
    }
    if let Some(a) = &local_as {
        if !headline.is_empty() {
            headline.push_str(", ");
        }
        headline.push_str(&format!("local AS {a}"));
    }
    if !headline.is_empty() {
        headline.push_str(" — ");
    }
    headline.push_str(&format!("{n} peers"));

    let mut doc = Doc::new("bgp-summary", headline);

    let mut notable: Vec<&Peer> = Vec::new();
    for p in &peers {
        let sect = if p.section.is_empty() {
            String::new()
        } else {
            format!(", {}", p.section)
        };
        let label = format!("peer {} (AS{}{})", p.addr, p.asn, sect);
        let mut flag = false;
        if p.state != "Established" {
            doc.alerts.push(format!(
                "{label}: session {} — up/down {}",
                p.state, p.updown
            ));
            flag = true;
        } else if p.pfx == Some(0) {
            doc.alerts.push(format!(
                "{label}: Established but 0 prefixes received (up {})",
                p.updown
            ));
            flag = true;
        } else if p.updown.starts_with("00:") {
            doc.alerts.push(format!(
                "{label}: session reset recently — up only {}",
                p.updown
            ));
            flag = true;
        }
        if let Some(fl) = p.flaps {
            if fl >= 3 {
                doc.alerts.push(format!("{label}: {fl} flaps"));
                flag = true;
            }
        }
        if flag {
            notable.push(p);
        }
    }

    let states = by_state
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ");
    doc.summary.push(format!("peers: {n} ({states})"));
    doc.summary.push(format!(
        "prefixes received: {total_pfx} across {} established peers",
        est.len()
    ));
    let mut top: Vec<&&Peer> = est.iter().filter(|p| p.pfx.unwrap_or(0) > 0).collect();
    top.sort_by_key(|p| std::cmp::Reverse(p.pfx.unwrap_or(0)));
    if !top.is_empty() {
        let s = top
            .iter()
            .take(3)
            .map(|p| format!("AS{} {} ({})", p.asn, p.pfx.unwrap_or(0), p.addr))
            .collect::<Vec<_>>()
            .join(", ");
        doc.summary.push(format!("largest peers by prefixes: {s}"));
    }
    let sections: Vec<String> = {
        let mut v: Vec<String> = peers.iter().map(|p| p.section.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let multi_section = sections.len() > 1;
    if multi_section {
        doc.summary.push(format!(
            "sections (AF/VRF): {}",
            sections
                .iter()
                .map(|s| if s.is_empty() { "default" } else { s.as_str() })
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let listed: Vec<&Peer> = if full {
        peers.iter().collect()
    } else {
        notable
    };
    if !listed.is_empty() {
        let mut header: Vec<String> = ["neighbor", "AS", "state", "up/down", "pfx_rcvd"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        if multi_section {
            header.push("section".to_string());
        }
        let rows = listed
            .iter()
            .map(|p| {
                let mut r = vec![
                    p.addr.clone(),
                    p.asn.clone(),
                    p.state.clone(),
                    p.updown.clone(),
                    p.pfx.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                ];
                if multi_section {
                    r.push(if p.section.is_empty() {
                        "default".into()
                    } else {
                        p.section.clone()
                    });
                }
                r
            })
            .collect();
        let omitted = if full {
            None
        } else {
            let healthy = n - listed.len();
            (healthy > 0).then(|| {
                format!("{healthy} healthy Established peers omitted (--full to list all)")
            })
        };
        doc.tables.push(Table {
            title: if full {
                "PEERS".into()
            } else {
                "NOTABLE PEERS".into()
            },
            header,
            rows,
            omitted,
        });
    } else if !full && n > 0 {
        doc.notes.push(format!(
            "all {n} peers Established and receiving prefixes; per-peer table omitted (--full to list)"
        ));
    }
    doc
}
