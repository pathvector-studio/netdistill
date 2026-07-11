use crate::model::{Doc, Table};
use std::collections::BTreeMap;

/// Parse a full `show ip bgp` / `show bgp` table (IOS/FRR style) into an
/// aggregate digest. Per-prefix rows are never re-emitted — a full table can
/// be millions of tokens; the digest carries origin-AS and next-hop
/// statistics instead.
pub fn parse(input: &str, _full: bool) -> Doc {
    let mut router_id: Option<String> = None;
    let mut sections: Vec<String> = Vec::new();
    let mut path_col: Option<usize> = None;
    let mut nexthop_col: usize = 20;

    let mut networks = 0u64;
    let mut paths = 0u64;
    let mut best = 0u64;
    let (mut suppressed, mut damped, mut history, mut ribfail) = (0u64, 0u64, 0u64, 0u64);
    let mut local_orig = 0u64;
    let mut origin_best: BTreeMap<String, u64> = BTreeMap::new();
    let mut nexthops: BTreeMap<String, u64> = BTreeMap::new();
    let mut plen_sum = 0u64;
    let mut plen_max = 0usize;
    let mut plen_n = 0u64;

    let mut cur_net = String::new();
    let mut cur_net_has_best = true;
    let mut no_best_total = 0u64;
    let mut no_best_examples: Vec<String> = Vec::new();
    let close_net = |net: &str, has_best: bool, total: &mut u64, ex: &mut Vec<String>| {
        if !net.is_empty() && !has_best {
            *total += 1;
            if ex.len() < 5 {
                ex.push(net.to_string());
            }
        }
    };

    for raw in input.lines() {
        let t = raw.trim_end();
        if t.is_empty() {
            continue;
        }
        let ts = t.trim_start();
        if let Some(rest) = ts.strip_prefix("BGP table version is ") {
            let tag = "local router ID is ";
            if let Some(p) = rest.find(tag) {
                router_id = rest[p + tag.len()..]
                    .split_whitespace()
                    .next()
                    .map(str::to_string);
            }
            continue;
        }
        if let Some(rest) = ts.strip_prefix("For address family: ") {
            sections.push(rest.trim().to_string());
            continue;
        }
        if ts.starts_with("Status codes:")
            || ts.starts_with("Origin codes:")
            || ts.starts_with("RPKI validation codes:")
            || ts.starts_with("Nexthop codes:")
        {
            continue;
        }
        if t.contains("Network") && t.contains("Next Hop") && t.contains("Path") {
            path_col = t.find("Path");
            nexthop_col = t.find("Next Hop").unwrap_or(20);
            continue;
        }

        // Data rows start with status flags (*, >, s, d, h, r, i, ...).
        let stripped = t
            .chars()
            .take_while(|c| "*>sdhirSmbfxacVIN ".contains(*c))
            .count();
        if stripped == 0 || stripped >= t.len() {
            continue;
        }
        let flags = &t[..stripped];
        if !flags.chars().any(|c| "*sdhr".contains(c)) {
            continue;
        }
        let f: Vec<&str> = t[stripped..].split_whitespace().collect();
        if f.is_empty() {
            continue;
        }
        // The first token must be address-shaped (network or next-hop); this
        // rejects the indented continuation lines of the Status codes legend.
        if !(f[0].contains('.') || f[0].contains(':'))
            || !f[0]
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':' || c == '/')
        {
            continue;
        }

        // Network column is left-aligned right after the flags; a row whose
        // first token only starts near the Next Hop column is an additional
        // path for the previous network.
        let nh_idx = if stripped < nexthop_col.saturating_sub(2) {
            close_net(
                &cur_net,
                cur_net_has_best,
                &mut no_best_total,
                &mut no_best_examples,
            );
            cur_net = f[0].to_string();
            cur_net_has_best = false;
            networks += 1;
            1
        } else {
            0
        };

        paths += 1;
        let is_best = flags.contains('>');
        if is_best {
            best += 1;
            cur_net_has_best = true;
        }
        if flags.contains('s') {
            suppressed += 1;
        }
        if flags.contains('d') {
            damped += 1;
        }
        if flags.contains('h') {
            history += 1;
        }
        if flags.contains('r') {
            ribfail += 1;
        }
        if let Some(nh) = f.get(nh_idx) {
            *nexthops.entry(nh.to_string()).or_insert(0) += 1;
        }

        // The Path field is taken by column position (from the header line),
        // which cleanly separates it from the numeric Metric/LocPrf/Weight
        // columns that would otherwise be indistinguishable from AS numbers.
        let Some(pc) = path_col else { continue };
        let Some(pf) = t.get(pc..) else { continue };
        let toks: Vec<&str> = pf.split_whitespace().collect();
        if !toks.last().is_some_and(|o| ["i", "e", "?"].contains(o)) {
            continue;
        }
        let asns: Vec<&str> = toks
            .iter()
            .copied()
            .filter(|x| x.starts_with(|c: char| c.is_ascii_digit() || c == '{'))
            .collect();
        if asns.is_empty() {
            if is_best {
                local_orig += 1;
            }
        } else {
            plen_sum += asns.len() as u64;
            plen_n += 1;
            plen_max = plen_max.max(asns.len());
            if is_best {
                let oa = asns.last().unwrap().trim_matches(|c| c == '{' || c == '}');
                let oa = oa.split(',').next().unwrap_or(oa);
                *origin_best.entry(format!("AS{oa}")).or_insert(0) += 1;
            }
        }
    }
    close_net(
        &cur_net,
        cur_net_has_best,
        &mut no_best_total,
        &mut no_best_examples,
    );

    let mut headline = format!("{networks} networks, {paths} paths");
    if let Some(r) = &router_id {
        headline = format!("router-id {r} — {headline}");
    }
    let mut doc = Doc::new("bgp-table", headline);

    if no_best_total > 0 {
        doc.alerts.push(format!(
            "{no_best_total} network(s) with no best path (e.g. {})",
            no_best_examples.join(", ")
        ));
    }
    for (n, what) in [
        (suppressed, "suppressed"),
        (damped, "damped"),
        (history, "history (withdrawn, dampening)"),
        (ribfail, "RIB-failure"),
    ] {
        if n > 0 {
            doc.alerts.push(format!("{n} path(s) {what}"));
        }
    }

    doc.summary
        .push(format!("networks {networks}, paths {paths}, best {best}"));
    if local_orig > 0 {
        doc.summary
            .push(format!("locally originated best paths: {local_orig}"));
    }
    if plen_n > 0 {
        doc.summary.push(format!(
            "AS-path length: avg {:.1}, max {plen_max}",
            plen_sum as f64 / plen_n as f64
        ));
    }
    if !sections.is_empty() {
        doc.summary
            .push(format!("address families: {}", sections.join(", ")));
    }

    if !origin_best.is_empty() {
        let mut oa: Vec<(&String, &u64)> = origin_best.iter().collect();
        oa.sort_by_key(|(_, v)| std::cmp::Reverse(**v));
        let rows = oa
            .iter()
            .take(10)
            .map(|(k, v)| vec![(*k).clone(), v.to_string()])
            .collect();
        let omitted =
            (oa.len() > 10).then(|| format!("{} more origin ASes omitted", oa.len() - 10));
        doc.tables.push(Table {
            title: "TOP ORIGIN AS (by best paths)".into(),
            header: vec!["origin_as".into(), "best_paths".into()],
            rows,
            omitted,
        });
    }
    if !nexthops.is_empty() {
        let mut nh: Vec<(&String, &u64)> = nexthops.iter().collect();
        nh.sort_by_key(|(_, v)| std::cmp::Reverse(**v));
        let rows = nh
            .iter()
            .take(8)
            .map(|(k, v)| vec![(*k).clone(), v.to_string()])
            .collect();
        let omitted = (nh.len() > 8).then(|| format!("{} more next-hops omitted", nh.len() - 8));
        doc.tables.push(Table {
            title: "TOP NEXT-HOPS (by paths)".into(),
            header: vec!["next-hop".into(), "paths".into()],
            rows,
            omitted,
        });
    }
    doc.notes.push(
        "per-prefix rows are never expanded for bgp-table input (even with --full) — this is an aggregate digest; for per-prefix detail run the routing table through netdistill --full"
            .into(),
    );
    doc
}
