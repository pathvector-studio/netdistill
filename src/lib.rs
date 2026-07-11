pub mod detect;
pub mod http;
pub mod mcp;
pub mod model;
pub mod render;
pub mod token;

pub mod parse {
    pub mod bgp_summary;
    pub mod bgp_table;
    pub mod config;
    pub mod generic;
    pub mod interfaces;
    pub mod isis_neighbors;
    pub mod juniper_config;
    pub mod lldp_neighbors;
    pub mod ospf_neighbors;
    pub mod route;
}

use detect::Kind;
use model::Doc;

pub struct Options {
    pub full: bool,
    /// Force a specific parser instead of auto-detection.
    pub kind: Option<Kind>,
}

/// One-call convenience shared by the CLI, the MCP server, and the HTTP API:
/// raw input in, rendered digest (text or JSON) out.
pub fn distill_to_string(
    input: &str,
    full: bool,
    budget: Option<usize>,
    json: bool,
    kind: Option<Kind>,
) -> String {
    let in_tok = token::estimate(input);
    let docs = distill(input, &Options { full, kind });
    if json {
        render::render_json(&docs, in_tok)
    } else {
        render::render_text(docs, in_tok, budget)
    }
}

/// Distill one blob of input (possibly a whole terminal session containing
/// several commands) into one Doc per detected chunk.
pub fn distill(input: &str, opts: &Options) -> Vec<Doc> {
    split_sessions(input)
        .into_iter()
        .map(|(label, chunk)| {
            let mut doc = match opts.kind.unwrap_or_else(|| detect::detect(&chunk)) {
                Kind::BgpSummary => parse::bgp_summary::parse(&chunk, opts.full),
                Kind::BgpTable => parse::bgp_table::parse(&chunk, opts.full),
                Kind::RouteTable => parse::route::parse(&chunk, opts.full),
                Kind::Interfaces => parse::interfaces::parse(&chunk, opts.full),
                Kind::OspfNeighbors => parse::ospf_neighbors::parse(&chunk, opts.full),
                Kind::IsisNeighbors => parse::isis_neighbors::parse(&chunk, opts.full),
                Kind::LldpNeighbors => parse::lldp_neighbors::parse(&chunk, opts.full),
                Kind::Config => parse::config::parse(&chunk, opts.full),
                Kind::JunosConfig => parse::juniper_config::parse(&chunk, opts.full),
                Kind::Generic => parse::generic::parse(&chunk),
            };
            if let Some(cmd) = label {
                doc.headline = format!("`{cmd}` — {}", doc.headline);
            }
            doc
        })
        .collect()
}

/// Split a pasted terminal session on prompt lines like
/// `router1# show ip bgp summary` or `user@host> show route`.
/// Returns the whole input as one unlabeled chunk if no prompts are found.
pub fn split_sessions(input: &str) -> Vec<(Option<String>, String)> {
    let mut chunks: Vec<(Option<String>, String)> = Vec::new();
    let mut cur_label: Option<String> = None;
    let mut cur = String::new();
    let mut saw_prompt = false;

    for line in input.lines() {
        if let Some(cmd) = prompt_command(line) {
            saw_prompt = true;
            if !cur.trim().is_empty() {
                chunks.push((cur_label.clone(), std::mem::take(&mut cur)));
            } else {
                cur.clear();
            }
            cur_label = Some(cmd);
        } else {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    if !cur.trim().is_empty() || chunks.is_empty() {
        chunks.push((cur_label, cur));
    }

    if !saw_prompt {
        return vec![(None, input.to_string())];
    }
    // Drop tiny unlabeled fragments (banners, prompts-only noise) when the
    // session was split into labeled commands.
    if chunks.len() > 1 {
        chunks.retain(|(label, body)| {
            label.is_some() || body.lines().filter(|l| !l.trim().is_empty()).count() >= 3
        });
    }
    chunks
}

fn prompt_command(line: &str) -> Option<String> {
    let t = line.trim();
    for sep in ['#', '>'] {
        if let Some((pre, post)) = t.split_once(sep) {
            let cmd = post.trim();
            if !pre.is_empty()
                && pre.len() <= 64
                && !pre.contains(' ')
                && (cmd.starts_with("show ") || cmd.starts_with("display "))
            {
                return Some(cmd.to_string());
            }
        }
    }
    None
}
