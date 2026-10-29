"""Two-track bench: stock mdurl vs mdurl_rust on the frozen WORKLOAD.md corpus."""
import sys
import time

REPS = int(sys.argv[1]) if len(sys.argv) > 1 else 300
WHICH = sys.argv[2] if len(sys.argv) > 2 else "stock"

if WHICH == "stock":
    import mdurl
else:
    import mdurl_rust as mdurl

_URLS = []
for i in range(2000):
    m = i % 4
    if m == 0:
        _URLS.append(f"https://user{i}:pass{i}@example{i % 100}.com:8080/p/{i}?a=1&b={i}#frag")
    elif m == 1:
        _URLS.append(f"/relative/path/{i}?x={i}&y=z#s{i}")
    elif m == 2:
        _URLS.append(f"https://m\u00fcnchen-{i % 50}.de/\u00fcnicode/{i}")
    else:
        _URLS.append(f"mailto:user+tag-{i}@mail{i % 100}.example.com")


def run():
    n = 0
    for u in _URLS:
        p = mdurl.parse(u)
        n += len(mdurl.format(p))
    return n


run()
t0 = time.perf_counter()
for _ in range(REPS):
    n = run()
t1 = time.perf_counter()
wall = t1 - t0
print(f"arm={WHICH} reps={REPS} urls={len(_URLS) * REPS} wall={wall:.3f}s "
      f"per_url={wall / (len(_URLS) * REPS) * 1e6:.2f}us checksum={n}")
