#!/bin/bash
# Mini proof for Elmer S1: BIND(C) fixture + helper skip.
# NEVER edit this script to make it pass — fix the implementation.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d /tmp/mini-proof-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
FIX="$ROOT/tests/fixtures/mini"
REPO="$WORK/repo"
RECON="$WORK/recon"
HELDOUT="$WORK/heldout"
FORK="$WORK/fork"
STORE="$WORK/store.db"
RUN="mini-proof"
echo "== mini proof: work=$WORK"
cp -r "$FIX" "$REPO"

echo "-- recon"
cargo run -q -p rustsmith-cli -- recon --repo "$REPO" --out "$RECON" --heldout-out "$HELDOUT" --store "$STORE" --run-id "$RUN"

echo "-- worker + mirror (scope keeps only src/, helper skips)"
cat > "$WORK/worker.sh" <<EOF
#!/bin/sh
set -eu
cat > /dev/null
printf '{"tokens_in": 10, "tokens_out": 10}'
case "\${UNIT_ID:-}" in
  *mini_add*)
    crate="mini_add"
    ref="$ROOT/tests/fixtures/mini/ref/rust/\${crate}"
    mkdir -p "rust/\${crate}/src" 1>&2
    cp "\${ref}/Cargo.toml" "rust/\${crate}/Cargo.toml"
    cp "\${ref}/src/lib.rs" "rust/\${crate}/src/lib.rs"
    (cd "rust/\${crate}" && cargo build 1>&2 2>&1) 1>&2 2>&1 || true
    mkdir -p "build/rust" 1>&2
    cp "rust/\${crate}/target/debug/lib\${crate}.a" "build/rust/lib\${crate}.a" 1>&2
    ;;
  *) ;;
esac
EOF
chmod +x "$WORK/worker.sh"
export RUSTSMITH_WORKER_CMD="$WORK/worker.sh"
export RUSTSMITH_SCOPE="src"
cargo run -q -p rustsmith-cli -- mirror --repo "$REPO" --fork "$FORK" --recon-out "$RECON" --heldout "$HELDOUT" --store "$STORE" --run-id "$RUN" --template "$ROOT/mirror/Mini" 2>&1 | tee "$WORK/mirror.log"
grep -q "mirror: 1/1 passed divergence=0.0000" "$WORK/mirror.log" || { echo "FAIL: expected mirror 1/1 divergence=0.0000"; exit 1; }

python3 - "$STORE" <<'PYEOF'
import sqlite3, sys
c = sqlite3.connect(sys.argv[1])
units = dict(c.execute("select id, status from units"))
assert units.get("fortran:src/mini_add.F90") == "passed", units
assert units.get("c:include/mini_helper.h") == "skipped", units
print("store: BIND(C) passed, helper skipped OK")
PYEOF

echo "MINI PROOF GREEN"
