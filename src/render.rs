use crate::model::Doc;
use crate::token;
use serde::Serialize;

/// Output serialization for a distilled digest.
///
/// - `Text` — the default human-readable digest (markdown-ish, TSV tables).
/// - `Json` — structured JSON (verbose; good for programmatic consumers).
/// - `Toon` — Token-Oriented Object Notation: explicit `[N]{fields}` array
///   headers + comma rows. ~30-60% fewer tokens than JSON for uniform tables
///   while staying schema-explicit, so LLM parsing accuracy holds up.
/// - `Wire` — an LLM-only dense mode: all human affordances (markdown, section
///   labels, blank lines, column padding) stripped; one-char line tags. Still
///   plain in-distribution ASCII (not binary) so the model reasons over it —
///   pushing past that into base64/compressed bytes is a known dead end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Text,
    Json,
    Toon,
    Wire,
}

impl Format {
    pub const NAMES: [&'static str; 4] = ["text", "json", "toon", "wire"];

    pub fn from_name(s: &str) -> Option<Format> {
        Some(match s {
            "text" => Format::Text,
            "json" => Format::Json,
            "toon" => Format::Toon,
            "wire" => Format::Wire,
            _ => return None,
        })
    }
}

/// Render a distilled digest in the requested format, honoring an optional
/// token budget (Text/Toon/Wire progressively trim table rows and raw bodies;
/// Json is emitted whole).
pub fn render(
    docs: Vec<Doc>,
    input_tokens: usize,
    budget: Option<usize>,
    format: Format,
) -> String {
    match format {
        Format::Json => render_json(&docs, input_tokens),
        Format::Text => render_budgeted(docs, input_tokens, budget, render_body, Format::Text),
        Format::Toon => render_budgeted(docs, input_tokens, budget, render_body_toon, Format::Toon),
        Format::Wire => render_budgeted(docs, input_tokens, budget, render_body_wire, Format::Wire),
    }
}

/// Backwards-compatible text renderer (kept for existing callers/tests).
pub fn render_text(docs: Vec<Doc>, input_tokens: usize, budget: Option<usize>) -> String {
    render_budgeted(docs, input_tokens, budget, render_body, Format::Text)
}

/// Shared budget-enforcing loop: render the body, and if it overflows the
/// budget, trim the largest table/body and retry. Alerts and summary lines are
/// never trimmed. The receipt line/header is chosen per format.
fn render_budgeted(
    mut docs: Vec<Doc>,
    input_tokens: usize,
    budget: Option<usize>,
    body_fn: fn(&[Doc]) -> String,
    fmt: Format,
) -> String {
    // Approximate token cost of the receipt/header this format prepends.
    let receipt_tok = match fmt {
        Format::Wire => 12,
        Format::Toon => 16,
        _ => 20,
    };
    loop {
        let body = body_fn(&docs);
        let (body_tok, is_real) = token::count(&body);
        let out_tokens = body_tok + receipt_tok;
        let fits = budget.is_none_or(|b| out_tokens <= b);
        if fits || !shrink(&mut docs) {
            let pct: i64 = if input_tokens > 0 {
                100 - (out_tokens as i64 * 100 / input_tokens as i64)
            } else {
                0
            };
            let mut s = match fmt {
                Format::Wire => {
                    let tag = if is_real { "o200k" } else { "est" };
                    format!("nd in={input_tokens} out={out_tokens} save={pct}% ({tag})\n")
                }
                Format::Toon => format!(
                    "netdistill[1]{{in_tok,out_tok,saved}}:\n  {input_tokens},{out_tokens},{pct}%\n\n"
                ),
                _ => {
                    let src = if is_real { "o200k" } else { "estimates" };
                    format!(
                        "# netdistill — input ~{input_tokens} tok → output ~{out_tokens} tok ({pct}% saved; {src})\n\n"
                    )
                }
            };
            s.push_str(&body);
            if !fits {
                s.push_str(match fmt {
                    Format::Wire => "\n!budget-unmet: smallest faithful output shown\n",
                    _ => "\n[!] --budget could not be met; smallest faithful output shown\n",
                });
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

/// TOON body: string arrays as `key[N]:` lists, tables as `title[N]{cols}:`
/// with comma-separated rows. Schema-explicit and comma-delimited, so it drops
/// JSON's repeated keys and punctuation without hiding structure from the model.
pub fn render_body_toon(docs: &[Doc]) -> String {
    let mut s = String::new();
    for (i, d) in docs.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        s.push_str(&format!("[{}] {}\n", d.kind, d.headline));
        s.push_str(&format!("alerts[{}]:\n", d.alerts.len()));
        for a in &d.alerts {
            s.push_str(&format!("  - {a}\n"));
        }
        if !d.summary.is_empty() {
            s.push_str(&format!("summary[{}]:\n", d.summary.len()));
            for l in &d.summary {
                s.push_str(&format!("  - {l}\n"));
            }
        }
        for t in &d.tables {
            s.push_str(&format!(
                "{}[{}]{{{}}}:\n",
                t.title,
                t.rows.len(),
                t.header.join(",")
            ));
            for r in &t.rows {
                s.push_str("  ");
                s.push_str(&csv_row(r));
                s.push('\n');
            }
            if let Some(o) = &t.omitted {
                s.push_str(&format!("  # {o}\n"));
            }
        }
        if let Some(b) = &d.body {
            s.push_str("body:\n");
            s.push_str(b);
            s.push('\n');
        }
        if !d.notes.is_empty() {
            s.push_str(&format!("notes[{}]:\n", d.notes.len()));
            for note in &d.notes {
                s.push_str(&format!("  - {note}\n"));
            }
        }
    }
    s
}

/// Wire body: one-char line tags, no markdown/labels/blank lines, comma rows.
/// `#`=doc kind+headline, `!`=alert, `.`=summary, `|`=table header (`cols=`),
/// bare comma line=table row, `-`=elision note, `>`=raw body, `~`=note.
pub fn render_body_wire(docs: &[Doc]) -> String {
    let mut s = String::new();
    for d in docs {
        s.push_str(&format!("#{} {}\n", d.kind, d.headline));
        for a in &d.alerts {
            s.push_str(&format!("!{a}\n"));
        }
        for l in &d.summary {
            s.push_str(&format!(".{l}\n"));
        }
        for t in &d.tables {
            s.push_str(&format!("|{} cols={}\n", t.title, t.header.join(",")));
            for r in &t.rows {
                s.push_str(&csv_row(r));
                s.push('\n');
            }
            if let Some(o) = &t.omitted {
                s.push_str(&format!("-{o}\n"));
            }
        }
        if let Some(b) = &d.body {
            s.push_str(">body\n");
            s.push_str(b);
            if !b.ends_with('\n') {
                s.push('\n');
            }
        }
        for note in &d.notes {
            s.push_str(&format!("~{note}\n"));
        }
    }
    s
}

/// Comma-join a row, quoting any field that contains a comma, quote, or edge
/// whitespace (RFC4180-style) so commas inside values never break the schema.
fn csv_row(fields: &[String]) -> String {
    fields
        .iter()
        .map(|f| csv_field(f))
        .collect::<Vec<_>>()
        .join(",")
}

fn csv_field(f: &str) -> String {
    let needs_quote = f.contains(',')
        || f.contains('"')
        || f.contains('\n')
        || f.starts_with(' ')
        || f.ends_with(' ');
    if needs_quote {
        format!("\"{}\"", f.replace('"', "\"\""))
    } else {
        f.to_string()
    }
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
