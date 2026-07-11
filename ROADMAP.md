# netdistill roadmap

## v0.1 — prototype (shipped)
- [x] Auto-detect: bgp-summary / route-table / interfaces / config / generic
- [x] IOS / FRR / Junos-style tolerant parsers
- [x] Session-log splitting on prompt lines
- [x] `--full`, `--budget`, `--json`
- [x] Token receipt on every output

## v0.2 — integrations (shipped)
- [x] MCP server mode (`netdistill mcp`, stdio) — agents call `distill` as a tool
- [x] HTTP API mode (`netdistill serve`, POST /v1/distill) — callable from any language

## v0.3 — coverage (shipped)
- [x] `show ip bgp` full-table digest (origin-AS histogram, AS-path stats, s/d/r alerts)
- [x] `show ip ospf neighbor`, `show isis neighbors`, `show lldp neighbors`
- [x] Release binaries (release.yml: linux musl x86_64/aarch64, macOS x86_64/arm64) — no Rust toolchain needed to run
## v0.4 — dialects (shipped)
- [x] IOS-XR bgp summary (Spk column), NX-OS bgp summary sections
- [x] EOS / Huawei VRP word-state bgp summary dialects
- [x] NX-OS `show ip route` (ubest/mbest + *via format)
- [x] Junos `show configuration` — set-style and hierarchical, one extractor
- [x] `--kind` override (CLI flag, MCP tool arg, HTTP param) to skip auto-detection
- [ ] XR `show bgp instance all` instance headers
- [ ] NX-OS `show interface`, EOS-specific route table quirks

## v0.5 — diff (shipped)
- [x] `netdistill diff before.txt after.txt` — NEW/RESOLVED alerts, Δ tables, cell-level old→new
- [x] `diff` exposed as MCP tool and `POST /v1/diff`

## next
- [ ] `--section` drill-down (re-emit one elided table in full)
- [ ] Real tokenizer-based counting behind a feature flag
- [ ] Numeric-drift damping in diff (ignore counter columns below a change threshold)

## v0.6 — hardening
- [ ] Optional auth token for `serve` mode
- [ ] Library crate API stabilization
- [ ] ntc-templates corpus regression harness

## Positioning notes
- TextFSM / ntc-templates / Genie parse to *structure*; their JSON is often **bigger** than the raw text. The gap netdistill fills is the layer after parsing: aggregation, anomaly ranking, and explicit elision under a token budget.
- Batfish models the network but needs a lab-grade setup. netdistill is `cat | distill` — zero infrastructure, works on whatever the engineer just pasted.
