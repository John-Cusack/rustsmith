#!/bin/bash
# Capture backfill replay (ADR-056): re-grade the parked lane-capture units
# with recovered parity evidence, join OMP sessions, re-export.
#
# Run-once against the backfill stores in
# /home/john/firstmate/data/rs-lane-capture/backfill/ (`grade-unit` is
# single-shot per unit by design: it refuses units that already carry gate
# rows). Each lane store is backed up to store.db.pre-backfill-20261028
# before its parked rows are deleted. Evidence lives in
# /home/john/firstmate/data/rs-port-<pkg>/{capture,scratch}/; transcripts in
# ~/.omp/agent/sessions. Nothing here publishes.
#
# What it replays (all counts measured 2026-10-28, see ADR-056):
#   pycparser 0->3, tabulate 0->3, marshmallow 0->3, sqlparse 0->3,
#   sympy 0->4 (headroom:simplify stays parked: never ported),
#   docutils new lane 0->3 at published head 8e53457.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
R="$ROOT/target/release/rustsmith"
B=/home/john/firstmate/data/rs-lane-capture/backfill
D=/home/john/firstmate/data
T=/home/john/.omp/agent/sessions
[ -x "$R" ] || { echo "build first: cargo build --release" >&2; exit 2; }

clear_units() { # <lane> <unit>...
  local lane="$1"; shift
  python3 - "$B/$lane/store.db" "$@" <<'EOF'
import sqlite3, sys
con = sqlite3.connect(sys.argv[1])
for u in sys.argv[2:]:
    con.execute("delete from gate_results where unit_id=?", (u,))
    con.execute("delete from units where id=?", (u,))
con.commit()
EOF
}

lane_session() { # <lane> <run> <unit> <session-id> <omp> [phoenix]
  if [ -n "${6:-}" ]; then
    "$R" lane-capture session --sessions "$B/$1/sessions.csv" \
      --run "$2" --unit "$3" --session-id "$4" --omp "$5" --phoenix "$6"
  else
    "$R" lane-capture session --sessions "$B/$1/sessions.csv" \
      --run "$2" --unit "$3" --session-id "$4" --omp "$5"
  fi
}

drop_stale_sessions() { # <lane> <unit>...
  local lane="$1"; shift
  local expr
  expr=$(printf ',%s,|' "$@")
  expr="${expr%|}"
  grep -v "$expr" "$B/$lane/sessions.csv" > "$B/$lane/sessions.csv.new"
  mv "$B/$lane/sessions.csv.new" "$B/$lane/sessions.csv"
}

export_lane() { # <lane> <run>
  "$R" export-records --store "$B/$1/store.db" --out "$B/$1/export/records.jsonl" \
    --session-map "$B/$1/sessions.csv" --transcripts "$T" 2>"$B/$1/export/stderr.log" \
    | head -c 300; echo
}

echo "=== pycparser (0 -> 3) ==="
cp -n $B/pycparser/store.db $B/pycparser/store.db.pre-backfill-20261028
clear_units pycparser release:verification
CS=35c2965b7054c1e97f2d13dc4234878847016ee
"$R" lane-capture grade-unit --store $B/pycparser/store.db --run rs-port-pycparser-01 --unit 'upstream:suite' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-pycparser/dist/verification.json --evidence-kind verification \
  --evidence $D/rs-port-pycparser/capture/upstream-pytest.json --evidence-kind scoped-grade --wall-secs 1.65 --no-hunk
"$R" lane-capture grade-unit --store $B/pycparser/store.db --run rs-port-pycparser-01 --unit 'port:equiv' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-pycparser/capture/port-pytest.json --evidence-kind scoped-grade --wall-secs 0.27 \
  --evidence $D/rs-port-pycparser/capture/core.json --evidence-kind scoped-grade --wall-secs 2 --no-hunk
"$R" lane-capture grade-unit --store $B/pycparser/store.db --run rs-port-pycparser-01 --unit 'bench:two-track' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-pycparser/capture/bench.json --evidence-kind bench --wall-secs 32.87 --no-hunk
S=01a18292-015a-76c1-92af-4121eae28b8e
O=$T/-orca-workspaces-rustsmith-fm-rs-port-pycparser/2026-10-28T12-52-49-114Z_${S}.jsonl
P=c02fdd6779213b2e85b7a43abda594e
for u in 'upstream:suite' 'port:equiv' 'bench:two-track'; do
  lane_session pycparser rs-port-pycparser-01 "$u" $S "$O" $P >/dev/null
done
drop_stale_sessions pycparser release:verification

echo "=== tabulate (0 -> 3) ==="
cp -n $B/tabulate/store.db $B/tabulate/store.db.pre-backfill-20261028
clear_units tabulate release:verification headroom:probe
CS=b1fe9a2885becc215e00f8fc221d42ab629853c4
"$R" lane-capture grade-unit --store $B/tabulate/store.db --run rs-port-tabulate-01 --unit 'upstream:suite' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-tabulate/dist/verification.json --evidence-kind verification \
  --evidence $D/rs-port-tabulate/capture/upstream-pytest.json --evidence-kind scoped-grade --wall-secs 1.65 --no-hunk
"$R" lane-capture grade-unit --store $B/tabulate/store.db --run rs-port-tabulate-01 --unit 'equiv:corpus' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-tabulate/capture/equiv_rust.jsonl --evidence-kind equiv-pair \
  --expected $D/rs-port-tabulate/capture/equiv_pure.jsonl --wall-secs 0.25 --no-hunk
"$R" lane-capture grade-unit --store $B/tabulate/store.db --run rs-port-tabulate-01 --unit 'bench:two-track' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-tabulate/capture/bench.json --evidence-kind bench --wall-secs 8.32 --no-hunk
S=01a18292-5de9-70c0-be44-e82dd83d0053
O=$T/-orca-workspaces-rustsmith-fm-rs-port-tabulate/2026-10-28T12-53-12-809Z_${S}.jsonl
P=d9928e8c6928e28a7e9c9a661cba9f6e
for u in 'upstream:suite' 'equiv:corpus' 'bench:two-track'; do
  lane_session tabulate rs-port-tabulate-01 "$u" $S "$O" $P >/dev/null
done
drop_stale_sessions tabulate release:verification headroom:probe

echo "=== marshmallow (0 -> 3) ==="
cp -n $B/marshmallow/store.db $B/marshmallow/store.db.pre-backfill-20261028
clear_units marshmallow headroom:probe
CS=e8d863d6f18b74d95a4b0ed4b06c40a59716f313
"$R" lane-capture grade-unit --store $B/marshmallow/store.db --run rs-port-marshmallow-01 --unit 'upstream:gate' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-marshmallow/capture/upstream-pytest.json --evidence-kind scoped-grade --wall-secs 0.89 --no-hunk
"$R" lane-capture grade-unit --store $B/marshmallow/store.db --run rs-port-marshmallow-01 --unit 'port:tests' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-marshmallow/capture/port-pytest.json --evidence-kind scoped-grade --wall-secs 0.31 \
  --evidence $D/rs-port-marshmallow/probe.json --evidence-kind probe --no-hunk
"$R" lane-capture grade-unit --store $B/marshmallow/store.db --run rs-port-marshmallow-01 --unit 'bench:two-track' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-marshmallow/capture/bench.json --evidence-kind bench --wall-secs 2.32 --no-hunk
S=01a18292-bc8d-75e0-9d2b-650095ab729e
O=$T/-orca-workspaces-rustsmith-fm-rs-port-marshmallow/2026-10-28T12-53-37-037Z_${S}.jsonl
P=9ed3c101908ab7a4ab97aa77027249f8
for u in 'upstream:gate' 'port:tests' 'bench:two-track'; do
  lane_session marshmallow rs-port-marshmallow-01 "$u" $S "$O" $P >/dev/null
done
drop_stale_sessions marshmallow headroom:probe

echo "=== sqlparse (0 -> 3) ==="
cp -n $B/sqlparse/store.db $B/sqlparse/store.db.pre-backfill-20261028
clear_units sqlparse release:verification
CS=0d4fcd3ddb7d29afd61ea1e675568d0d566ef325
"$R" lane-capture grade-unit --store $B/sqlparse/store.db --run rs-port-sqlparse-01 --unit 'upstream:suite' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sqlparse/release-prep/verification.json --evidence-kind verification \
  --evidence $D/rs-port-sqlparse/capture/upstream-pytest.json --evidence-kind scoped-grade --wall-secs 1.45 --no-hunk
"$R" lane-capture grade-unit --store $B/sqlparse/store.db --run rs-port-sqlparse-01 --unit 'port:parity' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sqlparse/capture/port-pytest.json --evidence-kind scoped-grade --wall-secs 3.94 --no-hunk
"$R" lane-capture grade-unit --store $B/sqlparse/store.db --run rs-port-sqlparse-01 --unit 'bench:two-track' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sqlparse/capture/bench.json --evidence-kind bench --wall-secs 15.8 --no-hunk
S=01a182e9-a2d6-7099-8c8b-de0d39cd50d7
O=$T/-orca-workspaces-rustsmith-fm-rs-port-sqlparse/2026-10-28T14-28-32-086Z_${S}.jsonl
P=32fb747dec5e686504e6263a2eb7b7ae
for u in 'upstream:suite' 'port:parity' 'bench:two-track'; do
  lane_session sqlparse rs-port-sqlparse-01 "$u" $S "$O" $P >/dev/null
done
drop_stale_sessions sqlparse release:verification

echo "=== sympy (0 -> 4; headroom:simplify stays parked: never ported) ==="
cp -n $B/sympy/store.db $B/sympy/store.db.pre-backfill-20261028
clear_units sympy headroom:polys
CS=325b926e6799fe7a31e71689e5d4d1462e6b3718
"$R" lane-capture grade-unit --store $B/sympy/store.db --run rs-port-sympy-01 --unit 'equiv:kernel' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sympy/capture/equiv.json --evidence-kind scoped-grade --wall-secs 0.48 \
  --evidence $D/rs-port-sympy/polys.json --evidence-kind probe --no-hunk
"$R" lane-capture grade-unit --store $B/sympy/store.db --run rs-port-sympy-01 --unit 'core:rust' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sympy/capture/core.json --evidence-kind scoped-grade --wall-secs 1 --no-hunk
"$R" lane-capture grade-unit --store $B/sympy/store.db --run rs-port-sympy-01 --unit 'upstream:baseline' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sympy/capture/upstream-subset.json --evidence-kind scoped-grade --wall-secs 0.4 --no-hunk
"$R" lane-capture grade-unit --store $B/sympy/store.db --run rs-port-sympy-01 --unit 'bench:three-track' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-sympy/capture/bench.json --evidence-kind bench --wall-secs 3.62 --no-hunk
S=01a149da-4c37-7664-a797-1918419fe0f1
O=$T/-orca-workspaces-rustsmith-fm-rs-port-sympy/2026-10-17T12-33-25-559Z_${S}.jsonl
for u in 'equiv:kernel' 'core:rust' 'upstream:baseline' 'bench:three-track'; do
  lane_session sympy rs-port-sympy-01 "$u" $S "$O" >/dev/null
done
drop_stale_sessions sympy headroom:polys

echo "=== docutils (new lane, 0 -> 3) ==="
L=docutils; mkdir -p $B/$L/export
"$R" lane-capture init --store $B/$L/store.db --run rs-port-$L-01 \
  --repo https://github.com/John-Cusack/docutils-rust --lang python \
  --lane rs-port-$L --package docutils --upstream-license BSD-2-Clause \
  --upstream-repo https://github.com/python/docutils \
  --upstream-commit 97e4f8575f639f945d3d9af04532fc6452f0e288 \
  --scratch $B/$L/scratch --sessions $B/$L/sessions.csv >/dev/null
CS=8e534579680140891d446435ae25f7de6cb00c6
"$R" lane-capture grade-unit --store $B/$L/store.db --run rs-port-$L-01 --unit 'equiv:corpus' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-docutils/capture/equiv-corpus.json --evidence-kind scoped-grade --wall-secs 0.35 --no-hunk
"$R" lane-capture grade-unit --store $B/$L/store.db --run rs-port-$L-01 --unit 'port:core' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-docutils/capture/core.json --evidence-kind scoped-grade --wall-secs 1.63 --no-hunk
"$R" lane-capture grade-unit --store $B/$L/store.db --run rs-port-$L-01 --unit 'bench:two-track' \
  --status passed --commit-sha $CS \
  --evidence $D/rs-port-docutils/capture/bench.json --evidence-kind bench --wall-secs 169.58 --no-hunk
S=01a149da-c9a7-7343-a039-d2d92a830d4d
O=$T/-orca-workspaces-rustsmith-fm-rs-port-docutils/2026-10-17T12-33-57-671Z_${S}.jsonl
for u in 'equiv:corpus' 'port:core' 'bench:two-track'; do
  lane_session docutils rs-port-docutils-01 "$u" $S "$O" >/dev/null
done

echo "=== exports (all six lanes; transcripts joined, nothing published) ==="
for L in pycparser tabulate marshmallow sqlparse sympy docutils; do
  export_lane "$L" rs-port-$L-01
done
