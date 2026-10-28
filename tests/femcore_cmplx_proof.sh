#!/bin/bash
# T020 solve_cmplx proof: the admitted unit of the KIND= re-grade —
# `fem/src/solve_cmplx.F90` (the complex twin of the ported solve_real).
#
# One file-scope external subprogram with legacy `COMPLEX(KIND=dp)` dummies
# (T020 legacy-parse fix); the port moves the wrapper into Rust and keeps
# the numerics in LAPACK (`zgetrf_`/`zgetrs_`), which makes the equivalence
# corpus bit-identical by construction.
#
# Self-contained except the Elmer checkout (ELMER_FORK) and the port crate
# in the port repo (ER): the driver and the corpus below are clean-room
# proof code, not upstream text.
# Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the proof code or the port.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ELMER_FORK="${ELMER_FORK:-/home/john/runs/elmer-fork}"
ER="${ER:-/home/john/firstmate/data/rs-elmer-t020/elmer-rust}"
LANE="${RS_ELMER_T020_LANE:-/home/john/firstmate/data/rs-elmer-t020}"
WORK="$LANE/scratch/proof-cmplx"
mkdir -p "$WORK"
rm -rf "$WORK"/probe_* "$WORK"/solve_cmplx_* "$WORK"/target
echo "== T020 solve_cmplx proof: work=$WORK"
T0=$SECONDS

echo "-- pin toolchain + sources (calling convention is version-specific, ADR-027 §1/§3)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: unexpected gfortran $GFORTRAN_VERSION" >&2; exit 1 ;;
esac
RUSTC_VERSION="$(rustc +nightly --version)"
echo "   $RUSTC_VERSION"
case "$RUSTC_VERSION" in
  *"1.100.0-nightly (feaadeeac 2026-09-19)"*) ;;
  *) echo "FAIL: unexpected rustc $RUSTC_VERSION" >&2; exit 1 ;;
esac
ELMER_COMMIT="$(git -C "$ELMER_FORK" rev-parse HEAD 2>/dev/null || echo unknown)"
echo "   elmer_fork=$ELMER_COMMIT"
if command -v ccache >/dev/null 2>&1; then
  export CCACHE_DIR="${ELMER_LANES_CACHE:-/home/john/firstmate/data/elmer-lanes-cache}"
  export CCACHE_SLOPPINESS="file_macro include_file_mtime include_file_ctime time_macros pch_defines"
  export CCACHE_MAXSIZE="20G"
  FC="ccache gfortran"
  echo "   ccache: $(ccache --version | head -1), CCACHE_DIR=$CCACHE_DIR"
else
  FC="gfortran"
  echo "   ccache: not installed on this host; compiling with plain gfortran"
fi
T_PIN=$SECONDS

echo "-- driver (clean-room corpus code)"
cat > "$WORK/probe_cmplx.F90" <<'EOF'
program probe_cmplx
implicit none
integer :: n
double complex :: a(9), x(3), b(4), y(2), c(1), d(1), e(1)
! case 1: 2x2 dense, b = A*(1,2)
n = 2
a(1:4) = (/ (2.0d0,0.0d0), (1.0d0,1.0d0), (1.0d0,-1.0d0), (3.0d0,0.0d0) /)
x(1:2) = (/ (7.0d0,1.0d0), (13.0d0,-2.0d0) /)
call solvelapack_cmplx(n, a, x)
print *, 'X=', x
call flush(6)
! case 2: N=0 early return (no output from the wrapper)
n = 0
call solvelapack_cmplx(n, a, x)
print *, 'N0-OK'
call flush(6)
! case 3: singular zero matrix (ZGETRF info print, then ZGETRS)
n = 2
a(1:4) = (0.0d0,0.0d0)
x(1:2) = (/ (1.0d0,0.0d0), (2.0d0,0.0d0) /)
call solvelapack_cmplx(n, a, x)
print *, 'SING-DONE'
call flush(6)
! case 4: 1x1 complex
n = 1
c(1) = (2.0d0,1.0d0)
d(1) = (7.0d0,3.0d0)
call solvelapack_cmplx(n, c, d)
print *, 'D=', d
call flush(6)
! case 5: 3x3 dense with non-trivial phases
n = 3
b = 0.0d0
a(1:9) = (/ (3.0d0,0.0d0), (1.0d0,2.0d0), (0.0d0,1.0d0), &
  (1.0d0,-2.0d0), (4.0d0,0.0d0), (2.0d0,1.0d0), &
  (0.0d0,-1.0d0), (2.0d0,-1.0d0), (5.0d0,0.0d0) /)
e = 0.0d0
x = (/ (1.0d0,0.0d0), (0.0d0,1.0d0), (2.0d0,2.0d0) /)
call solvelapack_cmplx(n, a, x)
print *, 'X3=', x
call flush(6)
end program probe_cmplx
EOF
T_DRIVER=$SECONDS

echo "-- reference build (Types stub + Fortran object + LAPACK)"
echo "   solve_cmplx consumes exactly one item from Types: the kind parameter"
echo "   dp (COMPLEX(KIND=dp)). The real Types.F90 unconditionally USEs Lua,"
echo "   so the reference stages a clean-room stub with the identical"
echo "   definition (INTEGER, PARAMETER :: dp = SELECTED_REAL_KIND(12) -> 8)"
echo "   in BOTH arms. The wrapper's observable behavior cannot see the"
echo "   difference: dp selects the same complex*16 layout either way."
cat > "$WORK/Types.F90" <<'EOF'
MODULE Types
   IMPLICIT NONE
   INTEGER, PARAMETER :: dp = SELECTED_REAL_KIND(12)
END MODULE Types
EOF
$FC -c "$WORK/Types.F90" -o "$WORK/types_ref.o" -J "$WORK"
test -f "$WORK/types_ref.o"
src="$ELMER_FORK/fem/src/solve_cmplx.F90"
test -f "$src" || { echo "FAIL: no $src" >&2; exit 1; }
$FC -c "$src" -o "$WORK/solve_cmplx_ref.o" -J "$WORK" -I "$WORK"
test -f "$WORK/solve_cmplx_ref.o"
$FC -c "$WORK/probe_cmplx.F90" -o "$WORK/probe_cmplx.o" -I "$WORK"
test -f "$WORK/probe_cmplx.o"
$FC -o "$WORK/probe_cmplx_orig" "$WORK/probe_cmplx.o" "$WORK/solve_cmplx_ref.o" \
  "$WORK/types_ref.o" -llapack -lblas
test -f "$WORK/probe_cmplx_orig"
set +e
"$WORK/probe_cmplx_orig" > "$WORK/solve_cmplx_orig.out" 2> "$WORK/solve_cmplx_orig.err"
echo "$?" > "$WORK/solve_cmplx_orig.rc"
set -e
echo "   solve_cmplx: reference lines=$(wc -l < "$WORK/solve_cmplx_orig.out") exit=$(cat "$WORK/solve_cmplx_orig.rc")"
T_REF=$SECONDS

echo "-- port build (Rust staticlib) + substitute (drop the Fortran object)"
test -f "$ER/crates/elmer-fem-a/ports/solvecmplx/src/lib.rs" \
  || { echo "FAIL: no port crate under ER=$ER" >&2; exit 1; }
(cd "$ER/crates/elmer-fem-a/ports/solvecmplx" && CARGO_TARGET_DIR="$WORK/target" cargo +nightly build -q)
PORT_LIB="$WORK/target/debug/libsolvecmplx.a"
test -f "$PORT_LIB" || { echo "FAIL: no $PORT_LIB" >&2; exit 1; }
$FC -o "$WORK/probe_cmplx_ports" "$WORK/probe_cmplx.o" \
  -Wl,--whole-archive "$PORT_LIB" -Wl,--no-whole-archive -llapack -lblas -lgfortran -lm
test -f "$WORK/probe_cmplx_ports"
set +e
"$WORK/probe_cmplx_ports" > "$WORK/solve_cmplx_ports.out" 2> "$WORK/solve_cmplx_ports.err"
echo "$?" > "$WORK/solve_cmplx_ports.rc"
set -e
diff "$WORK/solve_cmplx_orig.out" "$WORK/solve_cmplx_ports.out" \
  || { echo "FAIL: solve_cmplx stdout diverges" >&2; diff "$WORK/solve_cmplx_orig.out" "$WORK/solve_cmplx_ports.out" | head -20 >&2; exit 1; }
diff "$WORK/solve_cmplx_orig.err" "$WORK/solve_cmplx_ports.err" \
  || { echo "FAIL: solve_cmplx stderr diverges" >&2; diff "$WORK/solve_cmplx_orig.err" "$WORK/solve_cmplx_ports.err" | head -20 >&2; exit 1; }
diff "$WORK/solve_cmplx_orig.rc" "$WORK/solve_cmplx_ports.rc" \
  || { echo "FAIL: solve_cmplx exit status diverges" >&2; exit 1; }
echo "   solve_cmplx: byte-identical stdout+stderr+exit ($(cat "$WORK/solve_cmplx_ports.rc"))"
T_PORT=$SECONDS

echo "-- refined strong-symbol gate (runbook §2: zero STRONG port-level dups)"
nm -g --defined-only "$WORK/solve_cmplx_ref.o" | awk 'NF==3{print $2, $3}' | sort > "$WORK/solve_cmplx_orig.syms"
nm -g --defined-only "$PORT_LIB" | awk 'NF==3{print $2, $3}' | sort -u > "$WORK/solve_cmplx_port.syms"
grep -qx "T solvelapack_cmplx_" "$WORK/solve_cmplx_port.syms" \
  || { echo "FAIL: solvecmplx does not define solvelapack_cmplx_" >&2; exit 1; }
printf 'solvelapack_cmplx_\n' | sort > "$WORK/solve_cmplx_want.txt"
awk '{print $2}' "$WORK/solve_cmplx_orig.syms" | sort -u > "$WORK/solve_cmplx_orig.names"
awk '{print $2}' "$WORK/solve_cmplx_port.syms" | sort -u > "$WORK/solve_cmplx_port.names"
comm -12 "$WORK/solve_cmplx_orig.names" "$WORK/solve_cmplx_port.names" > "$WORK/solve_cmplx_inter.txt"
diff "$WORK/solve_cmplx_want.txt" "$WORK/solve_cmplx_inter.txt" \
  || { echo "FAIL: solvecmplx strong-symbol intersection is not exactly the replaced symbol" >&2; exit 1; }
echo "   solvecmplx: defines [solvelapack_cmplx_] as T; zero STRONG port-level dups beyond it"
echo "   solvecmplx: whole-file portable (the port replaces solve_cmplx.o whole)"
T_SYM=$SECONDS

echo "T020-CMPLX PROOF GREEN (pin=$((T_PIN - T0))s driver=$((T_DRIVER - T_PIN))s ref=$((T_REF - T_DRIVER))s port=$((T_PORT - T_REF))s sym=$((T_SYM - T_PORT))s)"
