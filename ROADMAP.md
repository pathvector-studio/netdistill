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

## v0.6 — drill-down & hardening (shipped)
- [x] `--section` drill-down (re-emit one elided table in full; CLI/MCP/HTTP)
- [x] Numeric/uptime drift damping in diff (`--drift`, default 10%; timestamp-only alerts don't flap)
- [x] Optional auth token for `serve` mode (`--token` / NETDISTILL_TOKEN; /healthz stays open)

## v0.7 — output formats (shipped)
- [x] `--format text|json|toon|wire` (CLI flag; `format` arg on MCP/HTTP). `--json` kept as an alias.
- [x] `toon` — Token-Oriented Object Notation: `[N]{cols}` array headers + comma rows. Measured **48–69% fewer tokens than json** on tabular output, staying schema-explicit so LLM parsing holds up.
- [x] `wire` — dense LLM-only mode: markdown/labels/blank lines stripped, one-char line tags. 6–11% under the (already lean) text mode. Stays in-distribution ASCII on purpose — base64/compressed encodings tokenize worse *and* degrade reasoning (see notes).
- [x] The explicit-elision invariant is preserved in every format (verified in `tests/formats.rs`).

## v0.8 — fidelity eval (shipped)
- [x] `eval/` harness: renders each fixture in every format, asks a real LLM the same question, grades deterministically, records the backend's real `prompt_eval_count`. Backend-agnostic (default Ollama; one function to swap models).
- [x] 9 gold cases across bgp-summary / route-table / interfaces / config / ospf / lldp ([`eval/cases.json`](eval/cases.json)).
- [x] Result: on gemma3:12b **all formats scored 100% (9/9)** while `wire`/`toon`/`text` cut real prompt tokens to ~46-49% of raw — the digest carries the answer at half the tokens. Cross-checked on qwen2.5:14b. Reports in `eval/`.

## next
- [x] Real tokenizer-based counting inside the CLI (behind a feature flag) so the receipt line stops being an estimate — the eval already uses the model's true count. Build with `--features real-tokens` (o200k_base BPE, embedded/offline); the receipt then reads `o200k` instead of `est`/`estimates`.
- [ ] Larger/harder eval cases (multi-hop questions, diff snapshots) and a pass/fail CI gate on fidelity regressions
- [ ] Library crate API stabilization
- [ ] ntc-templates corpus regression harness
- [ ] XR `show bgp instance all`, NX-OS `show interface`

## Positioning notes
- TextFSM / ntc-templates / Genie parse to *structure*; their JSON is often **bigger** than the raw text. The gap netdistill fills is the layer after parsing: aggregation, anomaly ranking, and explicit elision under a token budget.
- Batfish models the network but needs a lab-grade setup. netdistill is `cat | distill` — zero infrastructure, works on whatever the engineer just pasted.
- **Why `wire` stops at terse ASCII rather than a "machine language".** Research on training/serving LLMs over compressed text (arithmetic coding, Equal-Info Windows) shows that strong, human-unreadable compression produces streams indistinguishable from noise to the model — reasoning collapses. base64 is understood but inconsistently, and it *expands* under BPE. The token-efficient sweet spot is *dense but in-distribution*: drop human affordances, keep text the model was actually trained on. That is exactly what `wire` does.
- **vs the prompt-compression literature (LLMLingua etc.).** Those methods silently drop low-information tokens — the precise failure mode netdistill exists to prevent (a model must never mistake "absent" for "fine"). netdistill's differentiator is *loud* lossiness: every omission is labeled, in every format.
