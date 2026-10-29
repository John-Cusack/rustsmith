"""Tier-1 pure-Python Dijkstra fast path (adjacency export + weight hoist).

Scope: non-multigraph, stock string weight attr (the representative path).
Callable weights and multigraphs fall back to stock networkx by construction.
Iteration order = G._adj insertion order, monotonic push counter preserved,
so distances AND paths AND tie-breaking order are identical to upstream.
"""

from heapq import heappop, heappush
from itertools import count, islice

__all__ = ["DijkstraExport", "is_fast_path"]


def is_fast_path(G, weight):
    return not G.is_multigraph() and isinstance(weight, str)


class DijkstraExport:
    """CSR-ish export: per node, a list of (neighbor, weight).

    Built once per (graph, weight-attr); reused across queries (export caching).
    Missing attr -> 1.0 (upstream default). Explicit None attr value -> hidden
    edge (matches data.get(weight, 1) returning None upstream).
    convert=True (T1/T2): float() at export. convert=False (T3): keep raw
    attr values, add at relax time.
    """

    def __init__(self, G, weight="weight", convert=True):
        self._nodes = list(G._adj.keys())
        adj = G._adj
        nb = {}
        if convert:
            for v, nbrs in adj.items():
                lst = [(u, float(e.get(weight, 1))) if e.get(weight, 1) is not None else (u, None)
                       for u, e in nbrs.items()]
                nb[v] = lst
        else:
            # T3: no float conversion, keep raw attr values; hoist attr key
            for v, nbrs in adj.items():
                lst = [(u, e.get(weight, 1)) for u, e in nbrs.items()]
                nb[v] = lst
        self.nb = nb
        self.convert = convert
        # staleness guard: edge count at export time
        self._m = G.number_of_edges()

    def check_fresh(self, G):
        if G.number_of_edges() != self._m:
            raise ValueError("stale export: graph mutated since export")

    def multi_source_length(self, sources, cutoff=None, target=None,
                            _pred=None, _paths=None):
        nb = self.nb
        dist = {}
        seen = {}
        c = count()
        fringe = []
        push = heappush
        pop = heappop
        for s in sources:
            seen[s] = 0.0
            push(fringe, (0.0, next(c), s))
        nsrc = len(seen)
        pred_dict = _pred if _paths is None or _pred is not None else {}
        while fringe:
            dist_v, _, v = pop(fringe)
            if v in dist:
                continue
            dist[v] = dist_v
            if v == target:
                break
            for u, w in nb[v]:
                if w is None:
                    continue
                vu = dist_v + w
                if cutoff is not None and vu > cutoff:
                    continue
                if u in dist:
                    ud = dist[u]
                    if vu < ud:
                        raise ValueError("Contradictory paths found:",
                                         "negative weights?")
                    elif _pred is not None and vu == ud:
                        pred_dict[u].append(v)
                elif u not in seen or vu < seen[u]:
                    seen[u] = vu
                    push(fringe, (vu, next(c), u))
                    if pred_dict is not None:
                        pred_dict[u] = [v]
                elif _pred is not None and vu == seen[u]:
                    pred_dict[u].append(v)
        if _paths is not None:
            if target is None:
                for v in islice(dist, nsrc, None):
                    _paths[v] = _paths[pred_dict[v][0]] + [v]
            else:
                path = _paths[target] = [target]
                while (cp := pred_dict.get(path[-1])) is not None:
                    path.append(cp[0])
                path.reverse()
        return dist
