#!/bin/bash
# T020 callback-ABI fixtures: the caller-side shapes a Rust solver-entry
# cdylib needs when IT calls into Elmer (GetReal-style hidden lengths,
# derived-type function results, allocatable-component hand-off).
# Each shape is measured on pinned gfortran 13.3.0 with a negative control;
# unmeasured shapes stay refused. Work lives under the lane scratch dir.
# Honest halts: any unexpected green or any divergence in `pos` is a FAIL.
# NEVER edit this script to make it pass — fix the fixture or the caller.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LANE="${RS_ELMER_T020_LANE:-/home/john/firstmate/data/rs-elmer-t020}"
FIX="$ROOT/tests/fixtures/solver_callback"
WORK="$LANE/scratch/proof-cb"
mkdir -p "$WORK"
rm -rf "$WORK/cb_mod.o" "$WORK/probe.o" "$WORK"/probe_* "$WORK"/*.out "$WORK/rust"
echo "== T020 callback-ABI proof: work=$WORK"
T0=$SECONDS

echo "-- pin toolchain (calling convention is version-specific, ADR-027 §1/§3)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: fixtures pinned to gfortran 13.3.0, re-measure on mismatch"; exit 1 ;;
esac
echo "   $(rustc --version)"

echo "-- orig build + reference"
gfortran -J"$WORK" -c "$FIX/src/cb_mod.F90" -o "$WORK/cb_mod.o"
gfortran -J"$WORK" -c "$FIX/tests/probe_cb.F90" -o "$WORK/probe.o"
gfortran -o "$WORK/probe_orig" "$WORK/probe.o" "$WORK/cb_mod.o"
"$WORK"/probe_orig > "$WORK/orig.out"
diff "$FIX/ref/expected_stdout.txt" "$WORK/orig.out"
echo "   orig matches checked-in reference"

echo "-- Rust callers build (Rust main links the Fortran object)"
cp -r "$FIX/ref/rust" "$WORK/rust"
export RUSTFLAGS="-C link-arg=$WORK/cb_mod.o -C link-arg=-L/usr/lib/gcc/x86_64-linux-gnu/13 -C link-arg=-lgfortran"
(cd "$WORK/rust" && cargo build --offline -q 2>/dev/null || cargo build -q)
unset RUSTFLAGS

echo "-- positive caller MUST be byte-identical to the Fortran reference"
"$WORK/rust/target/debug/pos" > "$WORK/pos.out"
diff "$WORK/orig.out" "$WORK/pos.out"
echo "   GREEN hidden-length + derived-result + alias + allocatable hand-off"

echo "-- negative 1: wrong hidden length MUST diverge, exit 0"
"$WORK/rust/target/debug/neglen" > "$WORK/neglen.out"
echo "   neglen exit: 0, output:"; sed 's/^/   | /' "$WORK/neglen.out"
if diff -q "$WORK/orig.out" "$WORK/neglen.out" >/dev/null; then
  echo "FAIL: wrong-length caller went green (length arg decorative?)"
  exit 1
fi
grep -q "LEN=3 S=\[Sou\]" "$WORK/neglen.out" || {
  echo "FAIL: wrong-length truncation does not look like a short LEN"; exit 1; }
echo "   OBSERVED truncation to the passed length (length is load-bearing)"

echo "-- negative 2: NULL result slot MUST fail fast (nonzero exit)"
set +e
"$WORK/rust/target/debug/negres" > "$WORK/negres.out" 2>&1
NEGRES_CODE=$?
set -e
echo "   negres exit: $NEGRES_CODE"
if [ "$NEGRES_CODE" -eq 0 ]; then
  echo "FAIL: NULL result slot went green"; exit 1
fi
echo "   OBSERVED hidden result arg is required"

echo "-- negative 3: double fill without free MUST fail fast (nonzero exit)"
set +e
"$WORK/rust/target/debug/negdbl" > "$WORK/negdbl.out" 2>&1
NEGDBL_CODE=$?
set -e
echo "   negdbl exit: $NEGDBL_CODE"
if [ "$NEGDBL_CODE" -eq 0 ]; then
  echo "FAIL: double fill went green"; exit 1
fi
grep -qi "already allocated" "$WORK/negdbl.out" || {
  echo "FAIL: double fill failed for the wrong reason:"; sed 's/^/   | /' "$WORK/negdbl.out"; exit 1; }
echo "   OBSERVED libgfortran ownership refusal"

echo "CALLBACK-ABI PROOF GREEN"
