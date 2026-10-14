#!/bin/bash
# splice-batch.sh: manifest-driven minimal splice + one provider per final binary.
# Usage: splice-batch.sh <manifest> <extract-dir>
# Manifest lines (whitespace-separated, `#` comments, blank lines ignored):
#   <stem> <archive> <object> [provider]
# provider = `provider` for exactly one object per final binary (the object
# carries `ld -r --whole-archive <archive>`); all other objects get the minimal
# `ld -r unit.o shim.o` form. Member selection via select-members.sh (named
# `ar x` only). Every object target is `test -f` checked first:
# DependInfo-listed-but-missing objects (e.g. phantom `elmergrid_core`
# `egutils.c.o` with ElmerGUI off) are warnings, never errors (spike §3.4).
set -u
[ $# -eq 2 ] || { echo "usage: $0 <manifest> <extract-dir>" >&2; exit 2; }
manifest=$1; xdir=$2
[ -f "$manifest" ] || { echo "splice-batch: manifest not found: $manifest" >&2; exit 2; }
mkdir -p "$xdir" || exit 2
here=$(dirname "$0")
n_ok=0; n_phantom=0; n_fail=0
while read -r stem ar obj prov; do
  case "$stem" in ""|\#*) continue;; esac
  if [ ! -f "$obj" ]; then echo "splice-batch: WARN phantom target, skipped: $obj"; n_phantom=$((n_phantom+1)); continue; fi
  [ -f "$ar" ] || { echo "splice-batch: FAIL archive not found: $ar"; n_fail=$((n_fail+1)); continue; }
  d="$xdir/$stem"; rm -rf "$d"
  if ! "$here/select-members.sh" "$ar" "$stem" "$d" > "$d.select.log" 2>&1; then
    echo "splice-batch: FAIL select-members $stem:"; cat "$d.select.log"; n_fail=$((n_fail+1)); continue
  fi
  unit=$(sed -n 's/^unit=//p' "$d.select.log"); shim=$(sed -n 's/^shim=//p' "$d.select.log")
  if [ "${prov:-}" = "provider" ]; then
    if ld -r -o "$obj" --whole-archive "$ar" --no-whole-archive 2> "$d.ld.log"; then
      echo "splice-batch: $stem -> $obj (provider)"; n_ok=$((n_ok+1))
    else echo "splice-batch: FAIL provider ld -r $stem"; tail -n 3 "$d.ld.log"; n_fail=$((n_fail+1)); fi
  else
    if ld -r -o "$obj" "$d/$unit" "$d/$shim" 2> "$d.ld.log"; then
      echo "splice-batch: $stem -> $obj (minimal)"; n_ok=$((n_ok+1))
    else echo "splice-batch: FAIL minimal ld -r $stem"; tail -n 3 "$d.ld.log"; n_fail=$((n_fail+1)); fi
  fi
done < "$manifest"
echo "splice-batch: ok=$n_ok phantom=$n_phantom fail=$n_fail"
[ "$n_fail" -eq 0 ]
