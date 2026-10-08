# SPDX-License-Identifier: Apache-2.0
"""Two-track bench + accept/reject parity harness for the dateutil parse port.

Report rs-dateutil-fuzzy section 5 acceptance:
- Track A (gate, representative): mixed bulk mirroring log ingest --
  80% clean lines via strict-try/fuzzy-fallback, 20% noisy lines via
  fuzzy=True -- plus a 100%-clean control. Pass bar: zero accept/reject
  divergence vs the pure-Python baseline, mean speedup on the mix with
  the control not regressed.
- Track B (showcase, ceiling): adversarial fuzzy-only workload --
  long junk-word lines (maximizes the 45-lower/parse chain), dot-runs
  (tokenizer pain), fuzzy_with_tokens (skip-recombination path).
- Tier C (edge cases): tokenizer-hostile one-liners (dot-runs, stray
  digits, lone zones/meridiems) that stay fuzzy-parseable; bench-only,
  outside the parity contracts.
- Tier D (large-N): 2000-line log-ingest bulk mirroring production
  volume; per-row iteration override keeps runtime bounded.

Methodology (all numbers citable):
- ``--reps`` x ``--iters`` x corpus, **median reported** with MAD spread.
  Tukey-fence (1.5 x IQR) outliers are flagged in the table and JSON but
  never dropped.
- Explicit noise threshold (``--noise-pct``, default 3.0%): |delta| below
  it reads "no change". Baselines compared with ``--baseline`` get a
  faster/slower/no-change verdict per row.
- Rows interleave at rep granularity (direction alternates each rep) with
  ``--warmup`` reps executed and discarded, so no workload runs
  back-to-back to completion.
- Every row emits a full figure record
  ``{median, spread, reps, iters, host, commit, command, profile}`` in the human
  table and JSON; bare point estimates are never the output.
- Release-profile gate (re-audit V15): bench mode refuses to time a
  non-release extension build (exit 3) unless `--allow-debug` stamps the
  records non-citable; every record carries `profile` + `debug_allowed`.
- Memory: ``peak_memory()`` runs each row in a fresh child process and
  reports per-row peak RSS (covers both sides: Python + native) and the
  tracemalloc allocation peak. Blind spot, stated honestly: tracemalloc
  sees Python-level allocs only; Rust-side (or any native) allocs are
  invisible to it, which favors the ported side on allocation deltas.
  RSS is the honest cross-side column.

Repro (frozen corpora, manifest hashes in MANIFEST.sha256):
  python bench_parser/bench.py --reps 7 --iters 2000 --warmup 2
  python bench_parser/bench.py --mode parity
  python bench_parser/bench.py --baseline before.json --out after.json

``--mode parity`` writes one JSON record per line (strict/fuzzy accept
flag + result repr) for before/after diffing; exit nonzero on any
corpus-contract violation. A fixed ``default=datetime(2000, 1, 1)``
keeps runs deterministic (convertyear only touches 2-digit years).
"""
import argparse
import json
import platform
import subprocess
import sys
import time
from datetime import datetime

from dateutil.parser import parse
from dateutil.parser import isoparse

HERE = __file__.rsplit("/", 1)[0]
DEFAULT = datetime(2000, 1, 1)

# Explicit noise threshold (%): A/B deltas smaller than this read
# "no change". Override with --noise-pct; recorded per figure record.
NOISE_PCT = 3.0

# tracemalloc blind-spot disclosure, attached to every figure record so a
# quoted memory column can never silently drop it.
MEM_NOTE = ("tracemalloc sees Python-level allocs only; native (Rust-side) "
            "allocs are invisible to it, favoring the ported side on "
            "allocation deltas. rss_kb (child-process peak RSS) covers "
            "both sides and is the honest cross-side column.")


def build_profile():
    """Release-profile probe (re-audit V15/C13): what was the measured
    extension built as?

    Returns "release" (Rust extension, optimized), "debug" (Rust extension,
    unoptimized), "unknown" (extension present but pre-probe -- rebuild with
    maturin -r/--release), or "pure-python" (no extension at all: the
    upstream baseline side, nothing Rust to mis-measure).
    """
    try:
        from dateutil import _dateutil
    except ImportError:
        return "pure-python"
    return getattr(_dateutil, "__build_profile__", "unknown")


def load(name):
    lines = []
    with open("%s/%s" % (HERE, name)) as f:
        for row in f:
            row = row.rstrip("\n")
            if row and not row.startswith("#"):
                lines.append(row)
    return lines


CLEAN = load("corpus_clean.txt")
NOISY = load("corpus_noisy.txt")
EDGE = load("corpus_edge.txt")
LARGE = load("corpus_large.txt")
ISO = ["2024-01-15T10:30:00", "2003-09-25T10:49:41+03:00"]
MIX = CLEAN[:8] + NOISY[:2]


def attempt(s, **kw):
    try:
        return ("OK", repr(parse(s, default=DEFAULT, **kw)))
    except Exception as e:  # noqa: BLE001 - accept/reject pin needs all
        return ("RAISE", "%s:%s" % (type(e).__name__, e))


def check_contracts():
    errors = []
    for s in CLEAN:
        st, _ = attempt(s)
        if st != "OK":
            errors.append("clean strict-FAILED: %r" % s)
    for s in NOISY:
        st, _ = attempt(s)
        if st != "RAISE":
            errors.append("noisy strict-FAILED (parsed!): %r" % s)
        st, _ = attempt(s, fuzzy=True)
        if st != "OK":
            errors.append("noisy fuzzy-FAILED: %r" % s)
    return errors


def _strict(s):
    return parse(s, default=DEFAULT)


def _fuzzy(s):
    return parse(s, default=DEFAULT, fuzzy=True)


def _fuzzy_tokens(s):
    return parse(s, default=DEFAULT, fuzzy_with_tokens=True)


def strict_fallback(s):
    try:
        parse(s, default=DEFAULT)
    except ValueError:
        parse(s, default=DEFAULT, fuzzy=True)


def _isoparse(s):
    return isoparse(s)


FUNCS = {
    "strict": _strict,
    "fuzzy": _fuzzy,
    "fuzzy_tokens": _fuzzy_tokens,
    "fallback": strict_fallback,
    "isoparse": _isoparse,
}

CORPORA = {
    "clean": CLEAN,
    "noisy": NOISY,
    "edge": EDGE,
    "large": LARGE,
    "iso": ISO,
    "mix": MIX,
}

# (row name, FUNCS key, CORPORA key, iters override or None, mem-iters)
# Large-N runs far fewer iters: 2000 lines x 2000 iters would measure the
# timer, not the workload.
ROWS = [
    ("A-control clean strict", "strict", "clean", None, 20),
    ("A-mix strict-try/fuzzy-fallback", "fallback", "mix", None, 20),
    ("A-noisy fuzzy", "fuzzy", "noisy", None, 20),
    ("B-noisy fuzzy", "fuzzy", "noisy", None, 20),
    ("B-noisy fuzzy_with_tokens", "fuzzy_tokens", "noisy", None, 20),
    ("B-clean fuzzy", "fuzzy", "clean", None, 20),
    ("iso control", "isoparse", "iso", None, 50),
    ("C-edge fuzzy", "fuzzy", "edge", None, 20),
    ("C-edge strict-try/fuzzy-fallback", "fallback", "edge", None, 20),
    ("D-large-N fuzzy", "fuzzy", "large", 5, 3),
]


# ---------------------------------------------------------------------------
# Statistics: median + MAD reported, Tukey fences flag (never drop).
# ---------------------------------------------------------------------------

def median(xs):
    s = sorted(xs)
    n = len(s)
    mid = n // 2
    if n % 2:
        return s[mid]
    return (s[mid - 1] + s[mid]) / 2.0


def mad(xs, med=None):
    if med is None:
        med = median(xs)
    return median([abs(x - med) for x in xs])


def _percentile(sorted_xs, pct):
    if len(sorted_xs) == 1:
        return sorted_xs[0]
    rank = pct / 100.0 * (len(sorted_xs) - 1)
    lo = int(rank)
    frac = rank - lo
    if lo + 1 >= len(sorted_xs):
        return sorted_xs[lo]
    return sorted_xs[lo] * (1 - frac) + sorted_xs[lo + 1] * frac


def tukey_outliers(xs):
    """Indices of samples outside the 1.5*IQR fences. Flagged, never dropped."""
    if len(xs) < 4:
        return [], (None, None)
    s = sorted(xs)
    q1 = _percentile(s, 25)
    q3 = _percentile(s, 75)
    iqr = q3 - q1
    lo, hi = q1 - 1.5 * iqr, q3 + 1.5 * iqr
    return [i for i, x in enumerate(xs) if x < lo or x > hi], (lo, hi)


# ---------------------------------------------------------------------------
# Timing: interleaved reps with warmup; memory: isolated child probe.
# ---------------------------------------------------------------------------

def sample_once(fn, lines, iters):
    t0 = time.perf_counter()
    for _ in range(iters):
        for s in lines:
            fn(s)
    return (time.perf_counter() - t0) / (iters * len(lines)) * 1e6  # us/call


def bench_interleaved(rows, reps, iters, warmup):
    """Rep-major interleave (direction alternates each rep); warmup discarded."""
    samples = {name: [] for name, _, _, _, _ in rows}
    order = list(rows)
    for r in range(warmup + reps):
        if r % 2:
            order = order[::-1]
        for name, func_key, corpus_key, iters_ov, _ in order:
            fn = FUNCS[func_key]
            lines = CORPORA[corpus_key]
            us = sample_once(fn, lines, iters if iters_ov is None else iters_ov)
            if r >= warmup:
                samples[name].append(us)
    return samples


def _peak_rss_kb():
    """Process peak RSS in KiB. Prefers /proc VmHWM (getrusage ru_maxrss
    is bogus in some containers: observed ~400 MB on a ~10 MB process);
    falls back to getrusage, else None."""
    try:
        with open("/proc/self/status") as f:
            for line in f:
                if line.startswith("VmHWM:"):
                    return int(line.split()[1])
    except OSError:
        pass
    try:
        import resource
    except ImportError:
        return None
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    # macOS reports bytes; Linux reports KiB.
    return rss // 1024 if rss > 1 << 22 else rss


def probe_row(row_name, iters):
    """In-process memory probe body (also used by --probe-row in the child).

    Returns (rss_kb or None, alloc_peak_bytes). RSS is the process
    high-water mark; callers wanting per-row isolation must run this in a
    fresh process -- see peak_memory().
    """
    name, func_key, corpus_key, _, _ = next(
        r for r in ROWS if r[0] == row_name)
    fn = FUNCS[func_key]
    lines = CORPORA[corpus_key]
    import tracemalloc
    tracemalloc.start()
    for _ in range(iters):
        for s in lines:
            fn(s)
    _, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    return _peak_rss_kb(), peak


def peak_memory(row_name, iters):
    """Reusable peak-memory helper: per-row (rss_kb, alloc_peak_b).

    Runs the row workload in a fresh child process, so rss_kb is isolated
    to that row instead of a process-lifetime high-water mark. Child RSS
    covers both sides (Python + native); the tracemalloc peak covers
    Python-level allocs only -- see MEM_NOTE for the blind spot. Returns
    (None, peak) on platforms without getrusage.
    """
    cmd = [sys.executable, __file__, "--probe-row", row_name,
           "--probe-iters", str(iters)]
    out = subprocess.run(cmd, capture_output=True, text=True, check=True)
    rec = json.loads(out.stdout)
    return rec["rss_kb"], rec["alloc_peak_b"]


def host_id():
    return "%s | %s" % (platform.node(), platform.platform())


def commit_id():
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"], capture_output=True,
            text=True, cwd=HERE + "/../..")
        if out.returncode == 0:
            return out.stdout.strip()
    except OSError:
        pass
    return "unknown"


def build_records(samples, args, command, mem, profile):
    host = host_id()
    commit = commit_id()
    records = []
    for name, func_key, corpus_key, iters_ov, mem_iters in ROWS:
        xs = samples[name]
        med = median(xs)
        spread = mad(xs, med)
        out_idx, (lo, hi) = tukey_outliers(xs)
        row_iters = args.iters if iters_ov is None else iters_ov
        rss_kb, alloc_b = mem.get(name, (None, None))
        records.append({
            "name": name,
            "func": func_key,
            "corpus": corpus_key,
            "n_lines": len(CORPORA[corpus_key]),
            "median_us": med,
            "mad_us": spread,
            "mean_us": sum(xs) / len(xs),
            "min_us": min(xs),
            "max_us": max(xs),
            "samples_us": xs,
            "outlier_reps": out_idx,
            "fences_us": [lo, hi],
            "reps": args.reps,
            "iters": row_iters,
            "warmup": args.warmup,
            "noise_pct": args.noise_pct,
            "profile": profile,
            "debug_allowed": args.allow_debug,
            "rss_kb": rss_kb,
            "alloc_peak_b": alloc_b,
            "mem_note": MEM_NOTE,
            "host": host,
            "commit": commit,
            "command": command,
        })
    return records, {"host": host, "commit": commit, "command": command,
                     "profile": profile, "debug_allowed": args.allow_debug,
                     "reps": args.reps, "iters": args.iters,
                     "warmup": args.warmup, "noise_pct": args.noise_pct}


def print_table(records):
    print("%-34s %10s %10s %6s %-8s %10s %10s" % (
        "row", "median", "MAD", "reps", "outlier", "rss_KB", "alloc_KB"))
    for r in records:
        flag = "-" if not r["outlier_reps"] else "*%d" % len(r["outlier_reps"])
        rss = "-" if r["rss_kb"] is None else "%d" % r["rss_kb"]
        alloc = "-" if r["alloc_peak_b"] is None else "%.1f" % (
            r["alloc_peak_b"] / 1024.0)
        print("%-34s %10.3f %10.3f %4d/%d %-8s %10s %10s" % (
            r["name"], r["median_us"], r["mad_us"], r["reps"], r["reps"],
            flag, rss, alloc))
    print("units: us/call; spread = MAD over reps; "
          "*N = N Tukey-fence outliers flagged, kept in median")
    for r in records:
        if r["outlier_reps"]:
            print("  * %s: rep(s) %s outside fences %s us" % (
                r["name"],
                ",".join(str(i) for i in r["outlier_reps"]),
                "[%.3f, %.3f]" % tuple(r["fences_us"])))


def verdict(new, old, noise_pct):
    if old == 0:
        return "n/a"
    delta = (new - old) / old * 100.0
    if abs(delta) < noise_pct:
        return "no-change (%.1f%% < %.1f%%)" % (delta, noise_pct)
    direction = "faster" if delta < 0 else "slower"
    return "%s (%+.1f%%)" % (direction, delta)


def print_compare(records, baseline, noise_pct):
    base = {r["name"]: r["median_us"] for r in baseline["records"]}
    print("\nvs baseline %s (noise %.1f%%):" % (
        baseline["meta"].get("commit", "?"), noise_pct))
    for r in records:
        if r["name"] in base:
            print("  %-34s %s" % (
                r["name"], verdict(r["median_us"], base[r["name"]],
                                   noise_pct)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=["bench", "parity"], default="bench")
    ap.add_argument("--reps", type=int, default=7)
    ap.add_argument("--iters", type=int, default=2000)
    ap.add_argument("--warmup", type=int, default=2,
                    help="warmup reps executed and discarded")
    ap.add_argument("--noise-pct", type=float, default=NOISE_PCT,
                    help="|delta| below this %% reads no-change")
    ap.add_argument("--mem-iters", type=int, default=None,
                    help="override per-row probe iters (default: per-row)")
    ap.add_argument("--no-mem", action="store_true",
                    help="skip the child-process memory pass")
    ap.add_argument("--allow-debug", action="store_true",
                    help="time a non-release build anyway; records are stamped "
                    "non-citable (profile + debug_allowed)")
    ap.add_argument("--baseline", default=None,
                    help="baseline JSON for faster/slower/no-change verdicts")
    ap.add_argument("--probe-row", default=None,
                    help="child-process memory probe (used by peak_memory)")
    ap.add_argument("--probe-iters", type=int, default=20)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    if args.probe_row is not None:
        rss, peak = probe_row(args.probe_row, args.probe_iters)
        print(json.dumps({"rss_kb": rss, "alloc_peak_b": peak}))
        return 0

    errors = check_contracts()
    if errors:
        for e in errors:
            print("CONTRACT: %s" % e, file=sys.stderr)
        return 2

    if args.mode == "parity":
        recs = []
        for s in CLEAN + NOISY:
            recs.append({"line": s, "strict": attempt(s),
                         "fuzzy": attempt(s, fuzzy=True)})
            recs.append({"line": s, "fuzzy_tokens": attempt(
                s, fuzzy_with_tokens=True)})
        bounds = []
        for row in load("corpus_boundary.txt"):
            parts = row.split(" ", 2)
            if len(parts) != 3:
                continue
            exp_strict, exp_fuzzy, s = parts
            got_strict, dt_s = attempt(s)
            got_fuzzy, dt_f = attempt(s, fuzzy=True)
            bounds.append({"line": s, "exp_strict": exp_strict,
                           "got_strict": got_strict, "strict_val": dt_s,
                           "exp_fuzzy": exp_fuzzy, "got_fuzzy": got_fuzzy,
                           "fuzzy_val": dt_f,
                           "match": (exp_strict == got_strict and
                                     exp_fuzzy == got_fuzzy)})
        out = {"records": recs, "boundary": bounds,
               "mismatch": [b for b in bounds if not b["match"]]}
        text = json.dumps(out, indent=1, sort_keys=True)
        if args.out:
            with open(args.out, "w") as f:
                f.write(text + "\n")
        else:
            print(text)
        return 1 if out["mismatch"] else 0

    # V15/C13 structural guard: a develop-built wheel timed as "the Rust
    # port" silently measures unoptimized code. Refuse bench mode unless the
    # loaded extension reports a release profile; --allow-debug overrides
    # and stamps the records non-citable. Parity mode (above) is correctness,
    # not speed, and stays ungated. Pure-Python runs (no extension) are the
    # baseline side and proceed.
    profile = build_profile()
    if profile not in ("release", "pure-python") and not args.allow_debug:
        print("PROFILE: refusing to time a non-release build (profile=%r). "
              "Build with `maturin develop --release` (iteration) or install "
              "a `maturin build --release` wheel, or pass --allow-debug to "
              "mark the figures non-citable." % profile, file=sys.stderr)
        return 3
    command = " ".join(sys.argv)
    samples = bench_interleaved(ROWS, args.reps, args.iters, args.warmup)
    mem = {}
    if not args.no_mem:
        for name, _, _, _, mem_iters in ROWS:
            n = args.mem_iters if args.mem_iters is not None else mem_iters
            mem[name] = peak_memory(name, n)
    records, meta = build_records(samples, args, command, mem, profile)
    print_table(records)
    if args.baseline:
        with open(args.baseline) as f:
            baseline = json.load(f)
        print_compare(records, baseline, args.noise_pct)
    if args.out:
        with open(args.out, "w") as f:
            json.dump({"meta": meta, "records": records}, f, indent=1,
                      sort_keys=True)
            f.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
