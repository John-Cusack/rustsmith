"""Tier-1 pure-Python bench for rs-port-idna (frozen WORKLOAD.md).

Driver mode (default): for each (arm, track), spawn a fresh subprocess
worker, report wall (min of N) + peak RSS (child ru_maxrss).
Worker mode: --worker ARM --track T --iters N.

Arms: stock = scratch venv idna 3.20; tier1 = installed package +
tier1.patch applied to a temp copy (prepended to sys.path).
Usage: scratch/venv/bin/python bench_tier1.py [--quick]
"""

import argparse
import os
import resource
import shutil
import subprocess
import sys
import tempfile
import time

DATA = os.path.dirname(os.path.abspath(__file__))
PATCH = os.path.join(DATA, "tier1.patch")
SCRATCH = "/home/john/firstmate/data/rs-port-idna/scratch"
VENV_SITE = os.path.join(SCRATCH, "venv/lib/python3.12/site-packages")

IDN = ["münchen.de", "日本語.jp", "münster.de", "faß.de",
       "пример.ru", "उदाहरण.in", "école.fr", "Zürich.ch"]


def gen(n, idn_frac=0.20, invalid_frac=0.10, seed=20261029):
    import random
    rng = random.Random(seed)
    out = []
    for i in range(n):
        r = rng.random()
        if r < invalid_frac:
            out.append(["nodot", "bad..dots.com", "-lead.com",
                        "trail-.com", "a" * 64 + ".com",
                        "under_score.com"][i % 6])
        elif r < invalid_frac + idn_frac:
            base, tld = IDN[i % len(IDN)].split(".")
            out.append(f"{base}-{i % 200}.{tld}")
        else:
            out.append(f"mail{i % 500}.example{i % 50}.com")
    # punycode-in: every 10th IDN domain pre-encoded (wire form)
    return out


def punycode_in(domains, frac=0.1):
    import idna
    out = list(domains)
    n = 0
    for i, d in enumerate(domains):
        try:
            d.encode("ascii")
        except UnicodeEncodeError:
            if (i % 10) == 0:
                try:
                    out[i] = idna.encode(d, uts46=True).decode("ascii")
                    n += 1
                except idna.IDNAError:
                    pass
    return out


def worker(arm, track, iters):
    if arm == "tier1":
        tmp = os.environ["IDNA_TIER1_DIR"]
        sys.path.insert(0, tmp)
        for m in list(sys.modules):
            if m == "idna" or m.startswith("idna."):
                del sys.modules[m]
    import idna
    print("package: " + idna.__file__, flush=True)
    if track == "mixed":
        domains = gen(8000)
    elif track == "ascii":
        domains = gen(16000, 0.0, 0.0)
    elif track == "idn":
        domains = punycode_in(gen(4000, 0.95, 0.05))
    elif track == "x1-valid":
        domains = ["mail1.example.com"]
    elif track == "x1-idn":
        domains = ["münchen-1.de"]
    elif track == "x1-invalid":
        domains = ["bad..dots.com"]
    else:
        raise ValueError(track)
    idna.encode("warm.example.com", uts46=True)
    walls = []
    for _ in range(iters):
        ok = err = 0
        t0 = time.perf_counter()
        for d in domains:
            try:
                idna.encode(d, uts46=True)
                ok += 1
            except idna.IDNAError:
                err += 1
            try:
                idna.decode(d, uts46=True)
                ok += 1
            except (idna.IDNAError, UnicodeError):
                err += 1
        walls.append(time.perf_counter() - t0)
    print(f"RESULT best_s={min(walls):.6f} n={len(domains)} ok={ok} err={err}")
    for w in walls:
        print(f"  iter_s={w:.6f}")
    import resource as _res
    print(f"SELF_RSS_KB={_res.getrusage(_res.RUSAGE_SELF).ru_maxrss}")


def apply_patch(srcdir, patch, destdir):
    shutil.copytree(srcdir, destdir, dirs_exist_ok=True)
    import re
    hunks = {}  # path -> [(old_start, minus, plus)]
    cur = None
    with open(patch) as f:
        lines = f.read().splitlines()
    i = 0
    while i < len(lines):
        m = re.match(r"^\+\+\+ b/(.*)", lines[i])
        if m:
            cur = os.path.join(destdir, m.group(1))
            hunks.setdefault(cur, [])
            i += 1
            continue
        m = re.match(r"^@@ -(\d+),?(\d*) \+(\d+),?(\d*) @@", lines[i])
        if m and cur:
            old_start = int(m.group(1))
            i += 1
            hunk = []
            while i < len(lines) and not lines[i].startswith("@@") \
                    and not lines[i].startswith("+++") \
                    and not lines[i].startswith("diff ") \
                    and not lines[i].startswith("--- "):
                hunk.append(lines[i])
                i += 1
            minus = [l[1:] if l else "" for l in hunk
                     if l[:1] in (" ", "-", "") and not l.startswith("\\")]
            plus = [l[1:] if l else "" for l in hunk
                    if l[:1] in (" ", "+", "") and not l.startswith("\\")]
            hunks[cur].append((old_start, minus, plus))
            continue
        i += 1
    for path, hs in hunks.items():
        with open(path) as f:
            srclines = f.read().splitlines()
        for old_start, minus, plus in sorted(hs, reverse=True):
            start = old_start - 1
            assert srclines[start:start + len(minus)] == minus, \
                f"hunk mismatch in {path} at {old_start}"
            srclines[start:start + len(minus)] = plus
        with open(path, "w") as f:
            f.write("\n".join(srclines) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--worker", default=None)
    ap.add_argument("--track", default=None)
    ap.add_argument("--iters", type=int, default=7)
    ap.add_argument("--quick", action="store_true")
    args = ap.parse_args()
    if args.worker:
        return worker(args.worker, args.track, 2 if args.quick else args.iters)
    py = sys.executable
    env = dict(os.environ)
    tmpd = None
    if os.path.exists(PATCH):
        tmpd = tempfile.mkdtemp(prefix="idna-tier1-")
        apply_patch(os.path.join(VENV_SITE, "idna"), PATCH, os.path.join(tmpd, "idna"))
        env["IDNA_TIER1_DIR"] = tmpd
    try:
        tracks = ["mixed", "ascii", "idn", "x1-valid", "x1-idn", "x1-invalid"]
        if args.quick:
            tracks = ["mixed", "x1-valid"]
        print("arm,track,best_s,rss_kb,ok,err")
        for arm in ["stock", "tier1"] if tmpd else ["stock"]:
            for track in tracks:
                iters = 2 if args.quick else 7
                p = subprocess.run(
                    [py, os.path.abspath(__file__), "--worker", arm,
                     "--track", track, "--iters", str(iters)],
                    capture_output=True, text=True, env=env)
                import re as _re
                m = _re.search(r"RESULT best_s=([\d.]+) n=\d+ ok=(\d+) err=(\d+)", p.stdout)
                rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
                if not m:
                    print(f"{arm},{track},FAILED,,{p.stdout[-500:]}")
                    print(p.stderr[-2000:], file=sys.stderr)
                else:
                    print(f"{arm},{track},{m.group(1)},{rss},{m.group(2)},{m.group(3)}")
    finally:
        if tmpd:
            shutil.rmtree(tmpd, ignore_errors=True)


if __name__ == "__main__":
    main()
