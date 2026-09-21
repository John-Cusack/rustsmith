#!/bin/bash
# M9 acceptance: batch/config/charts/round-0/finals/learn/generated-heldouts.
# NEVER edit this script to make it pass — fix the implementation.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="${1:-4e65ac4}"
STRSIM_PIN="${2:-115acaacf926b41a15664bd34e763d074682bda3}"
CRC_URL="https://github.com/Nicoretti/crc"
STRSIM_URL="https://github.com/luozhouyang/python-string-similarity"
echo "== M9: work=$WORK crc=$CRC_PIN strsimpy=$STRSIM_PIN"

echo "-- step 0: fixture baselines (ANY DIFFERENCE = STOP, report)"
git clone --quiet "$CRC_URL" "$WORK/orig"
git -C "$WORK/orig" checkout --quiet "$CRC_PIN"
cd "$WORK/orig" && PYTHONPATH=src python3 -m pytest test/unit test/integration -q 2>&1 | tail -n 2 | tee "$WORK/crc-base.txt"
cd "$ROOT"
grep -q "80 passed" "$WORK/crc-base.txt" || (echo "STOP: crc baseline differs"; exit 1)
git clone --quiet "$STRSIM_URL" "$WORK/strorig"
git -C "$WORK/strorig" checkout --quiet "$STRSIM_PIN" || git -C "$WORK/strorig" checkout --quiet "115acaa"
cd "$WORK/strorig" && python3 -m pytest -q 2>&1 | tail -n 2 | tee "$WORK/str-base.txt"
cd "$ROOT"
grep -q "18 passed" "$WORK/str-base.txt" || (echo "STOP: strsimpy baseline differs"; exit 1)
echo "baselines OK"

echo "-- step 1: run-batch, 2 local repos, one shared store"
cargo run -q -p rustsmith-cli -- run-batch --repo "$WORK/orig,$WORK/strorig" --work "$WORK/batch" --store "$WORK/batch/store.db" --run-id-prefix m9b
test -f "$WORK/batch/fork-0/src/lib.rs"
test -f "$WORK/batch/fork-1/src/lib.rs"
python3 - "$WORK/batch/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
runs = c.execute("select id, status from runs").fetchall()
assert len(runs) >= 2, runs
assert all(s != "halted" for _, s in runs), runs
print("batch runs OK:", runs)
EOF

echo "-- step 2: --config max_rounds override changes the optimize report"
cat > "$WORK/override.toml" <<'EOF'
[optimize]
max_rounds = 1
EOF
cargo run -q -p rustsmith-cli -- run --repo "$WORK/orig" --fork "$WORK/cfg/fork" --work "$WORK/cfg/work" --store "$WORK/cfg/store.db" --run-id m9cfg --config "$WORK/override.toml"
python3 - "$WORK/cfg/work/opt/optimize-report.json" <<'EOF'
import json, sys
r = json.load(open(sys.argv[1]))
assert r["config"]["optimize"]["max_rounds"] == 1, r.get("config")
assert len(r["rounds"]) <= 1, r["rounds"]
print("config override OK")
EOF

echo "-- step 3: HTML chart + DAG table agree with JSON numbers"
FORK="$WORK/batch/fork-0"
test -f "$FORK/rustsmith-report.html"
grep -q '<svg id="round-gains">' "$FORK/rustsmith-report.html" || (echo "FAIL: gain chart missing"; exit 1)
grep -q '<table id="unit-dag">' "$FORK/rustsmith-report.html" || (echo "FAIL: unit DAG table missing"; exit 1)
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, re
w = os.environ["WORKDIR"]
html = open(f"{w}/batch/fork-0/rustsmith-report.html").read()
rep = json.load(open(f"{w}/batch/fork-0/rustsmith-report.json"))
for e in rep["rounds"]:
    assert f"round {e['round']}" in html, e
    assert f"{e['gain']:.4f}" in html, e
print("chart+DAG numbers OK")
EOF

echo "-- step 4: >=1 Round-0 unit graded on crc (merged or failed, honestly)"
python3 - "$WORK/batch/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
o = c.execute("select count(*) from optimizations where round=0").fetchone()[0]
f = c.execute("select count(*) from failed_optimizations where round=0").fetchone()[0]
assert o + f >= 1, (o, f)
print(f"round-0 rows OK: merged={o} failed={f}")
EOF

echo "-- step 5: finals refusal names the missing tool; decision logic unit-tested"
python3 - "$WORK/cfg/work/opt/optimize-report.json" <<'EOF'
import json, sys
r = json.load(open(sys.argv[1]))
fins = {f["final"]: f for f in r["finals"]}
assert "llvm-profdata" in fins["pgo"]["reason"] or "llvm-profdata" in fins.get("pgo", {}).get("reason", ""), fins
print("finals refusal OK:", fins["pgo"])
EOF
cargo test -q -p rustsmith-cli decide_final 2>&1 | tail -n 3

echo "-- step 6: learn propose -> apply round-trips on a scratch store"
cargo run -q -p rustsmith-cli -- learn propose --store "$WORK/batch/store.db" --out "$WORK/proposal.json"
test -f "$WORK/proposal.json"
cargo run -q -p rustsmith-cli -- learn apply --store "$WORK/batch/store.db" --human "m9-test" --proposal "$WORK/proposal.json"
python3 - "$WORK/batch/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
n = c.execute("select count(*) from guidance_revisions").fetchone()[0]
assert n >= 1, n
v = c.execute("select guidance_version from guidance_revisions order by id desc limit 1").fetchone()[0]
print(f"learn round-trip OK: revisions={n} version={v}")
EOF

echo "-- step 7: generated held-outs green on pristine + catch the hardcode plant"
cargo run -q -p rustsmith-cli -- run --repo "$WORK/orig" --fork "$WORK/p7/fork" --work "$WORK/p7/work" --store "$WORK/p7/store.db" --run-id m9p7
test -f "$WORK/p7/work/heldout/test_heldout_generated.py"
if cargo run -q -p rustsmith-cli -- run --repo "$WORK/orig" --fork "$WORK/p7h/fork" --work "$WORK/p7h/work" --store "$WORK/p7h/store.db" --run-id m9p7h --plant-live hardcode; then
  echo "FAIL: hardcode plant should halt the run"; exit 1
fi
python3 - "$WORK/p7h/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
h = c.execute("select halt_reason from runs where id='m9p7h'").fetchone()[0]
assert h and h.startswith("heldout_divergence"), h
print("generated held-outs catch hardcode OK")
EOF

echo "M9 ACCEPTANCE GREEN"
