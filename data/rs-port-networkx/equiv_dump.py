"""Dump equiv pairs: stock nx vs networkx_rust on WORKLOAD graphs.

Each line: {"key": ..., "dist": [[node, dist]...], "path": [...] or None}.
dist lists are in iteration order (settlement order) so order mismatches
show as diffs. Run with --arm pure|rust.
"""

import argparse
import json
import sys

sys.path.insert(0, "/home/john/firstmate/data/rs-port-networkx/scratch")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arm", required=True)
    args = ap.parse_args()
    from bench_tier1 import build_graphs, sources_for
    if args.arm == "rust":
        sys.path.insert(0, "/home/john/firstmate/data/rs-port-networkx/port/python")
        import networkx_rust as m
        assert m.HAS_CORE
    else:
        import networkx as m
    import networkx as nx
    graphs = build_graphs()
    out = []
    for gi, (gid, G) in enumerate(graphs.items()):
        for s in sources_for(G, 3, 9000 + gi):
            if args.arm == "rust":
                d = m.single_source_dijkstra_path_length(G, s)
            else:
                d = nx.single_source_dijkstra_path_length(G, s)
            out.append({"key": f"{gid}/len/{s}",
                        "dist": [[str(k), v] for k, v in d.items()],
                        "path": None})
        srcs = sources_for(G, 4, 9100 + gi)
        for s, t in zip(srcs[:2], srcs[2:]):
            if args.arm == "rust":
                L, p = m.single_source_dijkstra(G, s, target=t)
            else:
                L, p = nx.single_source_dijkstra(G, s, target=t)
            out.append({"key": f"{gid}/pair/{s}/{t}",
                        "dist": [[str(t), L]], "path": [str(x) for x in p]})
        dd = (m if args.arm == "rust" else nx).multi_source_dijkstra_path_length(
            G, set(srcs[:2]))
        out.append({"key": f"{gid}/multi",
                    "dist": [[str(k), v] for k, v in dd.items()],
                    "path": None})
        pr, dd2 = (m if args.arm == "rust" else nx).dijkstra_predecessor_and_distance(
            G, srcs[0])
        out.append({"key": f"{gid}/pred/{srcs[0]}",
                    "dist": [[str(k), v] for k, v in dd2.items()],
                    "path": None,
                    "pred": [[str(k), sorted(map(str, v))]
                             for k, v in sorted(pr.items(), key=lambda kv: str(kv[0]))]})
    with open(f"/home/john/firstmate/data/rs-port-networkx/equiv_{args.arm}.jsonl",
              "w") as f:
        for row in out:
            f.write(json.dumps(row) + "\n")
    print(f"wrote {len(out)} rows equiv_{args.arm}.jsonl")


if __name__ == "__main__":
    main()
