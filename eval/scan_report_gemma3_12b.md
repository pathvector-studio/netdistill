# netdistill vs. the alternatives — gemma3:12b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 60% (3/5) | 862 | 441 | 100% | 5/5 |
| netdistill:text | 100% (5/5) | 432 | 203 | 46% | 5/5 |
| netdistill:toon | 40% (2/5) | 450 | 206 | 47% | 5/5 |
| netdistill:wire | 60% (3/5) | 409 | 183 | 41% | 5/5 |
| netdistill:kv | 80% (4/5) | 493 | 262 | 59% | 5/5 |

## per-case (✓ / ✗ / n·a unsupported)

| case | raw | text | toon | wire | kv |
|---|---|---|---|---|---|
| lldp_tengig_uplink | ✓ | ✓ | ✓ | ✓ | ✓ |
| route_bgp_gt2 | ✗ | ✓ | ✗ | ✗ | ✗ |
| route_ospf_gt_bgp | ✓ | ✓ | ✗ | ✓ | ✓ |
| config_transit_peergroup | ✗ | ✓ | ✓ | ✓ | ✓ |
| junos_ospf_gt_bgp | ✓ | ✓ | ✗ | ✗ | ✓ |
