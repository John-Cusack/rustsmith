#!/bin/bash
# select-members.sh: classify one Rust staticlib's members for minimal splice.
# Usage: select-members.sh <archive> <stem> <outdir>
# Prints two lines: unit=<member> / shim=<member>
# Extracts ONLY those two members via named `ar x` into <outdir>.
# Never materializes the ~230 runtime members. Fails honestly (exit 1) when
# the unit or shim member is missing or ambiguous.
# Shape (spike §3.4, ADR-031): unit member `<stem>-*.<stem>.*-cgu.*.rcgu.o`
# defines the port symbols; shim member `<stem>-*.<meta>.rcgu.o` defines the
# 5 `__rustc::__rust_alloc*` symbols; the prelude member defines zero text
# symbols and is never placed.
set -u
if [ $# -ne 3 ]; then echo "usage: $0 <archive> <stem> <outdir>" >&2; exit 2; fi
ar=$1; stem=$2; out=$3
[ -f "$ar" ] || { echo "select-members: archive not found: $ar" >&2; exit 1; }
members=$(ar t "$ar") || { echo "select-members: ar t failed: $ar" >&2; exit 1; }
unit=$(printf '%s\n' "$members" | grep -E "^${stem}-.*\.${stem}\..*-cgu\..*\.rcgu\.o$" || true)
shim=$(printf '%s\n' "$members" | grep -E "^${stem}-.*\.rcgu\.o$" | grep -v -E "\.${stem}\." || true)
[ -n "$unit" ] || { echo "select-members: no unit member for stem '$stem' in $ar" >&2; exit 1; }
[ -n "$shim" ] || { echo "select-members: no alloc-shim member for stem '$stem' in $ar" >&2; exit 1; }
[ "$(printf '%s\n' "$unit" | wc -l)" -eq 1 ] || { echo "select-members: ambiguous unit members for '$stem': $unit" >&2; exit 1; }
[ "$(printf '%s\n' "$shim" | wc -l)" -eq 1 ] || { echo "select-members: ambiguous shim members for '$stem': $shim" >&2; exit 1; }
mkdir -p "$out" || exit 1
(cd "$out" && ar x "$ar" "$unit" "$shim") || { echo "select-members: named ar x failed" >&2; exit 1; }
echo "unit=$unit"
echo "shim=$shim"
