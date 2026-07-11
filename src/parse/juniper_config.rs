use crate::model::{Doc, Table};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct JIf {
    desc: String,
    addrs: Vec<String>,
    disabled: bool,
}

#[derive(Default)]
struct JNbr {
    peer_as: String,
    group: String,
    desc: String,
    instance: String,
}

/// Parse a Junos configuration — either `show configuration | display set`
/// style or the native hierarchical (braces) style. Both are normalized to
/// flat statement paths first, so one extractor serves both.
pub fn parse(input: &str, _full: bool) -> Doc {
    let stmts = flatten(input);

    let mut hostname = String::new();
    let mut ifs: BTreeMap<String, JIf> = BTreeMap::new();
    let mut nbrs: BTreeMap<String, JNbr> = BTreeMap::new();
    let mut instances: BTreeSet<String> = BTreeSet::new();
    let mut policies: BTreeSet<String> = BTreeSet::new();
    let mut prefix_lists: BTreeSet<String> = BTreeSet::new();
    let mut users: BTreeSet<String> = BTreeSet::new();
    let mut other: BTreeSet<String> = BTreeSet::new();
    let mut statics = 0usize;
    let mut ntp = 0usize;
    let mut snmp = false;

    for s in &stmts {
        let t: Vec<&str> = s.iter().map(String::as_str).collect();
        match t.as_slice() {
            ["system", "host-name", h, ..] => hostname = h.to_string(),
            ["system", "ntp", "server", ..] => ntp += 1,
            ["system", "login", "user", u, ..] => {
                users.insert(u.to_string());
            }
            ["system", ..] => {}
            ["interfaces", ifn, rest @ ..] => {
                let e = ifs.entry(ifn.to_string()).or_default();
                if let Some(p) = rest.iter().position(|x| *x == "description") {
                    e.desc = rest[p + 1..].join(" ");
                } else if let Some(p) = rest.iter().position(|x| *x == "address") {
                    if let Some(a) = rest.get(p + 1) {
                        e.addrs.push(a.to_string());
                    }
                } else if rest == ["disable"] {
                    e.disabled = true;
                }
            }
            ["protocols", "bgp", rest @ ..] => bgp_stmt(rest, "", &mut nbrs),
            ["routing-instances", name, rest @ ..] => {
                instances.insert(name.to_string());
                if let ["protocols", "bgp", r2 @ ..] = rest {
                    bgp_stmt(r2, name, &mut nbrs);
                } else if rest.starts_with(&["routing-options", "static", "route"]) {
                    statics += 1;
                }
            }
            ["routing-options", "static", "route", ..] => statics += 1,
            ["policy-options", "policy-statement", n, ..] => {
                policies.insert(n.to_string());
            }
            ["policy-options", "prefix-list", n, ..] => {
                prefix_lists.insert(n.to_string());
            }
            ["snmp", ..] => snmp = true,
            [first, ..] => {
                other.insert(first.to_string());
            }
            [] => {}
        }
    }

    let host = if hostname.is_empty() {
        "(no host-name)".to_string()
    } else {
        hostname
    };
    let mut doc = Doc::new(
        "config (junos)",
        format!("{host} — Junos config, {} statements", stmts.len()),
    );

    let disabled = ifs.values().filter(|f| f.disabled).count();
    doc.summary
        .push(format!("interfaces: {} ({disabled} disabled)", ifs.len()));
    if !nbrs.is_empty() {
        doc.summary.push(format!("bgp: {} neighbors", nbrs.len()));
    }
    if !instances.is_empty() {
        doc.summary.push(format!(
            "routing-instances: {}",
            instances.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    doc.summary.push(format!(
        "policy: {} policy-statements, {} prefix-lists, {statics} static routes",
        policies.len(),
        prefix_lists.len()
    ));
    doc.summary.push(format!(
        "mgmt: ntp servers {ntp}, snmp {}, local users {}",
        if snmp { "yes" } else { "no" },
        users.len()
    ));

    if !ifs.is_empty() {
        let rows = ifs
            .iter()
            .map(|(n, f)| {
                let addr = match f.addrs.len() {
                    0 => "-".to_string(),
                    1 => f.addrs[0].clone(),
                    more => format!("{} (+{} more)", f.addrs[0], more - 1),
                };
                vec![
                    n.clone(),
                    addr,
                    if f.disabled {
                        "disable".into()
                    } else {
                        "up".into()
                    },
                    f.desc.clone(),
                ]
            })
            .collect();
        doc.tables.push(Table {
            title: "INTERFACES".into(),
            header: ["interface", "address", "admin", "description"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            omitted: None,
        });
    }
    if !nbrs.is_empty() {
        let rows = nbrs
            .iter()
            .map(|(ip, nb)| {
                vec![
                    ip.clone(),
                    if nb.peer_as.is_empty() {
                        "-".into()
                    } else {
                        nb.peer_as.clone()
                    },
                    if nb.group.is_empty() {
                        "-".into()
                    } else {
                        nb.group.clone()
                    },
                    if nb.instance.is_empty() {
                        "-".into()
                    } else {
                        nb.instance.clone()
                    },
                    nb.desc.clone(),
                ]
            })
            .collect();
        doc.tables.push(Table {
            title: "BGP NEIGHBORS".into(),
            header: ["neighbor", "peer-as", "group", "instance", "description"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            omitted: None,
        });
    }
    if !other.is_empty() {
        let mut list: Vec<&String> = other.iter().collect();
        list.truncate(15);
        doc.notes.push(format!(
            "top-level sections present but not summarized: {}{}",
            list.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            if other.len() > 15 { ", ..." } else { "" }
        ));
    }
    doc.notes.push(
        "secrets (encrypted-password, authentication-key, pre-shared-key) are not extracted".into(),
    );
    doc
}

fn bgp_stmt(rest: &[&str], instance: &str, nbrs: &mut BTreeMap<String, JNbr>) {
    let group = if rest.first() == Some(&"group") {
        rest.get(1).copied().unwrap_or("")
    } else {
        ""
    };
    let Some(pos) = rest.iter().position(|x| *x == "neighbor") else {
        return;
    };
    let Some(ip) = rest.get(pos + 1) else { return };
    let e = nbrs.entry(ip.to_string()).or_default();
    if !group.is_empty() {
        e.group = group.to_string();
    }
    if !instance.is_empty() {
        e.instance = instance.to_string();
    }
    match rest.get(pos + 2) {
        Some(&"peer-as") => {
            if let Some(a) = rest.get(pos + 3) {
                e.peer_as = a.to_string();
            }
        }
        Some(&"description") => e.desc = rest[pos + 3..].join(" "),
        _ => {}
    }
}

/// Flatten either config style into statement paths (without the `set` word).
fn flatten(input: &str) -> Vec<Vec<String>> {
    let set_lines: Vec<&str> = input
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("set "))
        .collect();
    if set_lines.len() >= 3 {
        return set_lines.iter().map(|l| tokenize(&l[4..])).collect();
    }

    let mut stack: Vec<Vec<String>> = Vec::new();
    let mut out: Vec<Vec<String>> = Vec::new();
    for raw in input.lines() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with("/*") || l.starts_with('*') {
            continue;
        }
        if l == "}" || l == "};" {
            stack.pop();
            continue;
        }
        if let Some(head) = l.strip_suffix('{') {
            stack.push(tokenize(head.trim()));
            continue;
        }
        if let Some(leaf) = l.strip_suffix(';') {
            let mut path: Vec<String> = stack.iter().flatten().cloned().collect();
            path.extend(tokenize(leaf.trim()));
            out.push(path);
        }
    }
    out
}

/// Whitespace tokenizer that keeps quoted strings ("a b") as one token.
fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
