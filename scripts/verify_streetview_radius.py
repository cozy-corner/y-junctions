#!/usr/bin/env python3
"""Check whether radius=10 is too tight for the Street View coverage batch.

The batch queries the metadata endpoint with radius=10 and then re-checks the
distance to the returned panorama (importer/google.rs). That guard only covers
the "OK but far away" direction — when radius=10 answers ZERO_RESULTS there is
no location to measure, so a panorama that actually sits within 10 m would be
recorded as uncovered and the junction dropped from the map (issue #306, PR-4).

This script looks for that case: radius=10 -> ZERO_RESULTS while radius=50
returns a panorama within 10 m. Metadata requests are free.

Usage:
    docker exec y-junctions-cockroachdb ./cockroach sql --insecure \
        --database y_junction --format csv \
        -e "SELECT osm_node_id, lat, lon FROM y_junctions \
            WHERE NOT (lon BETWEEN 73 AND 135 AND lat BETWEEN 18 AND 54) \
            ORDER BY random() LIMIT 30;" > sample.csv
    GOOGLE_MAPS_API_KEY=... python3 scripts/verify_streetview_radius.py sample.csv
"""

import csv
import json
import math
import os
import sys
import time
import urllib.parse
import urllib.request

ENDPOINT = "https://maps.googleapis.com/maps/api/streetview/metadata"
LIMIT_METERS = 10.0


def metadata(lat, lng, radius, key):
    params = {
        "location": f"{lat},{lng}",
        "radius": radius,
        "source": "outdoor",
        "key": key,
    }
    url = f"{ENDPOINT}?{urllib.parse.urlencode(params)}"
    with urllib.request.urlopen(url, timeout=10) as resp:
        return json.load(resp)


def distance_m(lat, lng, body):
    loc = body.get("location")
    if not loc:
        return None
    dy = (loc["lat"] - lat) * 111_320
    dx = (loc["lng"] - lng) * 111_320 * math.cos(math.radians(lat))
    return math.hypot(dx, dy)


def main():
    key = os.environ.get("GOOGLE_MAPS_API_KEY")
    if not key:
        sys.exit("GOOGLE_MAPS_API_KEY must be set")
    if len(sys.argv) < 2:
        sys.exit(f"usage: {sys.argv[0]} <sample.csv>")

    with open(sys.argv[1], newline="") as f:
        rows = [r for r in csv.DictReader(f)]

    missed = []       # radius=10 says no, radius=50 finds one within 10 m
    far_at_10 = []    # radius=10 says OK but the panorama is beyond 10 m
    agree = 0

    print(f"{'osm_node_id':>12}  {'radius=10':>20}  {'radius=50':>20}  verdict")
    for row in rows:
        nid, lat, lng = row["osm_node_id"], float(row["lat"]), float(row["lon"])
        narrow = metadata(lat, lng, 10, key)
        time.sleep(0.05)
        wide = metadata(lat, lng, 50, key)
        time.sleep(0.05)

        dn, dw = distance_m(lat, lng, narrow), distance_m(lat, lng, wide)
        sn = narrow["status"] + (f" {dn:6.1f}m" if dn is not None else "")
        sw = wide["status"] + (f" {dw:6.1f}m" if dw is not None else "")

        if narrow["status"] == "ZERO_RESULTS" and dw is not None and dw <= LIMIT_METERS:
            verdict = "MISSED by radius=10"
            missed.append((nid, dw))
        elif narrow["status"] == "OK" and dn is not None and dn > LIMIT_METERS:
            verdict = "far pano at radius=10 (distance check rejects)"
            far_at_10.append((nid, dn))
        else:
            verdict = "consistent"
            agree += 1
        print(f"{nid:>12}  {sn:>20}  {sw:>20}  {verdict}")

    print(f"\nsampled={len(rows)} consistent={agree} "
          f"missed_by_radius10={len(missed)} far_pano_at_radius10={len(far_at_10)}")
    if missed:
        print("radius=10 is too tight for:", missed)
    if far_at_10:
        print("distance check earns its keep for:", far_at_10)


if __name__ == "__main__":
    main()
