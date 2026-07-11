use crate::model::{Doc, Table};
use std::collections::BTreeSet;

/// Parse `show lldp neighbors` (IOS-style summary table).
///
/// Unlike state commands, this is inventory: every row carries unique
/// information, so nothing is elided — the win is structure (tabs instead of
/// padding) plus a topology summary.
pub fn parse(input: &str, _full: bool) -> Doc {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut declared: Option<u64> = None;
    let mut in_table = false;

    for raw in input.lines() {
        let t = raw.trim_end();
        if t.contains("Device ID") && t.contains("Port ID") {
            in_table = true;
            continue;
        }
        if let Some(rest) = t.trim_start().strip_prefix("Total entries displayed:") {
            declared = rest.trim().parse().ok();
            continue;
        }
        if !in_table {
            continue;
        }
        let f: Vec<&str> = t.split_whitespace().collect();
        if f.len() < 5 {
            continue;
        }
        rows.push(vec![
            f[0].to_string(),            // device
            f[1].to_string(),            // local intf
            f[3..f.len() - 1].join(","), // capability (skip hold-time f[2])
            f[f.len() - 1].to_string(),  // port id
        ]);
    }

    let n = rows.len();
    let devices: BTreeSet<&String> = rows.iter().map(|r| &r[0]).collect();
    let ports: BTreeSet<&String> = rows.iter().map(|r| &r[1]).collect();
    let mut doc = Doc::new(
        "lldp-neighbors",
        format!(
            "{n} LLDP entries — {} devices on {} local ports",
            devices.len(),
            ports.len()
        ),
    );
    if let Some(d) = declared {
        if d != n as u64 {
            doc.alerts.push(format!(
                "device declared {d} entries but {n} rows parsed — output may be truncated or an unrecognized format"
            ));
        }
    }
    doc.summary.push(format!(
        "devices: {}",
        devices
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if !rows.is_empty() {
        doc.tables.push(Table {
            title: "LLDP NEIGHBORS".into(),
            header: ["device", "local_intf", "capability", "port_id"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            omitted: None,
        });
        doc.notes
            .push("inventory data: all rows kept (only padding and hold-time dropped)".into());
    }
    doc
}
