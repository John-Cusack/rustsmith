#!/usr/bin/env python3
"""Generate data/elmer-lanes/lanes.json: 8-lane partition of remaining Elmer units.

Source: frozen recon dag.json (elmer-work/recon) — 3025 units, 61 out_of_scope.
Applies T001 vendored exclusion (mathlibs/, umfpack/) and removes the done
matc/ slice (26 units, T005-T007 green: 22 graded + 4 skipped-as-helpers).
Remaining 1157 units -> 8 lanes, subsystem-pure, balanced by cost model.

Cost model (est_min per unit; ALL lanes est_basis=estimate — only matc-class
was ever measured at ~270 s/unit ~= 5 min, plan.md wall-time math):
  interactive (Qt GUI, unmeasured build weight + headless risk): 10 flat
  foreign-shape Fortran (core-class analog ~480 s, plan.md): 8
  C/C++/Python elsewhere (leaf analog): 5

Rerun: python3 data/elmer-lanes/generate.py --dag /path/to/dag.json
  (default dag path is the frozen recon used for this partition).
"""
import argparse
import json
import sys

DEFAULT_DAG = "/home/john/runs/elmer-work/recon/dag.json"

LANE_RULES = [
    # (lane_id, shape, matcher on path, rate override)
    ("lane-leaf", "matc-like", lambda p: p.startswith("meshgen2d/"), None),
    ("lane-grid-io", "io-boundary",
     lambda p: p.split("/")[0] in ("elmergrid", "fhutiter", "ElmerWorkflows", "misc"), None),
    ("lane-fem-a", "foreign-shape",
     lambda p: p.startswith("fem/") and not p.startswith("fem/src/modules/"), None),
    ("lane-fem-b", "foreign-shape",
     lambda p: p.startswith("fem/src/modules/"), None),
    ("lane-ice", "foreign-shape", lambda p: p.startswith("elmerice/"), None),
    # GUI lanes assigned after sort-split of netgen (see main).
]

RATES = {"c": 5, "cxx": 5, "fortran": 8, "python": 5}
INTERACTIVE_RATE = 10


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dag", default=DEFAULT_DAG)
    ap.add_argument("--out", default="data/elmer-lanes/lanes.json")
    args = ap.parse_args()

    dag = json.load(open(args.dag))
    oos = set(dag["out_of_scope"])
    units = []
    excluded = {"out_of_scope": 0, "vendored": 0, "done_matc": 0}
    for u in dag["units"]:
        uid = u["id"]
        if uid in oos:
            excluded["out_of_scope"] += 1
            continue
        lang, path = uid.split(":", 1)
        if path.startswith("mathlibs/") or path.startswith("umfpack/"):
            excluded["vendored"] += 1
            continue
        if path.startswith("matc/"):
            excluded["done_matc"] += 1
            continue
        units.append((uid, lang, path))

    lanes = {lid: {"id": lid, "shape": shape, "units": []}
             for lid, shape, _, _ in LANE_RULES}
    gui_netgen, gui_shell = [], []
    unassigned = []
    for uid, lang, path in units:
        placed = False
        for lid, _, match, _ in LANE_RULES:
            if match(path):
                lanes[lid]["units"].append((uid, lang))
                placed = True
                break
        if placed:
            continue
        if path.startswith("ElmerGUI/netgen/"):
            gui_netgen.append((uid, lang))
        elif path.split("/")[0] in ("ElmerGUI", "ElmerGUIlogger", "ElmerGUItester"):
            gui_shell.append((uid, lang))
        else:
            unassigned.append(uid)
    if unassigned:
        sys.exit(f"unassigned units: {unassigned}")

    gui_netgen.sort()
    half = (len(gui_netgen) + 1) // 2
    lanes["lane-gui-a"] = {"id": "lane-gui-a", "shape": "interactive",
                           "units": gui_netgen[:half]}
    lanes["lane-gui-b"] = {"id": "lane-gui-b", "shape": "interactive",
                           "units": gui_netgen[half:]}
    lanes["lane-gui-c"] = {"id": "lane-gui-c", "shape": "interactive",
                           "units": sorted(gui_shell)}

    order = ["lane-leaf", "lane-grid-io", "lane-fem-a", "lane-fem-b",
             "lane-ice", "lane-gui-a", "lane-gui-b", "lane-gui-c"]
    out_lanes = []
    for lid in order:
        lane = lanes[lid]
        rows = []
        total = 0
        for uid, lang in sorted(lane["units"]):
            est = INTERACTIVE_RATE if lane["shape"] == "interactive" else RATES[lang]
            total += est
            rows.append({"id": uid, "lang": lang, "est_min": est})
        out_lanes.append({"id": lid, "shape": lane["shape"], "units": rows,
                          "n_units": len(rows), "est_total_min": total,
                          "est_basis": "estimate", "bench_sample_unit": None})
    doc = {
        "generated_by": "data/elmer-lanes/generate.py",
        "inventory": {"frozen_units": len(dag["units"]), **excluded,
                      "partitioned": sum(l["n_units"] for l in out_lanes)},
        "done_matc_note": ("26 matc/ units done in T005-T007 "
                           "(22 graded green + 4 skipped-as-helpers)"),
        "cost_model": {
            "anchor_measured": "matc-class ~270 s/unit ~= 5 min (plan.md wall-time math)",
            "rates": {**RATES, "interactive_flat": INTERACTIVE_RATE},
            "basis": ("all per-unit numbers are estimates anchored on the "
                      "matc measurement; foreign-shape 8 min from core-class "
                      "~480 s; interactive 10 min unmeasured"),
        },
        "bench_sampling": ("showcase benches sampled once per lane "
                           "(bench_sample_unit, null until dispatch), never per unit; "
                           "<=3 concurrent runs during measurement (ADR-045)"),
        "lanes": out_lanes,
    }
    assert doc["inventory"]["partitioned"] == 1157, doc["inventory"]
    json.dump(doc, open(args.out, "w"), indent=1)
    for l in out_lanes:
        print(f'{l["id"]:14s} {l["shape"]:14s} n={l["n_units"]:4d} est={l["est_total_min"]:5d} min')


if __name__ == "__main__":
    main()
