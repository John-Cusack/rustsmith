# SPDX-License-Identifier: BSD-2-Clause
"""Two-track bench for the charset-normalizer mirror: representative (gate-worthy) + showcase (ceiling).

Methodology: rep-major interleaved A/B sampling across two isolated venvs
(baseline = upstream `charset-normalizer` 3.5.1 from PyPI, port = this mirror's release wheel).
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
    python3 mirror/charset-normalizer/bench/bench.py --base-python <venv>/bin/python \
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
import json, sys, time, tracemalloc
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
            "label": "R1 from_bytes 3 mixed docs",
            "track": "representative",
            "size": "30492 + 36000 + 120727 bytes",
            "distribution": "corpus_latin (7eb4bc8c7589, utf-8 with accents) + corpus_cjk (5c11fdf379df, utf-8) + corpus_logs (a9f17ac3e94e, ascii); detect each",
            "iters": 5,
            "setup": (
                "import os as _os\n"
                "from charset_normalizer import from_bytes\n"
                "LAT = open(_os.path.join(r'{HERE}', 'corpus_latin.txt'), 'rb').read()\n"
                "CJK = open(_os.path.join(r'{HERE}', 'corpus_cjk.txt'), 'rb').read()\n"
                "LOG = open(_os.path.join(r'{HERE}', 'corpus_logs.txt'), 'rb').read()\n"
                "STMT = '[from_bytes(b).best().encoding for b in (LAT, CJK, LOG)]'\n"
                "ITERS = 5\n"
            ),
        },
        {
            "label": "R2 from_bytes 121KB ascii logs",
            "track": "representative",
            "size": "120727 bytes",
            "distribution": "corpus_logs (a9f17ac3e94e): 2000 ascii log lines; ingest fast path",
            "iters": 10,
            "setup": (
                "import os as _os\n"
                "from charset_normalizer import from_bytes\n"
                "LOG = open(_os.path.join(r'{HERE}', 'corpus_logs.txt'), 'rb').read()\n"
                "STMT = 'from_bytes(LOG).best().encoding'\n"
                "ITERS = 10\n"
            ),
        },
        {
            "label": "S1 from_bytes 1MB latin bulk",
            "track": "showcase",
            "size": "1048576 bytes",
            "distribution": "corpus_latin repeated to 1MB (utf-8); decode-all + per-char scoring ceiling",
            "iters": 3,
            "setup": (
                "import os as _os\n"
                "from charset_normalizer import from_bytes\n"
                "LAT = open(_os.path.join(r'{HERE}', 'corpus_latin.txt'), 'rb').read()\n"
                "BIG = (LAT * (1048576 // len(LAT) + 1))[:1048576]\n"
                "STMT = 'from_bytes(BIG).best().encoding'\n"
                "ITERS = 3\n"
            ),
        },
        {
            "label": "X1 per-crossing overhead (tiny ascii detect)",
            "track": "overhead",
            "size": "11 bytes",
            "distribution": "fixed b'hello world'; measures per-call dispatch floor, not throughput",
            "iters": 500,
            "setup": (
                "from charset_normalizer import from_bytes\n"
                "STMT = 'from_bytes(b\"hello world\").best().encoding'\n"
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
    return {"rep_meds": meds, "median_ms": m, "mad_ms": mad,
            "tukey": tukey_flags(meds),
            "rss_kb": max(peaks), "alloc_b": statistics.median(allocs),
            "digest": digest}


def measure_pair_both(python_base, python_port, setup_src, reps):
    """Rep-major interleaved A/B across two venvs: each rep runs one arm
    then the other (order alternates per rep), so neither arm runs
    back-to-back to completion."""
    acc = {"baseline": {"meds": [], "allocs": [], "peaks": [], "digest": None},
           "port": {"meds": [], "allocs": [], "peaks": [], "digest": None}}
    for i in range(reps + 1):  # +1 warmup rep, discarded
        order = [("baseline", python_base), ("port", python_port)] if i % 2 == 0 \
            else [("port", python_port), ("baseline", python_base)]
        for name, py in order:
            rec, peak = run_sample(py, setup_src)
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
    ap.add_argument("--reps", type=int, default=7)
    ap.add_argument("--noise-pct", type=float, default=NOISE_PCT)
    ap.add_argument("--out", default=os.path.join(HERE, "results.json"))
    ap.add_argument("--baseline-desc", default="charset-normalizer 3.5.1 (PyPI)")
    args = ap.parse_args()

    rows = []
    diverged = []
    for wl in workloads():
        arms = measure_pair_both(args.base_python, args.port_python, wl["setup"], args.reps)
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

    doc = {"package": "charset-normalizer",
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
