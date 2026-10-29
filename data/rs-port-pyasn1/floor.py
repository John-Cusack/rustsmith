"""Object-model floor for rs-port-pyasn1: build Rec objects from native
values with NO codec work. This is the decode-side cost a Rust framing
kernel cannot remove (object model stays Python behind the seam).
Compares against bench_tier1 dec-track walls for ceiling math.
Usage: <lane-venv-python> floor.py
"""

import subprocess
import sys
import time

WORKER = """\
import sys, time
from pyasn1.type import univ, namedtype

NREC = 3000
ITERS = int(sys.argv[1])

IDS = [100000 + i for i in range(NREC)]
BLOBS = [bytes((i + k) % 251 for k in range(64)) for i in range(NREC)]
FLAGS = [bool(i % 2) for i in range(NREC)]
COUNTS = [i * 7919 for i in range(NREC)]

class Rec(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("id", univ.Integer()),
        namedtype.NamedType("blob", univ.OctetString()),
        namedtype.NamedType("flag", univ.Boolean()),
        namedtype.NamedType("oid", univ.ObjectIdentifier()),
        namedtype.NamedType("count", univ.Integer()),
    )

OID = univ.ObjectIdentifier("1.3.6.1.4.1.2026")
SPEC = Rec()

def run_floor():
    # what a decode kernel hands back: native values per field, and
    # Python still builds every object (clone/setComponent mechanics
    # proxied by stock item assignment + one clone per record).
    total = 0
    for i in range(NREC):
        r = SPEC.clone()
        r["id"] = IDS[i]
        r["blob"] = BLOBS[i]
        r["flag"] = FLAGS[i]
        r["oid"] = OID
        r["count"] = COUNTS[i]
        total += int(r["id"]) + int(r["count"]) + len(bytes(r["blob"]))
    return total

run_floor()  # warmup
t0 = time.perf_counter()
for _ in range(ITERS):
    out = run_floor()
t1 = time.perf_counter()
print("%.6f %s" % ((t1 - t0) / ITERS, out))
"""

best = None
for _ in range(5):
    p = subprocess.run([sys.executable, "-c", WORKER, "3"],
                       capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(p.stdout + p.stderr)
    w, chk = p.stdout.strip().split()
    w = float(w)
    if best is None or w < best:
        best = w
print("floor (object-model build, no codec): %.4fs checksum=%s" % (best, chk))
print("vs dec-track A=0.1916 E=0.1557, rep-track A=0.2694 E=0.2307 (bench_tier1 min-of-5)")
print("decode-side infinite-codec ceiling over E: %.2fx" % (0.1557 / best if best < 0.1557 else -1.0))
print("rep-track ceiling over E if decode floor stays: %.2fx" % (0.2307 / (0.2307 - (0.1557 - best))) if best < 0.1557 else "floor >= E decode: codec fully dominated")
