# netdistill vs. the alternatives — gemma3:12b

Same fidelity question + deterministic grader for every method. Prompt tokens
are the backend's real `prompt_eval_count`. `est` is netdistill's char-based
estimate (model-free, covers unsupported-for-QA methods too).

| method | fidelity | avg real prompt tok | avg est ctx tok | size vs raw | cases |
|---|---|---|---|---|---|
| raw | 90% (9/10) | 2494 | 1686 | 100% | 10/10 |
| netdistill:text | 100% (10/10) | 461 | 226 | 13% | 10/10 |
| netdistill:toon | 100% (10/10) | 476 | 227 | 13% | 10/10 |
| netdistill:wire | 90% (9/10) | 440 | 206 | 12% | 10/10 |
| truncate | 40% (4/10) | 428 | 212 | 13% | 10/10 |
| strip-dedup | 70% (7/10) | 2388 | 1082 | 64% | 10/10 |
| llmlingua | 10% (1/10) | 1047 | 353 | 21% | 10/10 |
| textfsm-json | 67% (4/6) | 4110 | 9937 | 589% | 6/10 |

## per-case (✓ / ✗ / n·a unsupported)

| case | raw | text | toon | wire | truncate | strip-dedup | llmlingua | textfsm-json |
|---|---|---|---|---|---|---|---|---|
| bgp_down_peers | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | n·a |
| bgp_total_peers | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n·a |
| ospf_stuck | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |
| lldp_gi00_peer | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |
| intf_crc_errors | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✓ |
| intf_unexpected_down | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ |
| route_no_default | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ |
| config_bgp_neighbors | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ | ✗ | n·a |
| config_hostname | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | n·a |
| big_bgp_buried_down | ✗ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
