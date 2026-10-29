import sys
WHICH = sys.argv[1] if len(sys.argv) > 1 else "stock"
N = int(sys.argv[2]) if len(sys.argv) > 2 else 200000
mod = __import__("mdurl" if WHICH == "stock" else "mdurl_rust")
u = 'https://user1:pass1@example1.com:8080/p/1?a=1&b=1#frag'
import time
mod.parse(u)
t0 = time.perf_counter()
for _ in range(N):
    p = mod.parse(u)
t1 = time.perf_counter()
print(f"arm={WHICH} n={N} per_call={(t1-t0)/N*1e6:.3f}us fmtlen={len(mod.format(p))}")
