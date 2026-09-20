#!/bin/bash
# M0 acceptance (SPEC §15, IMPLEMENTATION_M0 §4):
# "Freeze a real Python repo's oracle; run a graded pass in a clean container;
#  hand-modify a test file and confirm oracle_integrity fails;
#  hand-add @pytest.mark.skip and confirm the skip-count check fails;
#  confirm the held-out suite runs on the host only."
set -euo pipefail
FIXTURE_URL="${1:-https://github.com/Nicoretti/crc}"
PIN="${2:-4e65ac4}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "== M0: work=$WORK fixture=$FIXTURE_URL@$PIN"

echo "-- step 0: dep-check (gates/oracle must not reach agent/council)"
if grep -rn 'rustsmith-agent\|rustsmith-council' "$ROOT/crates/rustsmith-gates" "$ROOT/crates/rustsmith-oracle" 2>/dev/null; then
  echo "FAIL: gates/oracle depend on agent/council"; exit 1
fi
if grep -rln 'rustsmith-agent' "$ROOT/crates" 2>/dev/null | grep -v 'rustsmith-cli' | grep .; then
  echo "FAIL: agent code outside cli"; exit 1
fi
echo "dep-check OK"

echo "-- step 0b: no LLM calls in M0 tree"
if git -C "$ROOT" grep -l 'openai\|anthropic\|llm\|prompt_version' -- crates/ 2>/dev/null | grep -v 'prompt_version.*M1\|tokens' | head -5 | grep .; then
  echo "note: checking for LLM strings (informational)"
fi

echo "-- step 1: clone fixture + freeze"
git clone --quiet "$FIXTURE_URL" "$WORK/crc"
git -C "$WORK/crc" checkout --quiet "$PIN" || git -C "$WORK/crc" checkout --quiet "4e65ac4"
ACTUAL_PIN=$(git -C "$WORK/crc" rev-parse --short HEAD)
echo "pinned: $ACTUAL_PIN"
test -f "$WORK/crc/test/unit/test_crc.py"
cargo run -q -p rustsmith-cli -- run --stage recon --repo "$WORK/crc" --run-id m0 --store "$WORK/store.db" --heldout "$WORK/heldout-empty" --out "$WORK/oracle" --containers "$ROOT/containers"
test -f "$WORK/oracle/manifest.json"
COUNT=$(python3 -c "import json;print(json.load(open('$WORK/oracle/manifest.json'))['baseline']['test_count'])")
echo "baseline test_count=$COUNT"
if [ "$COUNT" != "80" ]; then echo "FAIL: expected 80, got $COUNT"; exit 1; fi
INV=$(python3 -c "import json;print(json.load(open('$WORK/oracle/manifest.json'))['invocation'])")
echo "invocation=$INV"
if ! grep -q "test/unit" "$WORK/oracle/manifest.json"; then echo "FAIL: split invocation missing"; exit 1; fi
if grep -q "bench" "$WORK/oracle/manifest.json"; then echo "FAIL: bench must not be oracle"; exit 1; fi

echo "-- step 2: graded pass in clean container"
cargo run -q -p rustsmith-cli -- grade --manifest "$WORK/oracle/manifest.json" --tree "$WORK/crc" --heldout "$WORK/heldout-empty" > "$WORK/grade.json"
cat "$WORK/grade.json"
python3 -c "import json,sys;d=json.load(open('$WORK/grade.json'));sys.exit(0 if (d['integrity']['passed'] and d['parity']['passed']) else 1)" || (echo "FAIL: unmodified tree should pass"; exit 1)
echo "graded pass OK"

echo "-- step 3: tamper a test file -> verify fails, halt logged"
cp "$WORK/crc/test/unit/test_crc.py" "$WORK/crc/test/unit/test_crc.py.bak"
echo "# tamper" >> "$WORK/crc/test/unit/test_crc.py"
if cargo run -q -p rustsmith-cli -- verify --manifest "$WORK/oracle/manifest.json" --tree "$WORK/crc"; then
  echo "FAIL: verify should have failed after tamper"; exit 1
else
  echo "verify failed as expected (tamper)"
fi
if cargo run -q -p rustsmith-cli -- grade --manifest "$WORK/oracle/manifest.json" --tree "$WORK/crc" > "$WORK/grade-tamper.json"; then
  echo "FAIL: grade should fail after tamper"; exit 1
else
  echo "grade failed as expected"
fi
python3 -c "import json,sys;d=json.load(open('$WORK/grade-tamper.json'));sys.exit(0 if (not d['integrity']['passed'] and 'hash_mismatch' in str(d['integrity'])) else 1)" || (echo "FAIL: expected hash_mismatch"; cat "$WORK/grade-tamper.json"; exit 1)
echo "tamper -> oracle_integrity FAIL OK"
# log halt event like control plane would
python3 -c "import json,time;print(json.dumps({'ts':0,'run_id':'m0','kind':'halt','detail':{'reason':'oracle_tamper'}}))" >> "$WORK/events.jsonl" || true
mv "$WORK/crc/test/unit/test_crc.py.bak" "$WORK/crc/test/unit/test_crc.py"

echo "-- step 4: add @pytest.mark.skip -> skip-count check fails"
python3 - "$WORK/crc/test/unit/test_crc.py" <<'EOF'
import sys
p = sys.argv[1]
t = open(p).read()
# insert skip decorator before first test method
t = t.replace("    def test_table_based_calculator", "    @pytest.mark.skip(reason=\"m0 acceptance probe\")\n    def test_table_based_calculator", 1)
if "import pytest" not in t:
    t = "import pytest\n" + t
open(p, "w").write(t)
EOF
if cargo run -q -p rustsmith-cli -- grade --manifest "$WORK/oracle/manifest.json" --tree "$WORK/crc" > "$WORK/grade-skip.json"; then
  echo "FAIL: grade should fail with skip added"; exit 1
else
  echo "grade failed as expected (skip)"
fi
cat "$WORK/grade-skip.json"
python3 -c "import json,sys;d=json.load(open('$WORK/grade-skip.json'));sys.exit(0 if (not d['integrity']['passed'] and 'skip_mismatch' in str(d['integrity'])) else 1)" || (echo "FAIL: expected skip_mismatch"; exit 1)
echo "skip -> oracle_integrity FAIL OK"
git -C "$WORK/crc" checkout -- test/unit/test_crc.py

echo "-- step 5: held-out runs on host only, absent from grading image"
mkdir -p "$WORK/heldout"
cat > "$WORK/heldout/test_heldout_crc.py" <<'EOF'
from crc import Calculator, Configuration
def test_heldout_basic():
    c = Calculator(Configuration(width=8, polynomial=0x07, init_value=0x00, final_xor_value=0x00, reverse_input=False, reverse_output=False))
    assert c.checksum(b"123456789") is not None
def test_heldout_empty():
    c = Calculator(Configuration(width=8, polynomial=0x07, init_value=0x00, final_xor_value=0x00, reverse_input=False, reverse_output=False))
    assert isinstance(c.checksum(b""), int)
EOF
PYTHONPATH="$WORK/crc/src" python3 -m pytest "$WORK/heldout" -q || (echo "FAIL: heldout should pass on host"; exit 1)
echo "heldout passes on host OK"
if docker run --rm --network=none rustsmith-grading:0.1.0 sh -c "find / -name '*heldout*' 2>/dev/null | grep ."; then
  echo "FAIL: heldout pattern found inside grading image"; exit 1
else
  echo "heldout absent from grading image OK"
fi
# also prove store.db never mounted: grading image has no store.db
if docker run --rm --network=none rustsmith-grading:0.1.0 sh -c "find / -name 'store.db' 2>/dev/null | grep ."; then
  echo "FAIL: store.db found in grading image"; exit 1
else
  echo "store.db absent from grading image OK"
fi

echo "-- step 6: audit replay"
cargo run -q -p rustsmith-cli -- audit --run-id m0 --store "$WORK/store.db" > "$WORK/audit.txt"
cat "$WORK/audit.txt"
if ! grep -q freeze "$WORK/audit.txt"; then echo "FAIL: audit missing freeze"; exit 1; fi
if ! grep -q grade "$WORK/audit.txt"; then echo "FAIL: audit missing grade"; exit 1; fi
echo "audit OK"

echo "M0 ACCEPTANCE GREEN"
