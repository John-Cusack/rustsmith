"""Tier-1 memoization bench for rs-port-arrow (frozen WORKLOAD.md v1).

Arms A/B/C/D per WORKLOAD.md. Reports wall (min of 5) + peak RSS per arm.
Usage: <venv-python> bench_tier1.py
"""

import functools
import resource
import sys
import time

import arrow
from arrow import parser as arrow_parser

N = 2000
ISO_BATCH = [
    f"2021-01-01T{(i // 3600) % 24:02d}:{(i // 60) % 60:02d}:{i % 60:02d}+00:00"
    for i in range(N)
]
FMT = "YYYY-MM-DD HH:mm:ss"
FMT_BATCH = [f"2021-01-02 03:04:{(i % 60):02d}" for i in range(N)]


def peak_rss_mb():
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024.0


def timeit(fn, runs=5):
    fn()  # warmup
    best = min(
        (t0 := time.perf_counter(), fn(), time.perf_counter() - t0)[2]
        for _ in range(runs)
    )
    return best


def arm_baseline_parse():
    for s in FMT_BATCH:
        arrow.get(s, FMT)


def arm_baseline_full():
    parsed = [arrow.get(s, FMT) for s in FMT_BATCH]
    shifted = [a.shift(days=1, hours=2) for a in parsed]
    formatted = [a.format(FMT) for a in shifted]
    base = parsed[0]
    [a.humanize(base) for a in parsed[100:300]]


def arm_iso_parse():
    for s in ISO_BATCH:
        arrow.get(s)


def make_shared(cache_size):
    p = arrow_parser.DateTimeParser(cache_size=cache_size)

    def run():
        for s in FMT_BATCH:
            p.parse(s, FMT)

    return run


# Arm D: class-level memo prototype — cache keyed by (locale, fmt) shared
# across parser instances, mirroring the upstream-ready patch shape.
_class_memo = {}

_orig_generate = arrow_parser.DateTimeParser._generate_pattern_re


def _memoized_generate(self, fmt):
    key = (str(self.locale), fmt)
    hit = _class_memo.get(key)
    if hit is None:
        hit = _orig_generate(self, fmt)
        _class_memo[key] = hit
    return hit


def arm_class_memo_parse():
    arrow_parser.DateTimeParser._generate_pattern_re = _memoized_generate
    try:
        for s in FMT_BATCH:
            arrow.get(s, FMT)
    finally:
        arrow_parser.DateTimeParser._generate_pattern_re = _orig_generate


def main():
    rows = []
    w, r = timeit(arm_iso_parse), peak_rss_mb()
    rows.append(("A iso-parse (baseline arrow.get)", w, r))
    w, r = timeit(arm_baseline_parse), peak_rss_mb()
    rows.append(("A fmt-parse (baseline arrow.get)", w, r))
    w, r = timeit(arm_baseline_full), peak_rss_mb()
    rows.append(("A full pipeline (representative)", w, r))
    w, r = timeit(make_shared(0)), peak_rss_mb()
    rows.append(("B fmt-parse shared parser, no cache", w, r))
    w, r = timeit(make_shared(128)), peak_rss_mb()
    rows.append(("C fmt-parse shared parser, cache_size=128", w, r))
    _class_memo.clear()
    w, r = timeit(arm_class_memo_parse), peak_rss_mb()
    rows.append(("D fmt-parse arrow.get + class memo", w, r))

    print(f"{'arm':45s} {'wall_s':>10s} {'peakRSS_MiB':>12s}")
    for name, wall, rss in rows:
        print(f"{name:45s} {wall:10.4f} {rss:12.1f}")
    base = rows[1][1]
    print("\nfmt-parse speedups vs A-baseline:")
    for name, wall, _ in rows[3:]:
        print(f"  {name}: {base / wall:.2f}x")
    rep = rows[2][1]
    print("representative full-pipeline baseline wall: "
          f"{rep:.3f}s ({N} items)")


if __name__ == "__main__":
    sys.exit(main())
