#!/bin/bash
# Lane-capture acceptance (ADR-055): the agent-lane path writes scoped
# run/unit/gate/hunk rows, session joins, transcript provenance, wall time
# and rights into export-records output, verified end to end on synthetic
# fixtures (no network, no LLM). verify-sample.sh is the final gate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
# Scratch must be durable (never /tmp): keep it beside WORK's lifetime but
# outside tmpfs, removed on exit with everything else.
SCRATCH="$(mktemp -d "$HOME/.rs-lane-accept-XXXXXX")"
trap 'rm -rf "$WORK" "$SCRATCH"' EXIT
R="cargo run -q -p rustsmith-cli --"
echo "== lane-capture: work=$WORK"

echo "-- fixtures"
mkdir -p "$WORK/ev" "$WORK/ts"
cat > "$WORK/ev/pure.jsonl" <<'EOF'
{"key": "a|1", "status": "ok", "value": "1"}
{"key": "a|2", "status": "ok", "value": "2"}
EOF
cat > "$WORK/ev/rust.jsonl" <<'EOF'
{"key": "a|1", "status": "ok", "value": "1"}
{"key": "a|2", "status": "ok", "value": "WRONG"}
EOF
cat > "$WORK/ev/verification.json" <<'EOF'
{"passed": true, "stage_a": {"passed": true}, "stage_b": {"passed": true}}
EOF
cat > "$WORK/ev/scoped.json" <<'EOF'
{"affected": [], "failed": 1, "failed_tests": ["t1"],
 "integrity": {"passed": true, "detail": {"reason": "ok", "test_count": 10}},
 "parity": {"passed": false, "detail": {"failed": 1}}}
EOF
cat > "$WORK/ev/bench.json" <<'EOF'
[{"track": "t", "pure_wall_s": 1.0, "rust_wall_s": 0.5, "speedup": 2.0,
  "pure_rss_kb": 1, "rust_rss_kb": 1, "checksum": "x", "checksum_rust": "x"}]
EOF
cat > "$WORK/ts/sess.jsonl" <<'EOF'
{"type": "session", "version": 3, "id": "sess-acceptance-1"}
{"type": "message", "message": {"role": "assistant", "content": [], "provider": "p", "model": "m", "usage": {"input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 150, "reasoningTokens": 1, "cost": {"input": 0.001, "output": 0.0005, "cacheRead": 0, "cacheWrite": 0, "total": 0.0015}}}}
{"type": "message", "message": {"role": "user", "content": []}}
EOF

echo "-- /tmp scratch refused"
if $R lane-capture init --store "$WORK/lane.db" --run lane-01 --repo https://example.com/port-rust \
    --lang python --scratch /tmp/lane-evap --sessions "$WORK/sessions.csv" 2>/dev/null; then
    echo "FAIL: /tmp scratch accepted"; exit 1
fi
echo "ok: /tmp refused"

echo "-- init + grades + sessions + finish"
$R lane-capture init --store "$WORK/lane.db" --run lane-01 --repo https://example.com/port-rust \
    --lang python --lane lane-accept --package pkg --upstream-license MIT \
    --upstream-repo https://example.com/upstream --upstream-commit abc123 \
    --scratch "$SCRATCH" --sessions "$WORK/sessions.csv" >/dev/null
$R lane-capture grade-unit --store "$WORK/lane.db" --run lane-01 --unit 'python:parity' \
    --status failed --commit-sha deadbeef --evidence "$WORK/ev/rust.jsonl" \
    --evidence-kind equiv-pair --expected "$WORK/ev/pure.jsonl" --wall-secs 9 --no-hunk >/dev/null
$R lane-capture grade-unit --store "$WORK/lane.db" --run lane-01 --unit 'python:bench' \
    --status passed --commit-sha deadbeef --evidence "$WORK/ev/bench.json" \
    --evidence-kind bench --hunk-before abc123 --package pkg >/dev/null
$R lane-capture grade-unit --store "$WORK/lane.db" --run lane-01 --unit 'c:scoped' \
    --status failed --commit-sha deadbeef --evidence "$WORK/ev/scoped.json" \
    --evidence-kind scoped-grade --wall-secs 4 --no-hunk >/dev/null
$R lane-capture session --sessions "$WORK/sessions.csv" --run lane-01 --unit 'python:parity' \
    --session-id sess-acceptance-1 --omp "$WORK/ts/sess.jsonl" --phoenix trace-1 >/dev/null
$R lane-capture session --sessions "$WORK/sessions.csv" --run lane-01 --unit 'c:scoped' \
    --session-id sess-acceptance-1 --omp "$WORK/ts/sess.jsonl" >/dev/null
$R lane-capture finish --store "$WORK/lane.db" --run lane-01 --status passed >/dev/null

echo "-- store rows"
WORKDIR="$WORK" python3 - <<'EOF'
import os, sqlite3
w = os.environ['WORKDIR']
c = sqlite3.connect(f'{w}/lane.db')
units = dict(c.execute("select id, status from units"))
assert units == {'python:parity': 'failed', 'python:bench': 'passed', 'c:scoped': 'failed'}, units
gates = c.execute("select unit_id, gate, passed from gate_results").fetchall()
assert ('python:parity', 'oracle_parity', 0) in gates, gates
assert ('python:bench', 'oracle_parity', 1) in gates, gates
assert ('python:bench', 'benchmark', 1) in gates, gates
assert ('c:scoped', 'oracle_integrity', 1) in gates, gates
hunks = c.execute("select unit_id, source, before_ref, after_ref, verdict from hunks").fetchall()
assert hunks == [('python:bench', 'lane-port', 'abc123', 'deadbeef', 'merged')], hunks
run = c.execute("select status, ended_at from runs where id='lane-01'").fetchone()
assert run[0] == 'passed' and run[1] is not None, run
print('store OK: 3 units, gates, 1 hunk, run finished')
EOF
grep -q '^run,unit,session_id,omp,phoenix,tripwire$' "$WORK/sessions.csv"
[ "$(grep -c . "$WORK/sessions.csv")" = 3 ]
echo "store + sessions OK"

echo "-- export with session map + transcripts"
$R export-records --store "$WORK/lane.db" --out "$WORK/records.jsonl" \
    --session-map "$WORK/sessions.csv" --transcripts "$WORK/ts" 2>"$WORK/stderr.log"
WORKDIR="$WORK" python3 - <<'EOF'
import json, os
recs = [json.loads(l) for l in open(os.environ['WORKDIR'] + '/records.jsonl')]
assert len(recs) == 3, len(recs)
by_unit = {r['unit_id']: r for r in recs}
par = by_unit['python:parity']
assert par['measurement']['outcome'] == 'gate_failed', par['measurement']['outcome']
assert par['measurement']['oracle'] == {'passed': 1, 'total': 2, 'skipped': 0}, par['measurement']['oracle']
assert par['session_id'] == 'sess-acceptance-1', par['session_id']
assert par['trace_ids']['omp'] == os.environ['WORKDIR'] + '/ts/sess.jsonl', par['trace_ids']
assert par['trace_ids']['phoenix'] == 'trace-1', par['trace_ids']
assert par['provenance']['model'] == 'p/m', par['provenance']
assert par['provenance']['tokens_spent'] == 150, par['provenance']
assert abs(par['provenance']['cost_usd'] - 0.0015) < 1e-12, par['provenance']
assert par['provenance']['wall_seconds'] == 9, par['provenance']
bench = by_unit['python:bench']
assert bench['measurement']['outcome'] == 'merged', bench['measurement']['outcome']
assert bench['code']['before_ref'] == 'abc123' and bench['code']['after_ref'] == 'deadbeef', bench['code']
assert bench['rights']['upstream_repo'] == 'https://example.com/upstream', bench['rights']
assert bench['rights']['upstream_commit'] == 'abc123', bench['rights']
scoped = by_unit['c:scoped']
assert scoped['measurement']['oracle'] == {'passed': 10, 'total': 10, 'skipped': 0}, scoped['measurement']['oracle']
assert scoped['provenance']['wall_seconds'] == 4, scoped['provenance']
print('export OK: joins, provenance, walls, rights')
EOF

echo "-- GPL inline by default, opt-in split"
$R lane-capture init --store "$WORK/gpl.db" --run gpl-01 --repo https://example.com/e-rust \
    --lang c --upstream-license GPL-2.0-or-later --rights-note "permit (test)" \
    --scratch "$SCRATCH/g" --sessions "$WORK/gsessions.csv" >/dev/null
$R lane-capture grade-unit --store "$WORK/gpl.db" --run gpl-01 --unit 'c:u' \
    --status passed --commit-sha deadbeef --evidence "$WORK/ev/bench.json" \
    --evidence-kind bench --no-hunk >/dev/null
$R export-records --store "$WORK/gpl.db" --out "$WORK/gpl.jsonl" 2>/dev/null
python3 -c "
import json
r = json.load(open('$WORK/gpl.jsonl'))
assert r['rights']['record_license'] == 'GPL-2.0-or-later', r['rights']
assert 'permit (test)' in r['rights']['attribution_text'], r['rights']
print('GPL inline OK')"
$R export-records --store "$WORK/gpl.db" --out "$WORK/gpl2.jsonl" --license-split "$WORK/gpl-split.jsonl" 2>/dev/null
python3 -c "
import json
main = [json.loads(l) for l in open('$WORK/gpl2.jsonl')]
split = [json.loads(l) for l in open('$WORK/gpl-split.jsonl')]
assert main == [] and len(split) == 1, (len(main), len(split))
assert split[0]['rights']['record_license'] == 'GPL-2.0-or-later'
print('GPL opt-in split OK')"

echo "-- verify-sample"
bash "$ROOT/scripts/verify-sample.sh" --records "$WORK/records.jsonl" --store "$WORK/lane.db" --n 3
echo "lane-capture acceptance: PASS"
