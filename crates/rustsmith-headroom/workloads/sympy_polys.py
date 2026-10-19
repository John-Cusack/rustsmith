"""Headroom workload: SymPy polys kernel (domains + arithmetic).

Doc-driven synthetic: dense multivariate polynomial multiply + GCD over
``QQ[x,y,z]`` plus factorization — the polys-domain hot path the
kernel-scoped port thesis names (rerank §3 #3). Setup builds the
expressions once; run() executes the kernel ops.
"""

P1 = None
E = None


def setup():
    global P1, E
    from sympy import Poly, expand
    from sympy.abc import x, y, z
    from sympy import QQ

    a = expand((x + y + z + 1) ** 15)
    P1 = Poly(a, x, y, z, domain=QQ)
    E = expand((x**2 + 2 * x * y + y**2 - z**2) ** 6)


def run():
    from sympy import factor, gcd

    for _ in range(6):
        q = P1 * P1
        gcd(q, P1)
    for _ in range(6):
        factor(E)
