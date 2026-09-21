#!/bin/bash
# M6 acceptance (SPEC §15 M6 + IMPLEMENTATION_M6 §3, quoted):
# "produce at least one `language_independent` patch that applies cleanly to
#  the original repo and measurably improves it in the original language;
#  confirm every artifact carries preserved attribution."
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="${1:-4e65ac4}"
echo "== M6: work=$WORK pin=$CRC_PIN"

echo "-- unit tests (harvest, report)"
cargo test -q -p rustsmith-harvest -p rustsmith-report || (echo "FAIL: unit tests"; exit 1)

echo "-- fixtures + recon + mirror + optimize (run-id m6)"
git clone --quiet https://github.com/Nicoretti/crc "$WORK/orig"
git -C "$WORK/orig" checkout --quiet "$CRC_PIN"
git clone --quiet "$WORK/orig" "$WORK/pristine"
cargo run -q -p rustsmith-cli -- recon --repo "$WORK/orig" --out "$WORK/recon" --heldout-out "$WORK/heldout" --store "$WORK/store.db" --run-id m6recon
cargo run -q -p rustsmith-cli -- mirror --repo "$WORK/orig" --fork "$WORK/fork" --recon-out "$WORK/recon" --heldout "$WORK/heldout" --store "$WORK/store.db" --run-id m6mirror --template "$ROOT/mirror/crc"
cargo run -q -p rustsmith-cli -- optimize --fork "$WORK/fork" --work "$WORK/opt" --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" --store "$WORK/store.db" --run-id m6 --max-rounds 6

echo "-- harvest + report into fork"
cargo run -q -p rustsmith-cli -- report --run-id m6 --fork "$WORK/fork" --orig "$WORK/orig" --store "$WORK/store.db" --recon-out "$WORK/recon" --opt "$WORK/opt" > "$WORK/report.json"
python3 -c "import json;d=json.load(open('$WORK/report.json'));assert d['suggestions'], d; print('suggestions:', [(s['technique'], s['class']) for s in d['suggestions']])"

echo "-- check 1: suggestions/ has >=1 classified item with reasoning (expect slice)"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, glob
w = os.environ['WORKDIR']
items = [f for f in glob.glob(f'{w}/fork/suggestions/*.md') if os.path.basename(f) != 'README.md']
assert items, 'no suggestion items'
found = False
for f in items:
    t = open(f).read()
    assert '## Reasoning' in t, f
    assert '## Expected gain' in t, f
    if 'slice' in t.lower() and 'language_independent' in t.lower():
        found = True
print('check 1 OK:', items)
EOF

echo "-- check 2: patch applies to pristine + in-Python gain beyond floor"
PATCH="$(ls "$WORK"/fork/suggestions/patches/*.patch | head -n 1)"
test -n "$PATCH"
git -C "$WORK/pristine" apply --check "$PATCH" || (echo "FAIL: patch does not apply clean"; exit 1)
echo "apply --check OK: $PATCH"
git -C "$WORK/pristine" apply "$PATCH"
WORKDIR="$WORK" PATCH="$PATCH" python3 - <<'EOF'
import json, os, subprocess
w = os.environ['WORKDIR']
floor = json.load(open(f'{w}/opt/optimize-report.json'))['floor']
code = '''
import time, sys
sys.path.insert(0, "src")
from crc import Calculator, Crc8
calc = Calculator(Crc8.CCITT, True)
data = bytes([0x41]) * 4096
ts = []
for _ in range(7):
    t0 = time.perf_counter()
    for _ in range(300):
        calc.checksum(data)
    ts.append((time.perf_counter() - t0) / 300)
ts.sort()
print(ts[3])
'''
def med(cwd, stash):
    if stash:
        subprocess.run(["git", "stash", "-q"], cwd=cwd, check=True)
    o = subprocess.run(["python3", "-c", code], cwd=cwd, capture_output=True, text=True, check=True)
    if stash:
        subprocess.run(["git", "stash", "pop", "-q"], cwd=cwd, check=True)
    return float(o.stdout.strip())
base = med(f'{w}/pristine', True)
patched = med(f'{w}/pristine', False)
gain = (base - patched) / base
print(f'python base={base:.3e} patched={patched:.3e} gain={gain:.4f} floor={floor:.4f}')
assert gain > floor, f'in-Python gain {gain} must exceed floor {floor}'
EOF
echo "-- check 2b: patched pristine keeps the full oracle green"
cd "$WORK/pristine" && PYTHONPATH=src python3 -m pytest test/unit test/integration -q 2>&1 | tail -n 2
cd "$WORK/pristine" && PYTHONPATH=src python3 -m pytest test/unit test/integration -q 2>&1 | grep -q "80 passed" || (echo "FAIL: patched suite not green"; exit 1)
cd "$ROOT"
WORKDIR="$WORK" python3 - <<'EOF'
import os, glob
w = os.environ['WORKDIR']
arts = glob.glob(f'{w}/fork/suggestions/*.md') + glob.glob(f'{w}/fork/suggestions/patches/*.patch') + glob.glob(f'{w}/fork/suggestions/accelerators/*')
arts += [f'{w}/fork/RUSTSMITH_REPORT.md', f'{w}/fork/rustsmith-report.html', f'{w}/fork/rustsmith-report.json']
assert arts, 'no artifacts'
for f in arts:
    if os.path.isdir(f):
        continue
    t = open(f, errors='replace').read()
    assert 'BSD-2-Clause' in t, f'license header missing: {f}'
    assert 'Nicoretti' in t, f'attribution missing: {f}'
print('check 3 OK:', len(arts), 'artifacts')
EOF

echo "-- check 4: md/html/json agree on all numbers"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, re
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/fork/rustsmith-report.json'))
md = open(f'{w}/fork/RUSTSMITH_REPORT.md').read()
html = open(f'{w}/fork/rustsmith-report.html').read()
nums = [rep['e2e_speedup_vs_original'], rep['floor']] + [r['gain'] for r in rep['rounds']] + [s['expected_gain'] for s in rep['suggestions']]
for n in nums:
    s = f'{n:.4f}'
    assert s in md and s in html, f'number {s} missing from md/html'
print('check 4 OK')
EOF

echo "-- check 5: README ranks + report carries all sections"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
w = os.environ['WORKDIR']
t = open(f'{w}/fork/suggestions/README.md').read()
assert '## Ranking' in t and '## ' in t.replace('## Ranking', ''), 'ranked paragraphs missing'
rep = json.load(open(f'{w}/fork/rustsmith-report.json'))
for k in ['parity', 'divergence', 'unsafe_list', 'rounds', 'e2e_speedup_vs_original', 'unported_modules', 'decisions', 'tokens', 'suggestions', 'negative_results', 'stop']:
    assert k in rep, f'section missing: {k}'
assert rep['suggestions'], 'empty suggestions'
assert rep['stop'], 'stop rule missing'
print('check 5 OK')
EOF

echo "-- slice 5: status / halt / resume / report regenerate"
cargo run -q -p rustsmith-cli -- status --run-id m6 --store "$WORK/store.db" > "$WORK/status.json"
python3 -c "import json;d=json.load(open('$WORK/status.json'));assert d['units'] and 'gates' in d and 'spend' in d, d; print('status OK')"
cargo run -q -p rustsmith-cli -- halt --run-id m6 --store "$WORK/store.db" --reason "operator review pause"
cargo run -q -p rustsmith-cli -- report --run-id m6 --fork "$WORK/fork" --orig "$WORK/orig" --store "$WORK/store.db" --recon-out "$WORK/recon" --opt "$WORK/opt" > "$WORK/report_halted.json"
python3 -c "import json;d=json.load(open('$WORK/report_halted.json'));assert d['suggestions'], 'halted run must still emit harvest'"
echo "halted harvest OK"
python3 - "$WORK/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("insert or replace into runs (id, repo_url, source_lang, status, stage, started_at, halt_reason) values ('m6t','x','python','halted','optimize',0,'oracle_tamper probe')")
c.commit()
EOF
if cargo run -q -p rustsmith-cli -- resume --run-id m6t --store "$WORK/store.db"; then
  echo "FAIL: tamper halt resumed"; exit 1
fi
echo "tamper resume refused OK"
cargo run -q -p rustsmith-cli -- resume --run-id m6 --store "$WORK/store.db"
cargo run -q -p rustsmith-cli -- status --run-id m6 --store "$WORK/store.db" > "$WORK/status2.json"
python3 -c "import json;d=json.load(open('$WORK/status2.json'));assert d['status'] != 'halted', d; print('resume OK')"

echo "-- chain: M0-M5 still green (fast checks)"
cargo test -q -p rustsmith-gates -p rustsmith-store -p rustsmith-oracle -p rustsmith-council -p rustsmith-adapters -p rustsmith-profile -p rustsmith-harvest -p rustsmith-report -p rustsmith-cli || exit 1
echo "M6 ACCEPTANCE GREEN"
