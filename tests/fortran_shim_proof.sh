#!/bin/bash
# Fortran ABI shim proof for Elmer T003 (ADR-027 fixture plan).
# Grades the three shim shapes to divergence 0 and observes both negatives
# plus the COMMON refusal. Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the fixture or the code.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIX="$ROOT/tests/fixtures/fortran_shim"
WORK="$(mktemp -d /tmp/shim-proof-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "== fortran_shim proof: work=$WORK"

echo "-- pin gfortran (layout is version-specific, ADR-027 §2)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: fixture pinned to gfortran 13.3.0, re-dump on mismatch"; exit 1 ;;
esac

echo "-- orig build + reference"
gfortran -J"$WORK" -c "$FIX/src/shim_mod.F90" -o "$WORK/shim_mod.o"
gfortran -J"$WORK" -c "$FIX/src/scale.F" -o "$WORK/scale.o"
gfortran -J"$WORK" -c "$FIX/tests/probe_shim.F90" -o "$WORK/probe.o"
gfortran -o "$WORK/probe_orig" "$WORK/probe.o" "$WORK/shim_mod.o" "$WORK/scale.o"
"$WORK/probe_orig" > "$WORK/orig.out"
diff "$FIX/ref/expected_stdout.txt" "$WORK/orig.out"
echo "   orig matches checked-in reference"
echo "-- shim ports build (substitute mechanics: drop Fortran .o, link Rust)"
cp -r "$FIX/ref/rust" "$WORK/rust"
(cd "$WORK/rust/shim_ports" && cargo build --offline -q 2>/dev/null || cargo build -q)
PORT_LIB="$(ls "$WORK/rust/shim_ports/target/debug/libshim_ports.a")"
gfortran -o "$WORK/probe_ports" "$WORK/probe.o" "$PORT_LIB" -lgfortran
"$WORK/probe_ports" > "$WORK/ports.out"
diff "$WORK/orig.out" "$WORK/ports.out"
echo "   GREEN explicit_add + assumed_sum + fscale: divergence 0"

echo "-- negative 1: raw pointer into assumed-shape MUST diverge, exit 0"
(cd "$WORK/rust/shim_neg_rawptr" && cargo build --offline -q 2>/dev/null || cargo build -q)
NEG_LIB="$(ls "$WORK/rust/shim_neg_rawptr/target/debug/libshim_neg_rawptr.a")"
gfortran -o "$WORK/probe_neg" "$WORK/probe.o" "$NEG_LIB" -lgfortran
"$WORK/probe_neg" > "$WORK/neg.out"
echo "   neg exit: 0, output:"; sed 's/^/   | /' "$WORK/neg.out"
if diff -q "$WORK/orig.out" "$WORK/neg.out" >/dev/null; then
  echo "FAIL: raw-pointer fixture went green (silent wrong science)"
  exit 1
fi
echo "   OBSERVED raw-pointer diverges (documents the old refusal)"

echo "-- negative 2: zeroed-dtype descriptor MUST fail fast"
gcc -c "$FIX/tests/driver_zeroed_dtype.c" -o "$WORK/zeroed.o"
gfortran -o "$WORK/zeroed" "$WORK/zeroed.o" "$WORK/shim_mod.o" -lgfortran
set +e
"$WORK/zeroed" > "$WORK/zeroed.out" 2>&1
ZEROED_CODE=$?
set -e
echo "   zeroed exit: $ZEROED_CODE"
if [ "$ZEROED_CODE" -eq 0 ]; then
  echo "FAIL: zeroed descriptor went green"; exit 1
fi
echo "   OBSERVED zeroed-dtype fails fast"

echo "-- COMMON unit MUST pin the check_substitutable refusal"
cargo test -q -p rustsmith-adapters fortran_shim_common_refusal_pins_message 2>&1 | tail -3
echo "   OBSERVED COMMON refusal pinned"

echo "SHIM PROOF GREEN"
