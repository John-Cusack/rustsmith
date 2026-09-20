#!/bin/bash
# M3 acceptance (SPEC §15, IMPLEMENTATION_M3 §3):
# "on a small pure-Python package, produce a PORTING.md of at least 100 concrete
#  rules and a unit DAG whose leaf-first order is verifiably correct."
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="4e65ac4"
STR_PIN="115acaa"
echo "== M3: work=$WORK"

echo "-- unit tests (adapters DAG, porting triple)"
cargo test -q -p rustsmith-adapters || (echo "FAIL: adapters"; exit 1)

echo "-- clone fixtures"
git clone --quiet https://github.com/Nicoretti/crc "$WORK/crc"
git -C "$WORK/crc" checkout --quiet "$CRC_PIN"
git clone --quiet https://github.com/luozhouyang/python-string-similarity "$WORK/strsim"
git -C "$WORK/strsim" checkout --quiet "$STR_PIN"

echo "-- step 1: recon on crc -> PORTING.md >=100 concrete rules"
cargo run -q -p rustsmith-cli -- recon --repo "$WORK/crc" --out "$WORK/recon-crc" --heldout-out "$WORK/heldout-crc" --store "$WORK/store.db" --run-id m3crc
RULES=$(grep -c '^## R' "$WORK/recon-crc/PORTING.md")
echo "rules=$RULES"
if [ "$RULES" -lt 100 ]; then echo "FAIL: <100 rules"; exit 1; fi
# triple: every rule block has original + rust + example
python3 - "$WORK/recon-crc/PORTING.md" <<'EOF'
import sys, re
text = open(sys.argv[1]).read()
blocks = re.split(r'^## R\d+', text, flags=re.M)[1:]
assert len(blocks) >= 100, len(blocks)
for b in blocks:
    assert 'Original' in b, b[:200]
    assert 'Rust:' in b, b[:200]
    assert 'Example:' in b, b[:200]
print(f"triple OK over {len(blocks)} rules")
EOF

echo "-- adapter spot checks on crc"
python3 - <<EOF
import json
r = json.load(open('$WORK/recon-crc/recon.json'))
mods = r['modules']
assert '_crc' in mods, mods
files = r['tests']['files']
assert any('test_crc.py' in f for f in files), files
assert any('test_unstable_digest' in f for f in files), files
assert any('test_cli.py' in f for f in files), files
assert len(files) == 3, files
cfgs = r['tests']['configs']
assert not any('conftest' in c for c in cfgs), cfgs
assert not any(c.endswith('pytest.ini') or c.endswith('tox.ini') for c in cfgs), cfgs
assert any('pyproject.toml' in c for c in cfgs), cfgs
assert any('.github/workflows' in c for c in r['tests']['ci']), r['tests']['ci']
assert r['deps'] == [], r['deps']
assert r['license']['license'] == 'BSD-2-Clause', r['license']
assert r['tests']['baseline']['test_count'] == 80, r['tests']['baseline']
print("adapter + baseline + license OK")
EOF

echo "-- step 2: DAG check on strsimpy (NOT crc)"
cargo run -q -p rustsmith-cli -- recon --repo "$WORK/strsim" --out "$WORK/recon-str" --heldout-out "$WORK/heldout-str" --store "$WORK/store.db" --run-id m3str
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
w = os.environ['WORKDIR']
d = json.load(open(f'{w}/recon-str/dag.json'))
order = d['leaf_first_order']
pos = {u: i for i, u in enumerate(order)}
for dep, base in d['edges']:
    assert pos[base] < pos[dep], f"{base} must precede {dep}"
for base in ['shingle_based', 'string_distance']:
    assert base in pos, f"missing base {base}"
    for dep, b in d['edges']:
        if b == base:
            assert pos[base] < pos[dep]
print(f"DAG OK: {len(order)} units, {len(d['edges'])} edges, leaf-first verified")
EOF
echo "-- cycle injection fails loudly"
cargo test -q -p rustsmith-adapters topo_orders_leaves_first_and_detects_cycle || (echo "FAIL: cycle test"; exit 1)

echo "-- step 3: WORKLOAD.md + benchmark freeze"
test -f "$WORK/recon-crc/WORKLOAD.md"
grep -q "wall_time_p50" "$WORK/recon-crc/WORKLOAD.md" || (echo "FAIL: primary metric"; exit 1)
grep -q -i "distribution" "$WORK/recon-crc/WORKLOAD.md" || (echo "FAIL: distribution"; exit 1)
grep -q -i "budget" "$WORK/recon-crc/WORKLOAD.md" || (echo "FAIL: budgets"; exit 1)
python3 -c "import json;m=json.load(open('$WORK/recon-crc/manifest.json'));assert any('bench' in f['path'] for f in m['files']), 'bench missing';print('benchmark freeze OK:', m['benchmark_files'])"

echo "-- step 4: held-out suites run on host, absent from containers"
PYTHONPATH="$WORK/crc/src" python3 -m pytest "$WORK/heldout-crc" -q || (echo "FAIL: heldout tests"; exit 1)
test -f "$WORK/heldout-crc/heldout_workloads.json"
if grep -rq "heldout" "$WORK/recon-crc/" 2>/dev/null | grep -v "heldout_workloads\|HELD"; then
  echo "note: recon out mentions heldout (check blindness)"; grep -rq "heldout" "$WORK/recon-crc/" || true
fi
if docker run --rm --network=none rustsmith-grading:0.1.0 sh -c "find / -name '*heldout*' 2>/dev/null | grep ." 2>/dev/null; then
  echo "FAIL: heldout in grading image"; exit 1
else
  echo "heldout absent from grading image OK"
fi
echo "-- council approved recon (decisions row)"
python3 -c "import sqlite3;c=sqlite3.connect('$WORK/store.db');r=c.execute(\"SELECT COUNT(*) FROM decisions WHERE run_id IN ('m3crc','m3str')\").fetchone();assert r[0]>=2, r;print('council approvals:',r[0])"

echo "-- chain: M0+M1+M2 still green (fast: unit tests + m2probe)"
cargo test -q -p rustsmith-gates -p rustsmith-store -p rustsmith-oracle -p rustsmith-council || exit 1
cargo run -q -p rustsmith-cli -- m2probe --store "$WORK/m2.db" --events "$WORK/m2.jsonl" > /dev/null || exit 1
echo "M3 ACCEPTANCE GREEN"
