#!/bin/bash
# T017 shape A proof: the F77-external shape through the gfortran shim.
# The real fem-core unit `fem/src/Walls.F90` (four F77 global external
# subprograms: `solve_ufric`, `wall_law`, `d_wall_law`, `kewall`) grades
# green instead of refusing; the ADR-027 F77 assumed-size fixture still
# grades green, and every forbidden feature (COMMON / optional /
# allocatable / derived-type / character) still refuses.
# Self-contained except the Elmer checkout (ELMER_FORK) and the port crate
# in the port repo (ER): the driver and the corpus below are clean-room
# proof code, not upstream text.
# Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the proof code or the gate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE_DB="${STORE_DB:-/home/john/firstmate/data/rs-elmer-t017/proof/store.db}"
ELMER_FORK="${ELMER_FORK:-/home/john/runs/elmer-fork}"
ER="${ER:-/home/john/firstmate/data/rs-elmer-t017/elmer-rust}"
SRC="$ELMER_FORK/fem/src/Walls.F90"
CRATE="$ER/crates/elmer-fem-a/ports/walls"
if [ ! -f "$SRC" ]; then
  echo "SKIP: no Elmer checkout at $SRC (set ELMER_FORK)"
  exit 0
fi
if [ ! -f "$CRATE/src/lib.rs" ]; then
  echo "SKIP: no port crate at $CRATE (set ER to the elmer-rust checkout)"
  exit 0
fi
WORK="$(mktemp -d /tmp/f77ext-proof-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "== T017 shape A proof (F77-external): work=$WORK"
T0=$SECONDS

echo "-- pin toolchain + sources (calling convention is version-specific, ADR-027 §1/§3)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: proof pinned to gfortran 13.3.0, re-measure on mismatch"; exit 1 ;;
esac
RUSTC_VERSION="$(rustc +nightly --version)"
echo "   $RUSTC_VERSION"
case "$RUSTC_VERSION" in
  *"feaadeeac"*) ;;
  *) echo "FAIL: proof pinned to nightly feaadeeac"; exit 1 ;;
esac
ELMER_COMMIT="$(git -C "$ELMER_FORK" rev-parse HEAD 2>/dev/null || echo unknown)"
SRC_MD5="$(md5sum "$SRC" | cut -d' ' -f1)"
echo "   elmer_fork=$ELMER_COMMIT src_md5=$SRC_MD5"
T_PIN=$SECONDS

echo "-- gate probe (relaxed gate admits the four F77 globals of Walls.F90)"
(cd "$ROOT" && ELMER_FORK="$ELMER_FORK" cargo test -q -p rustsmith-adapters fortran_gate -- --nocapture 2>&1 | tail -3)
echo "-- negative cargo gates (forbidden features on the F77 shape still refuse)"
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_gate_refuses_forbidden_features 2>&1 | tail -2)
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_gate_refuses_later_shapes 2>&1 | tail -2)
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_shim_common_refusal_pins_message 2>&1 | tail -2)
echo "   OBSERVED refuses stand (COMMON/optional/allocatable/derived/character/internal-proc;"
echo "   module-procedure explicit-shape and non-REAL(8) shapes stay refused)"
T_GATE=$SECONDS

echo "-- ADR-027 fixture proof (F77 assumed-size array dummy travels raw)"
(cd "$ROOT" && bash "$ROOT/tests/fortran_shim_proof.sh" 2>&1 | tail -6)
T_FIX=$SECONDS

echo "-- orig build + reference (whole .o: Walls.F90 has no USE, it links standalone)"
cp "$SRC" "$WORK/Walls.F90"
cat > "$WORK/probe_walls.F90" <<'EOF'
program probe_walls
  implicit none
  double precision, external :: wall_law, d_wall_law
  double precision :: densit, viscos, dist, rough, ut
  double precision :: ufric, dfx, fx, dfxw, tk, teps, tomg
  double precision :: cd(12), cv(12), cdw(12), cr(12), cut(12)
  integer :: i

  cd  = (/ 1000.0d0, 1000.0d0,  500.0d0, 1000.0d0, 1.0d0, 1.0d0, 998.0d0, 1.0d3, 250.0d0, 1.0d3, 1.0d0, 500.0d0 /)
  cv  = (/ 1.0d-3, 1.0d-3,  1.0d-3, 1.0d-6, 1.0d-3, 1.0d-5, 1.0d-3, 2.5d-4, 1.0d-3, 1.0d-6, 1.0d-2, 5.0d-4 /)
  cdw = (/ 0.5d0,  1.0d0,   0.05d0, 0.1d0,  0.01d0, 1.0d0,  0.5d0,  0.01d0, 2.0d-1, 5.0d-3, 2.0d0, 1.0d-4 /)
  cr  = (/ 0.001d0,0.05d0, 0.01d0, 0.02d0, 0.001d0,0.01d0, 0.003d0, 0.1d0, 0.007d0, 0.05d0, 0.001d0, 0.2d0 /)
  cut = (/ 1.0d0,  0.5d0,   2.0d0,  1.0d0,  0.25d0, 5.0d0,  0.125d0, 10.0d0, 3.0d0, 7.5d0, 0.01d0, 0.75d0 /)

  do i = 1, 12
     densit = cd(i); viscos = cv(i); dist = cdw(i); rough = cr(i); ut = cut(i)
     call solve_ufric(densit, viscos, dist, rough, ut, ufric, dfx)
     call put(ufric); call put(dfx)
     fx = wall_law(ufric, ut, densit, viscos, dist, rough)
     dfxw = d_wall_law(ufric, ut, densit, viscos, dist, rough)
     call put(fx); call put(dfxw)
     call kewall(tk, teps, tomg, ut, dist, rough, viscos, densit)
     call put(tk); call put(teps); call put(tomg)
  end do
contains
  subroutine put(x)
    double precision, intent(in) :: x
    write(*,'(ES25.17E3)') x
  end subroutine put
end program probe_walls
EOF
gfortran -c "$WORK/Walls.F90" -o "$WORK/walls_orig.o"
test -f "$WORK/walls_orig.o"
gfortran -c "$WORK/probe_walls.F90" -o "$WORK/probe.o"
test -f "$WORK/probe.o"
gfortran -o "$WORK/probe_orig" "$WORK/probe.o" "$WORK/walls_orig.o"
test -f "$WORK/probe_orig"
"$WORK/probe_orig" > "$WORK/orig.out" 2> "$WORK/orig.err"
test ! -s "$WORK/orig.err" || { echo "FAIL: orig stderr nonempty"; cat "$WORK/orig.err"; exit 1; }
echo "   reference lines: $(wc -l < "$WORK/orig.out")"
T_ORIG=$SECONDS

echo "-- port build + grade (substitute mechanics: drop Fortran .o, link Rust)"
(cd "$CRATE" && CARGO_TARGET_DIR="$WORK/target" cargo +nightly build -q)
PORT_LIB="$WORK/target/debug/libwalls.a"
test -f "$PORT_LIB"
gfortran -o "$WORK/probe_ports" "$WORK/probe.o" \
  -Wl,--whole-archive "$PORT_LIB" -Wl,--no-whole-archive -lgfortran -lm
test -f "$WORK/probe_ports"
"$WORK/probe_ports" > "$WORK/ports.out" 2> "$WORK/ports.err"
test ! -s "$WORK/ports.err" || { echo "FAIL: port stderr nonempty"; cat "$WORK/ports.err"; exit 1; }
diff "$WORK/orig.out" "$WORK/ports.out"
echo "   GREEN solve_ufric + wall_law + d_wall_law + kewall: 12/12 byte-identical"
echo "   (12 cases x 7 printed values = 84 lines at 17 significant figures;"
echo "    cases 11-12 held out: low-viscosity and high-roughness regimes)"
T_PORT=$SECONDS

echo "-- refined strong-symbol gate (runbook §2: zero STRONG port-level dups)"
nm -g --defined-only "$WORK/walls_orig.o" | awk 'NF==3{print $2, $3}' | sort > "$WORK/orig.syms"
nm -g --defined-only "$PORT_LIB" | awk 'NF==3{print $2, $3}' | sort -u > "$WORK/port.syms"
for sym in solve_ufric_ wall_law_ d_wall_law_ kewall_; do
  grep -qx "T $sym" "$WORK/port.syms" || { echo "FAIL: port does not define $sym"; exit 1; }
done
echo "   port defines all four unit symbols as T"
awk '{print $2}' "$WORK/orig.syms" | sort -u > "$WORK/orig.names"
awk '{print $2}' "$WORK/port.syms" | sort -u > "$WORK/port.names"
comm -12 "$WORK/orig.names" "$WORK/port.names" > "$WORK/inter.txt"
printf 'd_wall_law_\nkewall_\nsolve_ufric_\nwall_law_\n' > "$WORK/inter.want"
diff "$WORK/inter.want" "$WORK/inter.txt"
echo "   zero STRONG port-level dups beyond the four replaced symbols"
echo "   the port therefore replaces Walls.F90.o whole (whole-file portable)"
echo "   port-only T (toolchain class, T003 precedent — one-provider rule at merge):"
awk '$1=="T" && $2 !~ /^(_ZN|_RN|anon|DW)/{print}' "$WORK/port.syms" | head -8 || true
T_SYM=$SECONDS

echo "-- machine rows to store.db (grading transaction)"
mkdir -p "$(dirname "$STORE_DB")"
python3 - "$STORE_DB" "$GFORTRAN_VERSION" "$RUSTC_VERSION" "$ELMER_COMMIT" "$SRC_MD5" \
  "$((T_PIN - T0))" "$((T_GATE - T_PIN))" "$((T_FIX - T_GATE))" \
  "$((T_ORIG - T_FIX))" "$((T_PORT - T_ORIG))" "$((T_SYM - T_PORT))" <<'EOF'
import sqlite3, sys, json, time
db, gfver, rsver, commit, md5, w_pin, w_gate, w_fix, w_orig, w_port, w_sym = sys.argv[1:12]
con = sqlite3.connect(db)
con.executescript('''
CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, repo_url TEXT NOT NULL,
 source_lang TEXT NOT NULL, status TEXT NOT NULL, stage TEXT NOT NULL,
 started_at INTEGER NOT NULL, ended_at INTEGER, halt_reason TEXT);
CREATE TABLE IF NOT EXISTS units (id TEXT PRIMARY KEY, run_id TEXT NOT NULL,
 kind TEXT NOT NULL, spec_json TEXT NOT NULL, depends_on TEXT,
 status TEXT NOT NULL, attempts INTEGER DEFAULT 0, worktree TEXT,
 commit_sha TEXT, tokens_in INTEGER DEFAULT 0, tokens_out INTEGER DEFAULT 0);
CREATE TABLE IF NOT EXISTS gate_results (id INTEGER PRIMARY KEY,
 unit_id TEXT NOT NULL, gate TEXT NOT NULL, passed INTEGER NOT NULL,
 detail_json TEXT NOT NULL, ran_at INTEGER NOT NULL);
''')
now = int(time.time())
run = 't017-f77-external-grade'
con.execute('INSERT OR REPLACE INTO runs VALUES (?,?,?,?,?,?,?,?)',
  (run, 'elmer-fork', 'fortran', 'passed', 'grade', now, now, None))
unit = 'fortran:fem/src/Walls.F90#solve_ufric'
spec = {'shape': 'F77 global external subprogram, scalar intrinsic dummies (T017 shape A)',
        'exports': ['solve_ufric_', 'wall_law_', 'd_wall_law_', 'kewall_'],
        'gfortran': gfver, 'rustc': rsver, 'elmer_commit': commit, 'src_md5': md5,
        'crate': 'crates/elmer-fem-a/ports/walls'}
con.execute('INSERT OR REPLACE INTO units VALUES (?,?,?,?,?,?,?,?,?,?,?)',
  (unit, run, 'fortran-shim', json.dumps(spec), None, 'passed', 1, None, commit, 0, 0))
gates = [
  ('check_substitutable-admit', 1, {'wall_s': int(w_gate), 'scope_units': 4,
    'refused': ['COMMON', 'optional', 'allocatable', 'derived-type', 'character',
                'internal-procedure', 'module explicit-shape', 'non-REAL8-shape']}),
  ('adr027-fixture-assumed-size', 1, {'wall_s': int(w_fix),
    'fixture': 'tests/fixtures/fortran_shim/src/scale.F', 'divergence': 0}),
  ('equivalence-corpus', 1, {'cases': 12, 'values': 84, 'divergence': 0,
    'wall_s': int(w_port), 'precision': 'ES25.17E3',
    'regimes': ['yplus<<1', 'yplus~1', 'yplus~10', 'yplus~30', 'yplus>>100',
                'heldout-low-viscosity', 'heldout-high-roughness'],
    'stderr': 'empty-both-sides'}),
  ('strong-symbol', 1, {'wall_s': int(w_sym), 'dup_beyond_replaced': 0,
    'replaced': ['solve_ufric_', 'wall_law_', 'd_wall_law_', 'kewall_'],
    'whole_file_portable': True}),
  ('negative-controls', 1, {'refused': ['f77-common', 'f77-derived', 'f77-optional',
    'f77-allocatable', 'f77-character', 'f77-internal-proc']}),
]
for g, ok, det in gates:
    con.execute('INSERT INTO gate_results (unit_id, gate, passed, detail_json, ran_at) VALUES (?,?,?,?,?)',
      (unit, g, ok, json.dumps(det), now))
con.commit()
print('   rows:', con.execute('SELECT COUNT(*) FROM gate_results WHERE unit_id=?', (unit,)).fetchone()[0], 'gates for', unit)
EOF

echo "F77-EXTERNAL PROOF GREEN (pin=$((T_PIN - T0))s gate=$((T_GATE - T_PIN))s fixture=$((T_FIX - T_GATE))s orig=$((T_ORIG - T_FIX))s port=$((T_PORT - T_ORIG))s sym=$((T_SYM - T_PORT))s)"
echo "-- timing (ADR-045): lane-fem-a has no bench_sample_unit (null at dispatch),"
echo "   so no timing sample runs for this shape; per-unit grades carry no timing claim."
