"""Ceiling analysis for rs-port-pyasn1: attribute rep-track CPU by module group.

Groups: framing (codec/ber + codec/streaming -- what a Rust framing +
tag/length kernel could address) vs object model (type/ + compat --
stays Python behind the seam) vs rest. Prints infinite-speedup ceiling
of a framing-only kernel over the tier-1 baseline.
Usage: <lane-venv-python> ceiling.py [--reuse-spec]
"""

import cProfile
import pstats
import sys

sys.path.insert(0, "/home/john/firstmate/data/rs-port-pyasn1/scratch/venv/lib/python3.12/site-packages")

from pyasn1.codec.ber import encoder as ber_encoder
from pyasn1.codec.ber import decoder as ber_decoder
from pyasn1.type import univ, namedtype

NREC = 3000
REUSE_SPEC = "--reuse-spec" in sys.argv


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
SPEC = Rec()
WIRES = [ber_encoder.encode(r) for r in RECS]


def run_rep():
    total = 0
    for r, w in zip(RECS, WIRES):
        wire = ber_encoder.encode(r)
        spec = SPEC if REUSE_SPEC else Rec()
        back, _ = ber_decoder.decode(wire, asn1Spec=spec)
        total += int(back["id"]) + int(back["count"]) + len(bytes(back["blob"]))
    return total


run_rep()  # warmup
pr = cProfile.Profile()
pr.enable()
for _ in range(5):
    out = run_rep()
pr.disable()
pr.create_stats()

FRAMING = ("codec/ber/decoder.py", "codec/ber/encoder.py", "codec/streaming.py")
OBJMODEL = ("pyasn1/type/", "pyasn1/compat/")

framing = objm = rest = 0.0
for (fn, _ln, _f), (_cc, nc, tt, _ct, _callers) in pr.stats.items():
    if any(m in fn for m in FRAMING):
        framing += tt
    elif any(m in fn for m in OBJMODEL):
        objm += tt
    else:
        rest += tt
total = framing + objm + rest
print("checksum:", out, "reuse_spec:", REUSE_SPEC)
print("tottime total=%.3fs framing=%.3fs (%.1f%%) objmodel=%.3fs (%.1f%%) rest=%.3fs (%.1f%%)" % (
    total, framing, 100 * framing / total, objm, 100 * objm / total, rest, 100 * rest / total))
print("infinite-speedup ceiling of framing-only kernel: %.2fx" % (total / (total - framing)))
