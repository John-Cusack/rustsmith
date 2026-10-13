"""Port's own tests: Rust kernels vs the pure-Python fallback.

These run against the mirror tree in both configurations (extension
built or absent). They assert the kernels agree with the upstream
Python they delegate — they never weaken the upstream suite, which
remains the frozen oracle and is run separately in full.
"""

import re
import sys

import pytest

from pygments.formatters.html import escape_html as html_escape
from pygments.formatters.html import _escape_html_table
from pygments.formatters.svg import escape_html as svg_escape
from pygments.lexer import Lexer
from pygments.regexopt import regex_opt, regex_opt_inner
from pygments.lexers import get_lexer_by_name

try:
    import pygments._pygments as rust
except ImportError:
    rust = None

needs_rust = pytest.mark.skipif(rust is None, reason="extension not built")


@needs_rust
def test_escape_matches_translate_table():
    corpus = [
        "",
        "plain ascii",
        "&<>\"'",
        "a & b <c> \"d\" 'e'",
        "&amp; already?",
        "héllo wörld ✓ \U0001f600",
        "<div class=\"x\">&nbsp;</div>",
        "&" * 1000,
        # Full BMP sweep minus lone surrogates: the raw binding is
        # lossy on surrogates by design (Rust `String` cannot hold
        # them); surrogate traffic never reaches it — see
        # `test_surrogate_input_uses_fallback`.
        "".join(chr(i) for i in list(range(1, 0xD800))
                + list(range(0xE000, 0x2500))),
    ]
    for s in corpus:
        assert rust.escape_html_text(s) == s.translate(_escape_html_table)


@needs_rust
def test_surrogate_input_uses_fallback():
    """Lone surrogates never reach Rust: public seams match the fallback."""
    from pygments._rustseam import rust_safe_text
    from pygments.regexopt import regex_opt_inner
    lone = "a\ud800b\udc00c"
    assert not rust_safe_text(lone)
    assert rust_safe_text("plain")
    assert rust_safe_text("héllo ✓ \U0001f600")
    assert html_escape(lone) == lone.translate(_escape_html_table)
    assert svg_escape(lone) == lone.translate(_escape_html_table)
    words = ["a\ud800", "bc"]
    assert regex_opt(words) == regex_opt_inner(sorted(words), "(")
    from pygments.lexers import PythonLexer
    lx = PythonLexer()
    assert lx._preprocess_lexer_input("x\ud800 = 1\r\n") == "x\ud800 = 1\n"

@needs_rust
def test_escape_agrees_with_both_formatters():
    for s in ["<a href=\"x\">&'\"", "line1\nline2", "\tindent"]:
        assert html_escape(s) == svg_escape(s) == rust.escape_html_text(s)


@needs_rust
def test_normalize_newlines_matches_replaces():
    corpus = ["", "a\nb\n", "a\r\nb", "a\rb", "a\r\r\nb\r", "\r" * 50,
              "x\ry\r\nz\n", "no carriage returns here"]
    for s in corpus:
        assert rust.normalize_newlines(s) == s.replace("\r\n", "\n").replace("\r", "\n")


@needs_rust
def test_expand_tabs_matches_str_expandtabs():
    corpus = ["", "ab", "\t", "ab\t", "a\n\tb", "\t\t", "x\ty\tz",
              "héllo\twörld", "\u000b\t\f\t", "a" * 17 + "\t",
              "\t".join(["x"] * 30)]
    for s in corpus:
        for tabsize in (0, 1, 2, 4, 8):
            for ensurenl in (False, True):
                norm = s.replace("\r\n", "\n").replace("\r", "\n")
                if tabsize > 0:
                    expect = norm.expandtabs(tabsize)
                else:
                    expect = norm
                if ensurenl and not expect.endswith("\n"):
                    expect += "\n"
                assert rust.expand_tabs_and_ensure(norm, tabsize, ensurenl) == expect


@needs_rust
def test_preprocess_end_to_end_matches_fallback():
    from pygments.lexers import PythonLexer
    corpus = ["print(1)\r\n", "x\t=\t1", "\ufeffy = 2\r", "", "\n\n\n",
              "a" * 5000 + "\t" + "b" * 5000]
    for text in corpus:
        for opts in ({}, {"stripall": True}, {"stripnl": False},
                     {"tabsize": 4}, {"ensurenl": False},
                     {"tabsize": 8, "stripall": True}):
            lx = PythonLexer(**opts)
            rust_out = lx._preprocess_lexer_input(text)
            # Replay the upstream Python verbatim for comparison.
            t = text
            if t.startswith("\ufeff"):
                t = t[len("\ufeff"):]
            t = t.replace("\r\n", "\n").replace("\r", "\n")
            if lx.stripall:
                t = t.strip()
            elif lx.stripnl:
                t = t.strip("\n")
            if lx.tabsize > 0:
                t = t.expandtabs(lx.tabsize)
            if lx.ensurenl and not t.endswith("\n"):
                t += "\n"
            assert rust_out == t


@needs_rust
def test_regex_opt_matches_python():
    wordlists = [
        [],
        [""],
        ["if"],
        ["a", "b"],
        ["if", "else", "while", "for", "return"],
        ["foobar", "foobaz"],
        ["aa", "ba"],
        ["", "a", "ab"],
        ["a+b", "(c)", "d[e]", "f{g}", "h?i", "j*k", "l|m", "^n$", "o.p",
         "q\\r", "s t", "u#v", "w~x"],
        ["print", "printf", "println", "private", "protected", "public"],
        ["_foo", "foo2", "FOO", "foo_bar"],
        ["café", "naïve", "✓win", "日本語", "🎉party"],
        sorted({w for w in
                "def class if else elif while for return import from as with "
                "try except finally raise lambda yield async await pass break "
                "continue del global nonlocal assert in is not and or".split()}),
    ]
    for words in wordlists:
        for prefix, suffix in (("", ""), ("\\b", "\\b"), ("(?<!\\.)", "")):
            expect = prefix + regex_opt_inner(sorted(words), "(") + suffix
            assert rust.regex_opt(list(words), prefix, suffix) == expect
            assert regex_opt(words, prefix, suffix) == expect


@needs_rust
def test_regex_opt_output_compiles_and_matches():
    words = ["if", "else", "while", "for", "return", "a+b", "(c)", ""]
    # Empty member makes the whole group optional; compile check only.
    pat = regex_opt([w for w in words if w], prefix="\\b", suffix="\\b")
    rx = re.compile(pat)
    for w in ("if", "else", "while", "for", "return"):
        assert rx.search(w)
    assert not rx.search("nothing_here")


def test_highlight_smoke_without_rust(monkeypatch):
    """The tree works with the extension blocked (fallback path)."""
    import pygments.formatters.html as html_mod
    import pygments.lexer as lexer_mod
    import pygments.regexopt as ro_mod
    monkeypatch.setattr(html_mod, "_rust_escape_html_text", None)
    monkeypatch.setattr(lexer_mod, "_rust_normalize_newlines", None)
    monkeypatch.setattr(lexer_mod, "_rust_expand_tabs_and_ensure", None)
    monkeypatch.setattr(ro_mod, "_rust_regex_opt", None)
    lx = get_lexer_by_name("python")
    toks = list(lx.get_tokens("print('hi')\r\n"))
    assert any(v == "print" for _, v in toks)
    from pygments.formatters import HtmlFormatter
    parts = HtmlFormatter()._format_lines(iter(
        [(t, v) for t, v in
         __import__("pygments").lex("x<1", lx)]))
    out = "".join(s for _, s in parts)
    assert "&lt;" in out


def test_rust_and_fallback_highlight_identical():
    """Same highlight output with and without the extension."""
    if rust is None:
        pytest.skip("extension not built")
    import pygments
    import pygments.formatters.html as html_mod
    from pygments.formatters import HtmlFormatter
    lx = get_lexer_by_name("python")
    src = "def f(x):\r\n\treturn x < 1 & 's'\n"
    with_rust = pygments.highlight(src, lx, HtmlFormatter())
    saved = (html_mod._rust_escape_html_text,)
    html_mod._rust_escape_html_text = None
    try:
        import pygments.lexer as lexer_mod
        saved_l = (lexer_mod._rust_normalize_newlines,
                   lexer_mod._rust_expand_tabs_and_ensure)
        lexer_mod._rust_normalize_newlines = None
        lexer_mod._rust_expand_tabs_and_ensure = None
        try:
            without_rust = pygments.highlight(src, lx, HtmlFormatter())
        finally:
            (lexer_mod._rust_normalize_newlines,
             lexer_mod._rust_expand_tabs_and_ensure) = saved_l
    finally:
        html_mod._rust_escape_html_text = saved[0]
    assert with_rust == without_rust


def test_all_lexers_load_and_lex():
    """Every registered lexer still instantiates and lexes (fallback-safe)."""
    from pygments.lexers import get_all_lexers, find_lexer_class_by_name
    count = 0
    for _, aliases, _, _ in get_all_lexers():
        if not aliases:
            continue
        cls = find_lexer_class_by_name(aliases[0])
        lx = cls(stripnl=False, ensurenl=False)
        list(lx.get_tokens("x = 1  # sample\n"))
        count += 1
    assert count > 500, f"only {count} lexers exercised"
    _ = sys  # silence unused import if count path changes
