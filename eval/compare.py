#!/usr/bin/env python3
"""netdistill vs. the alternatives — a head-to-head fidelity+token comparison.

run.py compares netdistill's own output formats. This compares netdistill
against the *other* things an engineer actually reaches for to get device
output into an LLM:

  raw            the untouched output (baseline)
  netdistill:*   text / toon / wire digests
  truncate       naive head-to-budget (the "just cut it" floor)
  strip-dedup    domain-agnostic whitespace-collapse + identical-line dedup
                 (a fair stand-in for generic compressors like sqz/defluffer)
  textfsm-json   ntc-templates / TextFSM parsed -> JSON (structure, not summary)

Same fidelity question and deterministic grader as run.py, plus each method's
real prompt-token count. The point: "smaller" is worthless if the answer dies,
and "parsed" is worthless if the JSON is bigger than what you started with.

Needs the eval venv (textfsm + ntc-templates); the model backend is Ollama.
    uv venv --python 3.12 evalenv && uv pip install textfsm ntc-templates
    evalenv/bin/python eval/compare.py --model gemma3:12b
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request
from collections import OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "fixtures"
BIN = next((p for p in [ROOT / "target/release/netdistill", ROOT / "target/debug/netdistill"] if p.exists()), None)
OLLAMA_URL = os.environ.get("NETDISTILL_EVAL_URL", "http://localhost:11435")
NUM_CTX = int(os.environ.get("NETDISTILL_EVAL_NUM_CTX", "16384"))

# fixtures a TextFSM template can parse: (platform, command). Others are
# unsupported — itself a finding (TextFSM is platform-locked, no config coverage).
TFSM = {
    "ios_show_interfaces.txt": ("cisco_ios", "show interfaces"),
    "ios_route_vrf_all.txt": ("cisco_ios", "show ip route"),
    "ios_ospf_neighbors.txt": ("cisco_ios", "show ip ospf neighbor"),
    "ios_lldp_neighbors.txt": ("cisco_ios", "show lldp neighbors"),
    "big_bgp_ios_500.txt": ("cisco_ios", "show ip bgp summary"),
}

PROMPT_TMPL = (
    "You are a network engineer's assistant. Answer the QUESTION using ONLY the "
    "CONTEXT below, which is output from a network device (it may be summarized, "
    "parsed, or compressed). Be concise: give just the specific answer — names, IP "
    "addresses, or numbers. If the answer is not in the context, reply 'unknown'.\n\n"
    "CONTEXT:\n{ctx}\n\nQUESTION: {q}\nANSWER:"
)


def est_tok(s):
    """netdistill's own char-based estimate, for a model-free size column."""
    return (len(s) * 2 + 6) // 7


def netdistill(raw, fmt, kind):
    return subprocess.run(
        [str(BIN), "--format", fmt, "--kind", kind],
        input=raw, capture_output=True, text=True, check=True,
    ).stdout


def netdistill_kv(raw, kind):
    """Same distilled content as the other formats, but each table row is a
    *self-contained* key=value object instead of a positional row. The table
    research says this helps row-scan-with-condition questions (the model
    doesn't have to map a column position back to a header). Built from the JSON
    output — no Rust change needed."""
    doc_json = json.loads(netdistill(raw, "json", kind))
    out = []
    for d in doc_json["docs"]:
        out.append(f"[{d['kind']}] {d['headline']}")
        out += [f"ALERT: {a}" for a in d.get("alerts", [])]
        out += [f"- {s}" for s in d.get("summary", [])]
        for t in d.get("tables", []):
            hdr = t["header"]
            for row in t["rows"]:
                kvs = ", ".join(f"{h}={v}" for h, v in zip(hdr, row))
                out.append(f"{t['title']}: {kvs}")
            if t.get("omitted"):
                out.append(f"({t['omitted']})")
        out += [f"note: {n}" for n in d.get("notes", [])]
    return "\n".join(out)


def strip_dedup(raw):
    """Generic, domain-blind: collapse whitespace, drop blanks, dedup identical
    lines with an (xN) count. Represents the sqz/defluffer class."""
    counts = OrderedDict()
    for line in raw.splitlines():
        c = re.sub(r"[ \t]+", " ", line).strip()
        if c:
            counts[c] = counts.get(c, 0) + 1
    return "\n".join(l if n == 1 else f"{l}  (x{n})" for l, n in counts.items())


def truncate_to(raw, target_chars):
    if len(raw) <= target_chars:
        return raw
    return raw[:target_chars] + "\n[... truncated to fit budget ...]"


_LINGUA = None
_LINGUA_FAILED = False


def llmlingua_compress(raw, rate=0.33):
    """LLMLingua-2 (Microsoft): task-agnostic, model-based token dropping — the
    academic SOTA for prompt compression. Domain-blind and *silently* lossy,
    which is the exact contrast with netdistill's explicit elision. Returns None
    if the package/model isn't available (kept optional)."""
    global _LINGUA, _LINGUA_FAILED
    if _LINGUA_FAILED:
        return None
    if _LINGUA is None:
        try:
            from llmlingua import PromptCompressor
            _LINGUA = PromptCompressor(
                model_name="microsoft/llmlingua-2-bert-base-multilingual-cased-meetingbank",
                use_llmlingua2=True,
                device_map="cpu",
            )
        except Exception as e:
            print(f"  (llmlingua unavailable: {e})", file=sys.stderr)
            _LINGUA_FAILED = True
            return None
    try:
        return _LINGUA.compress_prompt(raw, rate=rate, force_tokens=["\n", ":", "/", "."])[
            "compressed_prompt"
        ]
    except Exception as e:
        return f"[llmlingua error: {e}]"


def textfsm_json(raw, fixture):
    spec = TFSM.get(fixture)
    if not spec:
        return None  # unsupported: no matching template / not tabular
    try:
        from ntc_templates.parse import parse_output
        rows = parse_output(platform=spec[0], command=spec[1], data=raw)
        return json.dumps(rows, indent=2)
    except Exception as e:
        return f"[textfsm parse error: {e}]"


def build_methods(raw, case, wanted):
    """Return ordered {method: context_or_None} for the `wanted` methods only.
    None = unsupported for this case. Thunks so we never pay for (e.g.) the
    LLMLingua model load unless that method was requested."""
    kind, fixture = case["kind"], case["fixture"]
    thunks = OrderedDict([
        ("raw", lambda: raw),
        ("netdistill:text", lambda: netdistill(raw, "text", kind)),
        ("netdistill:toon", lambda: netdistill(raw, "toon", kind)),
        ("netdistill:wire", lambda: netdistill(raw, "wire", kind)),
        ("netdistill:kv", lambda: netdistill_kv(raw, kind)),
        ("truncate", lambda: truncate_to(raw, len(netdistill(raw, "wire", kind)))),
        ("strip-dedup", lambda: strip_dedup(raw)),
        ("llmlingua", lambda: llmlingua_compress(raw)),
        ("textfsm-json", lambda: textfsm_json(raw, fixture)),
    ])
    return OrderedDict((m, thunks[m]()) for m in wanted)


def ollama_generate(model, prompt):
    body = json.dumps({
        "model": model, "prompt": prompt, "stream": False,
        "options": {"temperature": 0, "num_predict": 128, "num_ctx": NUM_CTX},
    }).encode()
    req = urllib.request.Request(
        f"{OLLAMA_URL}/api/generate", data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=600) as r:
        d = json.loads(r.read())
    return d.get("response", "").strip(), d.get("prompt_eval_count", 0)


def grade(answer, case):
    a = answer.lower()
    return (all(s.lower() in a for s in case["must_contain"])
            and not any(s.lower() in a for s in case.get("must_not_contain", [])))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default=os.environ.get("NETDISTILL_EVAL_MODEL", "gemma3:12b"))
    ap.add_argument("--cases", default=str(Path(__file__).parent / "cases.json"))
    ap.add_argument("--report", default=str(Path(__file__).parent / "compare_report.md"))
    ap.add_argument("--methods", default="", help="comma-separated subset to run")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()
    if BIN is None:
        sys.exit("netdistill binary not found — run `cargo build --release`")

    cases = json.loads(Path(args.cases).read_text())["cases"]
    default_methods = ["raw", "netdistill:text", "netdistill:toon", "netdistill:wire",
                       "truncate", "strip-dedup", "llmlingua", "textfsm-json"]
    methods = [m.strip() for m in args.methods.split(",") if m.strip()] or default_methods
    # results[m] = list over cases of dict(ok, ptok, est, supported)
    results = {m: [] for m in methods}
    print(f"model: {args.model}   methods: {len(methods)}   cases: {len(cases)}   num_ctx: {NUM_CTX}\n")

    for case in cases:
        raw = (FIXTURES / case["fixture"]).read_text()
        built = build_methods(raw, case, methods)
        line = f"{case['id']:<20}"
        for m in methods:
            ctx = built[m]
            if ctx is None:
                results[m].append(dict(supported=False))
                line += f"  {m.split(':')[-1]}:n/a"
                continue
            answer, ptok = ollama_generate(args.model, PROMPT_TMPL.format(ctx=ctx, q=case["question"]))
            ok = grade(answer, case)
            results[m].append(dict(supported=True, ok=ok, ptok=ptok, est=est_tok(ctx)))
            line += f"  {m.split(':')[-1]}:{'✓' if ok else '✗'}"
            if args.verbose:
                print(f"    [{m}] {ptok}tok est{est_tok(ctx)} -> {answer[:70]!r} {'PASS' if ok else 'FAIL'}")
        print(line)

    def agg(m):
        r = [x for x in results[m] if x.get("supported")]
        n = len(r)
        acc = sum(1 for x in r if x["ok"]) / n if n else 0
        avg_ptok = sum(x["ptok"] for x in r) / n if n else 0
        avg_est = sum(x["est"] for x in r) / n if n else 0
        return n, acc, avg_ptok, avg_est

    raw_est = agg("raw")[3]
    lines = [
        f"# netdistill vs. the alternatives — {args.model}",
        "",
        "Same fidelity question + deterministic grader for every method. Prompt tokens",
        "are the backend's real `prompt_eval_count`. `est` is netdistill's char-based",
        "estimate (model-free, covers unsupported-for-QA methods too).",
        "",
        "| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |",
        "|---|---|---|---|---|---|",
    ]
    for m in methods:
        n, acc, ptok, est = agg(m)
        vs = "—" if not raw_est else f"{est / raw_est * 100:.0f}%"
        lines.append(f"| {m} | {acc*100:.0f}% ({sum(1 for x in results[m] if x.get('supported') and x['ok'])}/{n}) | {ptok:.0f} | {est:.0f} | {vs} | {n}/{len(cases)} |")
    lines += ["", "## per-case (✓ / ✗ / n·a unsupported)", "",
              "| case | " + " | ".join(m.split(":")[-1] for m in methods) + " |",
              "|---|" + "|".join(["---"] * len(methods)) + "|"]
    for i, case in enumerate(cases):
        cells = []
        for m in methods:
            x = results[m][i]
            cells.append("n·a" if not x.get("supported") else ("✓" if x["ok"] else "✗"))
        lines.append("| " + case["id"] + " | " + " | ".join(cells) + " |")

    report = "\n".join(lines) + "\n"
    Path(args.report).write_text(report)
    print("\n" + report + f"(written to {args.report})")


if __name__ == "__main__":
    main()
