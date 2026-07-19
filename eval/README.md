# netdistill fidelity eval

> **Looking for the write-up?** [`RESULTS.md`](RESULTS.md) is the paper-level
> report: methodology, both models, per-case grids, the fidelity‑vs‑size figure,
> and threats to validity. This file is the how-to for running the harnesses.


**Claim under test:** an LLM reaches the *same answer* from a distilled digest
as from the raw CLI output — at a fraction of the tokens.

The token-savings numbers elsewhere in this repo are estimates about *size*.
This harness measures the thing that actually matters: **does the answer
survive the compression?** If a format is smaller but the model can no longer
answer, that format is worthless. This is netdistill's own "Equal-Info Window":
push density only as far as measured comprehension allows.

## How it works

For every `(case, format)` pair the harness:

1. Renders the fixture in that format (`raw`, `text`, `json`, `toon`, `wire`)
   via the netdistill binary — `raw` is the untouched device output.
2. Feeds the model **only** that context + the question (identical prompt
   template across formats).
3. Grades the answer deterministically — no LLM judge: all `must_contain`
   substrings present AND no `must_not_contain` substrings present
   (case-insensitive). Cases live in [`cases.json`](cases.json).
4. Records the backend's real `prompt_eval_count` — the actual tokenizer count
   the model saw, not an estimate.

Questions target facts netdistill is designed to **preserve** (anomalies,
inventory, presence/absence), so the eval measures *format* fidelity, not
parser coverage. Elided-away healthy detail is deliberately never asked about —
that is the compression working as intended.

## Running

Backend-agnostic. The default backend is a local [Ollama](https://ollama.com)
server; the model call is a single function (`ollama_generate`) — swap it for an
Anthropic/OpenAI call to eval a different model.

```bash
cargo build --release                       # harness prefers the release binary
python3 eval/run.py                          # default model, all formats
python3 eval/run.py --model qwen2.5:14b
python3 eval/run.py --formats raw,text,toon,wire
NETDISTILL_EVAL_URL=http://mac-mini:11435 python3 eval/run.py --verbose
```

No third-party Python packages required (stdlib `urllib`).

## Results

Small fixtures are used **on purpose**: raw has to fit in context for the
answers to be comparable at all. The compression win grows with input size (a
50k-route table is millions of raw tokens and simply does not fit) — so these
token ratios *understate* the real-world saving while proving the fidelity.

Two independent models, 9 cases:

- [`report_gemma3_12b.md`](report_gemma3_12b.md) — gemma3:12b
- [`report_qwen25_14b.md`](report_qwen25_14b.md) — qwen2.5:14b

On gemma3:12b every format scored **100% (9/9)** while cutting real prompt
tokens roughly in half (`wire` to 46% of raw, `toon`/`text` ~48%, `json` 71%).
The full digest — including its explicit elision notes — carries the answer at
half the tokens.

## netdistill vs. the alternatives (`compare.py`)

`run.py` compares netdistill's own formats. [`compare.py`](compare.py) is the
head-to-head against the *other* ways engineers get device output into an LLM:

| method | what it is |
|---|---|
| `truncate` | naive head-to-budget — the "just cut it" floor |
| `strip-dedup` | domain-blind whitespace-collapse + identical-line dedup (stand-in for generic compressors like sqz / defluffer) |
| `llmlingua` | LLMLingua-2 (Microsoft) — academic SOTA task-agnostic prompt compression |
| `textfsm-json` | ntc-templates / TextFSM parsed → JSON (structure, not summary) |

Same fidelity question, same grader, real prompt-token counts. Result on
gemma3:12b (10 cases, see [`compare_report.md`](compare_report.md)):

| method | fidelity | size vs raw |
|---|---|---|
| netdistill:text / toon | **100%** | **13%** |
| raw | 90% | 100% |
| truncate | 40% | 13% |
| strip-dedup | 70% | 64% |
| llmlingua | (domain-blind, silently drops anomalies) | — |
| textfsm-json | 67% (6/10 covered) | **589%** (bigger than raw) |

Two findings worth the harness: netdistill can be **more accurate than raw**
(anomalies-first beats lost-in-the-middle on a 500-peer table), and every
alternative loses on at least one axis — truncation and generic compression
drop the anomalies, TextFSM *inflates* tokens, and LLMLingua-2 (trained on
meeting transcripts) shreds IP addresses and silently deletes the down peers.

`compare.py` needs the eval venv **plus** `textfsm ntc-templates` and,
optionally, `llmlingua` (heavy — pulls torch/transformers):

```bash
uv venv --python 3.12 evalenv
uv pip install --python evalenv/bin/python textfsm ntc-templates
uv pip install --python evalenv/bin/python torch --index-url https://download.pytorch.org/whl/cpu
uv pip install --python evalenv/bin/python llmlingua      # optional
evalenv/bin/python eval/compare.py --model gemma3:12b
```

Missing optional deps degrade gracefully — those methods report `n·a` instead of
failing the run.
