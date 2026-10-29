"""Headroom probe workload: pyasn1 BER encode/decode roundtrip (frozen v1).

Same corpus as WORKLOAD.md (3000 nested records). setup() builds the
records; run() encodes each to BER then decodes back and checksums fields.
"""

from pyasn1.codec.ber import encoder, decoder
from pyasn1.type import univ, namedtype

NREC = 3000


class Rec(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("id", univ.Integer()),
        namedtype.NamedType("blob", univ.OctetString()),
        namedtype.NamedType("flag", univ.Boolean()),
        namedtype.NamedType("oid", univ.ObjectIdentifier()),
        namedtype.NamedType("count", univ.Integer()),
    )


_RECS = []


def setup():
    global _RECS
    _RECS = []
    for i in range(NREC):
        r = Rec()
        r["id"] = 100000 + i
        r["blob"] = bytes((i + k) % 251 for k in range(64))
        r["flag"] = bool(i % 2)
        r["oid"] = univ.ObjectIdentifier("1.3.6.1.4.1.2026")
        r["count"] = i * 7919
        _RECS.append(r)


def run():
    total = 0
    for r in _RECS:
        wire = encoder.encode(r)
        back, _ = decoder.decode(wire, asn1Spec=Rec())
        total += int(back["id"]) + int(back["count"]) + len(bytes(back["blob"]))
    return total
