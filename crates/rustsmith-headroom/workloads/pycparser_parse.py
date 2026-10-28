"""Headroom workload: pycparser TU parse (lexer + recursive-descent parser).

Doc-driven synthetic, frozen per WORKLOAD.md (rs-port-pycparser lane):
setup() builds two deterministic TUs with a seeded RNG (no I/O, no cpp);
run() parses each with a fresh CParser (legacy dialect, codegen-time shape:
strings in, AST out). Returns a checksum over len(ast.ext).
"""

import random

SYNTH_SMALL = ""
SYNTH_BIG = ""

_TYPES = [
    "int", "long", "unsigned int", "unsigned long", "short", "char",
    "signed char", "unsigned char", "float", "double", "long double",
    "unsigned long long",
]
_OPS = ["+", "-", "*", "/", "%", "<<", ">>", "&", "|", "^", "<", "<=",
        ">", ">=", "==", "!="]


def _gen_decls(rng, n):
    lines = ['#line 1 "synth_decl.c"', ""]
    lines.append("typedef unsigned long size_t;")
    lines.append("typedef int status_t;")
    lines.append("typedef struct point point_t;")
    lines.append("struct point { int x; int y; };")
    lines.append("union __u { int i; float f; unsigned char b[4]; };")
    lines.append("enum color { RED = 1, GREEN = 2, BLUE = 4, ANY = RED | GREEN };")
    lines.append("struct blob { unsigned kind : 3; unsigned flags : 5;"
                 " int vals[8]; const char *name; };")
    lines.append("static const double kPi = 3.141592653589793;")
    lines.append('static const char *kTag = "pyc" "parser" "2026";')
    lines.append("int g_counter = 0;")
    for i in range(n):
        t = rng.choice(_TYPES)
        lines.append(f"static {t} g_v{i} = {rng.randint(0, 1000)};")
    lines.append("int proto_add(int a, int b);")
    lines.append("void proto_cb(void *ctx, const char *msg, ...);")
    return "\n".join(lines) + "\n"


def _gen_func(rng, idx):
    t = rng.choice(_TYPES)
    lines = [f"static {t} fn{idx}(int a{idx}, long b{idx}) {{",
             f"    {t} acc = ({t})a{idx} + ({t})b{idx} * 3;",
             f"    unsigned long arr[12];",
             f"    int i, n = (int)(b{idx} & 0xff);"]
    for _ in range(rng.randint(6, 10)):
        op = rng.choice(_OPS)
        c = rng.randint(1, 255)
        lines.append(f"    acc = acc {op} (({t}){c} + (n % 17));")
    lines.append("    arr[0] = (unsigned long)acc;")
    lines.append('    const char *s = "a\\t" "b\\n" "\\x41\\101";')
    lines.append("    char ch = 'Z';")
    lines.append("    double d = 1.5e-3 + 0x1.8p+2 + 0777u + 0b1010L;")
    lines.append("    if ((n > 0) && (acc != 0) || (ch == 'Z')) {")
    lines.append("        acc += sizeof(arr) + sizeof(struct point);")
    lines.append("    } else if (n < 0) {")
    lines.append("        acc -= (n > -100 ? 7 : 13);")
    lines.append("    }")
    lines.append("    switch (n & 7) {")
    lines.append("    case 0: acc |= 1u; break;")
    lines.append("    case 1: case 2: acc ^= 3uL; break;")
    lines.append("    default: acc &= ~0u; break;")
    lines.append("    }")
    lines.append("    for (i = 0; i < n && i < 12; ++i) {")
    lines.append("        arr[i] = (unsigned long)(acc >> (i & 31));")
    lines.append("        if (arr[i] == 0uL) continue;")
    lines.append("        if (arr[i] > 1000000uL) break;")
    lines.append("    }")
    lines.append("    while (n > 100) { n /= 2; --acc; }")
    lines.append("    do { acc++; n--; } while (n > 50);")
    lines.append("    goto done;")
    lines.append("    acc = -acc;")
    lines.append("done:")
    lines.append("    return acc + (n > 0 ? (int)d : -((int)*s + ch));")
    lines.append("}")
    return "\n".join(lines)


def _gen_big(rng, nfuncs):
    lines = ['#line 1 "synth_func.c"', "",
             '#pragma once-ish pack(4)',
             "typedef struct node node_t;",
             "struct node { int key; node_t *left, *right; };",
             "extern int g_counter;",
             "_Static_assert(sizeof(int) >= 2, \"int too small\");", ""]
    for i in range(nfuncs):
        lines.append(_gen_func(rng, i))
        lines.append("")
    lines.append("int table_driven(int sel, int x) {")
    lines.append("    return _Generic((x), int: x * 2, long: (int)(x + 1),"
                 " default: -1) + sel;")
    lines.append("}")
    return "\n".join(lines) + "\n"


def setup():
    global SYNTH_SMALL, SYNTH_BIG
    rng = random.Random(20261028)
    SYNTH_SMALL = _gen_decls(rng, 220)
    SYNTH_BIG = _gen_big(rng, 300)


def run():
    from pycparser import CParser
    total = 0
    total += len(CParser().parse(SYNTH_SMALL, "synth_decl.c").ext)
    total += len(CParser().parse(SYNTH_BIG, "synth_func.c").ext)
    return total
