# How netdistill works — logic deep-dive

English | [日本語](how-it-works.ja.md)

What netdistill actually does when it turns a 729k-token `show ip route vrf all` into 164 tokens — including every heuristic and threshold. Source locations are noted per section.

## Pipeline

```
input text
   │
   ▼
[1] session split (lib.rs::split_sessions)
   │   split on prompt lines "router1# show ..." into per-command chunks
   ▼
[2] kind detection (detect.rs::detect)        … per chunk
   │   bgp-summary / route-table / interfaces / config / generic
   ▼
[3] parse & distill (parse/*.rs)
   │   structure → aggregate → extract anomalies → decide what to elide
   ▼
[4] Doc model (model.rs)
   │   { headline, alerts[], summary[], tables[], notes[], body? }
   ▼
[5] render (render.rs)
   │   text or JSON; --budget trims tables until it fits
   ▼
output (token receipt on top)
```

The intermediate `Doc` is the keystone: every parser reduces its input to the same vocabulary — alerts, summary, tables, elision notes — so the renderer and budget logic are parser-agnostic.

## Design principles (why this shape)

1. **Anomalies first.** The reason you hand network output to an LLM is almost always diagnosis. Nearly all diagnostic signal lives in anomalies, changes, and aggregates — almost none in the parade of healthy entries.
2. **Elision must be explicit.** Silent truncation makes a model mistake "absent" for "fine" (hallucinated health). Every omission carries an inline declaration like "495 healthy peers omitted (--full to list all)" — which also tells the model exactly what to re-request.
3. **Structure beats prose.** Making an LLM eyeball fixed-width tables causes row misalignment and misread numbers. A pre-parsed tab-separated table is both smaller and more accurate.
4. **Degrade gracefully.** Unknown formats fall back to generic compaction, never an error. The invariant: even a misdetection must still yield a *smaller, honest* input.

## [1] Session splitting — `split_sessions`

Lets you paste a whole terminal log.

**Prompt detection** (`prompt_command`): split the line once on `#` or `>`; it is a prompt iff:

- the part before the separator (hostname) is non-empty, ≤64 chars, and **contains no spaces**
- the part after starts with `show ` or `display `

This rejects Junos route lines like `> to 10.1.1.2 via ...` (empty prefix) and ordinary lines containing `#` (prefix has spaces).

After splitting, if at least one prompt was found, **unlabeled fragments with fewer than 3 non-blank lines** (banners, prompt debris) are dropped. With no prompts, the whole input is one chunk. The detected command string is embedded in the Doc headline as `` `show ip bgp summary` — ... ``.

## [2] Kind detection — `detect`

A lightweight heuristic over the first 200 lines. **The order is part of the spec** (evaluated top-down):

| prio | kind | condition | why this order |
|---|---|---|---|
| 1 | `interfaces` | contains `"line protocol is"` | unique to show interfaces; never appears in configs |
| 2 | `bgp-summary` | any of `MsgRcvd` / `State/PfxRcd` / (`InPkt` and `OutPkt`) / `BGP router identifier` | header column names are the most vendor-stable fingerprint |
| 3 | `route-table` | `Routing Table:` / `Gateway of last resort` / `variably subnetted` / line starting `Codes:` / Junos RIB header | configs also contain `ip route`, so route tables must be claimed first via their unique headings |
| 4 | `config` | a line starts with `interface ` AND (≥3 bare-`!` lines OR a `hostname ` line) | interface blocks plus IOS's `!` separators |
| 5 | `generic` | none of the above | fallback, never an error |

The Junos RIB header check (`junos_rib_header`): split on `:` → name part contains `inet` with no spaces → remainder contains `destinations`. Matches both `inet.0:` and `CUST-A.inet6.0:`.

**Known limit**: a full-table `show ip bgp` starts with `BGP router identifier` and is misdetected as bgp-summary (planned for v0.2). The failure is honest: no line parses as a peer row, so the output reports zero peers rather than inventing anything (principle 4).

## [3] Parsers

### bgp-summary (`parse/bgp_summary.rs`)

**Dialect switching**: seeing a header with `InPkt`+`OutPkt` enables Junos mode; `Neighbor`+`MsgRcvd` enables IOS/FRR mode. It switches per header line — no upfront declaration.

**Row admission**: only lines whose first token looks like an IP are peer rows. `is_addr` = contains `.` with only digits/dots (v4), or contains `:` with only hex/colon/dot (v6). FRR's dynamic-peer `*` prefix is stripped.

**IOS/FRR rows** (≥10 columns): read positionally as `Neighbor V AS MsgRcvd MsgSent TblVer InQ OutQ Up/Down State/PfxRcd`. **Column 10 numeric → Established with that many received prefixes; otherwise it is the state string.** Two-token states like `Idle (Admin)` are joined only when the next column starts with `(` — so FRR's PfxSnt/Desc columns never get swallowed into the state.

**Junos rows** (≥8 columns): a last field shaped `digits/digits/...` means Established (first number = active prefixes). A word like `Establ` is taken as state and normalized `Estab*` → `Established`. Up/Down is the join of column 7 through the second-to-last (handles embedded spaces: `12w4d 2:03:11`). An indented continuation `inet.0: 1822/1830/1830/0` back-fills the prefix count of the **previous** peer.

**Alert rules** (evaluated in this order; first match wins, flaps checked independently):

| condition | alert |
|---|---|
| state ≠ Established | `session <state> — up/down <time>` |
| Established and pfx == 0 | `Established but 0 prefixes received` (a session that is up yet effectively dead) |
| Established and Up/Down starts `00:` | `session reset recently` (re-established within the hour = recent flap) |
| (Junos) Flaps ≥ 3 | `<n> flaps` |

**Aggregates**: peer count by state / total received prefixes / top-3 peers by prefixes (AS, count, address) / list of AF/VRF sections when more than one.

**Elision rule**: the table lists only alerting peers; healthy Established peers survive as a count in the elision note. If every peer is healthy, the table itself is elided — with a note saying so. `--full` switches to an all-peers table.

### route-table (`parse/route.rs`)

**Table (VRF) boundaries**: `Routing Table: <name>` (IOS), `VRF: <name>` (IOS-XR), or a Junos RIB header (`inet.0: N destinations, M routes (... K hidden)`) switch the current table. The Junos header also yields the hidden count.

**IOS-style route lines**: only **column-0 lines** are route candidates (IOS left-aligns route lines and indents parent headings like `10.0.0.0/8 is variably subnetted...` and ECMP continuations — this single rule separates them). First token must be a code (≤3 chars, letters plus `*%+`); a second token from the known subcode set (`IA E1 E2 EX N1 N2 L1 L2 su ...`) is skipped; the next token is the prefix. Lines containing `subnetted` are headings and excluded.

- Protocol normalization: `C→connected, L→local, S→static, O→ospf, B→bgp, D→eigrp, R→rip, i→isis, K→kernel`; `O IA`/`O E2` all fold into ospf, `S*` into static (`*` is just the candidate-default marker).
- Next-hop: the token after `via `. `is directly connected, Gi0/0` records `direct:<IF>`.
- Indented lines starting `[` and containing ` via ` are **ECMP/wrapped continuations**: they add a next-hop count but not a route.
- Default route: detected both from `Gateway of last resort is <X>` (`not set` = absent) and from `0.0.0.0/0` / `::/0` entries.

**Junos route lines**: first token is a prefix, second token is `*[Proto/pref]`; protocol comes from inside the bracket. Subsequent `> to <nh> via <if>` lines count next-hops, and if the preceding entry was the default route, its next-hop fixes `default_via`.

**Alerts**: tables with 0 routes; tables with hidden routes (Junos — policy-hidden routes are the first place to look during an incident).

**Aggregates & tables**: protocol totals (descending) / which tables lack a default route ("no default route in any table" if all) / the **VRF × protocol matrix** (top-6 protocol columns + other, with default_via per row) / **top-8 next-hop ranking** (where routes concentrate tells you the blast radius at a glance).

**Elision rule**: individual prefixes are never printed by default — this is the dominant compression source (50k routes → a couple of matrix rows). The note says "aggregated by table/protocol (--full to list every route)".

### interfaces (`parse/interfaces.rs`)

**Block start**: a column-0 line containing both ` is ` and `line protocol is`. Name = text before ` is `; admin-down = contains `administratively down`; oper-up = text after `line protocol is ` starts with `up`.

**Counter extraction**: from subsequent indented lines, take the **number immediately before** a keyword (`14411 input errors, 14200 CRC, ...` → 14411 before `input errors`). Only `Total output drops:` takes the number after. `Description:` and `Internet address is` are also captured.

**Alerts**: line-protocol down without admin-down (unintended outage) / input errors > 0 (with CRC — L1 quality) / output errors > 0 / output drops > 0 (congestion). Every threshold is simply "> 0" — interpreting magnitudes is left to the LLM.

**Elision rule**: table lists alerting interfaces only; healthy ones survive as a count.

### config (`parse/config.rs`)

Walks blocks using the IOS convention that child lines are indented by one space.

- `interface` blocks: description / IP (dotted masks like `255.255.255.0` are converted to `/24` by summing `count_ones()`) / vrf forwarding / shutdown / access vlan.
- `router bgp` block: AS number; `neighbor X remote-as/description/peer-group` merged per address; address-family list.
- Top-level one-liners: static route count / **names** (not line counts) of ACLs, prefix-lists, route-maps / vrf definitions / presence and counts of ntp, snmp, aaa, logging, username.
- Unrecognized top-level blocks are collected by first word and listed in a note: "present but not summarized: crypto, line, ..." — **the existence of unsummarized content is never hidden**.

**Secrets policy**: lines like `username ... secret 9 $9$...` are structurally discarded — the value is never written to any field. A note states "secrets/crypto/aaa details are not extracted". Since digests are destined for hosted LLMs, this is guaranteed by design, not by heuristic.

**Elision rule**: interfaces with nothing configured (no IP/desc/vrf/vlan/shutdown) are dropped from the table, with a count in the note.

### generic (`parse/generic.rs`)

Near-lossless compaction for unrecognized input. Exactly three transformations:

1. **Whitespace squeeze**: runs of ≥2 spaces inside a line (fixed-width column padding) become one tab; leading indent is capped at one space. Meaning preserved, tokens removed.
2. **Duplicate-line RLE**: consecutive identical lines collapse to `<line>  [x5]` (repeated syslog bursts).
3. **Blank runs collapse to one.**

The transformations are always declared in a note, so the LLM can read `[x5]` as annotation.

## [4] Token estimation — `token.rs`

`ceil(chars × 2 / 7)` ≈ **3.5 chars/token**. Rationale: network text is full of IP addresses (which tokenizers tend to split per octet), counters, and padding — reliably worse than the ~4 chars/token rule of thumb for English prose. No real tokenizer by design (zero deps, model-agnostic); output always marks counts with `~` and "estimates". A feature-gated real tokenizer is planned for v0.3.

## [5] Rendering and the budget loop — `render.rs`

**Output format** is a fixed structure optimized for LLM reading accuracy:

```
# netdistill — input ~X tok → output ~Y tok (Z% saved; estimates)   ← receipt
## [kind] headline
ALERTS (n): / ALERTS: none      ← always first; "none" is explicit (≠ silence)
SUMMARY:
<TABLE TITLE>:
header\tcolumns                  ← tabs: no alignment padding
(elision note)                   ← inline, directly under its table
NOTES:
```

**`--budget N` algorithm** (`shrink`): render → estimate → if over, shrink one step → repeat. Shrink order, lowest information value first:

1. **Halve the largest table** (only tables with ≥4 rows). The cut is appended to the elision note (`... 12 rows trimmed for --budget`).
2. Once all tables are ≤3 rows, **halve the largest raw body** (generic, ≥8 lines), appending `[... N lines trimmed]`.
3. When neither is possible, stop and emit `[!] --budget could not be met; smallest faithful output shown`.

The invariant: **ALERTS and SUMMARY are never trimmed**. However small the budget, the anomaly list and the overall picture survive intact. Halving means the loop re-renders only O(log rows) times.

## [6] Serving modes — `mcp.rs` / `http.rs`

Both servers are thin wrappers over the same `distill_to_string(input, full, budget, json)` call the CLI uses — no separate code path, so behavior is identical across all three entry points.

- **MCP** (`netdistill mcp`): newline-delimited JSON-RPC 2.0 over stdio. Implements `initialize` (echoes the client's protocol version), `ping`, `tools/list`, `tools/call`; notifications get no response, unknown methods/tools return proper JSON-RPC errors (-32601/-32602). One tool: `distill`.
- **HTTP** (`netdistill serve`): a 4-thread `tiny_http` server. `POST /v1/distill` accepts either the raw CLI output as the body (options via query string) or a JSON envelope; `route()` is a pure function of (method, path, query, content-type, body) and is unit-tested without sockets. No auth by design — bind to localhost (default) or front it yourself.

## Where the compression comes from

Decomposing the 50,000-route → 164-token (99.98%) result:

| technique | contribution |
|---|---|
| eliding individual prefixes (replaced by aggregates) | the overwhelming bulk: 50k lines → 2 matrix rows |
| eliding healthy entries (anomalies only) | dominant for peer-style output: 500 rows → 3 |
| padding → tabs, no repeated headers | 2–3× on every surviving line |
| explicit elision notes | **negative** contribution (adds tokens) — a deliberate cost paid to prevent hallucination |

netdistill is not string compression: it is **lossy compression down to the statistics an LLM needs for diagnosis, plus a declaration of exactly what was lost**.

## Known limits (v0.1)

- Full-table `show ip bgp` is misdetected as bgp-summary (fails safe, but yields no info)
- NX-OS / EOS / Huawei native formats unverified (fall back to generic)
- Junos hierarchical/set-style configs not covered by the config parser (fall back to generic)
- Token counts are estimates; real per-model counts differ
- `--full` on a full route table is, naturally, huge (that is what --full means)

See [ROADMAP.md](../ROADMAP.md) for what's next.
