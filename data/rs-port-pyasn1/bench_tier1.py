"""Tier-1 pure-Python bench for rs-port-pyasn1 (frozen WORKLOAD.md v1).

Driver mode (default): spawn each (arm, track) in a fresh subprocess,
report wall (min of 5) + peak RSS (child ru_maxrss).
Worker mode: --arm A|B|C|D|E --track rep|enc|dec|x1 --iters N.

Usage: <lane-venv-python> bench_tier1.py [--quick]
"""

import argparse
import resource
import subprocess
import sys
import time

NREC = 3000
OID_STR = "1.3.6.1.4.1.2026"

WORKER = """\
import sys, time
from pyasn1.codec.ber import encoder as ber_encoder
from pyasn1.codec.ber import decoder as ber_decoder
from pyasn1.type import univ, namedtype

NREC = 3000
OID_STR = "1.3.6.1.4.1.2026"

class Rec(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("id", univ.Integer()),
        namedtype.NamedType("blob", univ.OctetString()),
        namedtype.NamedType("flag", univ.Boolean()),
        namedtype.NamedType("oid", univ.ObjectIdentifier()),
        namedtype.NamedType("count", univ.Integer()),
    )

ARM = sys.argv[1]
TRACK = sys.argv[2]
ITERS = int(sys.argv[3])

def build():
    recs = []
    for i in range(NREC):
        r = Rec()
        r["id"] = 100000 + i
        r["blob"] = bytes((i + k) % 251 for k in range(64))
        r["flag"] = bool(i % 2)
        r["oid"] = univ.ObjectIdentifier(OID_STR)
        r["count"] = i * 7919
        recs.append(r)
    return recs

# --- tier-1 arm setup (pure Python, upstream-plausible) ---
_decoder = None
if ARM in ("B", "E"):
    # Arm B: reuse one SingleItemDecoder across messages so the
    # per-instance tag/tagSet caches survive (stock decode() builds
    # a fresh StreamingDecoder -> SingleItemDecoder per message).
    from pyasn1.codec.streaming import asSeekableStream, isEndOfStream, readFromStream
    from pyasn1 import error
    _decoder = ber_decoder.SingleItemDecoder()

def tier1_decode(wire, spec):
    if _decoder is None:
        back, _ = ber_decoder.decode(wire, asn1Spec=spec)
        return back
    substrate = asSeekableStream(wire)
    for asn1Object in _decoder(substrate, spec):
        if isinstance(asn1Object, error.SubstrateUnderrunError):
            raise error.SubstrateUnderrunError("short substrate")
        try:
            tail = next(readFromStream(substrate))
        except error.EndOfStreamError:
            tail = b""
        return asn1Object

if ARM in ("C", "E"):
    # Arm C: bytes.join octet assembly in SequenceEncoder.encodeValue
    from pyasn1.codec.ber import encoder as _enc
    _orig_seq_encodeValue = _enc.SequenceEncoder.encodeValue
    def _join_encodeValue(self, value, asn1Spec, encodeFun, **options):
        if asn1Spec is None and not options.get("defMode", True) is False:
            pass
        # only take the fast path when there is no openType wrapping
        # in play (our workload has none); else fall back.
        namedTypes = (value.componentType if asn1Spec is None
                      else asn1Spec.componentType)
        try:
            if any(getattr(namedTypes[idx], "openType", None)
                   for idx in range(len(value))):
                return _orig_seq_encodeValue(self, value, asn1Spec, encodeFun, **options)
        except Exception:
            return _orig_seq_encodeValue(self, value, asn1Spec, encodeFun, **options)
        chunks = []
        if asn1Spec is None:
            for component in value.values():
                chunks.append(encodeFun(component, asn1Spec, **options))
        else:
            for idx, namedType in enumerate(asn1Spec.componentType.namedTypes):
                chunks.append(encodeFun(value[namedType.name],
                                        namedType.asn1Object, **options))
        return b"".join(chunks), True, True
    _enc.SequenceEncoder.encodeValue = _join_encodeValue

if ARM in ("D", "E"):
    # Arm D: memoize pure header functions encodeTag/encodeLength
    from pyasn1.codec.ber import encoder as _enc2
    _tag_cache, _len_cache = {}, {}
    _orig_encodeTag = _enc2.AbstractItemEncoder.encodeTag
    _orig_encodeLength = _enc2.AbstractItemEncoder.encodeLength
    def _memo_encodeTag(self, singleTag, isConstructed):
        tagClass, tagFormat, tagId = singleTag
        key = (tagClass, tagFormat, tagId, bool(isConstructed))
        hit = _tag_cache.get(key)
        if hit is None:
            hit = _orig_encodeTag(self, singleTag, isConstructed)
            _tag_cache[key] = hit
        return hit
    def _memo_encodeLength(self, length, defMode):
        key = (length, bool(defMode))
        hit = _len_cache.get(key)
        if hit is None:
            hit = _orig_encodeLength(self, length, defMode)
            _len_cache[key] = hit
        return hit
    _enc2.AbstractItemEncoder.encodeTag = _memo_encodeTag
    _enc2.AbstractItemEncoder.encodeLength = _memo_encodeLength

RECS = build()
SPEC = Rec()
WIRES = [ber_encoder.encode(r) for r in RECS]

def run_rep():
    total = 0
    for r, w in zip(RECS, WIRES):
        wire = ber_encoder.encode(r)
        back = tier1_decode(wire, Rec())
        total += int(back["id"]) + int(back["count"]) + len(bytes(back["blob"]))
    return total

def run_enc():
    total = 0
    for r in RECS:
        total += len(ber_encoder.encode(r))
    return total

def run_dec():
    total = 0
    for w in WIRES:
        back = tier1_decode(w, Rec())
        total += int(back["id"]) + int(back["count"]) + len(bytes(back["blob"]))
    return total

def run_x1():
    from pyasn1.type import univ as _u
    seq = _u.SequenceOf(componentType=_u.Integer())
    seq.extend([1, 2, 3])
    total = 0
    for _ in range(2000):
        wire = ber_encoder.encode(seq)
        back, _ = ber_decoder.decode(wire, asn1Spec=_u.SequenceOf(componentType=_u.Integer()))
        total += sum(int(x) for x in back)
    return total

FN = {"rep": run_rep, "enc": run_enc, "dec": run_dec, "x1": run_x1}[TRACK]
FN()  # warmup
t0 = time.perf_counter()
for _ in range(ITERS):
    out = FN()
t1 = time.perf_counter()
print("%.6f %s" % ((t1 - t0) / ITERS, out))
"""


def run_worker(python, arm, track, iters):
    t0 = time.perf_counter()
    p = subprocess.run(
        [python, "-c", WORKER, arm, track, str(iters)],
        capture_output=True, text=True,
    )
    wall_total = time.perf_counter() - t0
    if p.returncode != 0:
        raise RuntimeError(
            "worker %s/%s failed:\n%s\n%s" % (arm, track, p.stdout, p.stderr))
    per_iter, checksum = p.stdout.strip().split()
    rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss / 1024.0
    return float(per_iter), checksum, rss, wall_total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true",
                    help="1 run per cell (smoke only, not evidence)")
    args = ap.parse_args()
    python = sys.executable
    arms = ["A", "B", "C", "D", "E"]
    tracks = ["rep", "enc", "dec", "x1"]
    iters = {"rep": 3, "enc": 3, "dec": 3, "x1": 3}
    runs = 1 if args.quick else 5
    rows = []
    checks = {}
    for track in tracks:
        for arm in arms:
            best, rss, chk = None, None, None
            for _ in range(runs):
                w, c, r, _ = run_worker(python, arm, track, iters[track])
                if best is None or w < best:
                    best, rss, chk = w, r, c
            rows.append((arm, track, best, rss, chk))
            checks.setdefault(track, set()).add(chk)
    print("%-4s %-5s %10s %12s %s" % ("arm", "track", "wall_s", "peakRSS_MiB", "checksum"))
    for arm, track, wall, rss, chk in rows:
        print("%-4s %-5s %10.4f %12.1f %s" % (arm, track, wall, rss, chk))
    base = {t: next(w for a, t2, w, _, _ in rows if a == "A" and t2 == t)
            for t in tracks}
    print("\nspeedups vs A-baseline (representative track decides):")
    for arm, track, wall, _, _ in rows:
        print("  arm %s %s: %.2fx" % (arm, track, base[track] / wall))
    ok = all(len(v) == 1 for v in checks.values())
    print("\nchecksums agree within track: %s" % ok)
    if not ok:
        raise SystemExit("CHECKSUM MISMATCH: %r" % checks)


if __name__ == "__main__":
    main()
