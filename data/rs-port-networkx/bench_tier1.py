"""Tier-1 bench: baseline (stock nx) vs T1 (export per query) vs T2 (cached export).

Usage: bench_tier1.py --arm base|t1|t2 --track rep|path|multi|target|x1
Prints one JSON line: {arm, track, wall, rss_mib, checksum}.
Driver: --driver runs every (arm, track) in a fresh subprocess x REPS,
reports median walls + checksums + tier-1 gain table.
"""

import argparse
import json
import random
import subprocess
import sys
import time
from statistics import median

VPY = "/home/john/firstmate/data/rs-port-networkx/scratch/venv/bin/python"
HERE = "/home/john/firstmate/data/rs-port-networkx/scratch"


def build_graphs():
    import networkx as nx
    graphs = {}

    def ring(n, seed, extra_random):
        rng = random.Random(seed)
        G = nx.Graph()
        G.add_nodes_from(range(n))
        for i in range(n):
            G.add_edge(i, (i + 1) % n, weight=1.0 + (i % 9))
            G.add_edge(i, (i * 37 + 11) % n, weight=1.0 + ((i * 13) % 17))
            G.add_edge(i, (i * 101 + 7) % n, weight=1.0 + ((i * 29) % 23))
            if extra_random and rng.random() < 0.3:
                G.add_edge(i, rng.randrange(n),
                           weight=1.0 + rng.random() * 9)
        return G

    graphs["R1"] = ring(1000, 20261028, True)
    graphs["R2"] = ring(2500, 20261029, True)
    graphs["R5"] = ring(5000, 20261030, False)
    rng = random.Random(20261031)
    GRID = nx.grid_2d_graph(50, 50)
    for u, v in GRID.edges():
        GRID.edges[u, v]["weight"] = 1.0 + rng.random() * 9
    graphs["GRID"] = GRID
    return graphs


def sources_for(G, k, seed):
    rng = random.Random(seed)
    nodes = list(G.nodes())
    return [nodes[rng.randrange(len(nodes))] for _ in range(k)]


def run_track(arm, track):
    import networkx as nx
    sys.path.insert(0, HERE)
    graphs = build_graphs()
    exp = {}
    if arm in ("t1", "t2"):
        from tier1_fast import DijkstraExport
        if arm == "t2":
            for gid, G in graphs.items():
                exp[gid] = DijkstraExport(G, "weight")

    def length(G, gid, s, cutoff=None, target=None):
        if arm == "base":
            if target is not None:
                L = nx.dijkstra_path_length(G, s, target, weight="weight")
                return {target: L}
            return nx.single_source_dijkstra_path_length(
                G, s, cutoff=cutoff, weight="weight")
        from tier1_fast import DijkstraExport
        e = exp.get(gid) if arm == "t2" else (DijkstraExport(G, "weight", convert=False) if arm == "t3" else DijkstraExport(G, "weight"))
        d = e.multi_source_length([s], cutoff=cutoff, target=target)
        if target is not None:
            return {target: d[target]} if target in d else {}
        return d

    def path_pair(G, gid, s, t):
        if arm == "base":
            L, p = nx.single_source_dijkstra(G, s, target=t, weight="weight")
            return p, L
        from tier1_fast import DijkstraExport
        e = exp.get(gid) if arm == "t2" else (DijkstraExport(G, "weight", convert=False) if arm == "t3" else DijkstraExport(G, "weight"))
        pred, paths = {}, {}
        d = e.multi_source_length([s], target=t, _pred=pred, _paths=paths)
        return paths[t], d[t]

    def multi(G, gid, srcs):
        if arm == "base":
            return nx.multi_source_dijkstra_path_length(
                G, set(srcs), weight="weight")
        from tier1_fast import DijkstraExport
        e = exp.get(gid) if arm == "t2" else (DijkstraExport(G, "weight", convert=False) if arm == "t3" else DijkstraExport(G, "weight"))
        return e.multi_source_length(srcs)

    dist_sum = 0.0
    reached = 0
    plen_sum = 0
    t0 = time.perf_counter()
    if track == "rep":
        for i, (gid, G) in enumerate(graphs.items()):
            for s in sources_for(G, 6, 9000 + i):
                d = length(G, gid, s)
                dist_sum += sum(d.values())
                reached += len(d)
    elif track == "path":
        for i, (gid, G) in enumerate(graphs.items()):
            srcs = sources_for(G, 8, 9100 + i)
            for s, t in zip(srcs[:4], srcs[4:]):
                p, L = path_pair(G, gid, s, t)
                dist_sum += L
                plen_sum += len(p)
                reached += 1
    elif track == "multi":
        for i, (gid, G) in enumerate(graphs.items()):
            d = multi(G, gid, sources_for(G, 8, 9200 + i))
            dist_sum += sum(d.values())
            reached += len(d)
    elif track == "target":
        for i, (gid, G) in enumerate(graphs.items()):
            srcs = sources_for(G, 8, 9300 + i)
            for s, t in zip(srcs[:4], srcs[4:]):
                d = length(G, gid, s, target=t)
                dist_sum += sum(d.values())
                reached += len(d)
    elif track == "x1":
        G = graphs["R1"]
        srcs = sources_for(G, 2, 9400)
        p, L = path_pair(G, "R1", srcs[0], srcs[1])
        dist_sum += L
        plen_sum += len(p)
        reached += 1
    wall = time.perf_counter() - t0
    import resource
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024.0
    print(json.dumps({"arm": arm, "track": track, "wall": wall,
                      "rss_mib": rss,
                      "checksum": [round(dist_sum, 6), reached, plen_sum]}))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arm", default="base")
    ap.add_argument("--track", default="rep")
    ap.add_argument("--driver", action="store_true")
    args = ap.parse_args()
    if not args.driver:
        run_track(args.arm, args.track)
        return
    import os
    os.chdir(HERE)
    ARMS = ["base", "t1", "t2", "t3"]
    TRACKS = ["rep", "path", "multi", "target", "x1"]
    REPS = {"rep": 5, "path": 5, "multi": 5, "target": 5, "x1": 9}
    rows = {}
    for track in TRACKS:
        for arm in ARMS:
            walls, rsss, cks = [], [], []
            for r in range(REPS[track]):
                out = subprocess.run(
                    [VPY, "bench_tier1.py", "--arm", arm,
                     "--track", track], capture_output=True, text=True)
                if out.returncode != 0:
                    print("FAIL", arm, track, out.stderr[-3000:],
                          file=sys.stderr)
                    sys.exit(1)
                j = json.loads(out.stdout.strip().splitlines()[-1])
                walls.append(j["wall"])
                rsss.append(j["rss_mib"])
                cks.append(tuple(j["checksum"]))
            rows[(arm, track)] = (median(walls), median(rsss), cks[0],
                                  len(set(cks)) == 1)
            print(f"{arm:4s} {track:6s} wall={median(walls):.4f}s "
                  f"rss={median(rsss):.1f}MiB checksum={cks[0]} "
                  f"stable={len(set(cks)) == 1}", flush=True)
    # cross-arm checksum agreement per track
    for track in TRACKS:
        cks = {rows[(a, track)][2] for a in ARMS}
        print(f"AGREE {track}: {len(cks) == 1} {cks}")
    print("\n## tier-1 gains (base/t1, base/t2, wall medians)")
    for track in TRACKS:
        b = rows[("base", track)][0]
        print(f"{track:6s} base={b:.4f}s t1={b / rows[('t1', track)][0]:.2f}x "
              f"t2={b / rows[('t2', track)][0]:.2f}x t3={b / rows[('t3', track)][0]:.2f}x")


if __name__ == "__main__":
    main()
