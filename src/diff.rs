//! Diff mode: compare two snapshots of the same command output and emit only
//! what changed. Unchanged state is the ultimate token waste — a diff digest
//! of two healthy snapshots collapses to "no changes detected".
//!
//! Both snapshots are distilled with `full: true` internally, so changes in
//! entries that the normal digest would elide (healthy peers, clean
//! interfaces) are still caught.

use crate::model::{Doc, Table};
use std::collections::{BTreeMap, BTreeSet};

/// Pair up docs from two snapshots (by kind, in order) and diff each pair.
pub fn diff_docs(before: Vec<Doc>, after: Vec<Doc>) -> Vec<Doc> {
    let mut out = Vec::new();
    let mut b_used = vec![false; before.len()];
    for a in &after {
        let bi = before
            .iter()
            .enumerate()
            .find(|(i, b)| !b_used[*i] && b.kind == a.kind)
            .map(|(i, _)| i);
        match bi {
            Some(i) => {
                b_used[i] = true;
                out.push(diff_pair(&before[i], a));
            }
            None => {
                let mut d = Doc::new(&format!("diff ({})", a.kind), a.headline.clone());
                d.alerts
                    .push("section present only in AFTER snapshot".into());
                out.push(d);
            }
        }
    }
    for (i, b) in before.iter().enumerate() {
        if !b_used[i] {
            let mut d = Doc::new(&format!("diff ({})", b.kind), b.headline.clone());
            d.alerts
                .push("section present only in BEFORE snapshot".into());
            out.push(d);
        }
    }
    out
}

fn diff_pair(b: &Doc, a: &Doc) -> Doc {
    let headline = if b.headline == a.headline {
        a.headline.clone()
    } else {
        format!("{}  →  {}", b.headline, a.headline)
    };
    let mut d = Doc::new(&format!("diff ({})", a.kind), headline);
    let mut changes = 0usize;

    // Alerts: appeared / resolved.
    for x in &a.alerts {
        if !b.alerts.contains(x) {
            d.alerts.push(format!("NEW: {x}"));
            changes += 1;
        }
    }
    for x in &b.alerts {
        if !a.alerts.contains(x) {
            d.alerts.push(format!("RESOLVED: {x}"));
            changes += 1;
        }
    }
    let carried = a.alerts.iter().filter(|x| b.alerts.contains(*x)).count();
    if carried > 0 {
        d.notes.push(format!(
            "{carried} alert(s) present in both snapshots (unchanged)"
        ));
    }

    // Summary lines, matched by their "key:" prefix.
    let keyed = |v: &[String]| -> BTreeMap<String, String> {
        v.iter()
            .filter_map(|s| {
                s.split_once(':')
                    .map(|(k, val)| (k.trim().to_string(), val.trim().to_string()))
            })
            .collect()
    };
    let bs = keyed(&b.summary);
    let asum = keyed(&a.summary);
    for (k, av) in &asum {
        match bs.get(k) {
            Some(bv) if bv != av => {
                d.summary.push(format!("{k}: {bv}  →  {av}"));
                changes += 1;
            }
            None => {
                d.summary.push(format!("{k} (new): {av}"));
                changes += 1;
            }
            _ => {}
        }
    }
    for (k, bv) in &bs {
        if !asum.contains_key(k) {
            d.summary.push(format!("{k} (gone): {bv}"));
            changes += 1;
        }
    }

    // Tables, matched by title; rows keyed by their leading column(s).
    for at in &a.tables {
        match b.tables.iter().find(|t| t.title == at.title) {
            Some(bt) => {
                if let Some((tbl, n)) = diff_table(bt, at) {
                    changes += n;
                    d.tables.push(tbl);
                }
            }
            None => {
                d.tables.push(marked_table(at, "+"));
                changes += at.rows.len();
            }
        }
    }
    for bt in &b.tables {
        if !a.tables.iter().any(|t| t.title == bt.title) {
            d.tables.push(marked_table(bt, "-"));
            changes += bt.rows.len();
        }
    }

    if changes == 0 {
        d.summary.push("no changes detected".into());
    }
    d.notes.push(
        "diff of fully-parsed snapshots; unchanged entries omitted. Δ: + added, - removed, ~ changed (cells shown as old→new). Counter columns (prefix counts, uptimes) drift naturally"
            .into(),
    );
    d
}

/// Rows are keyed by the first column; if that is not unique within either
/// table (e.g. LLDP: one device on several ports), the first two columns are
/// used instead.
fn key_width(t: &Table) -> usize {
    let mut seen = BTreeSet::new();
    for r in &t.rows {
        if let Some(c0) = r.first() {
            if !seen.insert(c0.clone()) {
                return 2;
            }
        }
    }
    1
}

fn diff_table(bt: &Table, at: &Table) -> Option<(Table, usize)> {
    let kw = key_width(bt).max(key_width(at));
    let key = |r: &Vec<String>| -> String {
        r.iter().take(kw).cloned().collect::<Vec<_>>().join("\u{1}")
    };
    let bmap: BTreeMap<String, &Vec<String>> = bt.rows.iter().map(|r| (key(r), r)).collect();
    let amap: BTreeSet<String> = at.rows.iter().map(&key).collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    for r in &at.rows {
        match bmap.get(&key(r)) {
            None => rows.push(marked_row(r, "+")),
            Some(br) if *br != r => {
                let mut cells = vec!["~".to_string()];
                for (i, ac) in r.iter().enumerate() {
                    let bc = br.get(i).map(String::as_str).unwrap_or("");
                    cells.push(if bc == ac {
                        ac.clone()
                    } else {
                        format!("{bc}→{ac}")
                    });
                }
                rows.push(cells);
            }
            _ => {}
        }
    }
    for r in &bt.rows {
        if !amap.contains(&key(r)) {
            rows.push(marked_row(r, "-"));
        }
    }
    if rows.is_empty() {
        return None;
    }
    let changes = rows.len();
    let touched = rows.iter().filter(|r| r[0] != "-").count();
    let unchanged = at.rows.len().saturating_sub(touched);
    let mut header = vec!["Δ".to_string()];
    header.extend(at.header.iter().cloned());
    Some((
        Table {
            title: at.title.clone(),
            header,
            rows,
            omitted: (unchanged > 0).then(|| format!("{unchanged} unchanged rows omitted")),
        },
        changes,
    ))
}

fn marked_row(r: &[String], m: &str) -> Vec<String> {
    let mut out = vec![m.to_string()];
    out.extend(r.iter().cloned());
    out
}

fn marked_table(t: &Table, m: &str) -> Table {
    let mut header = vec!["Δ".to_string()];
    header.extend(t.header.iter().cloned());
    Table {
        title: t.title.clone(),
        header,
        rows: t.rows.iter().map(|r| marked_row(r, m)).collect(),
        omitted: None,
    }
}
