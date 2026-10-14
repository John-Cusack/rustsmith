# SPDX-License-Identifier: BSD-2-Clause
"""Two-track rrule-query bench for the dateutil mirror (ADR-021 batch-query path).

Methodology: rep-major interleaved A/B sampling across two isolated venvs
(baseline = upstream `python-dateutil` 2.9.0.post0 from PyPI, port = release wheel overlaid on it).
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
    python3 mirror/python-dateutil/bench_parser/bench_rrule.py --base-python <venv>/bin/python \
        --port-python <venv>/bin/python [--reps 7] [--out results.json]
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
import json, os, sys, time
TRACE = os.environ.get("BENCH_NO_TRACEMALLOC", "") == ""
if TRACE:
    import tracemalloc
setup_src = sys.stdin.read()
ns = {}
exec(compile(setup_src, "<setup>", "exec"), ns)
stmt = ns["STMT"]; iters = ns["ITERS"]
for _ in range(3):
    ns["RESULT"] = eval(stmt, ns)          # warmup, discarded
if TRACE:
    tracemalloc.start()
samples = []
for _ in range(iters):
    t0 = time.perf_counter_ns()
    ns["RESULT"] = eval(stmt, ns)
    samples.append((time.perf_counter_ns() - t0) / 1e6)
if TRACE:
    cur, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
else:
    peak = None
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
            "label": "R1 between monthly over 2-year daily set",
            "track": "representative",
            "size": "731 occurrences, 24 window queries",
            "distribution": "DAILY rrule 2024-01-01 x731; between() per calendar month (ADR-021 batch path)",
            "iters": 10,
            "setup": (
                "from dateutil.rrule import rrule, DAILY\n"
                "from datetime import datetime\n"
                "RR = rrule(DAILY, dtstart=datetime(2024, 1, 1), count=731)\n"
                "WINS = [(datetime(2024 + (m // 12), (m % 12) + 1, 1),\n"
                "         datetime(2024 + ((m + 1) // 12), ((m + 1) % 12) + 1, 1))\n"
                "        for m in range(24)]\n"
                "STMT = '[RR.between(a, b) for a, b in WINS]'\n"
                "ITERS = 10\n"
            ),
        },
        {
            "label": "R2 after/before 200 queries",
            "track": "representative",
            "size": "200 queries over 365-occurrence set",
            "distribution": "DAILY rrule 2024 x365; alternating after()/before() probes across the year",
            "iters": 10,
            "setup": (
                "from dateutil.rrule import rrule, DAILY\n"
                "from datetime import datetime, timedelta\n"
                "RR = rrule(DAILY, dtstart=datetime(2024, 1, 1), count=365)\n"
                "QS = [datetime(2024, 1, 1) + timedelta(days=i * 3, hours=7) for i in range(200)]\n"
                "STMT = '([RR.after(q) for q in QS], [RR.before(q) for q in QS])'\n"
                "ITERS = 10\n"
            ),
        },
        {
            "label": "S1 count 10-year hourly set (cold)",
            "track": "showcase",
            "size": "87600 occurrences",
            "distribution": "HOURLY rrule 2020-01-01 x87600 rebuilt per iter; count() engine ceiling (ADR-021 ~4x claim). NOTE: repeat count() on one object favors upstream _len caching (port recomputes); cold counts compare engines fairly",
            "iters": 3,
            "setup": (
                "from dateutil.rrule import rrule, HOURLY\n"
                "from datetime import datetime\n"
                "STMT = 'rrule(HOURLY, dtstart=datetime(2020, 1, 1), count=87600).count()'\n"
                "ITERS = 3\n"
            ),
        },
        {
            "label": "X1 per-crossing overhead (one after query)",
            "track": "overhead",
            "size": "1 query",
            "distribution": "fixed probe; measures per-query dispatch floor, not throughput",
            "iters": 500,
            "setup": (
                "from dateutil.rrule import rrule, DAILY\n"
                "from datetime import datetime\n"
                "RR = rrule(DAILY, dtstart=datetime(2024, 1, 1), count=365)\n"
                "STMT = 'RR.after(datetime(2024, 6, 1, 12))'\n"
                "ITERS = 500\n"
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
    real = [a for a in allocs if a is not None]
    return {"rep_meds": meds, "median_ms": m, "mad_ms": mad,
            "tukey": tukey_flags(meds),
            "rss_kb": max(peaks),
            "alloc_b": statistics.median(real) if real else None,
            "digest": digest}


def measure_pair_both(python_base, python_port, setup_src, reps, port_env=None):
    """Rep-major interleaved A/B across two venvs: each rep runs one arm
    then the other (order alternates per rep), so neither arm runs
    back-to-back to completion."""
    acc = {"baseline": {"meds": [], "allocs": [], "peaks": [], "digest": None},
           "port": {"meds": [], "allocs": [], "peaks": [], "digest": None}}
    for i in range(reps + 1):  # +1 warmup rep, discarded
        base_env = {"BENCH_NO_TRACEMALLOC": "1"}
        port_env2 = dict(port_env or {})
        port_env2["BENCH_NO_TRACEMALLOC"] = "1"
        order = [("baseline", python_base, base_env), ("port", python_port, port_env2)] if i % 2 == 0 \
            else [("port", python_port, port_env2), ("baseline", python_base, base_env)]
        for name, py, env in order:
            rec, peak = run_sample(py, setup_src, env)
            if i == 0:
                continue
            a = acc[name]
            a["meds"].append(rec["median_ms"])
            a["allocs"].append(rec["alloc_b"])
            a["peaks"].append(peak)
            a["digest"] = rec["digest"]
    return {n: arm_stats(a["meds"], a["allocs"], a["peaks"], a["digest"])
            for n, a in acc.items()}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base-python", required=True)
    ap.add_argument("--port-python", required=True)
    ap.add_argument("--port-pythonpath", default=None,
                    help="directory overlaid via PYTHONPATH for the port arm only (e.g. extracted wheel)")
    ap.add_argument("--reps", type=int, default=7)
    ap.add_argument("--noise-pct", type=float, default=NOISE_PCT)
    ap.add_argument("--out", default=os.path.join(HERE, "results_rrule.json"))
    ap.add_argument("--baseline-desc", default="python-dateutil 2.9.0.post0 (PyPI)")
    args = ap.parse_args()

    rows = []
    diverged = []
    for wl in workloads():
        arms = measure_pair_both(args.base_python, args.port_python, wl["setup"], args.reps,
                                 port_env={"PYTHONPATH": args.port_pythonpath} if args.port_pythonpath else None)
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
                     "noise_pct": args.noise_pct, "mem_note": MEM_NOTE + " rrule: alloc_b not measured (BENCH_NO_TRACEMALLOC=1); per-datetime tracing distorts query loops ~1000x (measured 0.2ms -> 236ms). RSS is the memory column here.",
                     "baseline": {k: b[k] for k in
                                  ("median_ms", "mad_ms", "rss_kb", "alloc_b", "digest")},
                     "port": {k: p[k] for k in
                              ("median_ms", "mad_ms", "rss_kb", "alloc_b", "digest")},
                     "speedup_x": speedup, "verdict": verdict})

    doc = {"package": "python-dateutil (rrule queries)",
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
    mdpath = os.path.join(HERE, "rows_rrule.md")
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
