# netdistill fidelity eval — qwen2.5:14b

Same question, same grader, different context format. Prompt tokens are the
backend's real `prompt_eval_count` (whole prompt; template is constant across
formats, so differences are context cost).

| format | accuracy | avg prompt tokens | vs raw |
|---|---|---|---|
| raw | 100% (9/9) | 957 | 100% tokens |
| text | 100% (9/9) | 444 | 46% tokens |
| json | 100% (9/9) | 602 | 63% tokens |
| toon | 100% (9/9) | 461 | 48% tokens |
| wire | 100% (9/9) | 427 | 45% tokens |

## per-case (✓ correct / ✗ wrong)

| case | raw | text | json | toon | wire |
|---|---|---|---|---|---|
| bgp_down_peers | ✓ | ✓ | ✓ | ✓ | ✓ |
| bgp_total_peers | ✓ | ✓ | ✓ | ✓ | ✓ |
| ospf_stuck | ✓ | ✓ | ✓ | ✓ | ✓ |
| lldp_gi00_peer | ✓ | ✓ | ✓ | ✓ | ✓ |
| intf_crc_errors | ✓ | ✓ | ✓ | ✓ | ✓ |
| intf_unexpected_down | ✓ | ✓ | ✓ | ✓ | ✓ |
| route_no_default | ✓ | ✓ | ✓ | ✓ | ✓ |
| config_bgp_neighbors | ✓ | ✓ | ✓ | ✓ | ✓ |
| config_hostname | ✓ | ✓ | ✓ | ✓ | ✓ |
