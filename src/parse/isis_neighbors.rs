use crate::model::{Doc, Table};
use std::collections::BTreeMap;

struct Nbr {
    sysid: String,
    typ: String,
    iface: String,
    addr: String,
    state: String,
    hold: String,
}

/// Parse `show isis neighbors` (IOS/FRR style).
pub fn parse(input: &str, full: bool) -> Doc {
    let mut nbrs: Vec<Nbr> = Vec::new();
    for raw in input.lines() {
        let f: Vec<&str> = raw.split_whitespace().collect();
        if f.len() >= 6 && ["UP", "DOWN", "INIT"].contains(&f[4].to_uppercase().as_str()) {
            nbrs.push(Nbr {
                sysid: f[0].to_string(),
                typ: f[1].to_string(),
                iface: f[2].to_string(),
                addr: f[3].to_string(),
                state: f[4].to_uppercase(),
                hold: f[5].to_string(),
            });
        }
    }

    let n = nbrs.len();
    let mut doc = Doc::new("isis-neighbors", format!("{n} IS-IS adjacencies"));

    let mut notable: Vec<&Nbr> = Vec::new();
    for nb in &nbrs {
        if nb.state != "UP" {
            doc.alerts.push(format!(
                "adjacency {} ({}) on {} ({}): state {} — hold {}",
                nb.sysid, nb.typ, nb.iface, nb.addr, nb.state, nb.hold
            ));
            notable.push(nb);
        }
    }

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_iface: BTreeMap<String, usize> = BTreeMap::new();
    for nb in &nbrs {
        *by_type.entry(nb.typ.clone()).or_insert(0) += 1;
        *by_iface.entry(nb.iface.clone()).or_insert(0) += 1;
    }
    doc.summary.push(format!(
        "levels: {}",
        by_type
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

    let listed: Vec<&Nbr> = if full { nbrs.iter().collect() } else { notable };
    if !listed.is_empty() {
        let rows = listed
            .iter()
            .map(|nb| {
                vec![
                    nb.sysid.clone(),
                    nb.typ.clone(),
                    nb.iface.clone(),
                    nb.addr.clone(),
                    nb.state.clone(),
                    nb.hold.clone(),
                ]
            })
            .collect();
        let omitted = if full {
            None
        } else {
            let healthy = n - listed.len();
            (healthy > 0).then(|| format!("{healthy} UP adjacencies omitted (--full to list all)"))
        };
        doc.tables.push(Table {
            title: if full {
                "ADJACENCIES".into()
            } else {
                "NOTABLE ADJACENCIES".into()
            },
            header: ["system_id", "type", "interface", "address", "state", "hold"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            omitted,
        });
    } else if !full && n > 0 {
        doc.notes.push(format!(
            "all {n} adjacencies UP; table omitted (--full to list)"
        ));
    }
    doc
}
