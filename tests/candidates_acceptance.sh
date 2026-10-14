#!/bin/bash
# candidates acceptance: the finder classifies every input package into exactly
# one path with evidence, deterministically offline (snapshot + curated table,
# zero network). Follows the tests/m*_acceptance.sh conventions.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "== candidates: work=$WORK"

echo "-- step 1: classify offline fixture"
cargo run -q -p rustsmith-cli -- candidates classify \
  --input "$ROOT/tests/fixtures/candidates/snapshot.json" \
  --out "$WORK/candidates.md" --offline >"$WORK/stderr.log" 2>&1
test -f "$WORK/candidates.md"
test -f "$WORK/candidates.json"

echo "-- step 2: every package classified exactly once"
ROWS=$(grep -c '^| [0-9]' "$WORK/candidates.md")
# count path tags inside table rows only (one per row)
ROWTAGS=$(grep '^| [0-9]' "$WORK/candidates.md" | grep -o '(a) adopt/wrap\|(b) mirror\|(c) upstream' | wc -l)
if [ "$ROWTAGS" != "8" ]; then echo "FAIL: expected 8 path tags in rows, got $ROWTAGS"; exit 1; fi
echo "rows=$ROWS path-tags=$ROWTAGS"

echo "-- step 3: known verdicts"
JINJA=$(grep '^| [0-9] | jinja2 |' "$WORK/candidates.md")
echo "$JINJA" | grep -q '(a) adopt/wrap' || { echo "FAIL: jinja2 not adopt"; exit 1; }
echo "$JINJA" | grep -q 'minijinja' || { echo "FAIL: jinja2 missing minijinja link"; exit 1; }
PKG=$(grep '^| [0-9] | packaging |' "$WORK/candidates.md")
echo "$PKG" | grep -q '(b) mirror' || { echo "FAIL: packaging not mirror"; exit 1; }
echo "$PKG" | grep -q 'pep440_rs' || { echo "FAIL: packaging missing curated gap"; exit 1; }
PAN=$(grep '^| [0-9] | pandas |' "$WORK/candidates.md")
echo "$PAN" | grep -q '(c) upstream' || { echo "FAIL: pandas not upstream"; exit 1; }
CER=$(grep '^| [0-9] | certifi |' "$WORK/candidates.md")
echo "$CER" | grep -q '(a) adopt/wrap' || { echo "FAIL: certifi not adopt"; exit 1; }
echo "verdicts OK"

echo "-- step 3b: learned filters (ADR-043) apply offline"
IDNA=$(grep '^| [0-9] | idna |' "$WORK/candidates.md")
echo "$IDNA" | grep -q '(a) adopt/wrap' || { echo "FAIL: idna not adopt (full verdict)"; exit 1; }
echo "$IDNA" | grep -q 'crates.io/crates/idna' || { echo "FAIL: idna missing idna-crate link"; exit 1; }
BOTO=$(grep '^| [0-9] | boto3 |' "$WORK/candidates.md")
echo "$BOTO" | grep -q '(b) mirror' || { echo "FAIL: boto3 lost its mirror path"; exit 1; }
echo "$BOTO" | grep -q 'EXCLUDED from nomination (aws_owned)' || { echo "FAIL: boto3 not excluded"; exit 1; }
echo "$BOTO" | grep -q 'aws_owned/exclude' || { echo "FAIL: boto3 missing Filter slot"; exit 1; }
PYG=$(grep '^| [0-9] | pygments |' "$WORK/candidates.md")
echo "$PYG" | grep -q 'Demoted (shared_engine)' || { echo "FAIL: pygments not demoted"; exit 1; }
LP=$(grep -n '^| [0-9] | packaging |' "$WORK/candidates.md" | cut -d: -f1)
LG=$(grep -n '^| [0-9] | pygments |' "$WORK/candidates.md" | cut -d: -f1)
LB=$(grep -n '^| [0-9] | boto3 |' "$WORK/candidates.md" | cut -d: -f1)
if [ "$LP" -gt "$LG" ]; then echo "FAIL: clean mirror must precede demoted mirror"; exit 1; fi
if [ "$LP" -gt "$LB" ]; then echo "FAIL: clean mirror must precede excluded mirror"; exit 1; fi
echo "filters OK"

echo "-- step 4: mirrors rank first (payoff desc), evidence notes cover all rows"
LP=$(grep -n '^| [0-9] | packaging |' "$WORK/candidates.md" | cut -d: -f1)
LJ=$(grep -n '^| [0-9] | jinja2 |' "$WORK/candidates.md" | cut -d: -f1)
if [ "$LP" -gt "$LJ" ]; then echo "FAIL: mirror row must precede adopt row"; exit 1; fi
for p in packaging jinja2 certifi pandas unknownpkg-zz boto3 pygments idna; do
  grep -q "^### $p (dl-rank #" "$WORK/candidates.md" || { echo "FAIL: no evidence note for $p"; exit 1; }
done
echo "ordering + evidence OK"

echo "-- step 5: json sidecar agrees with markdown"
N=$(python3 -c "import json;print(len(json.load(open('$WORK/candidates.json'))))")
if [ "$N" != "8" ]; then echo "FAIL: json sidecar has $N rows"; exit 1; fi
python3 - "$WORK/candidates.json" <<'EOF'
import json, sys
rows = json.load(open(sys.argv[1]))
paths = {r["project"]: r["path"] for r in rows}
assert paths["jinja2"] == "(a) adopt/wrap", paths
assert paths["packaging"] == "(b) mirror", paths
assert paths["pandas"] == "(c) upstream", paths
assert paths["idna"] == "(a) adopt/wrap", paths
assert all(r["evidence"] and r["why"] and r["alternative"] for r in rows), "empty evidence"
filts = {r["project"]: r["filter"] for r in rows}
assert filts["boto3"] == "aws_owned/exclude", filts
assert filts["pygments"] == "shared_engine/demote", filts
assert filts["packaging"] is None, filts
print("json agrees")
EOF

echo "candidates acceptance: PASS"
