"""Ceiling attribution on the tier-1 arm (warmed cProfile, 3x runs)."""
import sys, cProfile, pstats, io, re
sys.path.insert(0, "/home/john/firstmate/data/rs-port-idna/scratch/t1pkg")
sys.path.insert(0, "/home/john/orca/workspaces/rustsmith/fm-rs-port-idna/data/rs-port-idna")
from bench_tier1 import gen, punycode_in
import idna
assert "t1pkg" in idna.core.__file__, idna.core.__file__

def run_track(domains):
    for d in domains:
        try: idna.encode(d, uts46=True)
        except idna.IDNAError: pass
        try: idna.decode(d, uts46=True)
        except (idna.IDNAError, UnicodeError): pass

tracks = {"mixed": gen(8000), "idn": punycode_in(gen(4000, 0.95, 0.05))}
for name, domains in tracks.items():
    run_track(domains[:10])  # warm
    pr = cProfile.Profile()
    for _ in range(3):
        pr.enable(); run_track(domains); pr.disable()
    s = io.StringIO()
    ps = pstats.Stats(pr, stream=s).sort_stats("tottime")
    ps.print_stats(60)
    txt = s.getvalue()
    total = ps.total_tt
    buckets = {
        "idna-own (core+intranges+codec+init)": r"idna/(core|intranges|codec|__init__|compat)\.py",
        "stdlib-punycode (pure py, reimplementable)": r"encodings/punycode\.py",
        "C (bisect/unicodedata/re/str/bytes/codecs)": r"built-in|method .* of (str|bytes|bytearray|re\.Pattern|list)|unicodedata\.",
        "stdlib-other-python": r"encodings/|codecs\.py|re/|\.py.*\(.*\)",
    }
    print(f"=== {name}: total_tt={total:.4f}s ===")
    accounted = 0.0
    for bname, pat in buckets.items():
        t = 0.0; nc = 0
        seen = set()
        for line in txt.splitlines():
            m = re.match(r"\s*(\d+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+(.*)", line)
            if m and re.search(pat, m.group(6)) and m.group(6) not in seen:
                # avoid double count: first bucket wins
                t += float(m.group(2)); nc += int(m.group(1)); seen.add(m.group(6))
        print(f"  {bname}: {t:.4f}s ({t/total*100:.1f}%)")
        accounted += t
    print(f"  unaccounted: {total-accounted:.4f}s")
    own = 0.0
    for line in txt.splitlines():
        m = re.match(r"\s*(\d+)\s+([\d.]+)\s+.*(idna/.*\.py|encodings/punycode\.py.*)", line)
        # recompute portable = idna-own + punycode precisely
    print("---- top20 ----")
    print("\n".join(txt.splitlines()[5:30]))
