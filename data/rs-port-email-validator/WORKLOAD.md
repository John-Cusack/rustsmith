# WORKLOAD.md — rs-port-email-validator frozen workload (2026-10-29)

All benches run with `check_deliverability=False` (network excluded by
construction). Corpus is deterministic (`seed=20261029`); sizes count
addresses per `run()` call. Machine: Ryzen 9 5950X, CPython 3.12.3.

## Tracks

| Track | Name | N | Mix |
|---|---|---|---|
| representative | `mixed-2000` | 2000 | 60% valid ASCII (`user.name+tag-N@mailM.example.com`), 10% valid IDN (`Üser-N@münchen-K.de`, `user@xn--...` ascii-IDNA in, `test@日本語.jp`), 8% quoted-local (`"quoted name N"@example.com`), 22% invalid (no-@, bad domain chars, leading/trailing dot, `..`, overlong local 70ch, special-use `user@gmail.test`, dotless `user@localhost`, overlong label 64ch) |
| showcase | `ascii-valid-5000` | 5000 | 100% valid ASCII, short domains — best case for a syntax kernel |
| showcase | `idn-heavy-1000` | 1000 | 100% internationalized domains — idna-dominated ceiling probe |
| overhead | `x1-valid`, `x1-invalid`, `x1-idn` | 1 | single-address crossing floor (per-call toll rows) |

## What is timed

`validate_email(addr, check_deliverability=False)` per address in a
plain Python loop (baseline arm). Wall = `time.perf_counter` around the
loop; peak RSS = child-process `ru_maxrss` both arms. Attribution arm:
`cProfile` over one `run()`, frames bucketed into `email_validator`
(own cost), `idna` (shared-workload caveat: pure Python on this path,
not a C engine — porting email-validator cannot remove it unless the
kernel also absorbs IDNA), `re`, `stdlib-other`.

## Kill criteria (brief §1)

- Tier-1 gain recorded separately; ceiling recomputed vs tier-1 baseline.
- Kill at <2x over tier-1 on `mixed-2000`, or remaining time dominated
  by `idna`/stdlib outside email-validator's own code → honest no_gain.
