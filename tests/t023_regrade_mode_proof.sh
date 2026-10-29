#!/bin/bash
# T023 regrade flag proof: `--mode substitution|entry` dispatches to the two
# committed gate functions without changing either, and `--library-manifest`
# emits per-library verdicts (per-file PluginEntry AND + `nm` dynsym veto).
# Hermetic: fixtures are generated in a temp dir; only gfortran + nm required.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
BIN="$ROOT/target/release/elmer-regrade"
echo "== T023: work=$WORK"

[ -x "$BIN" ] || { echo "build the binary first: cargo build --release -p rustsmith-adapters --bin elmer-regrade"; exit 1; }

mkdir -p "$WORK/tree" "$WORK/ref"
cat > "$WORK/tree/both_admit.F90" <<'EOF'
SUBROUTINE both_admit(x, n)
  IMPLICIT NONE
  REAL(KIND=8) :: x
  INTEGER :: n
  x = x + n
END SUBROUTINE both_admit
EOF
cat > "$WORK/tree/entry_only.F90" <<'EOF'
SUBROUTINE entry_only(model, x)
  IMPLICIT NONE
  TYPE :: model_t
    REAL(KIND=8) :: x
  END TYPE model_t
  TYPE(model_t) :: model
  REAL(KIND=8) :: x
  x = 1.0D0
END SUBROUTINE entry_only
EOF
cat > "$WORK/tree/both_refuse.F90" <<'EOF'
MODULE mymod
  IMPLICIT NONE
CONTAINS
  SUBROUTINE myproc(model)
    TYPE(model_t) :: model
  END SUBROUTINE myproc
END MODULE mymod
EOF
cat > "$WORK/dag.json" <<'EOF'
{"units": [
  {"id": "fortran:both_admit.F90", "exports": [{"linkage": "both_admit", "bind_c": false}]},
  {"id": "fortran:entry_only.F90", "exports": [{"linkage": "entry_only", "bind_c": false}]},
  {"id": "fortran:both_refuse.F90", "exports": [{"linkage": "__mymod_MOD_myproc", "bind_c": false}]}
]}
EOF
cat > "$WORK/units.json" <<'EOF'
[{"unit": "fortran:both_admit.F90", "ledger": "lane-fem-a"},
 {"unit": "fortran:entry_only.F90", "ledger": "lane-fem-a"},
 {"unit": "fortran:both_refuse.F90", "ledger": "lane-fem-a"}]
EOF

echo "-- default mode is substitution"
"$BIN" --dag "$WORK/dag.json" --tree "$WORK/tree" --units "$WORK/units.json" \
  --out "$WORK/default.json" 2>"$WORK/default.err"
python3 - "$WORK/default.json" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
assert "mode" not in d, "default output must stay byte-shape (no mode key)"
v = {u["unit"]: u["verdict"] for u in d["units"]}
assert v == {"fortran:both_admit.F90": "admit", "fortran:entry_only.F90": "refuse",
             "fortran:both_refuse.F90": "refuse"}, v
print("default=substitution, verdicts pinned")
EOF

echo "-- explicit substitution matches default verdicts"
"$BIN" --mode substitution --dag "$WORK/dag.json" --tree "$WORK/tree" \
  --units "$WORK/units.json" --out "$WORK/subst.json" 2>/dev/null
python3 - "$WORK/default.json" "$WORK/subst.json" <<'EOF'
import json, sys
a = json.load(open(sys.argv[1]))["units"]
b = json.load(open(sys.argv[2]))["units"]
assert [(u["unit"], u["verdict"], u["reason"]) for u in a] == \
       [(u["unit"], u["verdict"], u["reason"]) for u in b]
assert json.load(open(sys.argv[2]))["mode"] == "substitution"
print("substitution explicit == default verdicts")
EOF

echo "-- entry mode flips exactly the F77-opaque unit"
"$BIN" --mode entry --dag "$WORK/dag.json" --tree "$WORK/tree" \
  --units "$WORK/units.json" --out "$WORK/entry.json" 2>/dev/null
python3 - "$WORK/entry.json" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
assert d["mode"] == "entry"
v = {u["unit"]: u["verdict"] for u in d["units"]}
assert v == {"fortran:both_admit.F90": "admit", "fortran:entry_only.F90": "admit",
             "fortran:both_refuse.F90": "refuse"}, v
print("entry flips only entry_only (F77 derived-opaque)")
EOF

echo "-- bad mode rejected"
if "$BIN" --mode bogus --dag "$WORK/dag.json" --tree "$WORK/tree" \
    --units "$WORK/units.json" --out "$WORK/bogus.json" 2>/dev/null; then
  echo "FAIL: bogus mode accepted"; exit 1
fi
echo "bogus mode rejected"

echo "-- reference .so for dynsym checks"
( cd "$WORK" && gfortran -c tree/both_admit.F90 -o both_admit.o )
( cd "$WORK" && gfortran -c tree/entry_only.F90 -o entry_only.o )
( cd "$WORK" && gfortran -shared -o ref/prooflib.so both_admit.o entry_only.o )
nm -D --defined-only "$WORK/ref/prooflib.so" | awk '{print $NF}' | sort

cat > "$WORK/manifest.json" <<'EOF'
{"libraries": [
  {"name": "prooflib", "so": "prooflib.so",
   "units": [{"unit": "fortran:both_admit.F90"}, {"unit": "fortran:entry_only.F90"}],
   "expected_dynsyms": ["both_admit_", "entry_only_"]},
  {"name": "badlib", "so": "prooflib.so",
   "units": [{"unit": "fortran:both_admit.F90"}, {"unit": "fortran:both_refuse.F90"}],
   "expected_dynsyms": ["both_admit_"]},
  {"name": "wrongsym", "so": "prooflib.so",
   "units": [{"unit": "fortran:both_admit.F90"}],
   "expected_dynsyms": ["both_admit_", "no_such_symbol_"]}
]}
EOF

echo "-- library verdicts: AND + dynsym veto"
"$BIN" --mode substitution --dag "$WORK/dag.json" --tree "$WORK/tree" \
  --units "$WORK/units.json" --library-manifest "$WORK/manifest.json" \
  --reference-dir "$WORK/ref" --out "$WORK/libs.json" 2>/dev/null
python3 - "$WORK/libs.json" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
libs = {l["name"]: l for l in d["libraries"]}
assert libs["prooflib"]["verdict"] == "admit", libs["prooflib"]
assert libs["prooflib"]["dynsym_match"] is True
assert libs["prooflib"]["mode"] == "entry"  # library verdict is always entry
assert libs["badlib"]["verdict"] == "refuse"
assert any("both_refuse" in r for r in libs["badlib"]["reason"]), libs["badlib"]
assert libs["wrongsym"]["verdict"] == "refuse"
assert any("dynsym-mismatch" in r for r in libs["wrongsym"]["reason"]), libs["wrongsym"]
assert any("no_such_symbol_" in r for r in libs["wrongsym"]["reason"])
print("library AND + dynsym veto pinned")
EOF

echo "-- manifest without --reference-dir records without comparing"
"$BIN" --dag "$WORK/dag.json" --tree "$WORK/tree" \
  --library-manifest "$WORK/manifest.json" --out "$WORK/noref.json" 2>/dev/null
python3 - "$WORK/noref.json" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
libs = {l["name"]: l for l in d["libraries"]}
assert libs["prooflib"]["verdict"] == "admit"
assert libs["prooflib"]["dynsym_match"] is None
assert libs["prooflib"]["actual_dynsyms"] is None
assert len(d["units"]) == 3  # members graded even without --units
print("unverified dynsyms recorded, not compared")
EOF

echo "-- --reference-dir without manifest rejected"
if "$BIN" --dag "$WORK/dag.json" --tree "$WORK/tree" --units "$WORK/units.json" \
    --reference-dir "$WORK/ref" --out "$WORK/orphan.json" 2>/dev/null; then
  echo "FAIL: orphan --reference-dir accepted"; exit 1
fi
echo "orphan --reference-dir rejected"

echo "T023 PROOF GREEN"
