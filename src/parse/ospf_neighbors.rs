use crate::model::{Doc, Table};
use std::collections::BTreeMap;

struct Nbr {
    id: String,
    state: String,
    dead: String,
    addr: String,
    iface: String,
}

/// Parse `show ip ospf neighbor` (IOS/FRR style).
pub fn parse(input: &str, full: bool) -> Doc {
    let mut nbrs: Vec<Nbr> = Vec::new();
    for raw in input.lines() {
        let f: Vec<&str> = raw.split_whitespace().collect();
        if f.len() < 6
            || !f[0].contains('.')
            || !f[0].chars().all(|c| c.is_ascii_digit() || c == '.')
            || !f[1].chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        // Point-to-point prints "FULL/  -": the state splits into two tokens.
        let (state, rest) = if f[3] == "-" {
            (format!("{}-", f[2]), 4)
        } else {
            (f[2].to_string(), 3)
        };
        if f.len() < rest + 3 {
            continue;
        }
        nbrs.push(Nbr {
            id: f[0].to_string(),
            state,
            dead: f[rest].to_string(),
            addr: f[rest + 1].to_string(),
            iface: f[rest + 2..].join(" "),
        });
    }

    let n = nbrs.len();
    let is_healthy =
        |s: &str| s.starts_with("FULL") || s.starts_with("2WAY") || s.starts_with("2-WAY");
    let mut doc = Doc::new("ospf-neighbors", format!("{n} OSPF neighbors"));

    let mut notable: Vec<&Nbr> = Vec::new();
    for nb in &nbrs {
        if !is_healthy(&nb.state) {
            doc.alerts.push(format!(
                "neighbor {} on {} ({}): state {} — adjacency not FULL (dead {})",
                nb.id, nb.iface, nb.addr, nb.state, nb.dead
            ));
            notable.push(nb);
        }
    }

    let mut by_state: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_iface: BTreeMap<String, usize> = BTreeMap::new();
    for nb in &nbrs {
        let base = nb.state.split('/').next().unwrap_or(&nb.state).to_string();
        *by_state.entry(base).or_insert(0) += 1;
        *by_iface.entry(nb.iface.clone()).or_insert(0) += 1;
    }
    doc.summary.push(format!(
        "states: {}",
        by_state
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    doc.summary.push(format!(
        "interfaces: {}",
        by_iface
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if by_state
        .keys()
        .any(|s| s.starts_with("2WAY") || s.starts_with("2-WAY"))
    {
        doc.notes.push(
            "2WAY/DROTHER is normal on broadcast segments between two non-DR/BDR routers".into(),
        );
    }

    let listed: Vec<&Nbr> = if full { nbrs.iter().collect() } else { notable };
    if !listed.is_empty() {
        let rows = listed
            .iter()
            .map(|nb| {
                vec![
                    nb.id.clone(),
                    nb.state.clone(),
                    nb.dead.clone(),
                    nb.addr.clone(),
                    nb.iface.clone(),
                ]
            })
            .collect();
        let omitted = if full {
            None
        } else {
            let healthy = n - listed.len();
            (healthy > 0).then(|| {
                format!("{healthy} healthy (FULL/2WAY) neighbors omitted (--full to list all)")
            })
        };
        doc.tables.push(Table {
            title: if full {
                "NEIGHBORS".into()
            } else {
                "NOTABLE NEIGHBORS".into()
            },
            header: ["neighbor_id", "state", "dead", "address", "interface"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            omitted,
        });
    } else if !full && n > 0 {
        doc.notes.push(format!(
            "all {n} neighbors healthy (FULL or 2WAY/DROTHER); table omitted (--full to list)"
        ));
    }
    doc
}
