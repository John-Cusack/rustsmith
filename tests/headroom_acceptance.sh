#!/bin/bash
# headroom acceptance: the probe writes one machine-written JSON row per
# package (schema, fractions, score, verdict) from a workload file, and
# `candidates classify --headroom` applies rows as the learned rule 7.
# Fully offline: fixture workloads are stdlib-only; the exclude row is
# synthetic test data exercising the wiring (never presented as measured).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "== headroom: work=$WORK"

echo "-- step 1: probe the mixed fixture workload"
cargo run -q -p rustsmith-cli -- headroom probe \
  --package fakepkg-mixed \
  --workload-file "$ROOT/tests/fixtures/headroom/workload_mixed.py" \
  --workload-name fixture-mixed --iters 50 \
  --detail "acceptance fixture: regex finditer + pure loop" \
  --python python3 --out "$WORK/mixed.json" >"$WORK/mixed.log" 2>&1
test -f "$WORK/mixed.json"

echo "-- step 2: probe the pure fixture workload"
cargo run -q -p rustsmith-cli -- headroom probe \
  --package fakepkg-pure \
  --workload-file "$ROOT/tests/fixtures/headroom/workload_pure.py" \
  --workload-name fixture-pure --iters 30 \
  --detail "acceptance fixture: tight checksum loop over 60KB buffer" \
  --python python3 --out "$WORK/pure.json" >"$WORK/pure.log" 2>&1
test -f "$WORK/pure.json"

echo "-- step 3: rows carry the schema, fractions sum to 1, verdicts sane"
python3 - "$WORK/mixed.json" "$WORK/pure.json" <<'EOF'
import json, sys
for path in sys.argv[1:]:
    r = json.load(open(path))
    for k in ("package", "version", "workload", "tool", "machine", "python",
              "wall_secs", "tottime_secs", "fractions", "concentration",
              "top", "loop_nature", "loop_factor", "native_cover",
              "score", "verdict", "why"):
        assert k in r, f"{path}: missing {k}"
    f = r["fractions"]
    tot = f["pure_python"] + f["native"] + f["io_wait"] + f["regex"] + f["overhead"]
    assert abs(tot - 1.0) < 1e-6, f"{path}: fractions sum {tot}"
    assert 0.0 <= r["score"] <= 1.0, f"{path}: score {r['score']}"
    assert r["verdict"] in ("rewrite", "upstream_only", "exclude"), r["verdict"]
    assert r["tool"] == "cprofile" and r["wall_secs"] > 0 and r["top"], path
    print(f"{r['package']}: pure={f['pure_python']:.0%} regex={f['regex']:.0%} "
          f"loop={r['loop_nature']} score={r['score']:.2f} -> {r['verdict']}")
mixed = json.load(open(sys.argv[1]))
assert mixed["fractions"]["regex"] > 0.05, "mixed workload must show the re engine"
pure = json.load(open(sys.argv[2]))
assert pure["verdict"] == "rewrite", f"pure loop must score rewrite, got {pure['verdict']}"
assert pure["score"] >= 0.50, pure["score"]
print("rows OK")
EOF

echo "-- step 4: finder applies rows as rule 7 (synthetic exclude row for wiring)"
python3 - "$WORK/pure.json" <<'EOF'
import json, sys
r = json.load(open(sys.argv[1]))
ex = dict(r)
ex["package"] = "fakepkg-ex"
ex["verdict"] = "exclude"
ex["score"] = 0.04
ex["why"] = "synthetic acceptance row: saturated test double"
ex["workload"] = dict(r["workload"], name="fixture-ex")
json.dump(ex, open("/tmp/headroom-ex-marker", "w"))
EOF
mkdir -p "$WORK/rows"
cp "$WORK/pure.json" "$WORK/rows/fakepkg-pure.json"
python3 -c "import json; r=json.load(open('/tmp/headroom-ex-marker')); json.dump(r, open('$WORK/rows/fakepkg-ex.json','w'))"
rm -f /tmp/headroom-ex-marker
# rename pure row package to match the snapshot project below
python3 - "$WORK/rows/fakepkg-pure.json" <<'EOF'
import json, sys
p = sys.argv[1]
r = json.load(open(p))
r["package"] = "fakepkg-rw"
json.dump(r, open(p, "w"))
EOF
cat >"$WORK/snapshot.json" <<'EOF'
{"last_update": "2026-10-14 00:00:00", "source": "headroom-fixture",
 "rows": [{"project": "fakepkg-rw", "download_count": 5000000},
          {"project": "fakepkg-ex", "download_count": 9000000},
          {"project": "fakepkg-unmeasured", "download_count": 9000001}]}
EOF
cargo run -q -p rustsmith-cli -- candidates classify \
  --input "$WORK/snapshot.json" --out "$WORK/candidates.md" \
  --limit 3 --offline --headroom "$WORK/rows" >"$WORK/classify.log" 2>&1
test -f "$WORK/candidates.md"
test -f "$WORK/candidates.json"

echo "-- step 5: rule-7 effects in markdown and sidecar"
EX=$(grep '^| [0-9] | fakepkg-ex |' "$WORK/candidates.md")
echo "$EX" | grep -q 'EXCLUDED from nomination (headroom:' || { echo "FAIL: ex not excluded"; exit 1; }
echo "$EX" | grep -q 'headroom/exclude' || { echo "FAIL: ex missing Filter slot"; exit 1; }
RW=$(grep '^| [0-9] | fakepkg-rw |' "$WORK/candidates.md")
echo "$RW" | grep -q '(b) mirror' || { echo "FAIL: rewrite row must keep mirror path"; exit 1; }
echo "$RW" | grep -q 'payoff #' || { echo "FAIL: rewrite row must keep its nomination rank"; exit 1; }
grep -q '^### fakepkg-ex (dl-rank #2)$' "$WORK/candidates.md" || { echo "FAIL: no evidence note for ex"; exit 1; }
grep -q 'headroom probe (measured' "$WORK/candidates.md" || { echo "FAIL: no headroom evidence line"; exit 1; }
LPRW=$(grep -n '^| [0-9] | fakepkg-rw |' "$WORK/candidates.md" | cut -d: -f1)
LPEX=$(grep -n '^| [0-9] | fakepkg-ex |' "$WORK/candidates.md" | cut -d: -f1)
if [ "$LPRW" -gt "$LPEX" ]; then echo "FAIL: nominated mirror must precede excluded mirror"; exit 1; fi
python3 - "$WORK/candidates.json" <<'EOF'
import json, sys
rows = {r["project"]: r for r in json.load(open(sys.argv[1]))}
assert rows["fakepkg-ex"]["headroom_verdict"] == "exclude", rows["fakepkg-ex"]
assert rows["fakepkg-ex"]["headroom_filter"] == "headroom/exclude"
assert rows["fakepkg-rw"]["headroom_verdict"] == "rewrite"
assert rows["fakepkg-rw"]["headroom_filter"] is None
assert rows["fakepkg-rw"]["headroom_score"] >= 0.50
assert rows["fakepkg-unmeasured"]["headroom_score"] is None, "unmeasured stays unmeasured"
assert rows["fakepkg-unmeasured"]["headroom_verdict"] is None
print("sidecar agrees")
EOF

echo "headroom acceptance: PASS"
