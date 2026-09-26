#!/usr/bin/env python3
"""指定ノードに接続する highway way の node 列を PBF から抜き出す。

issue #322 の再現ノード確認用。閉路 way（先頭==末尾）が継ぎ目でノードに
接続しているかを目視するために使う。

usage: inspect_node_ways.py <region.osm.pbf> <node_id> [<node_id> ...]
"""
import subprocess
import sys

pbf, targets = sys.argv[1], {int(a) for a in sys.argv[2:]}
found = {t: [] for t in targets}

proc = subprocess.Popen(
    ["osmium", "tags-filter", "-f", "opl", pbf, "w/highway"],
    stdout=subprocess.PIPE, text=True, bufsize=1 << 20,
)
for line in proc.stdout:
    if not line.startswith("w"):
        continue
    way_id, refs, tags = line.split(" ", 1)[0][1:], None, None
    for field in line.rstrip("\n").split(" "):
        if field.startswith("N"):
            refs = field[1:]
        elif field.startswith("T"):
            tags = field[1:]
    if not refs:
        continue
    nodes = [int(r[1:]) for r in refs.split(",") if r.startswith("n")]
    hit = targets & set(nodes)
    for nid in hit:
        found[nid].append((way_id, nodes, tags))
proc.stdout.close()
proc.wait()

for nid, ways in found.items():
    print(f"=== node {nid} : {len(ways)} way(s)")
    for way_id, nodes, tags in ways:
        closed = nodes[0] == nodes[-1]
        idx = [i for i, n in enumerate(nodes) if n == nid]
        print(f"  way {way_id} len={len(nodes)} closed={closed} pos={idx}/{len(nodes)-1} "
              f"ends=({nodes[0]},{nodes[-1]}) tags={tags}")
