"""Equivalence: stock vs tier-1-patched pyasn1 over a BER corpus.

Emits canonical lines (re-encoded hex + decoded pretty-print, or
exception type+message) for each case. Run with the stock venv python,
then the patched venv python, and diff the outputs.
Usage: <venv-python> equiv_check.py
"""

from pyasn1.codec.ber import encoder, decoder
from pyasn1.type import univ, namedtype, tag, char


class Rec(univ.Sequence):
    componentType = namedtype.NamedTypes(
        namedtype.NamedType("id", univ.Integer()),
        namedtype.NamedType("blob", univ.OctetString()),
        namedtype.NamedType("flag", univ.Boolean()),
        namedtype.NamedType("oid", univ.ObjectIdentifier()),
        namedtype.NamedType("count", univ.Integer()),
    )


def show(label, make, spec=None, raw=None, decode_spec=True):
    try:
        if raw is not None:
            wire = raw
        else:
            wire = encoder.encode(make())
        back, tail = decoder.decode(
            wire, asn1Spec=spec() if (decode_spec and spec) else None)
        reenc = encoder.encode(back)
        print("OK   %s wire=%s tail=%s back=%s reenc_eq=%s" % (
            label, wire.hex(), tail.hex(), back.prettyPrint(),
            reenc == wire if raw is None else "n/a"))
    except Exception as e:  # noqa: BLE001 - comparing exception behavior
        print("ERR  %s -> %s: %s" % (label, type(e).__name__, e))


def make_rec():
    r = Rec()
    r["id"] = 7
    r["blob"] = b"xy"
    r["flag"] = True
    r["oid"] = univ.ObjectIdentifier("1.3.6")
    r["count"] = 42
    return r


cases = [
    ("int-zero", lambda: univ.Integer(0), univ.Integer),
    ("int-neg", lambda: univ.Integer(-129), univ.Integer),
    ("int-big", lambda: univ.Integer(2 ** 100), univ.Integer),
    ("oct-empty", lambda: univ.OctetString(b""), univ.OctetString),
    ("oct-short", lambda: univ.OctetString(b"abc"), univ.OctetString),
    ("oct-longform", lambda: univ.OctetString(bytes(range(256))), univ.OctetString),
    ("bool-t", lambda: univ.Boolean(True), univ.Boolean),
    ("bool-f", lambda: univ.Boolean(False), univ.Boolean),
    ("null", lambda: univ.Null(""), univ.Null),
    ("oid-short", lambda: univ.ObjectIdentifier("1.3.6"), univ.ObjectIdentifier),
    ("oid-longarc", lambda: univ.ObjectIdentifier("1.3.6.1.4.1.2026.9999999999"),
     univ.ObjectIdentifier),
    ("seq-nested", make_rec, Rec),
    ("seq-empty", lambda: univ.Sequence(), univ.Sequence),
    ("seqof", lambda: univ.SequenceOf(componentType=univ.Integer()),
     lambda: univ.SequenceOf(componentType=univ.Integer())),
    ("set", lambda: [univ.Integer(1), univ.Integer(2)], None),
    ("tagged-explicit", lambda: univ.Integer(5).subtype(
        explicitTag=tag.Tag(tag.tagClassContext, tag.tagFormatSimple, 3)),
     lambda: univ.Integer().subtype(
         explicitTag=tag.Tag(tag.tagClassContext, tag.tagFormatSimple, 3))),
    ("tagged-implicit", lambda: univ.Integer(5).subtype(
        implicitTag=tag.Tag(tag.tagClassContext, tag.tagFormatSimple, 4)),
     lambda: univ.Integer().subtype(
         implicitTag=tag.Tag(tag.tagClassContext, tag.tagFormatSimple, 4))),
    ("indef-len", None, univ.SequenceOf, b"\x30\x80\x02\x01\x01\x02\x01\x02\x00\x00"),
    ("truncated", None, None, b"\x30\x05\x02\x01"),
    ("bad-tag", None, None, b"\xff\x01\x00"),
    ("empty-input", None, None, b""),
    ("bitstring", lambda: univ.BitString((1, 0, 1, 1, 0, 0, 1, 0, 1)),
     univ.BitString),
    ("real", lambda: univ.Real(3.14), univ.Real),
    ("utf8", lambda: char.UTF8String("héllo→"), char.UTF8String),
    ("enum", lambda: univ.Enumerated(3), univ.Enumerated),
]

for entry in cases:
    label, make, spec = entry[0], entry[1], entry[2]
    raw = entry[3] if len(entry) > 3 else None
    if label == "seqof":
        seq = univ.SequenceOf(componentType=univ.Integer())
        seq.extend([1, 2, 3])
        show(label, lambda: seq, spec)
    elif label == "set":
        s = univ.Set(componentType=namedtype.NamedTypes(
            namedtype.NamedType("a", univ.Integer()),
            namedtype.NamedType("b", univ.Integer())))
        s["a"] = 1
        s["b"] = 2
        show(label, lambda: s, None)
    else:
        show(label, make, spec, raw)
