#!/usr/bin/env bash
# Generate large synthetic inputs to demo compression at realistic scale.
#   gen_big_inputs.sh <outdir> [routes] [peers]
set -eu
out=${1:?usage: gen_big_inputs.sh <outdir> [routes] [peers]}
routes=${2:-50000}
peers=${3:-500}
mkdir -p "$out"

# --- show ip route vrf all (IOS style) ---
{
  echo "Routing Table: default"
  echo "Codes: L - local, C - connected, S - static, R - RIP, M - mobile, B - BGP"
  echo "       D - EIGRP, EX - EIGRP external, O - OSPF, IA - OSPF inter area"
  echo
  echo "Gateway of last resort is 10.0.0.1 to network 0.0.0.0"
  echo
  echo "S*    0.0.0.0/0 [1/0] via 10.0.0.1"
  awk -v n="$routes" 'BEGIN {
    for (i = 1; i <= n; i++) {
      o1 = 20 + int(i / 65536) % 200; o2 = int(i / 256) % 256; o3 = i % 256
      nh = (i % 4) + 2
      printf "B        %d.%d.%d.0/24 [20/0] via 10.0.12.%d, 5w4d\n", o1, o2, o3, nh
    }
  }'
  echo
  echo "Routing Table: MGMT"
  echo "Gateway of last resort is not set"
  echo
  echo "C        192.168.100.0/24 is directly connected, Vlan100"
  echo "L        192.168.100.1/32 is directly connected, Vlan100"
} > "$out/big_route_vrf_all.txt"

# --- show bgp summary (FRR style) ---
{
  echo "BGP router identifier 10.255.0.1, local AS number 64512 vrf-id 0"
  echo
  echo "Neighbor        V         AS   MsgRcvd   MsgSent   TblVer  InQ OutQ  Up/Down State/PfxRcd"
  awk -v n="$peers" 'BEGIN {
    for (i = 1; i <= n; i++) {
      addr = sprintf("10.%d.%d.%d", int(i / 65536), int(i / 256) % 256, i % 256)
      if (i == 37)       printf "%-15s 4      65001         0         0        0    0    0 never    Active\n", addr
      else if (i == 205) printf "%-15s 4      65002        42        40        0    0    0 1d03h    Idle\n", addr
      else if (i == 411) printf "%-15s 4      65003      1211      1209   812734    0    0 00:42:11            0\n", addr
      else               printf "%-15s 4      65%03d    182341    182350   812734    0    0 5w4d           1823\n", addr, i % 999
    }
  }'
  echo
  echo "Total number of neighbors $peers"
} > "$out/big_bgp_summary.txt"

echo "wrote $out/big_route_vrf_all.txt ($(wc -l < "$out/big_route_vrf_all.txt") lines)"
echo "wrote $out/big_bgp_summary.txt ($(wc -l < "$out/big_bgp_summary.txt") lines)"
