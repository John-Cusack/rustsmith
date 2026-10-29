"""Validate tier1.patch through the stock public call shape.

Runs the WORKLOAD.md representative track with plain
encoder.encode / decoder.decode (no bench-local monkeypatching)
under whichever venv python invokes it. Run under stock and
patched venvs and compare.
Usage: <venv-python> validate_patch.py [--quick]
"""

import sys
import time

NREC = 3000

from pyasn1.codec.ber import encoder, decoder
from pyasn1.type import univ, namedtype


class Rec(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("id", univ.Integer()),
        namedtype.NamedType("blob", univ.OctetString()),
        namedtype.NamedType("flag", univ.Boolean()),
        namedtype.NamedType("oid", univ.ObjectIdentifier()),
        namedtype.NamedType("count", univ.Integer()),
    )


def build():
    recs = []
    for i in range(NREC):
        r = Rec()
        r["id"] = 100000 + i
        r["blob"] = bytes((i + k) % 251 for k in range(64))
        r["flag"] = bool(i % 2)
        r["oid"] = univ.ObjectIdentifier("1.3.6.1.4.1.2026")
        r["count"] = i * 7919
        recs.append(r)
    return recs


RECS = build()


def run_rep():
    total = 0
    for r in RECS:
        wire = encoder.encode(r)
        back, _ = decoder.decode(wire, asn1Spec=Rec())
        total += int(back["id"]) + int(back["count"]) + len(bytes(back["blob"]))
    return total


def run_dec():
    wires = [encoder.encode(r) for r in RECS]
    total = 0
    for w in wires:
        back, _ = decoder.decode(w, asn1Spec=Rec())
        total += int(back["id"]) + int(back["count"]) + len(bytes(back["blob"]))
    return total


def timeit(fn, runs):
    fn()
    best = None
    for _ in range(runs):
        t0 = time.perf_counter()
        out = fn()
        dt = time.perf_counter() - t0
        if best is None or dt < best:
            best = dt
    return best, out


runs = 1 if "--quick" in sys.argv else 5
rep_w, rep_c = timeit(run_rep, runs)
dec_w, dec_c = timeit(run_dec, runs)
print("rep %.4f checksum=%s" % (rep_w, rep_c))
print("dec %.4f checksum=%s" % (dec_w, dec_c))
