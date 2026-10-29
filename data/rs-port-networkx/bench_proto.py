"""Prototype ceiling bench: base vs t2 (cached export) vs proto (Rust dict-walk+CSR+heap).

Usage: bench_proto.py --gid R1 --which base|t2|proto   (single query set)
       bench_proto.py --driver                        (medians, fresh procs)
"""

import argparse
import json
import subprocess
import sys
import time
from statistics import median

VPY = "/home/john/firstmate/data/rs-port-networkx/scratch/venv/bin/python"
HERE = "/home/john/firstmate/data/rs-port-networkx/scratch"


def one(gid, which, rep, multi=False):
    import networkx as nx
    sys.path.insert(0, HERE)
    from bench_tier1 import build_graphs, sources_for
    graphs = build_graphs()
    G = graphs[gid]
    gi = {"R1": 0, "R2": 1, "R5": 2, "GRID": 3}[gid]
    srcs = sources_for(G, 6, 9000 + gi)
    s = srcs[rep % len(srcs)]
    srclist = sources_for(G, 8, 9200 + gi) if multi else [s]
    if which == "proto":
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "proto_dijkstra", HERE + "/proto_dijkstra.so")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        t0 = time.perf_counter()
        if multi:
            d = mod.dijkstra_length(G._adj, srclist, "weight")
        else:
            d = mod.dijkstra_length(G._adj, [s], "weight")
        wall = time.perf_counter() - t0
    elif which == "t2":
        from tier1_fast import DijkstraExport
        e = DijkstraExport(G, "weight")
        t0 = time.perf_counter()
        # include amortized export? No: steady-state cached; export built above
        # (measures loop only — generous to tier-1; proto pays its walk inline)
        d = e.multi_source_length(srclist)
        wall = time.perf_counter() - t0
    else:
        t0 = time.perf_counter()
        if multi:
            d = nx.multi_source_dijkstra_path_length(
                G, set(srclist), weight="weight")
        else:
            d = nx.single_source_dijkstra_path_length(G, s, weight="weight")
        wall = time.perf_counter() - t0
    import resource
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024.0
    print(json.dumps({"which": which, "gid": gid, "multi": multi,
                      "wall": wall, "rss_mib": rss,
                      "checksum": [round(sum(d.values()), 6), len(d)]}))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gid", default="R1")
    ap.add_argument("--which", default="base")
    ap.add_argument("--rep", type=int, default=0)
    ap.add_argument("--multi", action="store_true")
    ap.add_argument("--driver", action="store_true")
    args = ap.parse_args()
    if not args.driver:
        one(args.gid, args.which, args.rep, args.multi)
        return
    import os
    os.chdir(HERE)
    out = {}
    for gid in ["R1", "R2", "R5", "GRID"]:
        for which in ["base", "t2", "proto"]:
            walls, rsss, cks = [], [], []
            for r in range(7):
                cmd = [VPY, "bench_proto.py", "--gid", gid,
                       "--which", which, "--rep", str(r)]
                if args.multi:
                    cmd.append("--multi")
                p = subprocess.run(cmd, capture_output=True, text=True)
                if p.returncode != 0:
                    print("FAIL", gid, which, p.stderr[-2000:],
                          file=sys.stderr)
                    sys.exit(1)
                j = json.loads(p.stdout.strip().splitlines()[-1])
                walls.append(j["wall"])
                rsss.append(j["rss_mib"])
                cks.append(tuple(j["checksum"]))
            out[(gid, which)] = (median(walls), median(rsss), cks[0])
            print(f"{gid} {which:5s} wall={median(walls)*1e3:.3f}ms "
                  f"rss={median(rsss):.1f}MiB checksum={cks[0]} "
                  f"stable={len(set(cks)) == 1}", flush=True)
    print("\n## agreement per graph:", all(
        len({out[(g, w)][2] for w in ["base", "t2", "proto"]}) == 1
        for g in ["R1", "R2", "R5", "GRID"]))
    print("## speedup vs base / vs t2 (medians):")
    for gid in ["R1", "R2", "R5", "GRID"]:
        b, t, p = out[(gid, "base")][0], out[(gid, "t2")][0], out[(gid, "proto")][0]
        print(f"{gid}: proto_vs_base={b / p:.2f}x proto_vs_t2={t / p:.2f}x")


if __name__ == "__main__":
    main()
