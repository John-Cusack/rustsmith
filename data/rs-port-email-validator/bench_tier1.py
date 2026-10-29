"""Tier-1 pure-Python bench for rs-port-email-validator (frozen WORKLOAD.md).

Driver mode (default): for each (arm, track), spawn a fresh subprocess
worker, report wall (min of N) + peak RSS (child ru_maxrss).
Worker mode: --worker ARM --track T --iters N.

Arms: stock = installed email_validator; tier1 = installed package +
data tier1.patch applied to a temp copy (prepended to sys.path).
Usage: <lane-venv-python> bench_tier1.py [--quick]
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

IDN_DOMAINS = ["münchen.de", "日本語.jp", "münster-{k}.de", "faß-{k}.de",
               "उदाहरण-{k}.in", "пример-{k}.ru"]


def gen(n, idn_frac=0.10, invalid_frac=0.22, quoted_frac=0.08, seed=20261029):
    import random
    rng = random.Random(seed)
    out = []
    for i in range(n):
        r = rng.random()
        if r < invalid_frac:
            kind = rng.randrange(8)
            if kind == 0:
                out.append(f"bad-{i}-no-at-sign.example.com")
            elif kind == 1:
                out.append(f"user{i}@bad_domain_{i}.com")
            elif kind == 2:
                out.append("x" * 70 + f"@long{i}.example.com")
            elif kind == 3:
                out.append(f".leading{i}@example.com")
            elif kind == 4:
                out.append(f"double..dot{i}@example.com")
            elif kind == 5:
                out.append(f"user{i}@gmail.test")
            elif kind == 6:
                out.append(f"user{i}@localhost")
            else:
                out.append(f"user{i}@" + "l" * 64 + ".com")
        elif r < invalid_frac + quoted_frac:
            out.append(f'"quoted name {i}"@example.com')
        elif r < invalid_frac + quoted_frac + idn_frac:
            d = rng.choice(IDN_DOMAINS).format(k=i % 50)
            out.append(f"Üser-{i}@{d}" if rng.random() < 0.3 else f"user-{i}@{d}")
        else:
            out.append(f"user.name+tag-{i}@mail{i % 200}.example.com")
    return out


def worker(arm, track, iters):
    if arm == "tier1":
        import email_validator
        src = os.path.dirname(email_validator.__file__)
        tmp = tempfile.mkdtemp(prefix="ev_tier1_")
        dst = os.path.join(tmp, "email_validator")
        shutil.copytree(src, dst, ignore=shutil.ignore_patterns("__pycache__"))
        r = subprocess.run(["patch", "-p1", "-s", "-d", tmp, "-i", PATCH],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print("patch failed: " + r.stderr[-2000:])
            sys.exit(2)
        sys.path.insert(0, tmp)
        for m in [m for m in list(sys.modules) if m.startswith("email_validator")]:
            del sys.modules[m]
    from email_validator import EmailNotValidError, validate_email
    import email_validator as ev
    print("package: " + ev.__file__, flush=True)
    tracks = {"mixed": gen(2000), "ascii": gen(5000, 0.0, 0.0, 0.0),
              "idn": gen(1000, 1.0, 0.0, 0.0),
              "x1-valid": ["user.name+tag-1@mail1.example.com"],
              "x1-invalid": ["bad-1-no-at-sign.example.com"],
              "x1-idn": ["Üser-1@münchen.de"]}
    addrs = tracks[track]
    validate_email("warm@example.com", check_deliverability=False)
    walls = []
    for _ in range(iters):
        t0 = time.perf_counter()
        ok = err = 0
        for a in addrs:
            try:
                validate_email(a, check_deliverability=False)
                ok += 1
            except EmailNotValidError:
                err += 1
        walls.append(time.perf_counter() - t0)
    print(f"RESULT best_s={min(walls):.6f} n={len(addrs)} ok={ok} err={err}")
    for w in walls:
        print(f"  iter_s={w:.6f}")
    import resource as _res
    print(f"SELF_RSS_KB={_res.getrusage(_res.RUSAGE_SELF).ru_maxrss}")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--worker", default=None)
    ap.add_argument("--track", default="mixed")
    ap.add_argument("--iters", type=int, default=7)
    ap.add_argument("--quick", action="store_true")
    args = ap.parse_args()
    if args.worker:
        worker(args.worker, args.track, args.iters)
        return
    iters = 3 if args.quick else 7
    tracks = ["mixed", "ascii", "idn", "x1-valid", "x1-invalid", "x1-idn"]
    arms = ["stock", "tier1"]
    print("arm,track,n,best_s,peak_rss_kb,ok,err")
    for arm in arms:
        for track in tracks:
            ti = 3 if (args.quick or track.startswith("x1")) else iters
            cmd = [sys.executable, __file__, "--worker", arm,
                   "--track", track, "--iters", str(ti)]
            out = subprocess.run(cmd, capture_output=True, text=True)
            if out.returncode != 0:
                print(f"# FAILED {arm}/{track}:\n{out.stdout}\n{out.stderr[-2000:]}")
                continue
            rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
            line = [ln for ln in out.stdout.splitlines() if ln.startswith("RESULT")]
            selfrss = [ln for ln in out.stdout.splitlines() if ln.startswith("SELF_RSS_KB=")]
            import re as _re
            m = _re.search(r"best_s=(\S+) n=(\d+) ok=(\d+) err=(\d+)", line[0])
            rss = int(selfrss[0].split("=")[1]) if selfrss else rss
            print(f"{arm},{track},{m.group(2)},{m.group(1)},{rss},{m.group(3)},{m.group(4)}")


if __name__ == "__main__":
    main()
