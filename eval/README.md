# netdistill fidelity eval

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
