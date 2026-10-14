"""Headroom acceptance fixture workload: mixed regex + pure-Python loop.

Self-contained (stdlib only, generated inputs): exercises both the shared
C `re` engine and a portable Python loop so the row carries nonzero
regex and pure-python fractions. Deterministic on every box.
"""

import re

RX = re.compile(r"[a-z_]+=\d+")
CHUNK = "abc_def=123 " * 2000


def setup():
    pass


def run():
    total = 0
    for _ in range(50):
        for m in RX.finditer(CHUNK):
            total += len(m.group(0))
        acc = 0
        for i in range(2000):
            acc += i * i
        total += acc
    return total
