# netdistill vs. the alternatives — qwen2.5:14b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 75% (3/4) | 898 | 468 | 100% | 4/4 |
| netdistill:text | 100% (4/4) | 440 | 212 | 45% | 4/4 |
| netdistill:toon | 75% (3/4) | 462 | 216 | 46% | 4/4 |
| netdistill:wire | 75% (3/4) | 421 | 192 | 41% | 4/4 |
| netdistill:kv | 100% (4/4) | 505 | 280 | 60% | 4/4 |

## per-case (✓ / ✗ / n·a unsupported)

| case | raw | text | toon | wire | kv |
|---|---|---|---|---|---|
| lldp_tengig_uplink | ✓ | ✓ | ✓ | ✓ | ✓ |
| route_bgp_gt2 | ✗ | ✓ | ✓ | ✓ | ✓ |
| route_ospf_gt_bgp | ✓ | ✓ | ✗ | ✗ | ✓ |
| config_transit_peergroup | ✓ | ✓ | ✓ | ✓ | ✓ |
