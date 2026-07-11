use clap::{Parser, Subcommand};
use netdistill::{distill, render, token, Options};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "netdistill",
    version,
    about = "Distill network CLI output and configs into LLM-friendly context.\nPipe in `show` output, configs, or whole terminal session logs."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Input files (default: stdin)
    files: Vec<PathBuf>,
    /// Do not elide healthy/normal entries
    #[arg(long)]
    full: bool,
    /// Approximate output token budget; table detail is trimmed to fit
    #[arg(long)]
    budget: Option<usize>,
    /// Emit structured JSON instead of text
    #[arg(long)]
    json: bool,
    /// Force a specific parser instead of auto-detection
    /// (bgp-summary, bgp-table, route-table, interfaces, ospf-neighbors,
    /// isis-neighbors, lldp-neighbors, config, junos-config, generic)
    #[arg(long)]
    kind: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run as an MCP server on stdio (for Claude Code and other agents)
    Mcp,
    /// Run an HTTP API server callable from any language (POST /v1/distill)
    Serve {
        /// Address to listen on
        #[arg(long, default_value = "127.0.0.1:8321")]
        listen: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let kind = cli.kind.as_deref().map(|name| {
        netdistill::detect::Kind::from_name(name).unwrap_or_else(|| {
            eprintln!(
                "netdistill: unknown --kind '{name}' (expected one of: {})",
                netdistill::detect::Kind::NAMES.join(", ")
            );
            std::process::exit(2);
        })
    });

    match &cli.command {
        Some(Cmd::Mcp) => {
            netdistill::mcp::serve_stdio();
            return;
        }
        Some(Cmd::Serve { listen }) => {
            netdistill::http::serve(listen);
            return;
        }
        None => {}
    }
    let opts = Options {
        full: cli.full,
        kind,
    };
    let mut docs = Vec::new();
    let mut in_tok = 0usize;

    let mut feed = |text: &str, name: Option<String>| {
        in_tok += token::estimate(text);
        let mut ds = distill(text, &opts);
        if let Some(nm) = name {
            for d in &mut ds {
                d.headline = format!("[{nm}] {}", d.headline);
            }
        }
        docs.extend(ds);
    };

    if cli.files.is_empty() {
        let mut input = String::new();
        if std::io::stdin().read_to_string(&mut input).is_err() {
            eprintln!("netdistill: failed to read stdin");
            std::process::exit(1);
        }
        feed(&input, None);
    } else {
        let many = cli.files.len() > 1;
        for f in &cli.files {
            match std::fs::read_to_string(f) {
                Ok(s) => {
                    let name = many.then(|| {
                        f.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| f.display().to_string())
                    });
                    feed(&s, name);
                }
                Err(e) => {
                    eprintln!("netdistill: cannot read {}: {e}", f.display());
                    std::process::exit(1);
                }
            }
        }
    }

    if cli.json {
        println!("{}", render::render_json(&docs, in_tok));
    } else {
        print!("{}", render::render_text(docs, in_tok, cli.budget));
    }
}
