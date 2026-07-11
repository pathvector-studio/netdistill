use crate::model::Doc;
use crate::token;
use serde::Serialize;

/// Render the final text digest, enforcing an optional token budget by
/// progressively trimming table rows and raw bodies (never alerts/summary).
pub fn render_text(mut docs: Vec<Doc>, input_tokens: usize, budget: Option<usize>) -> String {
    loop {
        let body = render_body(&docs);
        let out_tokens = token::estimate(&body) + 20; // + receipt line
        let fits = budget.is_none_or(|b| out_tokens <= b);
        if fits || !shrink(&mut docs) {
            let pct: i64 = if input_tokens > 0 {
                100 - (out_tokens as i64 * 100 / input_tokens as i64)
            } else {
                0
            };
            let mut s = format!(
                "# netdistill — input ~{input_tokens} tok → output ~{out_tokens} tok ({pct}% saved; estimates)\n\n"
            );
            s.push_str(&body);
            if !fits {
                s.push_str("\n[!] --budget could not be met; smallest faithful output shown\n");
            }
            return s;
        }
    }
}

pub fn render_body(docs: &[Doc]) -> String {
    let mut s = String::new();
    for (i, d) in docs.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        s.push_str(&format!("## [{}] {}\n", d.kind, d.headline));
        if d.alerts.is_empty() {
            s.push_str("ALERTS: none\n");
        } else {
            s.push_str(&format!("ALERTS ({}):\n", d.alerts.len()));
            for a in &d.alerts {
                s.push_str(&format!("- {a}\n"));
            }
        }
        if !d.summary.is_empty() {
            s.push_str("SUMMARY:\n");
            for l in &d.summary {
                s.push_str(&format!("- {l}\n"));
            }
        }
        for t in &d.tables {
            s.push('\n');
            s.push_str(&format!("{}:\n", t.title));
            s.push_str(&t.header.join("\t"));
            s.push('\n');
            for r in &t.rows {
                s.push_str(&r.join("\t"));
                s.push('\n');
            }
            if let Some(o) = &t.omitted {
                s.push_str(&format!("({o})\n"));
            }
        }
        if let Some(b) = &d.body {
            s.push('\n');
            s.push_str(b);
            s.push('\n');
        }
        if !d.notes.is_empty() {
            s.push('\n');
            s.push_str("NOTES:\n");
            for note in &d.notes {
                s.push_str(&format!("- {note}\n"));
            }
        }
    }
    s
}

fn shrink(docs: &mut [Doc]) -> bool {
    // 1) Halve the biggest table.
    let mut best: Option<(usize, usize, usize)> = None;
    for (di, d) in docs.iter().enumerate() {
        for (ti, t) in d.tables.iter().enumerate() {
            if t.rows.len() >= 4 && best.is_none_or(|(_, _, r)| t.rows.len() > r) {
                best = Some((di, ti, t.rows.len()));
            }
        }
    }
    if let Some((di, ti, r)) = best {
        let t = &mut docs[di].tables[ti];
        let keep = r / 2;
        let cut = r - keep;
        t.rows.truncate(keep);
        t.omitted = Some(match &t.omitted {
            Some(o) => format!("{o}; {cut} rows trimmed for --budget"),
            None => format!("{cut} rows trimmed for --budget"),
        });
        return true;
    }
    // 2) Halve the biggest raw body.
    let mut bi: Option<(usize, usize)> = None;
    for (di, d) in docs.iter().enumerate() {
        if let Some(b) = &d.body {
            let lc = b.lines().count();
            if lc >= 8 && bi.is_none_or(|(_, l)| lc > l) {
                bi = Some((di, lc));
            }
        }
    }
    if let Some((di, lc)) = bi {
        let keep = lc / 2;
        let b = docs[di].body.take().unwrap();
        let kept: Vec<&str> = b.lines().take(keep).collect();
        docs[di].body = Some(format!(
            "{}\n[... {} lines trimmed for --budget]",
            kept.join("\n"),
            lc - keep
        ));
        return true;
    }
    false
}

#[derive(Serialize)]
struct JsonOut<'a> {
    input_tokens_est: usize,
    output_tokens_est: usize,
    docs: &'a [Doc],
}

pub fn render_json(docs: &[Doc], input_tokens: usize) -> String {
    let body = render_body(docs);
    let out = JsonOut {
        input_tokens_est: input_tokens,
        output_tokens_est: token::estimate(&body),
        docs,
    };
    serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".into())
}
