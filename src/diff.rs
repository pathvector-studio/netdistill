//! Diff mode: compare two snapshots of the same command output and emit only
//! what changed. Unchanged state is the ultimate token waste — a diff digest
//! of two healthy snapshots collapses to "no changes detected".
//!
//! Both snapshots are distilled with `full: true` internally, so changes in
//! entries that the normal digest would elide (healthy peers, clean
//! interfaces) are still caught.

use crate::model::{Doc, Table};
use std::collections::{BTreeMap, BTreeSet};

/// Default drift threshold: numeric cells changing by less than this many
/// percent (and pure time-format cells) are treated as natural counter drift.
pub const DEFAULT_DRIFT_PCT: u32 = 10;

/// Pair up docs from two snapshots (by kind, in order) and diff each pair.
/// Uses the default drift damping; see [`diff_docs_with`] to tune it.
pub fn diff_docs(before: Vec<Doc>, after: Vec<Doc>) -> Vec<Doc> {
    diff_docs_with(before, after, DEFAULT_DRIFT_PCT)
}

/// `drift_pct = 0` disables damping (every changed cell is reported).
pub fn diff_docs_with(before: Vec<Doc>, after: Vec<Doc>, drift_pct: u32) -> Vec<Doc> {
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
                out.push(diff_pair(&before[i], a, drift_pct));
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

/// "5w4d", "00:00:31", "1d03h", "never" — values that move on every refresh.
fn is_timeish(s: &str) -> bool {
    let s = s.trim();
    if s == "never" {
        return true;
    }
    if s.is_empty() || !s.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let colon_time = s.contains(':')
        && s.chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == ' ');
    let wdh = s.chars().any(|c| "wdhms".contains(c))
        && s.chars().all(|c| c.is_ascii_digit() || "wdhms".contains(c));
    colon_time || wdh
}

fn numeric_drift(a: &str, b: &str, pct: u32) -> bool {
    let (Ok(x), Ok(y)) = (a.parse::<f64>(), b.parse::<f64>()) else {
        return false;
    };
    let base = x.abs().max(y.abs());
    if base == 0.0 {
        return true;
    }
    ((x - y).abs() / base) * 100.0 < pct as f64
}

fn is_drift(bc: &str, ac: &str, pct: u32) -> bool {
    if pct == 0 {
        return false;
    }
    (is_timeish(bc) && is_timeish(ac)) || numeric_drift(bc, ac, pct)
}

/// Alert text with time-format tokens masked, so an alert that only moved
/// its timestamp ("up only 00:42:11" → "up only 00:55:02") compares equal.
fn mask_alert(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let core = w.trim_matches(|c: char| "(),".contains(c));
            if is_timeish(core) {
                "<t>".to_string()
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn diff_pair(b: &Doc, a: &Doc, drift_pct: u32) -> Doc {
    let headline = if b.headline == a.headline {
        a.headline.clone()
    } else {
        format!("{}  →  {}", b.headline, a.headline)
    };
    let mut d = Doc::new(&format!("diff ({})", a.kind), headline);
    let mut changes = 0usize;

    // Alerts: appeared / resolved. Alerts differing only in time tokens are
    // treated as carried over, not as a NEW/RESOLVED pair.
    let b_masks: Vec<String> = b.alerts.iter().map(|x| mask_alert(x)).collect();
    let a_masks: Vec<String> = a.alerts.iter().map(|x| mask_alert(x)).collect();
    let mut carried = 0usize;
    for x in &a.alerts {
        if b.alerts.contains(x) || b_masks.contains(&mask_alert(x)) {
            carried += 1;
        } else {
            d.alerts.push(format!("NEW: {x}"));
            changes += 1;
        }
    }
    for x in &b.alerts {
        if !a.alerts.contains(x) && !a_masks.contains(&mask_alert(x)) {
            d.alerts.push(format!("RESOLVED: {x}"));
            changes += 1;
        }
    }
    if carried > 0 {
        d.notes.push(format!(
            "{carried} alert(s) present in both snapshots (unchanged or timestamp-only)"
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
                if let Some((tbl, n)) = diff_table(bt, at, drift_pct) {
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

fn diff_table(bt: &Table, at: &Table, drift_pct: u32) -> Option<(Table, usize)> {
    let kw = key_width(bt).max(key_width(at));
    let key = |r: &Vec<String>| -> String {
        r.iter().take(kw).cloned().collect::<Vec<_>>().join("\u{1}")
    };
    let bmap: BTreeMap<String, &Vec<String>> = bt.rows.iter().map(|r| (key(r), r)).collect();
    let amap: BTreeSet<String> = at.rows.iter().map(&key).collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut dampened = 0usize;
    for r in &at.rows {
        match bmap.get(&key(r)) {
            None => rows.push(marked_row(r, "+")),
            Some(br) if *br != r => {
                // A row whose every changed cell is counter/uptime drift is
                // suppressed — it carries no diagnostic signal.
                let all_drift = r.iter().enumerate().all(|(i, ac)| {
                    let bc = br.get(i).map(String::as_str).unwrap_or("");
                    bc == ac || is_drift(bc, ac, drift_pct)
                });
                if all_drift {
                    dampened += 1;
                    continue;
                }
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
    if rows.is_empty() && dampened == 0 {
        return None;
    }
    let changes = rows.len();
    let touched = rows.iter().filter(|r| r[0] != "-").count();
    let unchanged = at
        .rows
        .len()
        .saturating_sub(touched)
        .saturating_sub(dampened);
    let mut notes: Vec<String> = Vec::new();
    if unchanged > 0 {
        notes.push(format!("{unchanged} unchanged rows omitted"));
    }
    if dampened > 0 {
        notes.push(format!(
            "{dampened} rows changed only by counter/uptime drift (<{drift_pct}%) — suppressed; --drift 0 to show"
        ));
    }
    let mut header = vec!["Δ".to_string()];
    header.extend(at.header.iter().cloned());
    Some((
        Table {
            title: at.title.clone(),
            header,
            rows,
            omitted: (!notes.is_empty()).then(|| notes.join("; ")),
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
