# netdistill vs. the alternatives — qwen2.5:3b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 20% (1/5) | 866 | 441 | 100% | 5/5 |
| netdistill:text | 40% (2/5) | 426 | 203 | 46% | 5/5 |
| netdistill:toon | 40% (2/5) | 448 | 206 | 47% | 5/5 |
| netdistill:wire | 40% (2/5) | 408 | 183 | 41% | 5/5 |
| netdistill:kv | 40% (2/5) | 483 | 262 | 59% | 5/5 |

## per-case (✓ / ✗ / n·a unsupported)

| case | raw | text | toon | wire | kv |
|---|---|---|---|---|---|
| lldp_tengig_uplink | ✓ | ✓ | ✓ | ✓ | ✓ |
| route_bgp_gt2 | ✗ | ✗ | ✗ | ✗ | ✗ |
| route_ospf_gt_bgp | ✗ | ✗ | ✗ | ✗ | ✗ |
| config_transit_peergroup | ✗ | ✓ | ✓ | ✓ | ✓ |
| junos_ospf_gt_bgp | ✗ | ✗ | ✗ | ✗ | ✗ |
