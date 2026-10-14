"""Workload-labeled highlight benchmarks for the pygments mirror.

Compares the same tree with the Rust extension live vs blocked (seam
flags nulled; the module stays loaded, so import cost is identical —
only kernel time differs). Interleaved A/B sampling, median of reps.

Usage:
    PYTHONPATH=mirror/pygments python3 mirror/pygments/bench/highlight.py
"""

import resource
import statistics
import sys
import time

import pygments
from pygments.formatters import HtmlFormatter
from pygments.lexers import get_lexer_by_name
import pygments.formatters.html as html_mod
import pygments.formatters.svg as svg_mod
import pygments.lexer as lexer_mod
import pygments.regexopt as ro_mod

RUST_FLAGS = (
    (html_mod, "_rust_escape_html_text"),
    (svg_mod, "_rust_escape_html_text"),
    (lexer_mod, "_rust_normalize_newlines"),
    (lexer_mod, "_rust_expand_tabs_and_ensure"),
    (ro_mod, "_rust_regex_opt"),
)
try:
    import pygments._pygments as _ext  # noqa: F401

    HAS_RUST = True
except ImportError:
    HAS_RUST = False

# Live kernel objects captured once; toggling only swaps module globals.
LIVE = [(mod, name, getattr(mod, name, None)) for mod, name in RUST_FLAGS]


def set_rust(on):
    for mod, name, live in LIVE:
        setattr(mod, name, live if on else None)


def peak_rss_mb():
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024


def bench_pair(fn, reps=15):
    """Rep-interleaved A/B: alternate the seam every rep (warmup 3 per arm,
    discarded). Returns (rust_med, rust_mad, fb_med, fb_mad)."""
    import hashlib
    for arm in (True, False):
        set_rust(arm)
        for _ in range(3):
            fn()
    samples = {True: [], False: []}
    for i in range(reps + 1):  # +1 warmup rep, discarded
        order = (True, False) if i % 2 == 0 else (False, True)
        for arm in order:
            set_rust(arm)
            t0 = time.perf_counter()
            fn()
            dt = (time.perf_counter() - t0) * 1000
            if i == 0:
                continue
            samples[arm].append(dt)
    # Parity digest: post-loop outputs of each arm must agree.
    set_rust(True)
    d_rust = hashlib.sha256(repr(fn()).encode()).hexdigest()[:16]
    set_rust(False)
    d_fb = hashlib.sha256(repr(fn()).encode()).hexdigest()[:16]
    set_rust(True)

    def mm(xs):
        m = statistics.median(xs)
        return m, statistics.median([abs(x - m) for x in xs])
    (rm, rmad), (fm, fmad) = mm(samples[True]), mm(samples[False])
    return {"rust": (rm, rmad), "fb": (fm, fmad),
            "digest_rust": d_rust, "digest_fb": d_fb}


def workload_highlight_html(src):
    lx = get_lexer_by_name("python")
    fmt = HtmlFormatter()

    def run():
        return pygments.highlight(src, lx, fmt)

    return run


def workload_lex_only(src):
    lx = get_lexer_by_name("python")

    def run():
        for _ in lx.get_tokens(src):
            pass

    return run


def workload_escape_heavy(src):
    lx = get_lexer_by_name("javascript")
    fmt = HtmlFormatter(nowrap=True)

    def run():
        return pygments.highlight(src, lx, fmt)

    return run


def workload_tiny():
    lx = get_lexer_by_name("python")
    fmt = HtmlFormatter(nowrap=True)

    def run():
        return pygments.highlight("x = 1\n", lx, fmt)

    return run


TRACKS = {
    "R1 highlight 110KB python->HTML": ("representative",
        "corpus_py.txt: 110KB CPython stdlib-flavored source; full highlight pipeline"),
    "R2 lex-only 110KB python": ("representative",
        "corpus_py.txt; tokenize only, `re` engine dominates (ADR-038)"),
    "R3 highlight 70KB escape-heavy JS->HTML": ("representative",
        "corpus_escape.txt: 70KB '<>&\"'-dense JS; escape-preprocess path"),
    "S1 regex_opt C-keyword list": ("showcase",
        "import-time builder over 44 C keywords; pure-Python regex assembly vs Rust"),
    "X1 per-crossing overhead (tiny highlight)": ("overhead",
        "fixed 6-byte snippet; per-call dispatch floor, not throughput"),
}


def alloc_peak_b(fn):
    import tracemalloc
    tracemalloc.start()
    fn()
    _, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    return peak


def main():
    import argparse
    import hashlib
    import json
    import platform
    ap = argparse.ArgumentParser()
    ap.add_argument("--reps", type=int, default=15)
    ap.add_argument("--noise-pct", type=float, default=3.0)
    ap.add_argument("--out", default=str(
        __import__("pathlib").Path(__file__).parent / "results.json"))
    args = ap.parse_args()
    here = __import__("pathlib").Path(__file__).parent
    py_src = (here / "corpus_py.txt").read_text()
    js_src = (here / "corpus_escape.txt").read_text()
    print(f"extension present: {HAS_RUST}")
    print(f"corpus sizes: py={len(py_src)} chars, escape-heavy={len(js_src)} chars")
    from pygments.regexopt import regex_opt
    words = ("if else while for return struct typedef enum union static const "
             "extern volatile register auto signed unsigned long short int "
             "char float double void goto switch case default break continue "
             "do sizeof inline restrict _Bool _Complex _Static_assert").split()
    rows = [
        ("R1 highlight 110KB python->HTML", workload_highlight_html(py_src)),
        ("R2 lex-only 110KB python", workload_lex_only(py_src)),
        ("R3 highlight 70KB escape-heavy JS->HTML", workload_escape_heavy(js_src)),
        ("S1 regex_opt C-keyword list", lambda: regex_opt(words, "\\b", "\\b")),
        ("X1 per-crossing overhead (tiny highlight)", workload_tiny()),
    ]
    out_rows = []
    diverged = []
    md = ["| Workload | Track | Rust | Fallback | Speedup | Verdict |",
          "|---|---|---|---|---|---|"]
    for label, run in rows:
        track, dist = TRACKS[label]
        if not HAS_RUST:
            print(f"{label}: fallback-only (extension not built)")
            continue
        r = bench_pair(run, reps=args.reps)
        (rm, rmad), (fm, fmad) = r["rust"], r["fb"]
        set_rust(True)
        rss_r = peak_rss_mb()
        ra = alloc_peak_b(run)
        set_rust(False)
        rss_f = peak_rss_mb()
        fa = alloc_peak_b(run)
        set_rust(True)
        if r["digest_rust"] != r["digest_fb"]:
            diverged.append(label)
            verdict, sp = "DIVERGED", None
        else:
            delta = (fm - rm) / fm * 100
            sp = fm / rm
            verdict = "no-change" if abs(delta) < args.noise_pct \
                else ("faster" if delta > 0 else "slower")
        spt = f"{sp:.2f}x" if sp else "—"
        print(f"{label}: rust={rm:.3f}ms fallback={fm:.3f}ms speedup={spt} "
              f"verdict={verdict} rss_rust={rss_r:.1f}MB rss_fb={rss_f:.1f}MB")
        md.append(f"| {label} | {track} | {rm:.3f}ms | {fm:.3f}ms | {spt} | {verdict} |")
        out_rows.append({"label": label, "track": track, "size": label.split(" ", 2)[1],
                         "distribution": dist, "iters": 1, "reps": args.reps,
                         "noise_pct": args.noise_pct,
                         "mem_note": ("tracemalloc sees Python-level allocs only; "
                                      "native allocs invisible, favoring the ported side. "
                                      "rss_mb covers both sides."),
                         "baseline": {"median_ms": fm, "mad_ms": fmad,
                                      "rss_kb": rss_f * 1024, "alloc_b": fa,
                                      "digest": r["digest_fb"]},
                         "port": {"median_ms": rm, "mad_ms": rmad,
                                  "rss_kb": rss_r * 1024, "alloc_b": ra,
                                  "digest": r["digest_rust"]},
                         "speedup_x": sp, "verdict": verdict})
    doc = {"package": "pygments",
           "baseline": "same tree, seam-blocked (pure-Python fallback)",
           "host": {"platform": platform.platform(),
                    "cpu": platform.processor() or "unknown",
                    "python": platform.python_version()},
           "command": sys.argv,
           "measured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "workloads": out_rows}
    with open(args.out, "w") as f:
        json.dump(doc, f, indent=2)
    mdpath = str(here / "rows.md")
    with open(mdpath, "w") as f:
        f.write("\n".join(md) + "\n")
    print("\n".join(md))
    print(f"\nwrote {args.out} + {mdpath}")
    print("overhead note: import cost identical both arms (module stays "
          "loaded); null-formatter lex delta isolates the preprocess kernel.")
    if diverged:
        print(f"DIVERGED: {diverged}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
