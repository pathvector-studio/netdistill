# netdistill vs. the alternatives — qwen2.5:14b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 90% (9/10) | 2504 | 1686 | 100% | 10/10 |
| netdistill:text | 100% (10/10) | 453 | 226 | 13% | 10/10 |
| netdistill:toon | 100% (10/10) | 470 | 227 | 13% | 10/10 |
| netdistill:wire | 100% (10/10) | 437 | 206 | 12% | 10/10 |
| truncate | 30% (3/10) | 445 | 212 | 13% | 10/10 |
| strip-dedup | 90% (9/10) | 2383 | 1082 | 64% | 10/10 |
| llmlingua | 10% (1/10) | 959 | 353 | 21% | 10/10 |
| textfsm-json | 67% (4/6) | 3925 | 9937 | 589% | 6/10 |

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
