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

Timing: `--reps` x `--iters` x corpus, min reported (matches the report
methodology). A fixed ``default=datetime(2000, 1, 1)`` keeps runs
deterministic (convertyear only touches 2-digit years, absent here).

``--mode bench`` prints the timing table. ``--mode parity`` writes one
JSON record per line (strict/fuzzy accept flag + result repr) for
before/after diffing; exit nonzero on any corpus-contract violation.
"""
import argparse
import json
import sys
import time
from datetime import datetime

from dateutil.parser import parse
from dateutil.parser import isoparse

HERE = __file__.rsplit("/", 1)[0]
DEFAULT = datetime(2000, 1, 1)


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


def time_call(fn, iters):
    best = None
    for _ in range(iters):
        fn()
    return best


def bench(fn, lines, reps, iters):
    best = float("inf")
    for _ in range(reps):
        t0 = time.perf_counter()
        for _ in range(iters):
            for s in lines:
                fn(s)
        dt = (time.perf_counter() - t0) / (iters * len(lines))
        best = min(best, dt)
    return best * 1e6  # us/call


def strict_fallback(s):
    try:
        parse(s, default=DEFAULT)
    except ValueError:
        parse(s, default=DEFAULT, fuzzy=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=["bench", "parity"], default="bench")
    ap.add_argument("--reps", type=int, default=7)
    ap.add_argument("--iters", type=int, default=2000)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

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

    mix = CLEAN[:8] + NOISY[:2]  # 80/20 representative mix
    rows = [
        ("A-control clean strict", lambda s: parse(s, default=DEFAULT),
         CLEAN),
        ("A-mix strict-try/fuzzy-fallback", strict_fallback, mix),
        ("A-noisy fuzzy", lambda s: parse(s, default=DEFAULT, fuzzy=True),
         NOISY),
        ("B-noisy fuzzy", lambda s: parse(s, default=DEFAULT, fuzzy=True),
         NOISY),
        ("B-noisy fuzzy_with_tokens",
         lambda s: parse(s, default=DEFAULT, fuzzy_with_tokens=True), NOISY),
        ("B-clean fuzzy", lambda s: parse(s, default=DEFAULT, fuzzy=True),
         CLEAN),
        ("iso control", lambda s: isoparse(s),
         ["2024-01-15T10:30:00", "2003-09-25T10:49:41+03:00"]),
    ]
    table = []
    for name, fn, lines in rows:
        us = bench(fn, lines, args.reps, args.iters)
        table.append((name, us))
        print("%-32s %8.3f us/call" % (name, us))
    if args.out:
        with open(args.out, "w") as f:
            json.dump(table, f, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
