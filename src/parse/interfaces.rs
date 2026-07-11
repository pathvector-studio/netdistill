use crate::model::{Doc, Table};

#[derive(Default)]
struct Iface {
    name: String,
    admin_down: bool,
    oper_up: bool,
    desc: String,
    ip: String,
    in_err: u64,
    crc: u64,
    out_err: u64,
    out_drops: u64,
}

fn num_before(t: &str, kw: &str) -> Option<u64> {
    let p = t.find(kw)?;
    t[..p]
        .split_whitespace()
        .last()?
        .replace(',', "")
        .parse()
        .ok()
}

fn num_after(t: &str, kw: &str) -> Option<u64> {
    let p = t.find(kw)?;
    t[p + kw.len()..]
        .split_whitespace()
        .next()?
        .trim_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()
}

/// Parse IOS-style `show interfaces` output.
pub fn parse(input: &str, full: bool) -> Doc {
    let mut ifaces: Vec<Iface> = Vec::new();
    for raw in input.lines() {
        let t = raw.trim_end();
        let tt = t.trim_start();
        if !raw.starts_with(' ') && t.contains(" is ") && t.contains("line protocol is") {
            let name = t.split(" is ").next().unwrap_or("?").to_string();
            let admin_down = t.contains("administratively down");
            let oper_up = t
                .split("line protocol is ")
                .nth(1)
                .is_some_and(|s| s.trim_start().starts_with("up"));
            ifaces.push(Iface {
                name,
                admin_down,
                oper_up,
                ..Default::default()
            });
            continue;
        }
        let Some(cur) = ifaces.last_mut() else {
            continue;
        };
        if let Some(d) = tt.strip_prefix("Description: ") {
            cur.desc = d.trim().to_string();
        } else if let Some(d) = tt.strip_prefix("Internet address is ") {
            cur.ip = d.trim().to_string();
        } else if tt.contains("input errors,") {
            cur.in_err = num_before(tt, "input errors").unwrap_or(0);
            cur.crc = num_before(tt, "CRC").unwrap_or(0);
        } else if tt.contains("output errors,") {
            cur.out_err = num_before(tt, "output errors").unwrap_or(0);
        } else if tt.contains("Total output drops:") {
            cur.out_drops = num_after(tt, "Total output drops:").unwrap_or(0);
        }
    }

    let n = ifaces.len();
    let up = ifaces.iter().filter(|i| i.oper_up).count();
    let admin = ifaces.iter().filter(|i| i.admin_down).count();
    let down = n.saturating_sub(up).saturating_sub(admin);
    let mut doc = Doc::new(
        "interfaces",
        format!("{n} interfaces — {up} up, {down} down, {admin} admin-down"),
    );

    let mut notable: Vec<&Iface> = Vec::new();
    for i in &ifaces {
        let label = if i.desc.is_empty() {
            i.name.clone()
        } else {
            format!("{} ({})", i.name, i.desc)
        };
        let mut flag = false;
        if !i.admin_down && !i.oper_up {
            doc.alerts.push(format!("{label}: DOWN (not admin-down)"));
            flag = true;
        }
        if i.in_err > 0 {
            doc.alerts.push(format!(
                "{label}: {} input errors ({} CRC)",
                i.in_err, i.crc
            ));
            flag = true;
        }
        if i.out_err > 0 {
            doc.alerts
                .push(format!("{label}: {} output errors", i.out_err));
            flag = true;
        }
        if i.out_drops > 0 {
            doc.alerts
                .push(format!("{label}: {} output drops", i.out_drops));
            flag = true;
        }
        if flag {
            notable.push(i);
        }
    }

    doc.summary.push(format!(
        "interfaces: {n} (up {up}, down {down}, admin-down {admin})"
    ));
    doc.summary.push(format!(
        "{} interface(s) with errors/drops or unexpected down state",
        notable.len()
    ));

    let listed: Vec<&Iface> = if full {
        ifaces.iter().collect()
    } else {
        notable
    };
    if !listed.is_empty() {
        let rows = listed
            .iter()
            .map(|i| {
                let state = if i.admin_down {
                    "admin-down"
                } else if i.oper_up {
                    "up"
                } else {
                    "down"
                };
                vec![
                    i.name.clone(),
                    state.to_string(),
                    if i.ip.is_empty() {
                        "-".into()
                    } else {
                        i.ip.clone()
                    },
                    i.in_err.to_string(),
                    i.crc.to_string(),
                    i.out_drops.to_string(),
                    i.desc.clone(),
                ]
            })
            .collect();
        let omitted = if full {
            None
        } else {
            let healthy = n - listed.len();
            (healthy > 0)
                .then(|| format!("{healthy} healthy interfaces omitted (--full to list all)"))
        };
        doc.tables.push(Table {
            title: if full {
                "INTERFACES".into()
            } else {
                "NOTABLE INTERFACES".into()
            },
            header: [
                "interface",
                "state",
                "ip",
                "in_err",
                "crc",
                "out_drops",
                "description",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            rows,
            omitted,
        });
    } else if !full && n > 0 {
        doc.notes.push(format!(
            "all {n} interfaces healthy (up, zero error counters); table omitted (--full to list)"
        ));
    }
    doc
}
