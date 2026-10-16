#!/bin/bash
# strong-gate.sh: refined strong-symbol gate, re-run per merge (panel corrections 1-3).
# Usage: strong-gate.sh [--cache DIR] [--strict-global] <linkset-file> <archive>...
# linkset-file lines: `<binary>: <archive-path>` (same archive may serve several
# binaries). The gate asserts ZERO strong port-level dups WITHIN each binary's
# replacement+native link inputs. Cross-binary collisions are informational only:
# disjoint binaries legitimately repeat symbols (e.g. two `main`s in Matc_bin
# vs ElmerGrid) and never share a link line.
# Symbol rules (spike §3.1-3.2): `nm -g --defined-only`, all types, skip
# ar-header lines (require NF==3). Strong = type not in {W w V v}. Toolchain
# classes filtered: ^_R ^_ZN ^rust_ ^anon ^__* EXCEPT Fortran __<mod>_MOD_<proc>
# exports, which are real code (panel correction 1). Weak libm/compiler-builtin
# dups (W) are permanent and never asserted away; same-toolchain members carry
# the byte-identity argument (proven per campaign, not re-proven here).
# --cache DIR reuses per-archive symbol inventories across merges: keyed on
# (dev, ino, size, mtime); prints `cache-hit`/`cache-miss` counts to stderr.
# --strict-global additionally fails on any cross-binary strong dup (for slices
# that must be globally disjoint).
# Exit 0 on pass, 1 on gate violation, 2 on usage/IO error.
set -u
cache=""; strict=0
while [ $# -gt 0 ]; do case "$1" in
  --cache) cache=$2; shift 2;;
  --strict-global) strict=1; shift;;
  --) shift; break;;
  -*) echo "usage: $0 [--cache DIR] [--strict-global] <linkset> <archive>..." >&2; exit 2;;
  *) break;;
esac; done
[ $# -ge 2 ] || { echo "usage: $0 [--cache DIR] [--strict-global] <linkset> <archive>..." >&2; exit 2; }
linkset=$1; shift
[ -f "$linkset" ] || { echo "strong-gate: linkset not found: $linkset" >&2; exit 2; }

[ -n "$cache" ] && { mkdir -p "$cache" || { echo "strong-gate: cannot create cache dir: $cache" >&2; exit 2; }; }
tmp=$(mktemp -d) || exit 2
trap 'rm -rf "$tmp"' EXIT
hits=0; misses=0
# Build one inventory per archive (cached), then project onto binaries.
> "$tmp/all.tsv"   # sym \t type \t archive
for a in "$@"; do
  [ -f "$a" ] || { echo "strong-gate: archive not found: $a" >&2; exit 2; }
  key=$(stat -c "%d:%i:%s:%Y" "$a")
  base=$(printf '%s' "$a" | sha256sum | cut -c1-16)
  inv="$tmp/inv-$base.tsv"
  if [ -n "$cache" ] && [ -f "$cache/$base.syms" ] && [ -f "$cache/$base.meta" ] \
     && [ "$(cat "$cache/$base.meta")" = "$key $a" ]; then
    cp "$cache/$base.syms" "$inv"; hits=$((hits+1))
  else
    nm -g --defined-only "$a" 2>/dev/null | awk 'NF==3 {print $3 "\t" $2}' > "$inv" \
      || { echo "strong-gate: nm failed: $a" >&2; exit 2; }
    misses=$((misses+1))
    if [ -n "$cache" ]; then cp "$inv" "$cache/$base.syms"; printf '%s %s' "$key" "$a" > "$cache/$base.meta"; fi
  fi
  awk -v ar="$a" '{print $1 "\t" $2 "\t" ar}' "$inv" >> "$tmp/all.tsv"
done
# Port-level strong symbols: drop weak + toolchain (keep Fortran MOD exports).
awk -F'\t' '$2!="W" && $2!="w" && $2!="V" && $2!="v" {print $1 "\t" $2 "\t" $3}' "$tmp/all.tsv" | awk -F'\t' '$1 !~ /^_R/ && $1 !~ /^_ZN/ && $1 !~ /^rust_/ && $1 !~ /^anon/ && ($1 !~ /^__/ || $1 ~ /_MOD_/)' > "$tmp/strong.tsv"
fail=0
# Per-binary pass: for each strong port symbol, count defining archives per binary.
# Simpler robust pass (no nested-subshell pitfalls): recompute per binary in awk.
awk -F'\t' '{print $1}' "$tmp/strong.tsv" | sort -u > "$tmp/syms.txt"
: > "$tmp/violations.txt"
while read -r sym; do
  [ -n "$sym" ] || continue
  archives=$(awk -F'\t' -v s="$sym" '$1==s {print $3}' "$tmp/strong.tsv" | sort -u)
  # for each binary, count how many of ITS archives define sym
  while read -r bin; do
    [ -n "$bin" ] || continue
    bin_archives=$(grep -F "$bin:" "$linkset" | sed 's/^[^:]*:[[:space:]]*//')
    n=0; holders=""
    for ba in $bin_archives; do
      if printf '%s\n' "$archives" | grep -qxF "$ba"; then n=$((n+1)); holders="$holders $ba"; fi
    done
    if [ "$n" -gt 1 ]; then echo "DUP binary=$bin sym=$sym holders=$holders" >> "$tmp/violations.txt"; fi
  done < <(sed 's/:.*//' "$linkset" | sort -u)
done < "$tmp/syms.txt"
if [ -s "$tmp/violations.txt" ]; then fail=1; cat "$tmp/violations.txt"; fi
# Cross-binary informational collisions (partitioned by construction).
awk -F'\t' '{print $1}' "$tmp/strong.tsv" | sort | uniq -d > "$tmp/xbin.txt"
if [ -s "$tmp/xbin.txt" ]; then
  echo "strong-gate: cross-binary strong collisions (informational, partitioned):" >&2
  while read -r sym; do
    [ -n "$sym" ] || continue
    echo "  xbin $sym: $(awk -F'\t' -v s="$sym" '$1==s {print $3}' "$tmp/strong.tsv" | sort -u | tr '\n' ' ')" >&2
  done < "$tmp/xbin.txt"
  if [ "$strict" -eq 1 ]; then fail=1; echo "strong-gate: --strict-global: failing on cross-binary dups" >&2; fi
fi
[ "$fail" -eq 0 ] && echo "STRONG_GATE_GREEN ($(wc -l < "$tmp/strong.tsv") strong port symbols, 0 per-binary dups)"
exit "$fail"
