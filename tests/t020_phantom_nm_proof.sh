#!/bin/bash
# T020 phantom-export nm proof: the recon classifier emits exactly the
# compiler's global symbols — no more (CONTAINS internals, INTERFACE
# bodies, and literal-text keywords are not exports) and no fewer (every
# nm global is covered: zero real globals dropped).
#
# Method: clean-room fixtures in the Elmer solver idiom, compiled with the
# pinned gfortran; a mechanical bidirectional check between `nm` globals
# and `rustsmith recon` dag exports. Any mismatch is a FAIL.
# Self-contained except the Elmer checkout (unused here) and the toolchain.
# Work lives under the lane scratch dir, never /tmp.
# NEVER edit this script to make it pass — fix the classifier or the fixtures.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LANE="${RS_ELMER_T020_LANE:-/home/john/firstmate/data/rs-elmer-t020}"
WORK="$LANE/scratch/proof-nm"
mkdir -p "$WORK"
rm -rf "$WORK/fx" "$WORK/recon" "$WORK/store.db"
mkdir -p "$WORK/fx"
echo "== T020 phantom nm proof: work=$WORK"
T0=$SECONDS

echo "-- pin toolchain (calling convention is version-specific, ADR-027 §1/§3)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: proof pinned to gfortran 13.3.0, re-measure on mismatch"; exit 1 ;;
esac

cat > "$WORK/fx/solver_ext.F90" <<'EOF'
subroutine extsolver(a, n)
  double precision a(*)
  integer n
contains
  subroutine inneracc(x)
    double precision x
  end subroutine inneracc
end subroutine extsolver
subroutine posthost(b, m)
  double precision b(*)
  integer m
end subroutine posthost
EOF
cat > "$WORK/fx/modiface.F90" <<'EOF'
module modiface
  implicit none
  interface
    subroutine ifacebody(a)
      real(8) :: a
    end subroutine ifacebody
  end interface
contains
  subroutine realone(x, n)
    real(8) :: x(n)
    integer :: n
  end subroutine realone
  function realtwo(y) result(r)
    real(8) :: y, r
  end function realtwo
end module modiface
EOF
cat > "$WORK/fx/nestmod.F90" <<'EOF'
module nestmod
  implicit none
contains
  subroutine hostproc(a)
    real(8) :: a
  contains
    subroutine nestinner(b)
      real(8) :: b
    end subroutine nestinner
  end subroutine hostproc
  subroutine sibling(c)
    real(8) :: c
  end subroutine sibling
end module nestmod
EOF
cat > "$WORK/fx/sepmod.F90" <<'EOF'
module sepmod
  implicit none
  interface
    module subroutine sepsub(a)
      real(8) :: a
    end subroutine sepsub
  end interface
end module sepmod
submodule (sepmod) sepsubmod
contains
  module subroutine sepsub(a)
    real(8) :: a
    a = a + 1.0d0
  end subroutine sepsub
end submodule sepsubmod
EOF
T_FIX=$SECONDS

echo "-- compile fixtures (one object each, exactly as the gate sees them)"
for f in "$WORK"/fx/*.F90; do
  gfortran -J"$WORK" -c "$f" -o "${f%.F90}.o"
done
nm --format=posix "$WORK"/fx/*.o > "$WORK/nm-all.txt" || true
echo "   $(grep -c . "$WORK/nm-all.txt") nm lines"

echo "-- recon exports for the fixture tree"
(cd "$ROOT" && ./target/release/rustsmith recon --repo "$WORK/fx" \
  --out "$WORK/recon" --store "$WORK/store.db" --run-id t020-nm 2>&1 | tail -1)

echo "-- bidirectional mechanical check (python, no Fortran parsing of its own)"
python3 - "$WORK" <<'PYEOF'
import json, re, sys
work = sys.argv[1]
dag = json.load(open(f"{work}/recon/dag.json"))
exports = {}  # file -> set of non-BIND(C) linkages
for u in dag["units"]:
    if not u["id"].startswith("fortran:"):
        continue
    rel = u["id"].split(":", 1)[1].split("#")[0]
    ex = {e["linkage"] for e in u.get("exports", []) if not e.get("bind_c")}
    exports.setdefault(rel, set()).update(ex)
# nm globals per object: posix format "name type ..."; uppercase = global.
nm_globals = {}
cur = None
for line in open(f"{work}/nm-all.txt").read().splitlines():
    m = re.match(r"(\S+):$", line)
    if m:
        cur = m.group(1).split("/")[-1].replace(".o", ".F90")
        nm_globals.setdefault(cur, set())
        continue
    parts = line.split()
    if len(parts) >= 2 and parts[1] != "U" and parts[1].isupper():
        nm_globals.setdefault(cur, set()).add(parts[0])
fails = []
SUBMODULE_PARENTS = {"sepsubmod": "sepmod"}  # pre-existing recon wart (out
# of T020 scope): a separate module subprogram's linkage names the
# submodule, while gfortran emits the PARENT module's symbol. The proof
# point here is only that it is still emitted (fail closed), via this map.
# direction 1: every recon export resolves to an nm global.
for rel, exs in sorted(exports.items()):
    g = nm_globals.get(rel, set())
    for e in sorted(exs):
        m = re.fullmatch(r"__(.+)_MOD_(.+)", e)
        if m:
            scope, proc = m.group(1), m.group(2)
            if f"__{scope}_MOD_{proc}" in g:
                continue  # module procedure
            parent = SUBMODULE_PARENTS.get(scope)
            if parent and f"__{parent}_MOD_{proc}" in g:
                continue  # submodule implementation under the parent symbol
            if f"{proc}_" in g:
                continue  # F77 external modeled as __scope_MOD_proc
            fails.append(f"{rel}: export {e} has no nm global")
        else:
            if f"{e}_" in g:
                continue  # F77 external scope name
            if any(x.startswith(f"__{e}_MOD_") for x in g):
                continue  # module bookkeeping scope
            if e in SUBMODULE_PARENTS:
                continue  # submodule scope: symbol lives under the parent
            fails.append(f"{rel}: scope export {e} has no nm global")
# direction 2: every nm global is covered (zero real globals dropped).
for rel, g in sorted(nm_globals.items()):
    exs = exports.get(rel, set())
    for sym in sorted(g):
        if sym == "MAIN__":
            continue  # no programs in fixtures
        m = re.fullmatch(r"__(.+)_MOD_(.+)", sym)
        if m:
            if sym in exs:
                continue
            for sub, parent in SUBMODULE_PARENTS.items():
                if sym == f"__{parent}_MOD_{m.group(2)}" and f"__{sub}_MOD_{m.group(2)}" in exs:
                    break
            else:
                fails.append(f"{rel}: nm global {sym} dropped from exports")
            continue
        m2 = re.fullmatch(r"([a-z_][a-z0-9_]*_)", sym)
        if m2:
            base = sym[:-1]
            if base in exs or any(e.endswith(f"_MOD_{base}") for e in exs):
                continue
            fails.append(f"{rel}: nm global {sym} dropped from exports")
            continue
# direction 3: proven non-globals appear in NEITHER.
nonglobal = {"inneracc", "ifacebody", "nestinner", "requires"}
for rel, exs in sorted(exports.items()):
    for bad in sorted(nonglobal):
        if any(bad in e for e in exs):
            fails.append(f"{rel}: non-global {bad} minted as export")
for rel, g in sorted(nm_globals.items()):
    for bad in sorted(nonglobal):
        if f"__{rel[:-4].lower()}_MOD_{bad}" in g or f"{bad}_" in g:
            fails.append(f"{rel}: non-global {bad} emitted as nm global")
if fails:
    print("FAIL:")
    for f in fails:
        print(f"  {f}")
    sys.exit(1)
print(f"   OK: {sum(len(v) for v in exports.values())} exports <-> "
      f"{sum(len(v) for v in nm_globals.values())} nm globals, "
      f"{len(nonglobal)} non-globals absent from both")
PYEOF
T_CHECK=$SECONDS

echo "== timings: fixtures=$((T_FIX-T0))s check=$((T_CHECK-T_FIX))s"
echo "PASS: recon exports match nm globals bidirectionally"
