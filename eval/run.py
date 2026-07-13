#!/usr/bin/env python3
"""netdistill fidelity eval.

Question: does an LLM reach the *same answer* from a distilled digest as from
the raw CLI output — at a fraction of the tokens?

For every (case, format) pair this feeds the model ONLY that context plus the
question, grades the answer deterministically, and records the real prompt
token count reported by the backend. The report shows, per format, accuracy
alongside token cost — so you can see how far density can be pushed before the
answer degrades (netdistill's own "Equal-Info Window": bound density by
measured comprehension, not by guesswork).

Backend-agnostic: the model call is one function (`ollama_generate`). Swap it
for an Anthropic/OpenAI call to eval a different model; everything else is
format/grading plumbing that does not care which model answered.

Usage:
    python3 eval/run.py                      # default model, all formats
    python3 eval/run.py --model qwen2.5:14b
    python3 eval/run.py --formats raw,text,toon,wire
    NETDISTILL_EVAL_URL=http://host:11435 python3 eval/run.py
"""

import argparse
import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "fixtures"
BIN_CANDIDATES = [ROOT / "target/release/netdistill", ROOT / "target/debug/netdistill"]
OLLAMA_URL = os.environ.get("NETDISTILL_EVAL_URL", "http://localhost:11435")

PROMPT_TMPL = (
    "You are a network engineer's assistant. Answer the QUESTION using ONLY the "
    "CONTEXT below, which is output from a network device (it may be summarized). "
    "Be concise: give just the specific answer — names, IP addresses, or numbers. "
    "If the answer is not in the context, reply 'unknown'.\n\n"
    "CONTEXT:\n{ctx}\n\n"
    "QUESTION: {q}\nANSWER:"
)


def netdistill_bin():
    for p in BIN_CANDIDATES:
        if p.exists():
            return str(p)
    sys.exit("netdistill binary not found — run `cargo build` first")


def render(binary, raw, fmt, kind):
    """Return the context string for a given format (raw = untouched input)."""
    if fmt == "raw":
        return raw
    args = [binary, "--format", fmt, "--kind", kind]
    return subprocess.run(
        args, input=raw, capture_output=True, text=True, check=True
    ).stdout


def ollama_generate(model, prompt):
    """Return (response_text, prompt_token_count). prompt_eval_count is the real
    tokenizer count the model saw — not an estimate."""
    body = json.dumps(
        {
            "model": model,
            "prompt": prompt,
            "stream": False,
            "options": {"temperature": 0, "num_predict": 128},
        }
    ).encode()
    req = urllib.request.Request(
        f"{OLLAMA_URL}/api/generate", data=body, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=180) as r:
        d = json.loads(r.read())
    return d.get("response", "").strip(), d.get("prompt_eval_count", 0)


def grade(answer, case):
    a = answer.lower()
    for s in case["must_contain"]:
        if s.lower() not in a:
            return False
    for s in case.get("must_not_contain", []):
        if s.lower() in a:
            return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default=os.environ.get("NETDISTILL_EVAL_MODEL", "gemma3:12b"))
    ap.add_argument("--formats", default="raw,text,json,toon,wire")
    ap.add_argument("--cases", default=str(Path(__file__).parent / "cases.json"))
    ap.add_argument("--report", default=str(Path(__file__).parent / "last_report.md"))
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    formats = [f.strip() for f in args.formats.split(",") if f.strip()]
    binary = netdistill_bin()
    cases = json.loads(Path(args.cases).read_text())["cases"]

    # results[fmt] = list of (case_id, correct: bool, prompt_tokens: int)
    results = {f: [] for f in formats}
    print(f"model: {args.model}   formats: {', '.join(formats)}   cases: {len(cases)}\n")

    for case in cases:
        raw = (FIXTURES / case["fixture"]).read_text()
        line = f"{case['id']:<22}"
        for fmt in formats:
            ctx = render(binary, raw, fmt, case["kind"])
            answer, ptok = ollama_generate(args.model, PROMPT_TMPL.format(ctx=ctx, q=case["question"]))
            ok = grade(answer, case)
            results[fmt].append((case["id"], ok, ptok))
            line += f"  {fmt}:{'✓' if ok else '✗'}"
            if args.verbose:
                print(f"    [{fmt}] {ptok}tok -> {answer!r} => {'PASS' if ok else 'FAIL'}")
        print(line)

    # ---- aggregate report ----
    def acc(fmt):
        r = results[fmt]
        return sum(1 for _, ok, _ in r if ok) / len(r) if r else 0.0

    def avg_tok(fmt):
        r = results[fmt]
        return sum(t for _, _, t in r) / len(r) if r else 0.0

    raw_tok = avg_tok("raw") if "raw" in results else None
    lines = [
        f"# netdistill fidelity eval — {args.model}",
        "",
        "Same question, same grader, different context format. Prompt tokens are the",
        "backend's real `prompt_eval_count` (whole prompt; template is constant across",
        "formats, so differences are context cost).",
        "",
        "| format | accuracy | avg prompt tokens | vs raw |",
        "|---|---|---|---|",
    ]
    for fmt in formats:
        a = acc(fmt)
        t = avg_tok(fmt)
        vs = "—" if raw_tok in (None, 0) else f"{t / raw_tok * 100:.0f}% tokens"
        lines.append(f"| {fmt} | {a * 100:.0f}% ({sum(1 for _,ok,_ in results[fmt] if ok)}/{len(results[fmt])}) | {t:.0f} | {vs} |")
    lines += ["", "## per-case (✓ correct / ✗ wrong)", "", "| case | " + " | ".join(formats) + " |", "|---|" + "|".join(["---"] * len(formats)) + "|"]
    for i, case in enumerate(cases):
        row = "| " + case["id"] + " | "
        row += " | ".join("✓" if results[f][i][1] else "✗" for f in formats)
        lines.append(row + " |")

    report = "\n".join(lines) + "\n"
    Path(args.report).write_text(report)
    print("\n" + report)
    print(f"(written to {args.report})")


if __name__ == "__main__":
    main()
