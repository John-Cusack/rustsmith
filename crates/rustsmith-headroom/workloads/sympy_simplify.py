"""Headroom workload: SymPy simplify kernel.

Doc-driven synthetic: ``trigsimp`` of a trig identity plus ``simplify``
of a trig-augmented rational expression — the simplify-core hot path the
kernel-scoped port thesis names (rerank §3 #3). Setup builds the
expressions once; run() executes the kernel ops.
"""

EXPR = None
RAT = None


def setup():
    global EXPR, RAT
    from sympy import sin, cos
    from sympy.abc import x

    EXPR = (
        sin(x) ** 6
        + cos(x) ** 6
        + 3 * sin(x) ** 2 * cos(x) ** 2 * (sin(x) ** 2 + cos(x) ** 2)
    )
    RAT = (x**6 - 1) / (x**3 - x**2 + x - 1)


def run():
    from sympy import simplify, trigsimp

    for _ in range(40):
        s = trigsimp(EXPR)
        simplify(RAT + s)
