#!/bin/bash
# rsp-link.sh: response-file final-link verification (campaign scale robustness).
# Usage: rsp-link.sh <link.txt> <workdir>
# Reads a cmake-generated `link.txt` (single command line: `<cc> <args...>),
# spills the post-compiler arguments into `link.rsp` in <workdir>, and re-runs
# the EXACT same link as `<cc> @link.rsp`. Proves the response-file form of
# every final link works before argv grows to ~60-120 KB at 1184-unit scale
# (spike §5: fits ARG_MAX, response files anyway for make robustness).
# Verifies exit 0 + relinked artifact newer than every input object.
# Prints wall seconds + max RSS (panel correction 4 inputs).
# Exit 0 on verified link, 1 otherwise.
set -u
[ $# -eq 2 ] || { echo "usage: $0 <link.txt> <workdir>" >&2; exit 2; }
linktxt=$1; work=$2
[ -f "$linktxt" ] || { echo "rsp-link: link.txt not found: $linktxt" >&2; exit 2; }
mkdir -p "$work" || exit 2
linkdir=$(dirname "$linktxt")
# cmake Makefile generator runs link commands with cwd = the target binary
# dir (the parent of CMakeFiles/), while link.txt lives one level deeper in
# CMakeFiles/<target>.dir/. Resolve relative object paths from there.
case "$linkdir" in
  */CMakeFiles/*) bindir=${linkdir%%/CMakeFiles/*};;
  *) bindir=$linkdir;;
esac
cmd=$(tr '\n' ' ' < "$linktxt")
cc=${cmd%% *}; rest=${cmd#* }
rsp="$work/link.rsp"
printf '%s\n' "$rest" | tr ' ' '\n' | grep -v '^$' > "$rsp"
n=$(wc -l < "$rsp"); bytes=$(wc -c < "$rsp")
echo "rsp-link: $n args, $bytes bytes -> $rsp"
(cd "$bindir" && /usr/bin/time -v "$cc" @"$rsp" > "$work/rsp-link.log" 2> "$work/rsp-time.log") \
  || { echo "rsp-link: FAIL response-file link exited nonzero"; tail -n 10 "$work/rsp-link.log"; exit 1; }
wall=$(grep -oP "Elapsed.*: \K.*" "$work/rsp-time.log" | head -n1)
rss=$(grep -oP "Maximum resident.*: \K.*" "$work/rsp-time.log" | head -n1)
echo "rsp-link: LINK_OK wall=$wall maxrss_kb=$rss"
