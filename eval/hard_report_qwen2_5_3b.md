# netdistill vs. the alternatives — qwen2.5:3b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 60% (3/5) | 1242 | 623 | 100% | 5/5 |
| netdistill:text | 100% (5/5) | 474 | 238 | 38% | 5/5 |
| netdistill:toon | 80% (4/5) | 493 | 239 | 38% | 5/5 |
| netdistill:wire | 60% (3/5) | 456 | 216 | 35% | 5/5 |
| netdistill:kv | 80% (4/5) | 470 | 238 | 38% | 5/5 |

## per-case (✓ / ✗ / n·a unsupported)

| case | raw | text | toon | wire | kv |
|---|---|---|---|---|---|
| multihop_tophop_as | ✓ | ✓ | ✓ | ✗ | ✓ |
| multihop_maxpeer_routes | ✗ | ✓ | ✗ | ✗ | ✗ |
| adv_admin_vs_broken | ✗ | ✓ | ✓ | ✓ | ✓ |
| adv_egress_discard | ✓ | ✓ | ✓ | ✓ | ✓ |
| adv_up_but_erroring | ✓ | ✓ | ✓ | ✓ | ✓ |
