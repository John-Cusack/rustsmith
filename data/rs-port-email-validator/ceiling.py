"""Ceiling analysis for rs-port-email-validator: attribute the representative
track by module group over the tier-1 baseline.

Groups: email_validator (own code -- what a syntax-kernel port could
address) vs idna (shared-dependency cost -- a syntax port either keeps it
behind the seam or reimplements IDNA, a different package) vs rest
(C re engine, unicodedata, builtins -- already C speed).
Prints the infinite-speedup ceiling of an own-code-only kernel.
Usage: <lane-venv-python> ceiling.py
"""

import cProfile
import os
import pstats
import shutil
import subprocess
import sys
import tempfile

DATA = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, DATA)

from bench_tier1 import gen  # noqa: E402

import email_validator  # noqa: E402

src = os.path.dirname(email_validator.__file__)
tmp = tempfile.mkdtemp(prefix="ev_ceiling_")
dst = os.path.join(tmp, "email_validator")
shutil.copytree(src, dst, ignore=shutil.ignore_patterns("__pycache__"))
r = subprocess.run(["patch", "-p1", "-s", "-d", tmp, "-i",
                    os.path.join(DATA, "tier1.patch")],
                   capture_output=True, text=True)
assert r.returncode == 0, r.stderr[-2000:]
sys.path.insert(0, tmp)
for m in [m for m in list(sys.modules) if m.startswith("email_validator")]:
    del sys.modules[m]

from email_validator import EmailNotValidError, validate_email  # noqa: E402

ADDRS = gen(2000)


def run_rep():
    ok = err = 0
    for a in ADDRS:
        try:
            validate_email(a, check_deliverability=False)
            ok += 1
        except EmailNotValidError:
            err += 1
    return ok, err


run_rep()  # warmup: flush lazy imports out of the profiled region
pr = cProfile.Profile()
pr.enable()
for _ in range(5):
    out = run_rep()
pr.disable()
pr.create_stats()

OWN = ("email_validator/",)
SHARED = ("idna/",)

own = shared = rest = 0.0
for (fn, _ln, _f), (_cc, _nc, tt, _ct, _callers) in pr.stats.items():
    if any(m in fn for m in OWN):
        own += tt
    elif any(m in fn for m in SHARED):
        shared += tt
    else:
        rest += tt
total = own + shared + rest
print("checksum ok/err:", out)
print("tottime total=%.3fs own=%.3fs (%.1f%%) idna-shared=%.3fs (%.1f%%) rest=%.3fs (%.1f%%)"
      % (total, own, 100 * own / total, shared, 100 * shared / total, rest, 100 * rest / total))
print("infinite-speedup ceiling of an own-code-only kernel: %.2fx" % (total / (total - own)))
print("kill rule (brief s1): ceiling < 2x over tier-1 on mixed-2000 ->",
      "KILL" if total / (total - own) < 2 else "port")
