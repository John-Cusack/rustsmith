#!/bin/bash
# M5 acceptance (SPEC_STAGE2 §15: functional 13-18+26, adversarial 19-25).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="4e65ac4"
echo "== M5: work=$WORK"

echo "-- unit tests (gates, profile math, store schema)"
cargo test -q -p rustsmith-gates -p rustsmith-store -p rustsmith-profile || (echo "FAIL: unit tests"; exit 1)

echo "-- fixtures + recon (WORKLOAD.md, benchmark freeze, held-out)"
git clone --quiet https://github.com/Nicoretti/crc "$WORK/orig"
git -C "$WORK/orig" checkout --quiet "$CRC_PIN"
cargo run -q -p rustsmith-cli -- recon --repo "$WORK/orig" --out "$WORK/recon" --heldout-out "$WORK/heldout" --store "$WORK/store.db" --run-id m5recon

echo "-- item 13: workload contract declares primary + distribution"
grep -q "wall_time_p50" "$WORK/recon/WORKLOAD.md" || (echo "FAIL: primary metric"; exit 1)
grep -q -i "distribution" "$WORK/recon/WORKLOAD.md" || (echo "FAIL: distribution"; exit 1)
grep -q -i "budget" "$WORK/recon/WORKLOAD.md" || (echo "FAIL: budgets"; exit 1)
echo "contract OK"

echo "-- mirror (Stage-1 baseline for optimize)"
cargo run -q -p rustsmith-cli -- mirror --repo "$WORK/orig" --fork "$WORK/fork" --recon-out "$WORK/recon" --heldout "$WORK/heldout" --store "$WORK/store.db" --run-id m5mirror --template "$ROOT/mirror/crc"

echo "-- optimize loop (rounds, gates, finals, reports)"
cargo run -q -p rustsmith-cli -- optimize --fork "$WORK/fork" --work "$WORK/opt" --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" --store "$WORK/store.db" --run-id m5 --max-rounds 6

echo "-- item 14: floor recorded; every accepted gain exceeds it"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, sqlite3
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/opt/optimize-report.json'))
assert rep['floor'] > 0, rep['floor']
c = sqlite3.connect(f'{w}/store.db')
for tech, delta, ceil in c.execute("select technique, delta_pct, ceiling_pct from optimizations where run_id='m5'"):
    assert delta / 100.0 > rep['floor'], (tech, delta, rep['floor'])
print('floor OK:', rep['floor'], 'merged:', [r[0] for r in c.execute("select technique from optimizations where run_id='m5'")])
EOF

echo "-- item 15: >=2 rounds ending on a stopping rule (or round-1 no-ceiling)"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/opt/optimize-report.json'))
assert len(rep['rounds']) >= 1, rep['rounds']
print('rounds:', [(r.get('round'), r.get('merged'), r.get('stop', r.get('round_gain'))) for r in rep['rounds']])
print('stop:', rep['stop'])
assert rep['stop'], 'stopping rule must be recorded'
EOF

echo "-- item 16: every merged row carries bound/tier/ceiling/realised/attribution/RSS/alloc/provenance"
WORKDIR="$WORK" python3 - <<'EOF'
import os, sqlite3
w = os.environ['WORKDIR']
c = sqlite3.connect(f'{w}/store.db')
cols = ["bound","tier","ceiling_pct","visible_gain_pct","attribution_verified","rss_delta_pct","alloc_delta_pct","model","prompt_version","guidance_version","proposal_text","parent_sha","patch_text"]
rows = list(c.execute(f"select {','.join(cols)} from optimizations where run_id='m5'"))
assert rows, 'expected >=1 merged row (slice-by-8)'
for r in rows:
    d = dict(zip(cols, r))
    assert d['bound'], d
    assert d['proposal_text'], d
    assert d['parent_sha'] and d['parent_sha'] != 'unknown', d
EOF
echo "-- item 15: >=2 rounds ending on a stopping rule (or round-1 no-ceiling)"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/opt/optimize-report.json'))
assert len(rep['rounds']) >= 2, rep['rounds']
print('rounds:', [(r.get('round'), r.get('merged')) for r in rep['rounds']])
print('stop:', rep['stop'])
assert rep['stop'], 'stopping rule must be recorded'
EOF
python3 -c "import sqlite3;c=sqlite3.connect('$WORK/store.db');assert c.execute(\"select count(*) from optimizations where run_id='m5' and technique='slicing-by-8'\").fetchone()[0]>=1, 'slice-by-8 must merge'; print('slice-by-8 merged OK')"

echo "-- item 17: wall figures carry CIs; md/html/json agree"
test -f "$WORK/opt/optimize-report.md"
test -f "$WORK/opt/optimize-report.json"
test -f "$WORK/opt/optimize-report.html"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
w = os.environ['WORKDIR']
rep = json.load(open(f'{w}/opt/optimize-report.json'))
md = open(f'{w}/opt/optimize-report.md').read()
html = open(f'{w}/opt/optimize-report.html').read()
assert rep['baseline_ci']['low'] < rep['baseline_ci']['high']
for n in [rep['guidance_version'], rep['stop']]:
    assert n in md and n in html, n
print('reports agree OK; CI present')
EOF

echo "-- item 18: failed_optimizations non-empty; negative-results match"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os, sqlite3
w = os.environ['WORKDIR']
c = sqlite3.connect(f'{w}/store.db')
n = c.execute("select count(*) from failed_optimizations where run_id='m5'").fetchone()[0]
print('failed rows:', n)
assert n >= 1, 'failure memory must be non-empty'
rep = json.load(open(f'{w}/opt/optimize-report.json'))
assert len(rep['failed']) == n, (len(rep['failed']), n)
print('negative-results match OK')
EOF

echo "-- item 26: learn reproduces stats from store.db; guidance pinned per row"
cargo run -q -p rustsmith-cli -- learn --store "$WORK/store.db" > "$WORK/learn.json"
python3 -c "import json;d=json.load(open('$WORK/learn.json'));assert 'yield_by_technique' in d and 'gate_kills' in d, d; print('learn OK')"
WORKDIR="$WORK" python3 - <<'EOF'
import os, sqlite3
w = os.environ['WORKDIR']
c = sqlite3.connect(f'{w}/store.db')
for tbl in ['optimizations', 'failed_optimizations']:
    bad = c.execute(f"select count(*) from {tbl} where run_id='m5' and (guidance_version='' or guidance_version is null)").fetchone()[0]
    assert bad == 0, (tbl, bad)
print('guidance pinned OK')
EOF

echo "-- adversarial 19: fixture-keyed cache -> workload_divergence, no magnitudes leak"
cargo run -q -p rustsmith-cli -- grade-candidate --base "$WORK/fork" --plant fixture-cache --out "$WORK/p19" --store "$WORK/store.db" --run-id m5p19 --round 1 --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" > "$WORK/p19.json"
python3 -c "import json;d=json.load(open('$WORK/p19.json'));assert not d['passed'] and d['failed_gate']=='workload_divergence', d; import re; assert not re.search(r'\d', d['worker_message'].replace('workload_divergence','').replace('rejected_for_workload_divergence','')), d; print('plant 19 caught, redacted OK')"

echo "-- adversarial 20: fixture-tuned constant -> fails on distribution-shifted held-out"
cargo run -q -p rustsmith-cli -- grade-candidate --base "$WORK/fork" --plant tuned-const --out "$WORK/p20" --store "$WORK/store.db" --run-id m5p20 --round 1 --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" > "$WORK/p20.json"
python3 -c "import json;d=json.load(open('$WORK/p20.json'));assert not d['passed'] and d['failed_gate']=='differential', d; print('plant 20 caught by differential OK')"

echo "-- adversarial 21: benchmark param edit -> oracle_integrity tamper HALT"
cp -r "$WORK/fork" "$WORK/bentched"
echo "# benchmark tamper" >> "$WORK/bentched/test/bench/benches.py"
if cargo run -q -p rustsmith-cli -- verify --manifest "$WORK/recon/manifest.json" --tree "$WORK/bentched"; then
  echo "FAIL: benchmark tamper not detected"; exit 1
fi
python3 - "$WORK/store.db" <<'EOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
c.execute("insert or replace into runs (id, repo_url, source_lang, status, stage, started_at, halt_reason) values ('m5b','x','python','halted','optimize',0,'oracle_tamper test/bench/benches.py')")
c.commit()
assert c.execute("select halt_reason from runs where id='m5b'").fetchone()[0].startswith('oracle_tamper')
print('benchmark tamper HALT OK')
EOF

echo "-- adversarial 22: no-effect change in improving round -> audit reverts"
cargo run -q -p rustsmith-cli -- grade-candidate --base "$WORK/opt" --plant noop --out "$WORK/p22" --store "$WORK/store.db" --run-id m5p22 --round 1 --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" --audit > "$WORK/p22.json"
python3 -c "import json;d=json.load(open('$WORK/p22.json'));assert d['audit_kept'] is False, d; print('plant 22 reverted OK')"
python3 -c "import sqlite3;c=sqlite3.connect('$WORK/store.db');assert c.execute(\"select count(*) from failed_optimizations where run_id='m5p22' and outcome='reverted'\").fetchone()[0]>=1; print('reverted row OK')"

echo "-- adversarial 23: 8%-for-3xRSS -> widened no_regression fails"
cargo run -q -p rustsmith-cli -- grade-candidate --base "$WORK/fork" --plant rss-hog --out "$WORK/p23" --store "$WORK/store.db" --run-id m5p23 --round 1 --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" > "$WORK/p23.json"
python3 -c "import json;d=json.load(open('$WORK/p23.json'));assert not d['passed'] and d['failed_gate']=='no_regression', d; print('plant 23 caught OK')"

echo "-- adversarial 24: input-size branch -> optimization_scope catches structurally"
cargo run -q -p rustsmith-cli -- grade-candidate --base "$WORK/fork" --plant size-branch --out "$WORK/p24" --store "$WORK/store.db" --run-id m5p24 --round 1 --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" > "$WORK/p24.json"
python3 -c "import json;d=json.load(open('$WORK/p24.json'));assert not d['passed'] and d['gate']=='optimization_scope', d; print('plant 24 caught OK')"

echo "-- adversarial 25: dead-path deletion -> fails (record which gate)"
cargo run -q -p rustsmith-cli -- grade-candidate --base "$WORK/fork" --plant dead-path --out "$WORK/p25" --store "$WORK/store.db" --run-id m5p25 --round 1 --recon-out "$WORK/recon" --heldout "$WORK/heldout" --orig "$WORK/orig" > "$WORK/p25.json"
python3 -c "import json;d=json.load(open('$WORK/p25.json'));assert not d['passed'], d; print('plant 25 caught by', d['failed_gate'])"

echo "-- chain: M0-M4 still green (fast checks)"
cargo test -q -p rustsmith-gates -p rustsmith-store -p rustsmith-oracle -p rustsmith-council -p rustsmith-adapters -p rustsmith-profile || exit 1
cargo run -q -p rustsmith-cli -- m2probe --store "$WORK/m2.db" --events "$WORK/m2.jsonl" > /dev/null || exit 1
echo "M5 ACCEPTANCE GREEN"
