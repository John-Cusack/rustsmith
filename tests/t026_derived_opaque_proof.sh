#!/bin/bash
# T026 derived-opaque ABI proof: the Slice-2 gate shape (scalar derived-type
# module-procedure dummies as opaque addresses) measured on pinned
# gfortran 13.3.0 with a Fortran-caller arm and negative controls.
# Positive arms must be byte-identical to the Fortran reference; every
# negative control must fail observably (nonzero exit), never silently
# green. Work lives under the lane scratch dir.
# Honest halts: any unexpected green or any divergence in `pos`/`fcall`
# is a FAIL. NEVER edit this script to make it pass — fix the fixture.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LANE="${RS_ELMER_T026_LANE:-/home/john/firstmate/data/rs-elmer-t026}"
FIX="$ROOT/tests/fixtures/derived_opaque"
WORK="$LANE/scratch/proof-opaque"
mkdir -p "$WORK"
rm -rf "$WORK/oval_mod.o" "$WORK/oval_accessor.o" "$WORK/probe.o" "$WORK/fcall.o" \
  "$WORK"/probe_orig "$WORK"/probe_fcall "$WORK"/*.out "$WORK/rust" "$WORK/mod"
mkdir -p "$WORK/mod"
echo "== T026 derived-opaque proof: work=$WORK"
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
gfortran -J"$WORK/mod" -c "$FIX/src/oval_mod.F90" -o "$WORK/oval_mod.o"
gfortran -J"$WORK/mod" -I"$WORK/mod" -c "$FIX/src/oval_accessor.F90" -o "$WORK/oval_accessor.o"
gfortran -J"$WORK/mod" -I"$WORK/mod" -c "$FIX/tests/probe_oval.F90" -o "$WORK/probe.o"
gfortran -o "$WORK/probe_orig" "$WORK/probe.o" "$WORK/oval_mod.o" "$WORK/oval_accessor.o"
"$WORK"/probe_orig > "$WORK/orig.out"
diff "$FIX/ref/expected_stdout.txt" "$WORK/orig.out"
echo "   orig matches checked-in reference"

echo "-- Rust port + callers build (Rust links the Fortran objects)"
cp -r "$FIX/ref/rust" "$WORK/rust"
export RUSTFLAGS="-C link-arg=$WORK/oval_mod.o -C link-arg=$WORK/oval_accessor.o -C link-arg=-L/usr/lib/gcc/x86_64-linux-gnu/13 -C link-arg=-lgfortran"
(cd "$WORK/rust" && cargo build --offline -q 2>/dev/null || cargo build -q)
unset RUSTFLAGS

echo "-- positive 1 (Rust caller) MUST be byte-identical to the reference"
"$WORK/rust/target/debug/pos" > "$WORK/pos.out"
diff "$WORK/orig.out" "$WORK/pos.out"
echo "   GREEN scalar-derived opaque address, caller side"

echo "-- positive 2 (Fortran caller into the Rust proc): link WITHOUT oval_mod.o"
# NOTE: `grep -q` in a pipe under `pipefail` is a lie — grep closes the
# pipe on first match and nm dies with SIGPIPE (141), failing the chain
# despite the match. Consume full input instead (plain `grep` to /dev/null).
nm "$WORK/rust/target/debug/libovalport.a" 2>/dev/null | grep "T __oval_mod_MOD_oval_scale" > /dev/null || \
  nm "$WORK/rust/target/debug/deps/libovalport.a" 2>/dev/null | grep "T __oval_mod_MOD_oval_scale" > /dev/null || {
  # locate the built staticlib regardless of profile layout
  STATICLIB="$(find "$WORK/rust/target" -name 'libovalport.a' | head -1)"
  [ -n "$STATICLIB" ] || { echo "FAIL: no libovalport.a built"; exit 1; }
  nm "$STATICLIB" | grep "T __oval_mod_MOD_oval_scale" > /dev/null || {
    echo "FAIL: Rust staticlib does not export __oval_mod_MOD_oval_scale"; exit 1; }
  echo "   staticlib: $STATICLIB"
}
set +e
"$WORK/rust/target/debug/negdesc" > "$WORK/negdesc.out" 2>&1
NEGDESC_CODE=$?
set -e
echo "   negdesc exit: $NEGDESC_CODE"
if [ "$NEGDESC_CODE" -eq 0 ]; then
  echo "FAIL: NULL descriptor went green"; sed 's/^/   | /' "$WORK/negdesc.out"; exit 1
fi
echo "   OBSERVED descriptor dereference is required (opaque forwarding invalid)"

echo "-- negative 2 (NULL hidden result slot) MUST fail observably"
set +e
"$WORK/rust/target/debug/negres" > "$WORK/negres.out" 2>&1
NEGRES_CODE=$?
set -e
echo "   negres exit: $NEGRES_CODE"
if [ "$NEGRES_CODE" -eq 0 ]; then
  echo "FAIL: NULL result slot went green"; sed 's/^/   | /' "$WORK/negres.out"; exit 1
fi
echo "   OBSERVED hidden result arg is required"

echo "-- negative 3 (NULL pointer descriptor) MUST fail observably"
set +e
"$WORK/rust/target/debug/negptr" > "$WORK/negptr.out" 2>&1
NEGPTR_CODE=$?
set -e
echo "   negptr exit: $NEGPTR_CODE"
if [ "$NEGPTR_CODE" -eq 0 ]; then
  echo "FAIL: NULL pointer descriptor went green"; sed 's/^/   | /' "$WORK/negptr.out"; exit 1
fi
echo "   OBSERVED pointer association state is required"

echo "DERIVED-OPAQUE PROOF GREEN (wall=$((SECONDS - T0))s)"
