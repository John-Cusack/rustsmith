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


def bench(fn, reps=15):
    times = []
    for _ in range(3):  # warmup
        fn()
    for _ in range(reps):
        t0 = time.perf_counter()
        fn()
        times.append((time.perf_counter() - t0) * 1000)
    return statistics.median(times)


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


def main():
    here = __import__("pathlib").Path(__file__).parent
    py_src = (here / "corpus_py.txt").read_text()
    js_src = (here / "corpus_escape.txt").read_text()
    print(f"extension present: {HAS_RUST}")
    print(f"corpus sizes: py={len(py_src)} chars, escape-heavy={len(js_src)} chars")
    rows = [
        ("W1 highlight python->HTML", workload_highlight_html(py_src)),
        ("W2 lex-only python", workload_lex_only(py_src)),
        ("W3 highlight escape-heavy->HTML", workload_escape_heavy(js_src)),
    ]
    for label, run in rows:
        if HAS_RUST:
            set_rust(True)
            a = bench(run)
            rss_a = peak_rss_mb()
            set_rust(False)
            b = bench(run)
            rss_b = peak_rss_mb()
            set_rust(True)
            print(f"{label}: rust={a:.2f}ms fallback={b:.2f}ms "
                  f"speedup={b / a:.2f}x rss_rust={rss_a:.1f}MB rss_fb={rss_b:.1f}MB")
        else:
            print(f"{label}: fallback-only={bench(run):.2f}ms (extension not built)")
    # Import-time path: regex_opt over a C-keyword list.
    from pygments.regexopt import regex_opt

    words = ("if else while for return struct typedef enum union static const "
             "extern volatile register auto signed unsigned long short int "
             "char float double void goto switch case default break continue "
             "do sizeof inline restrict _Bool _Complex _Static_assert").split()
    if HAS_RUST:
        set_rust(True)
        a = bench(lambda: regex_opt(words, "\\b", "\\b"))
        set_rust(False)
        b = bench(lambda: regex_opt(words, "\\b", "\\b"))
        set_rust(True)
        print(f"W4 regex_opt C-keywords: rust={a:.3f}ms fallback={b:.3f}ms "
              f"speedup={b / a:.2f}x")
    print("overhead note: import cost identical both arms (module stays "
          "loaded); null-formatter lex delta isolates the preprocess kernel.")


if __name__ == "__main__":
    sys.exit(main())
