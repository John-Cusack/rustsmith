#!/bin/bash
# M8 acceptance: live worker-command plumbing + live seat wiring.
# NEVER edit this script to make it pass — fix the implementation.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="${1:-4e65ac4}"
CRC_URL="https://github.com/Nicoretti/crc"
echo "== M8: work=$WORK crc=$CRC_PIN"

echo "-- dep-check (gates/oracle must not reach agent/council)"
if grep -rn 'rustsmith-agent\|rustsmith-council' "$ROOT/crates/rustsmith-gates" "$ROOT/crates/rustsmith-oracle" 2>/dev/null; then
  echo "FAIL: gates/oracle depend on agent/council"; exit 1
fi
echo "dep-check OK"

echo "-- step 0: crc baseline (ANY DIFFERENCE = STOP, report)"
git clone --quiet "$CRC_URL" "$WORK/orig"
git -C "$WORK/orig" checkout --quiet "$CRC_PIN"
cd "$WORK/orig" && PYTHONPATH=src python3 -m pytest test/unit test/integration -q 2>&1 | tail -n 2 | tee "$WORK/crc-base.txt"
cd "$ROOT"
grep -q "80 passed" "$WORK/crc-base.txt" || (echo "STOP: crc baseline differs"; exit 1)
echo "baseline OK"

echo "-- step 1: worker-command plumbing (fake worker -> tokens + prompt + version)"
cat > "$WORK/fake-worker.sh" <<'EOF'
#!/bin/bash
# stdin=prompt; save it where the probe tells us, then report fixed usage.
cat > "$PROMPT_OUT"
printf '{"tokens_in": 1234, "tokens_out": 567, "note": "fake-ok"}'
EOF
chmod +x "$WORK/fake-worker.sh"
export RUSTSMITH_WORKER_CMD="$WORK/fake-worker.sh"
export PROMPT_OUT="$WORK/prompt.txt"
cargo run -q -p rustsmith-cli -- worker-probe --store "$WORK/store.db" --run-id m8w --unit u-mirror-crc --prompt-out "$WORK/prompt.txt"
grep -q "u-mirror-crc" "$WORK/prompt.txt" || (echo "FAIL: prompt lacks unit id"; exit 1)
echo "prompt contains unit id OK"
python3 - "$WORK/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
ti, to = c.execute("select tokens_in, tokens_out from units where id='u-mirror-crc'").fetchone()
assert ti == 1234 and to == 567, (ti, to)
rows = c.execute("select detail_json from gate_results where unit_id='u-mirror-crc'").fetchall()
assert rows, "no gate rows for probe unit"
assert any("worker-cmd-v1" in r[0] for r in rows), rows
assert not any("m1-stub-v1" in r[0] for r in rows), rows
print("tokens + prompt_version OK")
EOF

echo "-- step 2: seat wiring (disagreeing fake seats -> minority + tiebreak)"
cat > "$WORK/seat-arch.sh" <<'EOF'
#!/bin/bash
cat > /dev/null
printf '{"stance": "approve", "reasoning": "arch-minority-correct: cross-module invariant holds"}'
EOF
cat > "$WORK/seat-ver.sh" <<'EOF'
#!/bin/bash
cat > /dev/null
printf '{"stance": "reject", "reasoning": "ver-majority-wrong: surface check fails"}'
EOF
cat > "$WORK/seat-perf.sh" <<'EOF'
#!/bin/bash
cat > /dev/null
printf '{"stance": "reject", "reasoning": "perf-agrees-verifier: costs too much"}'
EOF
cat > "$WORK/seat-scope.sh" <<'EOF'
#!/bin/bash
cat > /dev/null
printf '{"stance": "approve", "reasoning": "scope-ok-minority"}'
EOF
chmod +x "$WORK"/seat-*.sh
export RUSTSMITH_SEAT_CMD_ARCHITECT="$WORK/seat-arch.sh"
export RUSTSMITH_SEAT_CMD_VERIFIER="$WORK/seat-ver.sh"
export RUSTSMITH_SEAT_CMD_PERFORMANCE="$WORK/seat-perf.sh"
export RUSTSMITH_SEAT_CMD_SCOPE="$WORK/seat-scope.sh"
cargo run -q -p rustsmith-cli -- seat-probe --store "$WORK/store.db" --run-id m8s --question "adopt X"
python3 - "$WORK/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
q, pos, res = c.execute(
  "select question, seat_positions_json, resolution from decisions where run_id='m8s' order by id desc limit 1").fetchone()
for needle in ["arch-minority-correct", "ver-majority-wrong", "perf-agrees-verifier"]:
    assert needle in pos, needle
assert "architect_tiebreak" in res, res
print("minority preserved + tiebreak OK")
EOF
python3 - "$WORK/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
by = c.execute("select resolved_by from decisions where run_id='m8s' order by id desc limit 1").fetchone()[0]
assert by == "architect_tiebreak", by
print("resolved_by OK")
EOF

echo "M8 ACCEPTANCE GREEN"
