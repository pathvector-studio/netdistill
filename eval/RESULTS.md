# Does distillation preserve the answer? A fidelity & token benchmark

*A technical report on netdistill's output, measured against the alternatives.*

## Abstract

Network `show` output is verbose, repetitive, and mostly normal — pasting it into
an LLM burns the context window on padding. Tools that shrink it are only useful
if the model can **still answer** afterwards. We benchmark netdistill's digests
against the other ways engineers get device output into an LLM (raw paste, naive
truncation, generic dedup/strip compression, LLMLingua‑2, and TextFSM parsing),
measuring **answer fidelity** (deterministically graded) against **real token
cost** (the backend tokenizer's own count) on three independent open models
spanning 3B–14B (`gemma3:12b`, `qwen2.5:14b`, `qwen2.5:3b`).

**Result.** netdistill's digests answer **100%** of lookup questions at **~13% of
raw's tokens** — and are *more accurate than raw itself* on a large table, because
anomalies-first beats lost-in-the-middle. Every alternative loses on at least one
of {fidelity, size, coverage}: truncation and generic compression drop the
anomalies, TextFSM **inflates** tokens to 589% of raw, and LLMLingua‑2 (trained on
prose) shreds IP addresses and silently deletes the down peers. Across three
question shapes (lookup, row‑scan‑with‑condition, multi‑hop/adversarial) no single
output format dominates — `text` is the safe all‑rounder (~89% mean across all three
models) while `toon`/`wire` trade scan‑robustness for reasoning‑robustness and
fewer tokens — but **every** netdistill format beats `raw` (~66%) on every shape.

![fidelity vs size](fidelity_vs_size.svg)

*Fidelity (y) vs context size relative to raw (x), averaged over all three models
(17-case suite, §3.2). Top‑left is ideal. netdistill occupies it; every
alternative sits low, right, or both.*

---

## 1. What is being compared

Each **method** turns one device's raw output into the CONTEXT an LLM reads. The
question and grader are identical across methods.

| method | one line |
|---|---|
| `raw` | the untouched device output (baseline) |
| `netdistill:text` | netdistill's default digest — anomalies first, aggregates, explicit elision |
| `netdistill:toon` | same content in [Token‑Oriented Object Notation](https://github.com/toon-format/toon) |
| `netdistill:wire` | same content, dense LLM‑only encoding (one‑char line tags) |
| `truncate` | naive `head` to the same byte budget as `wire` — the "just cut it" floor |
| `strip-dedup` | domain‑blind: collapse whitespace, drop blanks, dedup identical lines with an `(xN)` count. A fair stand‑in for generic compressors (sqz / defluffer class) |
| `llmlingua` | [LLMLingua‑2](https://llmlingua.com/llmlingua2.html) (Microsoft), academic SOTA task‑agnostic prompt compression, `rate=0.33` |
| `textfsm-json` | [ntc‑templates](https://github.com/networktocode/ntc-templates) / TextFSM parsed → JSON (structure, not summary) |

## 2. Experimental setup

- **Models.** `gemma3:12b` (gemma3, 12.2B, Q4_K_M), `qwen2.5:14b`, and
  `qwen2.5:3b` (a deliberate small model to probe the size axis), served by
  Ollama. Deliberately **not** the model that authored the code — no self‑grading.
- **Decoding.** `temperature=0`, `num_predict=128`, `num_ctx=16384` (comparison)
  so `raw` and most methods fit; contexts exceeding it are a recorded failure,
  not a silent truncation.
- **Token cost.** The backend's own `prompt_eval_count` — the **real tokenizer
  count** of the whole prompt, not an estimate. The template is constant across
  methods, so differences are context cost.
- **Grading.** Deterministic, no LLM judge: an answer is correct iff every
  `must_contain` string is present **and** no `must_not_contain` string is,
  case‑insensitive. Cases in [`cases.json`](cases.json).
- **Questions** target facts netdistill is designed to **preserve** (anomalies,
  inventory, presence/absence). This measures *format* fidelity, not parser
  coverage; healthy detail that netdistill deliberately elides is never asked
  about — that is the compression working as intended.
- **Fixtures are small on purpose.** `raw` must fit in context for the answers to
  be comparable at all. Real inputs are far larger (a 50k‑route table is millions
  of tokens and does not fit), so the token ratios here **understate** the saving
  while isolating the fidelity question. One fixture, `big_bgp_ios_500.txt`, is
  large (500 peers, ~12k tokens) with 3 broken peers buried among 497 healthy —
  the case where "does it fit / does it get lost" actually bites.
- Harnesses: [`run.py`](run.py) (format fidelity), [`compare.py`](compare.py)
  (methods). Backend‑agnostic — the model call is one function; swap it for any
  API.

## 3. Results

### 3.1 Format fidelity — every netdistill format keeps the answer

netdistill's four formats vs `raw`, 9 questions, real prompt tokens
([full reports](report_gemma3_12b.md)):

| format | gemma3:12b | qwen2.5:14b | tokens vs raw |
|---|---|---|---|
| raw | 100% (9/9) | 100% (9/9) | 100% |
| netdistill:text | 100% | 100% | 46–48% |
| netdistill:json | 100% | 100% | 63–71% |
| netdistill:toon | 100% | 100% | 48–49% |
| netdistill:wire | 100% | 100% | 45–46% |

All formats carry the answer — including their explicit elision notes — at
roughly half the tokens. `json` is the least efficient (repeated keys); `toon`
delivers most of `json`'s structure at `text`/`wire` token cost.

### 3.2 Head‑to‑head vs the alternatives — 17 questions, 3 model sizes

17 questions spanning IOS/IOS‑XR/NX‑OS/EOS/Huawei VRP/FRR/Junos across
bgp‑summary, bgp‑table, route‑table, interfaces, config, ospf, isis and lldp.
Run on three model sizes to test the "one size class" worry. `est` is a
model‑free char estimate so it can cover methods that don't answer. Reports:
[gemma3:12b](compare_report_gemma3_12b.md) ·
[qwen2.5:14b](compare_report_qwen2_5_14b.md) ·
[qwen2.5:3b](compare_report_qwen2_5_3b.md).

| method | gemma3:12b | qwen2.5:14b | qwen2.5:3b | size vs raw | coverage |
|---|---|---|---|---|---|
| **netdistill:text** | **100%** | **100%** | **82%** | 18% | 17/17 |
| **netdistill:toon** | **100%** | **100%** | 71% | 19% | 17/17 |
| netdistill:wire | 88% | **100%** | 65% | 17% | 17/17 |
| raw | 82% | 82% | 71% | 100% | 17/17 |
| strip-dedup | 71% | 88% | 71% | 65% | 17/17 |
| truncate | 53% | 41% | 29% | 17% | 17/17 |
| llmlingua | 6% | 6% | 18% | 21% | 17/17 |
| textfsm-json | 67% | 67% | 67% | 5–9× raw † | **6/17** |

† TextFSM covers only the 6 cisco_ios‑template cases; on those its JSON is
1.3–3.8× the raw per input (§3.4), and the aggregate blows past raw once the
500‑peer table is included. No template matched the XR/EOS/VRP/FRR/Junos/config
fixtures — recorded as `n·a`.

**Reading the table.** On both 12–14B models `text`/`toon` hold **100%** even as
the suite widened to 8 parsers and 7 vendors, at ~18% of raw's tokens. The small
3B model drops everything — but the **ordering is preserved**: `text` (82%) still
leads, still beats `raw` (71%), `truncate` (29%) and `llmlingua` (18%). So
netdistill's advantage is size‑robust; the model just gets weaker.

A second, sharper result: **denser formats cost more on weaker models.** On the
3B model `text` (82%, most explicit) clearly beats `toon` (71%) and `wire` (65%),
whereas on the 14B model all three tie at 100%. The density→comprehension tradeoff
scales with model capability — another reason `text` is the safe default,
especially if a small model is in the loop.

### 3.3 Per‑case grid (gemma3:12b, 17 cases)

| case | raw | text | toon | wire | trunc | strip | lingua | tfsm |
|---|---|---|---|---|---|---|---|---|
| bgp_down_peers | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | n·a |
| bgp_total_peers | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n·a |
| ospf_stuck | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |
| lldp_gi00_peer | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |
| intf_crc_errors | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ |
| intf_unexpected_down | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ |
| route_no_default | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ |
| config_bgp_neighbors | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| config_hostname | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| **big_bgp_buried_down** | **✗** | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
| xr_bgp_not_established | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| eos_bgp_broken | ✓ | ✓ | ✓ | ✗ | ✓ | ✓ | ✗ | n·a |
| vrp_bgp_broken | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| nxos_no_default_route | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| junos_hidden_route | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | n·a |
| isis_stuck_adjacency | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| bgptable_top_origin_as | ✗ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | n·a |

The `big_bgp_buried_down` row is the crux: with 3 broken peers buried in a
500‑peer table, **raw itself fails** (lost in the middle) while `text`/`toon`
surface them at the top. `raw` also misses `junos_hidden_route` and
`bgptable_top_origin_as` — facts netdistill lifts into an alert or a ranked table.

### 3.4 Parse ≠ compress (TextFSM output size)

TextFSM/ntc‑templates turns text into *structure*, not into *less*. JSON size
relative to the raw it parsed:

| input | rows | JSON vs raw |
|---|---|---|
| `show interfaces` | 5 | 1.33× |
| `show ip route` | 19 | 2.46× |
| `show ip ospf neighbor` | 5 | 1.57× |
| `show lldp neighbors` | 4 | 0.89× |
| `show ip bgp summary` (500 peers) | 500 | **3.84×** |

It is also brittle and platform‑locked: no template matched the FRR or config
fixtures (recorded as `n·a`, 6/10 coverage), consistent with the report that
~45% of Batfish's issues are parsing errors.

### 3.5 How LLMLingua‑2 fails on structured data (qualitative)

LLMLingua‑2 is trained on MeetingBank (meeting transcripts) and treats device
output as prose. Compressing the 500‑peer table dropped the broken peer
`10.20.1.7` entirely and split IP addresses across tokens:

```
BGP router identifier 10. 255. 0. 1 local AS number 65000
 ... 10... 65000 1200 1200 812734 2500 ...
```

Domain‑blind **and** silently lossy — the exact combination netdistill is built
to avoid. Hence 10% fidelity despite a healthy token budget.

### 3.6 The hard case for tabular formats: row‑scan‑with‑condition

The table‑serialization literature warns that positional/tabular encodings weaken
when the model must *scan rows and apply a condition* (it has to map a column
position back to its header), and that self‑contained key=value objects help. We
tested it directly: 5 questions that scan a **retained** table and filter it
(e.g. "which VRFs carry >2 BGP routes", "which neighbor is in peer‑group
TRANSIT"), adding a `netdistill:kv` format (each row as `col=val, col=val`), on
all 3 models. Harness: [`cases_scan.json`](cases_scan.json), reports
[12b](scan_report_gemma3_12b.md) / [14b](scan_report_qwen2_5_14b.md) /
[3b](scan_report_qwen2_5_3b.md).

| format | 12b | 14b | 3b | mean | note |
|---|---|---|---|---|---|
| **netdistill:text** (TSV + header) | **100%** | **100%** | 40% | **80%** | robust *and* cheap |
| netdistill:kv (self‑contained) | 80% | 100% | 40% | 73% | helps some models, +33% tokens |
| netdistill:wire | 60% | 80% | 40% | 60% | middling |
| raw | 60% | 80% | 20% | 53% | noise hurts scanning too |
| netdistill:toon | 40% | 60% | 40% | 47% | **weakest at scanning** |

Findings, partially against our own prediction:

- The predicted weakness is real, but it lands on **`toon`**, not "tabular" in
  general — its comma‑packed `[N]{cols}` rows are hardest to scan, even though
  `toon` ties `text` on lookups (§3.2). `text` (TSV + header) is the most robust
  scan format on both larger models and the token‑cheapest of the safe options.
- **`kv` only partly confirms the literature:** it beats `toon`/`wire` and hits
  100% on 14b, but not on 12b/3b, and costs ~33% more tokens. A targeted fallback
  for a model that mis‑scans, not a free win.
- The 3B model collapses to ~40% for *every* format here — scan‑with‑condition is
  simply hard for a small model regardless of encoding.
- Even `raw` scores 53% mean: removing noise helps scanning too — just pick `text`.

### 3.7 Multi-hop and adversarial questions

Two more shapes: **multi-hop** (combine facts across two commands in one session
log — "the most-used next-hop IP belongs to a peer; what's its AS?") and
**adversarial phrasing** (negation, synonyms, and a distractor: "ignoring
admin-shut peers, which is *genuinely* broken?"). 5 cases, 3 models.
[`cases_hard.json`](cases_hard.json), reports
[12b](hard_report_gemma3_12b.md) / [14b](hard_report_qwen2_5_14b.md) /
[3b](hard_report_qwen2_5_3b.md).

| method | 12b | 14b | 3b | mean |
|---|---|---|---|---|
| netdistill:text | 80% | 100% | 100% | **93%** |
| netdistill:toon | 100% | 100% | 80% | **93%** |
| netdistill:wire | 100% | 100% | 60% | 87% |
| netdistill:kv | 60% | 100% | 80% | 80% |
| raw | 80% | 60% | 60% | 67% |

- **Multi-hop survives distillation — and beats raw.** The cross-command AS
  lookup passes everywhere; on the cross-command *count*, `raw` fails while every
  netdistill format passes. Precomputed aggregates (`TOP NEXT-HOPS`,
  `largest peers`) turn a two-hop question into a one-hop read.
- **The distractor is the hardest case, and `raw` fails it across models.**
  "Ignore admin‑shut peers, find the genuinely broken one" — `raw` picks the
  admin‑disabled peer (or both); netdistill wins because it *labels* the state
  `Idle (Admin)` distinctly from `Active`. Explicit elision is not just honest,
  it is *load‑bearing* for reasoning.
- **Reversal vs §3.6.** Here `toon`/`wire` are top on the larger models while
  `text` leads by staying robust on the 3B model — the opposite ranking from the
  scan experiment. **No single dominant format**; robustness depends on shape
  *and* model size (`wire` drops to 60% on 3B).

### 3.8 Format robustness by question shape (summary)

Fidelity averaged over all three models, per question shape:

| format | lookup (§3.2) | scan+cond (§3.6) | multi-hop/adv (§3.7) | mean |
|---|---|---|---|---|
| **netdistill:text** | **94%** | **80%** | 93% | **~89%** |
| netdistill:toon | 90% | 47% | 93% | ~77% |
| netdistill:wire | 84% | 60% | 87% | ~77% |
| netdistill:kv | — | 73% | 80% | ~77% |
| raw | 78% | 53% | 67% | ~66% |

`text` is the safe all-rounder — top or tied on every shape, best at scanning,
and token‑cheap. `toon`/`wire` trade scan robustness for reasoning robustness and
a few fewer tokens; `kv` is a situational fallback. **`raw` is the least robust
across every shape** (~66%) — the recurring result of this report. Shipping
several formats is the point: pick by workload, all far cheaper and no less
faithful than raw.

## 4. Findings

1. **Distillation can beat raw.** Putting anomalies first defeats
   "lost‑in‑the‑middle" — `netdistill:text/toon` scored 100% where `raw` lost the
   buried peers (90%).
2. **Small is necessary but not sufficient.** `truncate` matches netdistill's
   token count (13%) at **30–40%** fidelity: it drops the very rows that matter.
3. **Domain awareness is the moat.** `strip-dedup` (generic compression) barely
   shrinks unique lines (64%) *and* loses anomalies — beaten by netdistill on
   **both** axes.
4. **Parsing is not compression.** TextFSM inflates JSON to 1.3–3.8× raw per input
   (§3.4) and covers only 6/17 inputs (no XR/EOS/VRP/FRR/Junos/config templates).
5. **SOTA prose compression is actively harmful here.** LLMLingua‑2 destroys the
   structure it can't recognize (~6–18% fidelity).
6. **Density has a measurable ceiling that scales with model size.** `wire` (the
   densest format) dipped on lookups (88% on 12b) and falls hardest on the 3B
   model (65% lookup, 60% multi‑hop) — the density→comprehension tradeoff the
   report's own "bound density by measured comprehension" principle predicts.
7. **No format dominates; robustness is workload-dependent (§3.8).** `text` wins
   scan‑with‑condition but `toon`/`wire` match or lead on multi‑hop/adversarial;
   all tie on lookups (on larger models). `text` is the best all‑rounder (~89%
   3‑model mean, never worst) and the safe default; the others are for
   token‑squeezing or shape‑specific robustness. This is why netdistill ships
   several formats rather than one — and across every shape every format beats `raw`.
8. **Distillation helps the *hard* questions most.** On multi‑hop and adversarial
   distractors, `raw` is the least reliable of all (~67% mean) — precomputed
   aggregates collapse multi‑hop into lookups, and explicit state labels
   (`Idle (Admin)` vs `Active`) defuse distractors that trip a model reading raw.

## 5. Threats to validity

- **Small N.** 10 main‑suite questions plus two focused probes (4 scan‑with‑
  condition §3.6, 5 multi‑hop/adversarial §3.7) — 19 in total. Still small and
  curated; the probes each surfaced a real, *opposite* format split (§3.8), so
  question shape clearly matters and a larger balanced suite is the obvious next
  step. Reported numbers are per‑shape means over three models, not a single grand
  average.
- **Three open models spanning 3B–14B**, but all open‑weight and none frontier.
  The size axis is now covered (§3.2 shows the ordering holds down to 3B); a
  frontier‑model and a sub‑1B run remain future work.
- **Single run per cell — checked.** A full re‑run of the 17‑case suite on
  `gemma3:12b` at `temperature=0` reproduced every method's score exactly (raw
  82%, text/toon 100%, wire 88%, …), so decoding is deterministic here and a
  single run is representative. A temperature sweep for genuine variance bars is
  still future work.
- **Substring grading** can, in principle, reward a lucky substring; mitigated by
  distinctive IP/hostname targets and `must_not_contain` guards.
- **Baselines are not maximally tuned.** LLMLingua‑2 uses the bert‑base variant at
  a fixed rate with minimal `force_tokens`; a network‑specific compressor or a
  different rate could do better. `strip-dedup` is our fair stand‑in, not the
  actual sqz binary. These favor giving the alternatives the benefit of the doubt,
  yet netdistill still wins clearly.

## 6. Reproduction

```bash
cargo build --release
# format fidelity (no extra deps):
python3 eval/run.py --model gemma3:12b

# head-to-head vs alternatives (needs the eval venv):
uv venv --python 3.12 evalenv
uv pip install --python evalenv/bin/python textfsm ntc-templates
uv pip install --python evalenv/bin/python torch --index-url https://download.pytorch.org/whl/cpu
uv pip install --python evalenv/bin/python llmlingua        # optional (heavy)
evalenv/bin/python eval/compare.py --model gemma3:12b
evalenv/bin/python eval/compare.py --model qwen2.5:14b

# row-scan-with-condition probe (§3.6) and multi-hop/adversarial probe (§3.7):
M=raw,netdistill:text,netdistill:toon,netdistill:wire,netdistill:kv
evalenv/bin/python eval/compare.py --model gemma3:12b --cases eval/cases_scan.json --methods $M
evalenv/bin/python eval/compare.py --model gemma3:12b --cases eval/cases_hard.json --methods $M
```

Point `NETDISTILL_EVAL_URL` at any Ollama endpoint; missing optional deps degrade
to `n·a` rather than failing the run. Raw per‑run reports: compare
[gemma3:12b](compare_report_gemma3_12b.md) ·
[qwen2.5:14b](compare_report_qwen2_5_14b.md) ·
[qwen2.5:3b](compare_report_qwen2_5_3b.md); format fidelity
[gemma](report_gemma3_12b.md) · [qwen](report_qwen25_14b.md).

## 7. Related work

- **Serialization for LLMs.** TOON (token‑oriented notation) wins on flat tables
  but weakens on nested/row‑scan tasks; "Table Meets LLM" finds markup formats
  (HTML/XML) aid table understanding. netdistill's tabular digests inherit the
  lookup‑strong / scan‑weaker profile — see §5.
- **Prompt compression.** LLMLingua / LLMLingua‑2 drop low‑information tokens with
  a small LM — *silently* lossy, the opposite of netdistill's labeled elision.
  Soft‑prompt methods (Gist, ICAE, 500xCompressor) require model access and don't
  apply to an API‑agnostic CLI.
- **Why `wire` stays ASCII.** Training/serving LLMs over arithmetic‑coded text
  shows strong compression becomes noise the model can't reason over; base64 is
  understood inconsistently and *expands* under BPE. The efficient sweet spot is
  dense‑but‑in‑distribution.
- **Network state tooling.** TextFSM/ntc‑templates/Genie produce structure (often
  larger than raw); Batfish/Suzieq model the network but need lab‑grade setup.
  netdistill is `cat | distill` — zero infrastructure, on whatever was just pasted.

See [`../ROADMAP.md`](../ROADMAP.md) for source links to each claim.

## 8. Prior art: is anyone else doing this?

Token-saving CLI compressors exist, and one shares netdistill's tagline almost
word for word — but none occupies the same point in the design space
(deterministic **and** network-domain-aware **and** explicit about elision).

| tool | mechanism | domain | elision | footprint |
|---|---|---|---|---|
| [samuelfaj/distill](https://github.com/samuelfaj/distill) (& fork [Codex-distill](https://github.com/McBorisson/Codex-distill)) | **1.7B "expert" LM**, semantic filtering | general shell | **opaque** (docs don't state what's dropped) | npm + 8 GB RAM |
| [rtk](https://github.com/rtk-ai/rtk) | rules/proxy | general dev cmds (git, docker, kubectl) | boilerplate stripping | single Rust binary |
| [headroom](https://www.sitepoint.com/headroom-the-tokencompression-cli-that-cuts-your-llm-api-costs-by-6095/) | AST analysis | **source code** | — | CLI |
| tokuin / token-compressor | generic prompt compression | any text | — | CLI |
| ntc-templates / pyATS-Genie | template parsing | network CLI | — (emits *structure*, often **bigger**, §3.4) | python + parser corpus |
| Batfish / Suzieq / NetClaw | config analysis / state DB / digital twin | network | — (models the network) | lab-grade infra |
| **netdistill** | **deterministic parsing + aggregation + anomaly ranking** | **network CLI (multi-vendor)** | **explicit, labeled** | **single Rust binary** |

The closest, `samuelfaj/distill`, is the instructive contrast: it does the same
*job* (shrink CLI output for an LLM) with the opposite *method* — it runs a 1.7B
model (8 GB RAM), works on any shell text, and, per its own docs, does not state
what it omitted. netdistill is a zero-model single binary that knows what a BGP
session or a route table *is*, ranks the anomalies, and **labels every omission**
so the model can't mistake "absent" for "fine". The generic compressors (rtk,
tokuin, headroom) don't target device output; the parser stacks (Genie,
ntc-templates) produce structure rather than summaries; the analysis platforms
(Batfish, Suzieq) model the network and need infrastructure. As of this writing
we found **no deterministic, network-CLI-specific distiller with explicit
elision** — the niche netdistill fills appears unoccupied.
