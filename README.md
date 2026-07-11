# netdistill

English | [日本語](README.ja.md)

**Stop pasting 50,000 lines of `show ip route` into your LLM.**

Network CLI output is built for human eyes: fixed-width padding, repeated headers, and thousands of lines where 99% say "everything is normal". Paste that into an LLM and you burn the context window on padding — a full-table `show route vrf all` is *millions* of tokens and simply does not fit.

`netdistill` is a single-binary CLI that distills `show` output and configs into what an LLM actually needs to reason about your network:

- **Anomalies first** — down sessions, error counters, flaps, hidden routes surface at the top.
- **Aggregates instead of dumps** — 50,000 BGP routes become a per-VRF × per-protocol matrix plus a next-hop ranking.
- **Explicit elision** — everything omitted is *labeled* ("495 healthy Established peers omitted"), so the model never mistakes "absent" for "fine".
- **Token receipt** — every output starts with `input ~X tok → output ~Y tok`, so you know what you saved.

```
$ netdistill big_route_vrf_all.txt        # 50,013 lines, ~729k tokens
# netdistill — input ~728781 tok → output ~164 tok (100% saved; estimates)

## [route-table] 2 routing table(s), 50003 routes
ALERTS: none
SUMMARY:
- routes by protocol: bgp 50000, connected 1, local 1, static 1
- no default route in: MGMT

TABLES/VRFs:
table    routes  bgp    connected  local  static  default_via
default  50001   50000  0          0      1       10.0.0.1
MGMT     2       0      1          1      0       -

TOP NEXT-HOPS:
next-hop   routes
10.0.12.2  12500
...
```

Measured on synthetic-but-realistic inputs (debug numbers from `examples/gen_big_inputs.sh`):

| input | raw (est. tokens) | distilled | reduction | time |
|---|---|---|---|---|
| `show ip route vrf all`, 50k routes | ~729,000 | ~164 | 99.98% | 20 ms |
| `show bgp summary`, 500 peers (3 broken) | ~12,600 | ~222 | 98% | <10 ms |

The three broken peers survive; the 497 healthy ones collapse into one line.

> ⚠️ **Prototype.** Parsers are heuristic and format-tolerant, not exhaustive. Verified against IOS/IOS-XE, FRR, and Junos-style output shapes — weird platforms will fall back to generic compaction rather than fail.

## What it understands today

| input | detected as | distillation |
|---|---|---|
| `show bgp summary` (IOS / FRR / EOS-style, Junos) | `bgp-summary` | peer-state census, prefix totals, top peers; only broken/notable peers listed |
| `show ip route [vrf all]` (IOS/XR), `show route` (Junos) | `route-table` | per-VRF × protocol matrix, default-route presence, top next-hops, hidden-route alerts |
| `show interfaces` (IOS-style) | `interfaces` | up/down census; only interfaces with errors, drops, or unexpected down state listed |
| running/startup config (IOS-style) | `config` | inventory digest: interfaces table, BGP neighbors, VRFs, policy object counts. Secrets are never echoed |
| anything else | `generic` | lossless-ish compaction: padding → tabs, duplicate lines → `[xN]`, blank runs collapsed |

Paste a **whole terminal session** and it splits on prompt lines (`router1# show ...`) and distills each command separately.

## Install

Requires Rust 1.75+.

```bash
cargo install --path .
# or: cargo build --release   → target/release/netdistill
```

## Usage

```bash
netdistill session.log                 # one or more files
ssh router1 'show ip route vrf all' | netdistill     # or stdin

netdistill --full show_bgp.txt         # keep every row (still restructured)
netdistill --budget 800 session.log    # trim tables to fit ~800 tokens
netdistill --json show_bgp.txt         # structured JSON for pipelines
```

Typical agent/pipeline use: run the command, pipe through `netdistill`, hand the digest to the model. If the model needs detail that was elided, re-run with `--full` for just that command — the elision notes tell it exactly what to ask for.

## Design principles

1. **The LLM's job is diagnosis; give it evidence, not logs.** Anomalies and aggregates carry nearly all the diagnostic signal of raw output at a fraction of the tokens.
2. **Elision must be explicit.** Silently truncated output makes a model hallucinate health. Every omission is stated inline.
3. **Structure beats prose.** Pre-parsed compact tables are both smaller and *more accurate* than making the model eyeball fixed-width columns.
4. **Degrade gracefully.** Unknown formats get generic compaction, never an error. A wrong parse should still be a smaller, honest input.

Every heuristic and threshold is documented in [docs/how-it-works.md](docs/how-it-works.md).

## License

MIT
