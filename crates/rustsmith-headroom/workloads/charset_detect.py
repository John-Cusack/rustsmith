"""Headroom workload: charset-normalizer encoding detection.

Doc-driven synthetic: detect a batch of small mixed-encoding blobs, the
package's documented entry point (`from_bytes(...).best()`). Deterministic;
no network, no repo files.
"""

BLOBS = []


def setup():
    global BLOBS
    texts = [
        ("Caf\u00e9 na\u00efve fa\u00e7ade r\u00e9sum\u00e9 d\u00e9j\u00e0 vu. " * 200, "latin-1"),
        ("Gr\u00fc\u00dfe aus M\u00fcnchen, \u00c4pfel und \u00d6l. " * 200, "latin-1"),
        ("Privet mir, eto testovyi tekst. " * 200, "cp1251"),
        ("Geia sou kosme dokimastiko keimeno. " * 200, "cp1253"),
        ("\u65e5\u672c\u8a9e\u306e\u30c6\u30b9\u30c8\u30c6\u30ad\u30b9\u30c8\u3067\u3059. " * 200, "utf-8"),
        ("T\u00fcrk\u00e7e karakterler: gusociI. " * 200, "cp1254"),
    ]
    BLOBS = [t.encode(e, errors="ignore") for t, e in texts] * 4


def run():
    from charset_normalizer import from_bytes
    for b in BLOBS:
        from_bytes(b).best()
