#!/bin/bash
# Hunks acceptance (ADR-031, narrowed): the mirror grade-to-merge transaction
# records one hunks row per real merge in store.db (source, not technique;
# wall/workload NULL, never fake zeros); combine-stores rolls stores up.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CRC_PIN="4e65ac4"
echo "== hunks: work=$WORK"

echo "-- unit tests (store crate)"
cargo test -q -p rustsmith-store || (echo "FAIL: store unit tests"; exit 1)

echo "-- fixtures + recon"
git clone --quiet https://github.com/Nicoretti/crc "$WORK/orig"
git -C "$WORK/orig" checkout --quiet "$CRC_PIN"
cargo run -q -p rustsmith-cli -- recon --repo "$WORK/orig" --out "$WORK/recon" --heldout-out "$WORK/heldout" --store "$WORK/store-a.db" --run-id hunks-recon

echo "-- mirror run A + run B (two stores, distinct run ids)"
cargo run -q -p rustsmith-cli -- mirror --repo "$WORK/orig" --fork "$WORK/fork-a" --recon-out "$WORK/recon" --heldout "$WORK/heldout" --store "$WORK/store-a.db" --run-id hunks-a --template "$ROOT/mirror/crc"
cargo run -q -p rustsmith-cli -- mirror --repo "$WORK/orig" --fork "$WORK/fork-b" --recon-out "$WORK/recon" --heldout "$WORK/heldout" --store "$WORK/store-b.db" --run-id hunks-b --template "$ROOT/mirror/crc"

echo "-- hunks: one row per merge, source set, wall/workload NULL"
WORKDIR="$WORK" python3 - <<'EOF'
import os, sqlite3
w = os.environ['WORKDIR']
for db, run in (('store-a.db', 'hunks-a'), ('store-b.db', 'hunks-b')):
    c = sqlite3.connect(f'{w}/{db}')
    passed = [r[0] for r in c.execute("select id from units where run_id=? and status='passed'", (run,))]
    hunks = list(c.execute("select unit_id, package, source, before_ref, after_ref, wall_delta_pct, workload, verdict from hunks where run_id=?", (run,)))
    assert passed, (db, 'expected passed units')
    assert 1 <= len(hunks) <= len(passed), (db, len(hunks), passed)
    for unit, pkg, source, before, after, wall, workload, verdict in hunks:
        assert unit in passed, (db, unit, passed)
        assert pkg == 'crc', pkg
        assert source == 'reference-template', source
        assert before and after and before != after, (unit, before, after)
        assert wall is None and workload is None, (unit, wall, workload)
        assert verdict == 'merged', verdict
    print(f'{db}: hunks OK ({len(hunks)} rows for {len(passed)} passed units)')
EOF

echo "-- combine-stores rollup"
cargo run -q -p rustsmith-cli -- combine-stores --stores "$WORK/store-a.db,$WORK/store-b.db" --out "$WORK/combined.db"
WORKDIR="$WORK" python3 - <<'EOF'
import os, sqlite3
w = os.environ['WORKDIR']
a = sqlite3.connect(f'{w}/store-a.db')
b = sqlite3.connect(f'{w}/store-b.db')
m = sqlite3.connect(f'{w}/combined.db')
na = a.execute('select count(*) from hunks').fetchone()[0]
nb = b.execute('select count(*) from hunks').fetchone()[0]
nm = m.execute('select count(*) from hunks').fetchone()[0]
assert na > 0 and nb > 0 and nm == na + nb, (na, nb, nm)
assert m.execute("select count(*) from hunks where source<>'reference-template'").fetchone()[0] == 0
print(f'combine OK: {na}+{nb}={nm}')
EOF

echo "HUNKS ACCEPTANCE GREEN"
