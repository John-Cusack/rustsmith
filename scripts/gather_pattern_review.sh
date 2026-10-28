#!/usr/bin/env bash
# gather_pattern_review.sh — assemble one pattern-review bundle for a port.
#
# A bundle is the single input to the pattern-review pass described in
# docs/PATTERN_REVIEW.md: before/after hot-path files, all measured
# evidence (bench rows, headroom row, retro, ADRs), and a numbers.md
# extracting every speedup/RSS figure verbatim with provenance, so the
# reviewer cites numbers instead of re-typing them from prose.
#
# Usage:
#   scripts/gather_pattern_review.sh <port> <original-dir> <rust-dir> <out-dir> \
#       [--hot <symbol>]... [--before-file <path>]... [--after-file <path>]...
#
#   <port>         port name, e.g. sqlparse (used for data/headroom/<port>.json)
#   <original-dir> checkout of the original (or the mirror's vendored tree)
#   <rust-dir>     checkout of the Rust port (mirror/<port> or John-Cusack/*-rust)
#   <out-dir>      bundle output directory (created; must not be inside either input)
#
# Hot-file selection: explicit --before-file/--after-file win; otherwise
# --hot symbols are grepped (first 20 matching source files per side);
# otherwise the 20 largest source (.py/.rs/.c/.h/.f90) files per side.
# The manifest records which rule picked every file. Missing evidence is
# listed as MISSING in the manifest, never silently absent.
#
# Exit codes: 0 bundle written (gaps listed), 2 usage/input error.

set -euo pipefail

usage() {
    echo "usage: $0 <port> <original-dir> <rust-dir> <out-dir> [--hot <symbol>]... [--before-file <path>]... [--after-file <path>]..." >&2
    exit 2
}

[ $# -ge 4 ] || usage
PORT=$1; ORIG=$2; RUST=$3; OUT=$4; shift 4

HOT=(); BEFORE_FILES=(); AFTER_FILES=()
while [ $# -gt 0 ]; do
    case $1 in
        --hot)         [ $# -ge 2 ] || usage; HOT+=("$2"); shift 2 ;;
        --before-file) [ $# -ge 2 ] || usage; BEFORE_FILES+=("$2"); shift 2 ;;
        --after-file)  [ $# -ge 2 ] || usage; AFTER_FILES+=("$2"); shift 2 ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done

for d in "$ORIG" "$RUST"; do
    [ -d "$d" ] || { echo "not a directory: $d" >&2; exit 2; }
done
case $(readlink -m "$OUT") in
    "$(readlink -m "$ORIG")"/*|"$(readlink -m "$RUST")"/*)
        echo "out-dir must not be inside an input dir (the bundle would copy itself)" >&2; exit 2 ;;
esac

REPO_ROOT=$(git -C "$(dirname "$0")" rev-parse --show-toplevel 2>/dev/null || echo "")
mkdir -p "$OUT/before" "$OUT/after" "$OUT/evidence"
MANIFEST="$OUT/MANIFEST.md"
NUMBERS="$OUT/numbers.md"
: > "$MANIFEST"; : > "$NUMBERS"

git_head() { git -C "$1" rev-parse HEAD 2>/dev/null || echo "(not a git checkout)"; }

{
    echo "# Pattern-review bundle: $PORT"
    echo
    echo "- gathered: $(date -u +%Y-%m-%dT%H:%M:%SZ) on $(uname -srm)"
    echo "- original: $ORIG @ $(git_head "$ORIG")"
    echo "- rust port: $RUST @ $(git_head "$RUST")"
    echo "- hot symbols: ${HOT[*]:-(none — size heuristic or explicit files)}"
    echo
    echo "Process: docs/PATTERN_REVIEW.md (bundle → review pass → proposed guide diff)."
    echo "Cite figures only from numbers.md; every line carries file:line provenance."
    echo
    echo "## Files"
} >> "$MANIFEST"

# copy_one <src> <dest-subdir> <why>
copy_one() {
    local src=$1 sub=$2 why=$3 rel
    [ -f "$src" ] || { echo "- MISSING: $src ($why)" >> "$MANIFEST"; return 0; }
    rel=$(basename "$src")
    mkdir -p "$OUT/$sub"
    # disambiguate same-basename collisions with a path hash prefix
    if [ -e "$OUT/$sub/$rel" ]; then
        rel="$(echo "$src" | md5sum | cut -c1-8)-$rel"
    fi
    cp "$src" "$OUT/$sub/$rel"
    echo "- \`$sub/$rel\` ← \`$src\` ($why)" >> "$MANIFEST"
}

# pick_hot <dir> <dest-subdir> <label> — fills via --hot grep or size heuristic
pick_hot() {
    local dir=$1 sub=$2 label=$3 n=0 f sym
    if [ ${#HOT[@]} -gt 0 ]; then
        for sym in "${HOT[@]}"; do
            while IFS= read -r f; do
                [ $n -ge 20 ] && break
                copy_one "$f" "$sub" "$label: matches --hot '$sym'"; n=$((n+1))
            done < <(grep -rl --include='*.py' --include='*.rs' --include='*.c' \
                            --include='*.h' --include='*.f90' --include='*.F90' \
                            -e "$sym" "$dir" 2>/dev/null \
                     | grep -v -e '/target/' -e '/\.git/' | sort -u | head -20)
            [ $n -ge 20 ] && break
        done
    else
        while IFS= read -r f; do
            copy_one "$f" "$sub" "$label: size heuristic (no --hot given)"; n=$((n+1))
        done < <(find "$dir" \( -name '*.py' -o -name '*.rs' -o -name '*.c' -o -name '*.h' \
                                -o -name '*.f90' -o -name '*.F90' \) \
                      -not -path '*/target/*' -not -path '*/.git/*' -type f -printf '%s %p\n' \
                  | sort -rn | head -20 | cut -d' ' -f2-)
    fi
    [ $n -gt 0 ] || echo "- MISSING: no $label hot files found under $dir" >> "$MANIFEST"
}

echo "### before/ (original hot path)" >> "$MANIFEST"
if [ ${#BEFORE_FILES[@]} -gt 0 ]; then
    for f in "${BEFORE_FILES[@]}"; do copy_one "$f" before "explicit --before-file"; done
else
    pick_hot "$ORIG" before "original"
fi

echo "### after/ (Rust hot path + binding/seam)" >> "$MANIFEST"
if [ ${#AFTER_FILES[@]} -gt 0 ]; then
    for f in "${AFTER_FILES[@]}"; do copy_one "$f" after "explicit --after-file"; done
else
    pick_hot "$RUST" after "port"
fi

echo "### evidence/ (measured only — gaps are findings)" >> "$MANIFEST"
found_evidence=0
while IFS= read -r f; do
    copy_one "$f" evidence "port evidence"; found_evidence=1
done < <(find "$RUST" \( -path '*/bench/*' -o -path '*/benches/*' -o -path '*/bench_parser/*' \) \
              \( -name '*.md' -o -name '*.json' -o -name '*.py' \) \
              -not -path '*/target/*' -not -path '*/.git/*' -type f | sort)
for f in "$RUST/README.md" "$ORIG/README.md"; do
    [ -f "$f" ] && { copy_one "$f" evidence "README (bench tables / scope)"; found_evidence=1; }
done
for f in "$RUST"/*retro*.md "$RUST"/docs/*retro*.md "$RUST"/WORKLOAD.md "$RUST"/*/WORKLOAD.md; do
    [ -f "$f" ] && { copy_one "$f" evidence "retro / workload contract"; found_evidence=1; }
done
if [ -n "$REPO_ROOT" ]; then
    if [ -f "$REPO_ROOT/data/headroom/$PORT.json" ]; then
        copy_one "$REPO_ROOT/data/headroom/$PORT.json" evidence "headroom probe row (ADR-044)"; found_evidence=1
    else
        echo "- MISSING: data/headroom/$PORT.json (P11: probe before porting)" >> "$MANIFEST"
    fi
    while IFS= read -r f; do
        copy_one "$f" evidence "rustsmith evidence for $PORT"; found_evidence=1
    done < <(find "$REPO_ROOT/data" -maxdepth 2 -path "*$PORT*" -name '*.md' -type f 2>/dev/null | sort)
    while IFS= read -r f; do
        copy_one "$f" evidence "scope/boundary ADR mentioning $PORT"
    done < <(grep -li -e "$PORT" "$REPO_ROOT"/docs/adr/*.md 2>/dev/null | sort | head -8)
fi
[ $found_evidence -eq 1 ] || echo "- MISSING: no bench/evidence files found — port is not pattern-eligible until measured" >> "$MANIFEST"

# numbers.md — verbatim figure extraction with provenance. No recomputation.
{
    echo "# Measured figures for $PORT (verbatim extracts)"
    echo
    echo "Every line below is copied byte-for-byte from the evidence file named"
    echo "in its heading. Cite as: value + workload label + that file. If a"
    echo "section is empty, the figure class (wall / RSS / overhead) is a gap."
} >> "$NUMBERS"
shopt -s nullglob
for f in "$OUT"/evidence/*.md; do
    hits=$(grep -n -i -e '[0-9][0-9.]*x' -e 'peak RSS' -e 'maxrss' -e 'speedup' "$f" 2>/dev/null || true)
    if [ -n "$hits" ]; then
        { echo; echo "## evidence/$(basename "$f")"; echo '```'; echo "$hits"; echo '```'; } >> "$NUMBERS"
    fi
done
for f in "$OUT"/evidence/*.json; do
    { echo; echo "## evidence/$(basename "$f") (headroom / machine row — full file in evidence/)"; echo '```json'; head -c 2000 "$f"; echo; echo '```'; } >> "$NUMBERS"
done
shopt -u nullglob

# bundle-local checklist pointing at the canonical one
cat > "$OUT/REVIEW.md" <<'EOF'
# Review of this bundle

Follow docs/PATTERN_REVIEW.md. For each finding, record:

- mechanism (allocation / dispatch / branch / crossing / access order /
  algorithm / memory shape), with before/after excerpts from this bundle;
- the numbers.md line(s) that prove it (value + workload + file:line);
- classification: Confirm / Extend / New pattern / Retire-or-narrow;
- the loser or no-change row, with its number (1.00x is a finding).

Deliverable: a proposed diff to docs/RUST_PATTERNS.md (pattern entry +
per-port index + Appendix A) as a PR that names this bundle's commits.
Unmeasured mechanisms go in Hypotheses wording or are dropped.
EOF

echo "bundle: $OUT"
echo "  before:   $(find "$OUT/before" -type f | wc -l) files"
echo "  after:    $(find "$OUT/after" -type f | wc -l) files"
echo "  evidence: $(find "$OUT/evidence" -type f | wc -l) files"
echo "  gaps (MISSING lines in MANIFEST.md): $(grep -c 'MISSING' "$MANIFEST" || true)"
echo "next: run the review pass per docs/PATTERN_REVIEW.md against $OUT"
