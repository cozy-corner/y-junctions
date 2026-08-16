#!/usr/bin/env python3
"""リージョン別の監査結果をマージして削除対象ノード ID を出す。

audit_passing_way_junctions.py はリージョン PBF 単位で
  node_id -> [way 本数, 実分岐数]
を出す。リージョン境界にまたがるノードは複数の PBF に現れるが、PBF は
境界で way を切るため、片方の結果では way も分岐も過小になりうる。
切り取りは数を減らすことしかしないので、ノードごとに各値の最大を取れば
本来の値が復元できる。

削除条件は detector.rs の候補判定と揃える:
  find_y_junction_candidates は way 本数 3 かつ実分岐数 3 のみを通す。
  よって「way 本数 3 かつ実分岐 4 以上」が #319 の誤検出。

usage: merge_audit_results.py <out_prefix> <audit-*.json>...
出力: <out_prefix>-delete-ids.csv と <out_prefix>-summary.json
"""

import collections
import json
import sys


def main():
    out_prefix, audit_paths = sys.argv[1], sys.argv[2:]
    if not audit_paths:
        sys.exit("no audit json given")

    # node_id -> [way 本数の最大, 実分岐数の最大]
    merged = {}
    for path in audit_paths:
        with open(path) as f:
            stats = json.load(f)
        for nid, (ways, branches) in stats.items():
            nid = int(nid)
            cur = merged.get(nid)
            if cur is None:
                merged[nid] = [ways, branches]
            else:
                cur[0] = max(cur[0], ways)
                cur[1] = max(cur[1], branches)
        print(f"{path}: {len(stats)} nodes", file=sys.stderr)

    delete_ids = []
    buckets = collections.Counter()
    branch_hist = collections.Counter()
    for nid, (ways, branches) in merged.items():
        if ways == 3 and branches == 3:
            buckets["3way_ok"] += 1
        elif ways == 3 and branches >= 4:
            buckets["3way_false_positive"] += 1
            branch_hist[branches] += 1
            delete_ids.append(nid)
        elif ways == 2:
            buckets["2way_path"] += 1
        else:
            buckets["other"] += 1

    delete_ids.sort()
    with open(f"{out_prefix}-delete-ids.csv", "w") as f:
        for nid in delete_ids:
            f.write(f"{nid}\n")

    summary = {
        "nodes_found_in_pbf": len(merged),
        "buckets": dict(buckets),
        "false_positive_branch_histogram": {str(k): v for k, v in sorted(branch_hist.items())},
        "delete_count": len(delete_ids),
    }
    with open(f"{out_prefix}-summary.json", "w") as f:
        json.dump(summary, f, indent=2)

    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
