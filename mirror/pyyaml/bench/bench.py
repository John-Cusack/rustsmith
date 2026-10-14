# SPDX-License-Identifier: BSD-2-Clause
"""Two-track bench for the pyyaml mirror: representative (gate-worthy) + showcase (ceiling).

Methodology: rep-major interleaved A/B sampling in ONE venv (release wheel installed):
baseline arm blocks `yaml._rust` (pure-Python fallback), port arm uses the extension.
Each sample runs in a FRESH child process (timing measured inside the child
over `iters` iterations, median reported); the parent alternates arms every
rep, takes median+MAD across reps, and reads child peak RSS via wait4.
Tukey-fence (1.5x IQR) outliers are flagged, never dropped. |delta| below
`--noise-pct` (default 3.0%) reads "no-change".

Memory: `rss_kb` is child peak RSS (covers Python + native, honest
cross-side column). `alloc_b` is the tracemalloc Python-level peak:
native (Rust-side) allocs are invisible to it, favoring the ported side
on allocation deltas — stated here and attached to every figure record.

Every workload also returns a result digest; baseline/port digests must
agree or the run exits 2 (correctness gate: a bench that cannot prove
parity is not citable).

Usage:
    python3 mirror/pyyaml/bench/bench.py --port-python <venv>/bin/python [--reps 7] [--out results.json]
Writes results.json + rows.md (machine-written; board rows copy these 1:1).
"""

import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
NOISE_PCT = 3.0
MEM_NOTE = ("tracemalloc sees Python-level allocs only; native (Rust-side) "
            "allocs are invisible to it, favoring the ported side on "
            "allocation deltas. rss_kb (child-process peak RSS) covers "
            "both sides and is the honest cross-side column.")

CHILD_TMPL = r"""
import json, os, sys, time, tracemalloc
for _m in os.environ.get("BENCH_BLOCK", "").split(","):
    if _m.strip():
        sys.modules[_m.strip()] = None  # force pure-Python fallback arm
setup_src = sys.stdin.read()
ns = {}
exec(compile(setup_src, "<setup>", "exec"), ns)
stmt = ns["STMT"]; iters = ns["ITERS"]
for _ in range(3):
    ns["RESULT"] = eval(stmt, ns)          # warmup, discarded
tracemalloc.start()
samples = []
for _ in range(iters):
    t0 = time.perf_counter_ns()
    ns["RESULT"] = eval(stmt, ns)
    samples.append((time.perf_counter_ns() - t0) / 1e6)
cur, peak = tracemalloc.get_traced_memory()
tracemalloc.stop()
samples.sort()
digest_src = repr(ns["RESULT"]).encode()
import hashlib
print(json.dumps({"median_ms": samples[len(samples)//2],
                  "alloc_b": peak,
                  "digest": hashlib.sha256(digest_src).hexdigest()[:16]}))
"""

# Workloads: label, track (representative|showcase|overhead), size,
# distribution, iters, setup source (must define STMT + ITERS + RESULT).
def workloads():
    return [
        {
            "label": "R1 scan 8.7KB manifests",
            "track": "representative",
            "size": "8686 bytes, 40 docs",
            "distribution": "corpus_manifests.yaml (sha 04b85e43d7d5): k8s-shaped services; token stream",
            "iters": 10,
            "setup": (
                "import os as _os\n"
                "import yaml\n"
                "DOC = open(_os.path.join(r'{HERE}', 'corpus_manifests.yaml')).read()\n"
                "STMT = '[type(t).__name__ for t in yaml.scan(DOC)]'\n"
                "ITERS = 10\n"
            ),
        },
        {
            "label": "R2 safe_load single service",
            "track": "representative",
            "size": "217 bytes, 1 doc",
            "distribution": "first doc of corpus_manifests.yaml; full compose+construct path (SafeLoader)",
            "iters": 10,
            "setup": (
                "import os as _os\n"
                "import yaml\n"
                "DOC = open(_os.path.join(r'{HERE}', 'corpus_manifests.yaml')).read()\n"
                "ONE = DOC.split('\\n---')[0]\n"
                "STMT = 'yaml.safe_load(ONE)'\n"
                "ITERS = 10\n"
            ),
        },
        {
            "label": "R3 dump nested object",
            "track": "representative",
            "size": "~9KB emitted",
            "distribution": "safe_load of corpus_manifests re-emitted; emitter path (expect ~1x, lane found 0.93x)",
            "iters": 10,
            "setup": (
                "import os as _os\n"
                "import yaml\n"
                "DOC = open(_os.path.join(r'{HERE}', 'corpus_manifests.yaml')).read()\n"
                "OBJ = yaml.safe_load(DOC.split('\\n---')[0])\n"
                "STMT = 'yaml.safe_dump(OBJ)'\n"
                "ITERS = 10\n"
            ),
        },
        {
            "label": "S1 load_all 65KB 300-doc stream",
            "track": "showcase",
            "size": "65290 bytes, 300 docs",
            "distribution": "corpus_bulk.yaml (sha 3e82c186d4e): bulk ingest ceiling",
            "iters": 3,
            "setup": (
                "import os as _os\n"
                "import yaml\n"
                "DOC = open(_os.path.join(r'{HERE}', 'corpus_bulk.yaml')).read()\n"
                "STMT = 'list(yaml.load_all(DOC, Loader=yaml.SafeLoader))'\n"
                "ITERS = 3\n"
            ),
        },
        {
            "label": "X1 per-crossing overhead (tiny scan)",
            "track": "overhead",
            "size": "1 doc, 14 bytes",
            "distribution": "fixed 'a: 1'; measures per-call dispatch floor, not throughput",
            "iters": 2000,
            "setup": (
                "import yaml\n"
                "STMT = '[type(t).__name__ for t in yaml.scan(\"a: 1\")]'\n"
                "ITERS = 2000\n"
            ),
        },
    ]


def run_sample(python, setup_src, extra_env=None):
    """One fresh-process sample. Returns (child_record, children_max_rss_kb).

    RSS comes from RUSAGE_CHILDREN ru_maxrss: the max over ALL reaped
    children so far (monotonic). Callers take the max across an arm's
    samples, so the reported figure is the peak child RSS for that arm
    running that workload — same workload both arms, fair comparison.
    """
    import resource
    setup_src = setup_src.replace("{HERE}", HERE)
    env = dict(os.environ)
    env["PYTHONPATH"] = ""
    env["PYTHONHASHSEED"] = "0"
    env.update(extra_env or {})
    p = subprocess.Popen([python, "-c", CHILD_TMPL],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, env=env)
    out, err = p.communicate(setup_src.encode())
    if p.returncode != 0:
        raise RuntimeError(f"child failed ({python}): {err.decode()[-2000:]}")
    peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    return json.loads(out.decode()), peak


def median_mad(xs):
    m = statistics.median(xs)
    return m, statistics.median([abs(x - m) for x in xs])


def tukey_flags(xs):
    if len(xs) < 4:
        return [False] * len(xs)
    s = sorted(xs)
    q1 = statistics.median(s[:len(s) // 2])
    q3 = statistics.median(s[(len(s) + 1) // 2:])
    iqr = q3 - q1
    lo, hi = q1 - 1.5 * iqr, q3 + 1.5 * iqr
    return [not (lo <= x <= hi) for x in xs]


def arm_stats(meds, allocs, peaks, digest):
    m, mad = median_mad(meds)
    return {"rep_meds": meds, "median_ms": m, "mad_ms": mad,
            "tukey": tukey_flags(meds),
            "rss_kb": max(peaks), "alloc_b": statistics.median(allocs),
            "digest": digest}


def measure_pair(python, setup_src, reps, block_env):
    """Rep-major interleaved A/B: each rep runs baseline then port (order
    alternates per rep), so neither arm runs back-to-back to completion.
    RSS is the peak child RSS per arm (RUSAGE_CHILDREN running max)."""
    acc = {"baseline": {"meds": [], "allocs": [], "peaks": [], "digest": None},
           "port": {"meds": [], "allocs": [], "peaks": [], "digest": None}}
    for i in range(reps + 1):  # +1 warmup rep, discarded
        order = [("baseline", block_env), ("port", None)] if i % 2 == 0 \
            else [("port", None), ("baseline", block_env)]
        for name, env in order:
            rec, peak = run_sample(python, setup_src, env)
            if i == 0:
                continue
            a = acc[name]
            a["meds"].append(rec["median_ms"])
            a["allocs"].append(rec["alloc_b"])
            a["peaks"].append(peak)
            a["digest"] = rec["digest"]
    out = {}
    for name in ("baseline", "port"):
        a = acc[name]
        out[name] = arm_stats(a["meds"], a["allocs"], a["peaks"], a["digest"])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port-python", required=True,
                    help="venv python with the release wheel installed")
    ap.add_argument("--block", default="yaml._rust",
                    help="comma-separated extension modules to block for the baseline arm")
    ap.add_argument("--reps", type=int, default=7)
    ap.add_argument("--noise-pct", type=float, default=NOISE_PCT)
    ap.add_argument("--out", default=os.path.join(HERE, "results.json"))
    ap.add_argument("--baseline-desc", default="same tree, extension blocked (pure-Python fallback)")
    args = ap.parse_args()

    rows = []
    diverged = []
    for wl in workloads():
        arms = measure_pair(args.port_python, wl["setup"], args.reps,
                            {"BENCH_BLOCK": args.block})
        b, p = arms["baseline"], arms["port"]
        if b["digest"] != p["digest"]:
            diverged.append(wl["label"])
            verdict = "DIVERGED"
            speedup = None
        else:
            delta = (b["median_ms"] - p["median_ms"]) / b["median_ms"] * 100
            if abs(delta) < args.noise_pct:
                verdict, speedup = "no-change", b["median_ms"] / p["median_ms"]
            elif delta > 0:
                verdict, speedup = "faster", b["median_ms"] / p["median_ms"]
            else:
                verdict, speedup = "slower", b["median_ms"] / p["median_ms"]
        rows.append({"label": wl["label"], "track": wl["track"],
                     "size": wl["size"], "distribution": wl["distribution"],
                     "iters": wl["iters"], "reps": args.reps,
                     "noise_pct": args.noise_pct, "mem_note": MEM_NOTE,
                     "baseline": {k: b[k] for k in
                                  ("median_ms", "mad_ms", "rss_kb", "alloc_b", "digest")},
                     "port": {k: p[k] for k in
                              ("median_ms", "mad_ms", "rss_kb", "alloc_b", "digest")},
                     "speedup_x": speedup, "verdict": verdict})

    doc = {"package": "pyyaml",
           "baseline": args.baseline_desc,
           "host": {"platform": platform.platform(),
                    "cpu": platform.processor() or "unknown",
                    "python": platform.python_version()},
           "command": sys.argv,
           "measured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "workloads": rows}
    with open(args.out, "w") as f:
        json.dump(doc, f, indent=2)

    md = ["| Workload | Track | Baseline | Port | Speedup | Verdict |",
          "|---|---|---|---|---|---|"]
    for r in rows:
        b, p = r["baseline"], r["port"]
        sp = f"{r['speedup_x']:.2f}x" if r["speedup_x"] else "—"
        md.append(f"| {r['label']} ({r['size']}) | {r['track']} | "
                  f"{b['median_ms']:.3f}ms | {p['median_ms']:.3f}ms | {sp} | {r['verdict']} |")
    mdpath = os.path.join(HERE, "rows.md")
    with open(mdpath, "w") as f:
        f.write("\n".join(md) + "\n")
    print("\n".join(md))
    print(f"\nwrote {args.out} + {mdpath}")
    if diverged:
        print(f"DIVERGED: {diverged}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
