# netdistill vs. the alternatives — qwen2.5:14b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 82% (14/17) | 1676 | 1080 | 100% | 17/17 |
| netdistill:text | 100% (17/17) | 417 | 200 | 18% | 17/17 |
| netdistill:toon | 100% (17/17) | 434 | 201 | 19% | 17/17 |
| netdistill:wire | 100% (17/17) | 401 | 180 | 17% | 17/17 |
| truncate | 41% (7/17) | 411 | 187 | 17% | 17/17 |
| strip-dedup | 88% (15/17) | 1590 | 701 | 65% | 17/17 |
| llmlingua | 6% (1/17) | 643 | 229 | 21% | 17/17 |
| textfsm-json | 67% (4/6) | 3925 | 9937 | 920% | 6/17 |

## per-case (✓ / ✗ / n·a unsupported)

| case | raw | text | toon | wire | truncate | strip-dedup | llmlingua | textfsm-json |
|---|---|---|---|---|---|---|---|---|
| bgp_down_peers | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| bgp_total_peers | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n·a |
| ospf_stuck | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |
| lldp_gi00_peer | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ |
| intf_crc_errors | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ |
| intf_unexpected_down | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ |
| route_no_default | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ |
| config_bgp_neighbors | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| config_hostname | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| big_bgp_buried_down | ✗ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ |
| xr_bgp_not_established | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| eos_bgp_broken | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| vrp_bgp_broken | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| nxos_no_default_route | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| junos_hidden_route | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| isis_stuck_adjacency | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| bgptable_top_origin_as | ✗ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | n·a |
