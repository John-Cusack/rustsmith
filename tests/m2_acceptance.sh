#!/bin/bash
# M2 acceptance (SPEC §15, IMPLEMENTATION_M2 §3):
# "present the council a decision with a deliberately wrong majority position;
#  confirm the minority reasoning is preserved in the log and the Architect's
#  tiebreak is recorded with justification."
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "== M2: work=$WORK"

echo "-- unit tests (council protocol, blind isolation, privacy)"
cargo test -q -p rustsmith-council || (echo "FAIL: council unit tests"; exit 1)

echo "-- m2probe: wrong-majority tiebreak + minority preservation + consensus + replan + halts + privacy"
cargo run -q -p rustsmith-cli -- m2probe --store "$WORK/store.db" --events "$WORK/events.jsonl"
echo "-- decisions table is append-only (no UPDATE/DELETE path)"
if grep -rn "UPDATE decisions\|DELETE FROM decisions" "$ROOT/crates/" 2>/dev/null | grep .; then
  echo "FAIL: decisions table has mutation path"; exit 1
fi
echo "append-only OK"
echo "-- blind-critique API shape (critique takes proposal+artifact only)"
if grep -rn "fn critique" "$ROOT/crates/rustsmith-council/src/lib.rs" | grep -v "seat.*proposal.*artifact" | grep -q "Position.*Position"; then
  echo "FAIL: critique signature leaks positions"; exit 1
fi
echo "blind API OK"
echo "-- models swappable via config only (no hardcode outside config)"
if grep -rn "gpt-\|claude\|gemini\|llama" "$ROOT/crates/" 2>/dev/null | grep -v test | grep .; then
  echo "FAIL: hardcoded model names"; exit 1
fi
test -f "$ROOT/config/default.toml"
echo "config OK"
echo "-- chain: M0 + M1 still green"
bash "$ROOT/tests/m0_acceptance.sh" > "$WORK/m0.log" 2>&1 | tail -n 2 || (cat "$WORK/m0.log"; exit 1)
bash "$ROOT/tests/m1_acceptance.sh" > "$WORK/m1.log" 2>&1 | tail -n 2 || (cat "$WORK/m1.log"; exit 1)
echo "M2 ACCEPTANCE GREEN"
