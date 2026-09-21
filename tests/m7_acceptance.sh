#!/bin/bash
# M7 acceptance (SPEC §16 items 1-12 + generic-template proof, quoted in
# IMPLEMENTATION_M7 §3). Runs `rustsmith run <url>` unattended, then asserts.
# NEVER edit this script to make it pass — fix the implementation.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="${1:-4e65ac4}"
STRSIM_PIN="${2:-115acaacf926b41a15664bd34e763d074682bda3}"
CRC_URL="https://github.com/Nicoretti/crc"
STRSIM_URL="https://github.com/luozhouyang/python-string-similarity"
echo "== M7: work=$WORK crc=$CRC_PIN strsimpy=$STRSIM_PIN"

echo "-- dep-check (gates/oracle must not reach agent/council)"
if grep -rn 'rustsmith-agent\|rustsmith-council' "$ROOT/crates/rustsmith-gates" "$ROOT/crates/rustsmith-oracle" 2>/dev/null; then
  echo "FAIL: gates/oracle depend on agent/council"; exit 1
fi
if grep -rn 'openai\|anthropic' "$ROOT/crates/rustsmith-gates" "$ROOT/crates/rustsmith-oracle" 2>/dev/null; then
  echo "FAIL: LLM strings in gates/oracle"; exit 1
fi
echo "dep-check OK"

echo "-- step 0: fixture baselines (ANY DIFFERENCE = STOP, report)"
git clone --quiet "$CRC_URL" "$WORK/orig"
git -C "$WORK/orig" checkout --quiet "$CRC_PIN"
echo "crc: $(git -C "$WORK/orig" rev-parse --short HEAD) $(wc -l < "$WORK/orig/src/crc/_crc.py") lines _crc.py"
cd "$WORK/orig" && PYTHONPATH=src python3 -m pytest test/unit test/integration -q 2>&1 | tail -n 2 | tee "$WORK/crc-base.txt"
cd "$ROOT"
grep -q "80 passed" "$WORK/crc-base.txt" || (echo "STOP: crc baseline differs"; exit 1)
git clone --quiet "$STRSIM_URL" "$WORK/strorig"
git -C "$WORK/strorig" checkout --quiet "$STRSIM_PIN" || git -C "$WORK/strorig" checkout --quiet "115acaa"
echo "strsimpy: $(git -C "$WORK/strorig" rev-parse --short HEAD)"
cd "$WORK/strorig" && python3 -m pytest -q 2>&1 | tail -n 2 | tee "$WORK/str-base.txt"
cd "$ROOT"
grep -q "18 passed" "$WORK/str-base.txt" || (echo "STOP: strsimpy baseline differs"; exit 1)
echo "baselines OK"

echo "-- step 1: unattended run on crc URL (no human input from here on)"
cargo run -q -p rustsmith-cli -- run --repo "$CRC_URL#$CRC_PIN" --fork "$WORK/clean/fork" --work "$WORK/clean/work" --store "$WORK/clean/store.db" --run-id m7
FORK="$WORK/clean/fork"
RW="$WORK/clean/work"
VENV="$FORK/.grade-venv"
PY="$VENV/bin/python"

echo "-- item 1: fork holds Rust implementation with the original public API"
test -f "$FORK/src/lib.rs"
test -f "$FORK/Cargo.toml"
"$PY" -c "
import crc
for n in ['Calculator','Register','TableBasedRegister','Crc8','Crc16','Crc32','Crc64','Configuration']:
    assert hasattr(crc, n), n
c = crc.Calculator(crc.Crc8.CCITT)
assert c.checksum(b'123456789') == 0xF4
assert c.verify(b'123456789', 0xF4)
r = crc.Register(crc.Crc8.CCITT); r.init(); r.update(b'abc'); assert isinstance(r.digest(), int)
t = crc.TableBasedRegister(crc.Crc8.CCITT); t.init(); t.update(b'abc'); assert isinstance(t.digest(), int)
print('public API OK')
"

echo "-- item 2: graded run shows 100% of the frozen oracle passing"
env -u PYTHONPATH "$PY" -m pytest "$WORK/orig/test/unit" "$WORK/orig/test/integration" -q 2>&1 | tail -n 2 | tee "$WORK/graded.txt"
grep -q "80 passed" "$WORK/graded.txt" || (echo "FAIL: not 80 passed"; exit 1)

echo "-- item 3: held-out divergence under 5pp (recomputed, never trusted)"
env -u PYTHONPATH "$PY" -m pytest "$RW/heldout" -q 2>&1 | tail -n 2 | tee "$WORK/held.txt"
WORKDIR="$WORK" python3 - <<'EOF'
import os, re
w = os.environ['WORKDIR']
def rate(path):
    t = open(path).read().lower()
    m = re.search(r'(\d+)\s+passed', t)
    p = int(m.group(1)) if m else 0
    m = re.search(r'(\d+)\s+failed', t)
    f = int(m.group(1)) if m else 0
    assert p + f > 0, path
    return p / (p + f)
vis, held = rate(f'{w}/graded.txt'), rate(f'{w}/held.txt')
div = vis - held
print(f'visible={vis:.4f} heldout={held:.4f} divergence={div:.4f}')
assert div < 0.05, f'divergence {div} >= 5pp'
EOF

echo "-- item 4: manifest hashes unchanged; count + skip list match baseline"
cargo run -q -p rustsmith-cli -- verify --manifest "$RW/recon/manifest.json" --tree "$FORK" || (echo "FAIL: manifest changed"; exit 1)
cargo run -q -p rustsmith-cli -- grade --manifest "$RW/recon/manifest.json" --tree "$FORK" > "$WORK/grade7.json"
python3 -c "import json;d=json.load(open('$WORK/grade7.json'));assert d['integrity']['passed'] and d['parity']['passed'], d; print('hashes+counts OK')"

echo "-- item 5: every unsafe at FFI boundary with SAFETY comment (0 expected)"
if grep -rn "unsafe" "$FORK/src/" | grep -v "^\s*//" | grep .; then echo "FAIL: unsafe tokens"; exit 1; else echo "unsafe count 0 OK"; fi

echo "-- item 6: miri and clippy clean"
cargo clippy --manifest-path "$FORK/Cargo.toml" -- -D warnings 2>&1 | tail -n 2
cargo +nightly miri test --manifest-path "$FORK/Cargo.toml" --lib 2>&1 | tail -n 3

echo "-- item 7: Stage 2 ran >=2 rounds, stopped on a rule, merged above floor"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, sqlite3
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/clean/work/opt/optimize-report.json'))
assert len(rep['rounds']) >= 2, rep['rounds']
assert rep['stop'], 'stopping rule must be recorded'
print('rounds:', [(r.get('round'), r.get('merged')) for r in rep['rounds']], 'stop:', rep['stop'])
c = sqlite3.connect(f'{w}/clean/store.db')
n = c.execute("select count(*) from optimizations where run_id='m7' and technique='slicing-by-8'").fetchone()[0]
assert n >= 1, 'slice-by-8 must merge'
for tech, delta in c.execute("select technique, delta_pct from optimizations where run_id='m7'"):
    assert delta / 100.0 > rep['floor'], (tech, delta, rep['floor'])
print('merged above floor OK')
EOF

echo "-- item 8: all three reports exist and agree on every number"
test -f "$FORK/RUSTSMITH_REPORT.md"
test -f "$FORK/rustsmith-report.json"
test -f "$FORK/rustsmith-report.html"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/clean/fork/rustsmith-report.json'))
md = open(f'{w}/clean/fork/RUSTSMITH_REPORT.md').read()
html = open(f'{w}/clean/fork/rustsmith-report.html').read()
nums = [rep['e2e_speedup_vs_original'], rep['floor']] + [r['gain'] for r in rep['rounds']] + [s['expected_gain'] for s in rep['suggestions']]
for n in nums:
    s = f'{n:.4f}'
    assert s in md and s in html, f'number {s} missing from md/html'
print('reports agree OK')
EOF

echo "-- item 9: suggestions/ holds >=1 classified item with reasoning"
WORKDIR="$WORK" python3 - <<'EOF'
import glob, os
w = os.environ['WORKDIR']
items = [f for f in glob.glob(f'{w}/clean/fork/suggestions/*.md') if os.path.basename(f) != 'README.md']
assert items, 'no suggestion items'
for f in items:
    t = open(f).read()
    assert '## Reasoning' in t, f
    assert '## Expected gain' in t, f
print('suggestions OK:', items)
EOF

echo "-- item 10: event log alone reconstructs units + council decisions"
cargo run -q -p rustsmith-cli -- audit --run-id m7 --store "$WORK/clean/store.db" > "$WORK/audit.txt"
for k in unit_start gate merge mirror_done; do
  grep -q "$k" "$WORK/audit.txt" || (echo "FAIL: audit missing $k"; exit 1)
done
python3 -c "import sqlite3;c=sqlite3.connect('$WORK/clean/store.db');assert c.execute(\"select count(*) from decisions where run_id='m7'\").fetchone()[0]>=1; print('decisions logged OK')"

echo "-- item 11 (adversarial): planted test-editing unit halts with tamper event"
if cargo run -q -p rustsmith-cli -- run --repo "$WORK/orig" --fork "$WORK/p11/fork" --work "$WORK/p11/work" --store "$WORK/p11/store.db" --run-id m7p11 --plant-live test-edit; then
  echo "FAIL: test-edit plant should halt the run"; exit 1
fi
echo "halt exit OK"
python3 - "$WORK/p11/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
r = c.execute("select halt_reason from runs where id='m7p11'").fetchone()
assert r and r[0].startswith('oracle_tamper'), r
print('halt logged OK:', r[0])
EOF
cargo run -q -p rustsmith-cli -- audit --run-id m7p11 --store "$WORK/p11/store.db" > "$WORK/audit11.txt"
grep -q "tamper" "$WORK/audit11.txt" || (echo "FAIL: tamper event missing"; exit 1)
echo "tamper event OK"

echo "-- item 12 (adversarial): planted hardcoded-visible-inputs unit caught by held-out/divergence halt"
if cargo run -q -p rustsmith-cli -- run --repo "$WORK/orig" --fork "$WORK/p12/fork" --work "$WORK/p12/work" --store "$WORK/p12/store.db" --run-id m7p12 --plant-live hardcode; then
  echo "FAIL: hardcode plant should halt the run"; exit 1
fi
echo "halt exit OK"
python3 - "$WORK/p12/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
r = c.execute("select halt_reason from runs where id='m7p12'").fetchone()
assert r and r[0].startswith('heldout_divergence'), r
print('halt logged OK:', r[0])
EOF
cargo run -q -p rustsmith-cli -- audit --run-id m7p12 --store "$WORK/p12/store.db" > "$WORK/audit12.txt"
grep -q "heldout_divergence" "$WORK/audit12.txt" || (echo "FAIL: divergence event missing"; exit 1)
echo "divergence halt OK"

echo "-- generic-template proof: same run command against strsimpy reaches mirror parity"
cargo run -q -p rustsmith-cli -- run --repo "$STRSIM_URL#$STRSIM_PIN" --fork "$WORK/str/fork" --work "$WORK/str/work" --store "$WORK/str/store.db" --run-id m7str
SFORK="$WORK/str/fork"
SVENV="$SFORK/.grade-venv"
SPY="$SVENV/bin/python"
test -f "$SFORK/src/lib.rs"
test -f "$SFORK/Cargo.toml"
"$SPY" -c "import strsimpy; print('strsimpy imports OK')"
env -u PYTHONPATH "$SPY" -m pytest "$WORK/strorig" -q -p no:cacheprovider 2>&1 | tail -n 2 | tee "$WORK/str-graded.txt"
grep -q "18 passed" "$WORK/str-graded.txt" || (echo "FAIL: strsimpy not 18 passed"; exit 1)
env -u PYTHONPATH "$SPY" -m pytest "$WORK/str/work/heldout" -q -p no:cacheprovider 2>&1 | tail -n 2 | tee "$WORK/str-held.txt"
WORKDIR="$WORK" python3 - <<'EOF'
import os, re
w = os.environ['WORKDIR']
def rate(path):
    t = open(path).read().lower()
    m = re.search(r'(\d+)\s+passed', t)
    p = int(m.group(1)) if m else 0
    m = re.search(r'(\d+)\s+failed', t)
    f = int(m.group(1)) if m else 0
    assert p + f > 0, path
    return p / (p + f)
div = rate(f'{w}/str-graded.txt') - rate(f'{w}/str-held.txt')
print(f'strsimpy divergence={div:.4f}')
assert div < 0.05, div
EOF
cargo run -q -p rustsmith-cli -- verify --manifest "$WORK/str/work/recon/manifest.json" --tree "$SFORK" || (echo "FAIL: strsimpy manifest changed"; exit 1)
if grep -rn "unsafe" "$SFORK/src/" | grep -v "^\s*//" | grep .; then echo "FAIL: strsimpy unsafe tokens"; exit 1; else echo "strsimpy unsafe 0 OK"; fi
cargo clippy --manifest-path "$SFORK/Cargo.toml" -- -D warnings 2>&1 | tail -n 2
cargo +nightly miri test --manifest-path "$SFORK/Cargo.toml" --lib 2>&1 | tail -n 3
python3 - "$WORK/str/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
r = c.execute("select halt_reason from runs where id='m7str'").fetchone()
assert r is None or r[0] is None, f'tamper never acceptable, got {r}'
print('strsimpy halt-free OK')
EOF

echo "M7 ACCEPTANCE GREEN"
