#!/usr/bin/env python3
"""Issue #319 の影響件数を数える。

DB に登録済みの Y字路ノードについて、OSM PBF から
  - way 本数 (detector.rs の `way_ids.len()` に相当)
  - 実分岐数 (ノードが way の途中なら +2、端点なら +1)
を数え直し、「way 本数 3 だが実分岐 4 以上」= 誤検出を集計する。

対象ノードは DB から取った ID 集合に限定するのでメモリは ID 数に比例するだけ。

usage: audit_passing_way_junctions.py <node_ids.csv> <region.osm.pbf> <out.json>
"""

import json
import subprocess
import sys

# detector.rs NodeConnectionCounter::new() の valid_highway_types と一致させること
VALID_HIGHWAY_TYPES = {
    "trunk", "primary", "secondary", "tertiary",
    "residential", "unclassified", "service",
    "trunk_link", "primary_link", "secondary_link", "tertiary_link",
    "steps", "pedestrian", "path",
}


def unescape(s):
    """OPL は空白等を %XX でエスケープする。highway 値の判定に必要な分だけ戻す。"""
    if "%" not in s:
        return s
    out, i = [], 0
    while i < len(s):
        if s[i] == "%" and i + 2 < len(s):
            try:
                out.append(chr(int(s[i + 1:i + 3], 16)))
                i += 3
                continue
            except ValueError:
                pass
        out.append(s[i])
        i += 1
    return "".join(out)


def highway_of(tag_field):
    """OPL の T フィールドから highway の値を取り出す。"""
    for kv in tag_field.split(","):
        if kv.startswith("highway="):
            return unescape(kv[len("highway="):])
    return None


def main():
    node_ids_path, pbf_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]

    targets = set()
    with open(node_ids_path) as f:
        for line in f:
            line = line.strip()
            if line.isdigit():
                targets.add(int(line))

    # node_id -> [way 本数, 実分岐数]
    stats = {}

    proc = subprocess.Popen(
        ["osmium", "tags-filter", "-f", "opl", pbf_path, "w/highway"],
        stdout=subprocess.PIPE, text=True, bufsize=1 << 20,
    )

    for line in proc.stdout:
        if not line.startswith("w"):
            continue
        highway, refs = None, None
        for field in line.rstrip("\n").split(" "):
            if field.startswith("T"):
                highway = highway_of(field[1:])
            elif field.startswith("N"):
                refs = field[1:]
        if highway not in VALID_HIGHWAY_TYPES or not refs:
            continue

        nodes = [int(r[1:]) for r in refs.split(",") if r.startswith("n")]
        last = len(nodes) - 1
        seen_in_way = set()
        for i, nid in enumerate(nodes):
            if nid not in targets:
                continue
            st = stats.get(nid)
            if st is None:
                st = stats[nid] = [0, 0]
            # way 本数は way 単位で 1 回だけ数える（閉路で同じノードが複数回出るため）
            if nid not in seen_in_way:
                seen_in_way.add(nid)
                st[0] += 1
            st[1] += 1 if (i == 0 or i == last) else 2

    proc.stdout.close()
    if proc.wait() != 0:
        sys.exit(f"osmium failed on {pbf_path}")

    with open(out_path, "w") as f:
        json.dump({str(k): v for k, v in stats.items()}, f)
    print(f"{pbf_path}: {len(stats)} target nodes found", file=sys.stderr)


if __name__ == "__main__":
    main()
