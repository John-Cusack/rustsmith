#!/usr/bin/env bash
# upstream_pr_packet.sh — build the review packet for an upstream PR or reply.
#
# The packet is the only input the review agents get
# (docs/UPSTREAM_PRS.md, "Review panel"): the diff, the raw evidence, the
# draft text, and for a reply the thread so far. The writer's notes and
# reasoning stay out, so the reviewers judge what the maintainer will see.
# Every file is hashed into a packet id; verdicts quote it, and a verdict
# only counts for the packet it names.
#
# Usage:
#   scripts/upstream_pr_packet.sh --repo <upstream-checkout> --evidence <dir> \
#       --body <draft.md> --out <packet-dir> [--title <title.txt>] \
#       [--base <ref> --head <ref>] [--thread <thread.md>]
#
#   --repo      upstream checkout; the packet links it at the base commit
#   --base/--head  refs in --repo; diff.patch is base...head (needed for a PR)
#   --evidence  directory of raw measurements and test output only
#   --body      the draft PR body or reply
#   --title     the draft PR title (PRs only)
#   --thread    the thread so far, oldest first (replies only)
#   --out       packet directory (created; must not exist or be empty)
#
# Exit codes: 0 packet written (checks may still FAIL: see checks.txt),
# 2 usage/input error.

set -euo pipefail

usage() {
    echo "usage: $0 --repo <dir> --evidence <dir> --body <file> --out <dir> [--title <file>] [--base <ref> --head <ref>] [--thread <file>]" >&2
    exit 2
}

REPO=""; EVIDENCE=""; BODY=""; OUT=""; TITLE=""; BASE=""; HEAD=""; THREAD=""
while [ $# -gt 0 ]; do
    case $1 in
        --repo)     [ $# -ge 2 ] || usage; REPO=$2; shift 2 ;;
        --evidence) [ $# -ge 2 ] || usage; EVIDENCE=$2; shift 2 ;;
        --body)     [ $# -ge 2 ] || usage; BODY=$2; shift 2 ;;
        --out)      [ $# -ge 2 ] || usage; OUT=$2; shift 2 ;;
        --title)    [ $# -ge 2 ] || usage; TITLE=$2; shift 2 ;;
        --base)     [ $# -ge 2 ] || usage; BASE=$2; shift 2 ;;
        --head)     [ $# -ge 2 ] || usage; HEAD=$2; shift 2 ;;
        --thread)   [ $# -ge 2 ] || usage; THREAD=$2; shift 2 ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done
if [ -z "$REPO" ] || [ -z "$EVIDENCE" ] || [ -z "$BODY" ] || [ -z "$OUT" ]; then usage; fi
if { [ -n "$BASE" ] && [ -z "$HEAD" ]; } || { [ -z "$BASE" ] && [ -n "$HEAD" ]; }; then
    echo "--base and --head go together" >&2; exit 2
fi
if [ -z "$THREAD" ] && [ -z "$BASE" ]; then
    echo "a PR packet needs --base and --head; a reply packet needs --thread" >&2; exit 2
fi
git -C "$REPO" rev-parse --git-dir >/dev/null 2>&1 || { echo "not a git checkout: $REPO" >&2; exit 2; }
[ -d "$EVIDENCE" ] || { echo "not a directory: $EVIDENCE" >&2; exit 2; }
for f in "$BODY" "$TITLE" "$THREAD"; do
    [ -z "$f" ] || [ -f "$f" ] || { echo "not a file: $f" >&2; exit 2; }
done
if [ -e "$OUT" ] && [ -n "$(ls -A "$OUT")" ]; then
    echo "out-dir exists and is not empty: $OUT" >&2; exit 2
fi

KIND="pr"; [ -z "$THREAD" ] || KIND="reply"
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$OUT/evidence"
cp "$BODY" "$OUT/draft.md"
[ -z "$TITLE" ] || cp "$TITLE" "$OUT/title.txt"
[ -z "$THREAD" ] || cp "$THREAD" "$OUT/thread.md"
cp -R "$EVIDENCE"/. "$OUT/evidence/"

BASE_SHA=""; HEAD_SHA=""
if [ -n "$BASE" ]; then
    BASE_SHA=$(git -C "$REPO" rev-parse "$BASE^{commit}")
    HEAD_SHA=$(git -C "$REPO" rev-parse "$HEAD^{commit}")
    git -C "$REPO" diff "$BASE_SHA...$HEAD_SHA" > "$OUT/diff.patch"
fi

CHECK_ARGS=(--body "$OUT/draft.md" --kind "$KIND")
[ ! -f "$OUT/title.txt" ] || CHECK_ARGS+=(--title "$OUT/title.txt")
[ ! -f "$OUT/diff.patch" ] || CHECK_ARGS+=(--diff "$OUT/diff.patch")
"$SCRIPT_DIR/upstream_pr_check.sh" "${CHECK_ARGS[@]}" > "$OUT/checks.txt" || true

PACKET_ID=$( (cd "$OUT" && find . -type f ! -name MANIFEST.md -print0 | LC_ALL=C sort -z \
    | xargs -0 sha256sum) | sha256sum | cut -c1-12)

{
    echo "# Review packet $PACKET_ID"
    echo
    echo "- kind: $KIND"
    echo "- upstream: $(git -C "$REPO" remote get-url origin 2>/dev/null || echo unknown)"
    echo "- upstream checkout: $(cd "$REPO" && pwd)"
    [ -z "$BASE_SHA" ] || echo "- base: $BASE_SHA"
    [ -z "$HEAD_SHA" ] || echo "- head: $HEAD_SHA"
    [ -z "$THREAD" ] || echo "- thread captured: $(date -u -r "$THREAD" +%Y-%m-%dT%H:%MZ) (re-fetch before posting)"
    echo "- checks: $(tail -n 1 "$OUT/checks.txt")"
    echo
    echo "Read the upstream checkout for context (README, CONTRIBUTING, git log);"
    echo "for a PR, read it as of the base commit (git -C <checkout> show <base>:<path>)."
    echo "Do not read anything outside this packet and that checkout."
    echo
    echo "## Files"
    echo
    (cd "$OUT" && find . -type f ! -name MANIFEST.md | LC_ALL=C sort | sed 's|^\./|- |')
} > "$OUT/MANIFEST.md"

echo "packet $PACKET_ID written to $OUT ($(tail -n 1 "$OUT/checks.txt"))"
