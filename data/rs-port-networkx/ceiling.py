"""Ceiling analysis: split per-query time into export / loop / result-build.

For each graph: time (a) stock base full-expansion, (b) export build,
(c) T2 loop (cached export), (d) result-dict build from a Rust-shaped
output (list of (idx, dist) -> dict with original labels).
A whole-graph-crossing Rust kernel pays (b) in Python per call + Rust loop
+ (d); infinite-kernel-speed ceiling = base / (b + d).
Prints medians over REPS in fresh subprocesses (one graph per process).
"""

import argparse
import json
import subprocess
import sys
import time
from statistics import median

VPY = "/home/john/firstmate/data/rs-port-networkx/scratch/venv/bin/python"
HERE = "/home/john/firstmate/data/rs-port-networkx/scratch"


def one(gid, rep):
    import networkx as nx
    sys.path.insert(0, HERE)
    from bench_tier1 import build_graphs, sources_for
    from tier1_fast import DijkstraExport
    graphs = build_graphs()
    G = graphs[gid]
    srcs = sources_for(G, 6, 9000 + {"R1": 0, "R2": 1, "R5": 2, "GRID": 3}[gid])
    s = srcs[rep % len(srcs)]

    out = {}
    t0 = time.perf_counter()
    d = nx.single_source_dijkstra_path_length(G, s, weight="weight")
    out["base"] = time.perf_counter() - t0

    t0 = time.perf_counter()
    e = DijkstraExport(G, "weight")
    out["export"] = time.perf_counter() - t0

    t0 = time.perf_counter()
    d2 = e.multi_source_length([s])
    out["t2loop"] = time.perf_counter() - t0

    # Rust-shaped result handoff: flat index-dist arrays -> label dict
    nodes = list(G._adj.keys())
    idx = {v: i for i, v in enumerate(nodes)}
    flat = [(idx[v], x) for v, x in d2.items()]
    t0 = time.perf_counter()
    rebuilt = {nodes[i]: x for i, x in flat}
    out["rebuild"] = time.perf_counter() - t0
    assert rebuilt.keys() == d.keys()
    assert all(abs(rebuilt[k] - d[k]) < 1e-9 for k in d)
    out["nodes"] = len(nodes)
    out["adjentries"] = sum(len(v) for v in e.nb.values())
    print(json.dumps(out))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gid", default="R1")
    ap.add_argument("--rep", type=int, default=0)
    ap.add_argument("--driver", action="store_true")
    args = ap.parse_args()
    if not args.driver:
        one(args.gid, args.rep)
        return
    import os
    os.chdir(HERE)
    for gid in ["R1", "R2", "R5", "GRID"]:
        acc = {}
        for r in range(7):
            p = subprocess.run(
                [VPY, "ceiling.py", "--gid", gid, "--rep", str(r)],
                capture_output=True, text=True)
            if p.returncode != 0:
                print("FAIL", gid, p.stderr[-2000:], file=sys.stderr)
                sys.exit(1)
            j = json.loads(p.stdout.strip().splitlines()[-1])
            for k, v in j.items():
                acc.setdefault(k, []).append(v)
        med = {k: (median(v) if isinstance(v[0], float) else v[0])
               for k, v in acc.items()}
        b, x, t2, rb = med["base"], med["export"], med["t2loop"], med["rebuild"]
        # Rust per-call estimate: export (py) + t2loop/50 (generous: Rust heap
        # loop ~50x the tuple-walking Python loop) + rebuild; infinite-speed:
        ceil_inf = b / (x + rb)
        ceil_50 = b / (x + t2 / 50.0 + rb)
        print(f"{gid} n={med['nodes']} adj={med['adjentries']} "
              f"base={b*1e3:.2f}ms export={x*1e3:.2f}ms "
              f"t2loop={t2*1e3:.2f}ms rebuild={rb*1e3:.3f}ms "
              f"ceil_inf={ceil_inf:.2f}x ceil_50x={ceil_50:.2f}x")


if __name__ == "__main__":
    main()
