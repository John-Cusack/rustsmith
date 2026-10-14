"""Headroom acceptance fixture workload: pure-Python buffer loop.

Self-contained (stdlib only): a tight checksum loop over a buffer, the
shape that must score `rewrite`. Deterministic on every box.
"""

DATA = b""


def setup():
    global DATA
    DATA = bytes(((i * 2654435761) & 0xFF) for i in range(60000))


def run():
    total = 0
    for _ in range(30):
        acc = 0
        for b in DATA:
            acc = ((acc * 31) + b) & 0xFFFFFFFF
        total += acc
    return total
