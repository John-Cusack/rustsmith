"""Headroom probe workload: rs-port-arrow representative pipeline (WORKLOAD.md v1).

setup() builds batches once; run() does parse -> shift -> format -> humanize.
Baseline public API (arrow.get per item), unmemoized.
"""

import arrow

FMT = "YYYY-MM-DD HH:mm:ss"
N = 2000

BATCH = None


def setup():
    global BATCH
    BATCH = [f"2021-01-02 03:04:{(i % 60):02d}" for i in range(N)]


def run():
    parsed = [arrow.get(s, FMT) for s in BATCH]
    shifted = [a.shift(days=1, hours=2) for a in parsed]
    formatted = [a.format(FMT) for a in shifted]
    base = parsed[0]
    for a in parsed[100:300]:
        a.humanize(base)
