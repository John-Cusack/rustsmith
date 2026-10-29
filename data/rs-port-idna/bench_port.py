"""Port bench: stock vs tier-1 vs port (rust) on the frozen WORKLOAD.md tracks.

Driver mode: for each (arm, track), spawn a fresh subprocess worker,
report wall (min of N) + peak RSS (child ru_maxrss).
Worker mode: --worker ARM --track T --iters N.
"""

import argparse
import os
import resource
import subprocess
import sys
import time

sys.path.insert(0, "/home/john/orca/workspaces/rustsmith/fm-rs-port-idna/data/rs-port-idna")
from bench_tier1 import gen, punycode_in

SCRATCH = "/home/john/firstmate/data/rs-port-idna/scratch"
ARMS = {
    "stock": f"{SCRATCH}/venv/bin/python",
    "tier1": f"{SCRATCH}/venv/bin/python",
    "port": f"{SCRATCH}/portvenv/bin/python",
}
PKG = {
    "stock": f"{SCRATCH}/venv/lib/python3.12/site-packages",
    "tier1": f"{SCRATCH}/t1pkg",
    "port": f"{SCRATCH}/idna-rust",
}


def worker(arm, track, iters):
    sys.path.insert(0, PKG[arm])
    import idna
    print("package: " + idna.__file__, flush=True)
    try:
        import idna._rust as _r
        print("rust: yes", flush=True)
    except ImportError:
        print("rust: no", flush=True)
    if track == "mixed":
        domains = gen(8000)
    elif track == "ascii":
        domains = gen(16000, 0.0, 0.0)
    elif track == "idn":
        domains = punycode_in(gen(4000, 0.95, 0.05))
    elif track == "x1-valid":
        domains = ["mail1.example.com"]
    elif track == "x1-idn":
        domains = ["münchen-1.de"]
    elif track == "x1-invalid":
        domains = ["bad..dots.com"]
    else:
        raise ValueError(track)
    idna.encode("warm.example.com", uts46=True)
    walls = []
    for _ in range(iters):
        ok = err = 0
        t0 = time.perf_counter()
        for d in domains:
            try:
                idna.encode(d, uts46=True)
                ok += 1
            except idna.IDNAError:
                err += 1
            try:
                idna.decode(d, uts46=True)
                ok += 1
            except (idna.IDNAError, UnicodeError):
                err += 1
        walls.append(time.perf_counter() - t0)
    print(f"RESULT best_s={min(walls):.6f} n={len(domains)} ok={ok} err={err}")
    for w in walls:
        print(f"  iter_s={w:.6f}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--worker", default=None)
    ap.add_argument("--track", default=None)
    ap.add_argument("--iters", type=int, default=7)
    args = ap.parse_args()
    if args.worker:
        return worker(args.worker, args.track, args.iters)
    import re as _re
    print("arm,track,best_s,rss_kb,ok,err")
    for arm in ["stock", "tier1", "port"]:
        for track in ["mixed", "ascii", "idn", "x1-valid", "x1-idn", "x1-invalid"]:
            env = dict(os.environ)
            if arm == "tier1":
                pass  # PKG insert happens in worker
            p = subprocess.run(
                [ARMS[arm], os.path.abspath(__file__), "--worker", arm,
                 "--track", track, "--iters", str(args.iters)],
                capture_output=True, text=True, env={**env, "PYTHONPATH": ""})
            m = _re.search(r"RESULT best_s=([\d.]+) n=\d+ ok=(\d+) err=(\d+)", p.stdout)
            rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
            if not m:
                print(f"{arm},{track},FAILED,,{p.stdout[-300:]}")
                print(p.stderr[-2000:], file=sys.stderr)
            else:
                print(f"{arm},{track},{m.group(1)},{rss},{m.group(2)},{m.group(3)}",
                      flush=True)


if __name__ == "__main__":
    main()
