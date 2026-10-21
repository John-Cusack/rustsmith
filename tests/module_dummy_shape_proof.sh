#!/bin/bash
# T017 shape B proof: the module-dummy shape through the gfortran shim.
# The real fem-b unit `fem/src/modules/CoordinateTransform.F90#vecutils`
# (module procedures `norm2`, `normalized`, `jac`, `transform2rank` —
# explicit-shape array dummies) grades green instead of refusing; the
# file's F77 sibling `rotmsolver_init` (derived-type dummies) still
# refuses, as do every forbidden feature.
# Self-contained except the Elmer checkout (ELMER_FORK), its module files
# (FMOD_DIR), its built lib (ELMER_LIB_DIR), and the port crate in the
# port repo (ER): the driver and the corpus below are clean-room proof
# code, not upstream text.
# Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the proof code or the gate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE_DB="${STORE_DB:-/home/john/firstmate/data/rs-elmer-t017/proof/store.db}"
ELMER_FORK="${ELMER_FORK:-/home/john/runs/elmer-fork}"
FMOD_DIR="${FMOD_DIR:-/home/john/firstmate/data/rs-elmer-s6/build-pristine/fmodules}"
ELMER_LIB_DIR="${ELMER_LIB_DIR:-/home/john/firstmate/data/rs-elmer-s6/build-pristine/fem/src}"
ER="${ER:-/home/john/firstmate/data/rs-elmer-t017/elmer-rust}"
SRC="$ELMER_FORK/fem/src/modules/CoordinateTransform.F90"
CRATE="$ER/crates/elmer-fem-b/ports/vecutils"
if [ ! -f "$SRC" ]; then
  echo "SKIP: no Elmer checkout at $SRC (set ELMER_FORK)"
  exit 0
fi
if [ ! -f "$FMOD_DIR/defutils.mod" ] || [ ! -f "$ELMER_LIB_DIR/libelmersolver.so" ]; then
  echo "SKIP: no Elmer module/lib inputs (FMOD_DIR=$FMOD_DIR ELMER_LIB_DIR=$ELMER_LIB_DIR)"
  exit 0
fi
if [ ! -f "$CRATE/src/lib.rs" ]; then
  echo "SKIP: no port crate at $CRATE (set ER to the elmer-rust checkout)"
  exit 0
fi
WORK="$(mktemp -d /tmp/moddummy-proof-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "== T017 shape B proof (module-dummy): work=$WORK"
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

echo "-- gate probe (relaxed gate admits #vecutils, refuses #rotmsolver_init)"
(cd "$ROOT" && ELMER_FORK="$ELMER_FORK" cargo test -q -p rustsmith-adapters fortran_gate -- --nocapture 2>&1 | tail -3)
echo "-- negative cargo gates (forbidden features on the module-dummy shape still refuse)"
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_gate_refuses_forbidden_features 2>&1 | tail -2)
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_gate_refuses_later_shapes 2>&1 | tail -2)
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_shim_common_refusal_pins_message 2>&1 | tail -2)
echo "   OBSERVED refuses stand (COMMON/optional/allocatable/derived/character on the"
echo "   module-dummy shape; non-REAL(8) assumed-shape, bare REAL, rank 3+ and"
echo "   assumed-shape-with-bounds shapes stay refused)"
T_GATE=$SECONDS

echo "-- orig build + reference (whole .o drags the F77 sibling: link the Elmer world)"
cp "$SRC" "$WORK/CoordinateTransform.F90"
cat > "$WORK/probe_vec.F90" <<'EOF'
program probe_vec
  use vecutils
  implicit none
  real(8) :: vv(4,3), rr(3), mm(3,3), oo(3,3), nn(3,3), jj(3,3), bb(3,3)
  integer :: ic

  vv(1,:) = (/ 1.0d0, 2.0d0, 2.0d0 /)
  vv(2,:) = (/ 3.0d0, 4.0d0, 12.0d0 /)
  vv(3,:) = (/ -1.5d0, 2.25d0, 0.125d0 /)
  vv(4,:) = (/ 1.0d0, 0.0d0, 0.0d0 /)

  do ic = 1, 4
     rr = norm2(vv(ic,:))
     call put3(rr)
     rr = normalized(vv(ic,:))
     call put3(rr)
  end do

  mm = reshape((/ 1.0d0, 0.0d0, 0.0d0, 0.0d0, 1.0d0, 0.0d0, 0.0d0, 0.0d0, 1.0d0 /), (/3,3/))
  oo = mm
  nn = mm
  jj = jac(oo, nn)
  call put9(jj)
  bb = transform2rank(mm, jj)
  call put9(bb)

  mm = reshape((/ 0.0d0, -1.0d0, 0.0d0, 2.0d0, 0.5d0, 0.25d0, 0.0d0, 0.0d0, 3.0d0 /), (/3,3/))
  oo = mm
  nn = reshape((/ 1.0d0, 0.0d0, 0.0d0, 0.0d0, 1.0d0, 0.0d0, 0.0d0, 0.0d0, 1.0d0 /), (/3,3/))
  jj = jac(oo, nn)
  call put9(jj)
  bb = transform2rank(mm, jj)
  call put9(bb)

  mm = reshape((/ 1.5d0, 2.0d0, -0.5d0, 0.25d0, -3.0d0, 1.0d0, 2.5d0, 0.75d0, -1.25d0 /), (/3,3/))
  oo = mm
  nn = reshape((/ 2.0d0, 0.5d0, -1.0d0, 0.0d0, 1.5d0, 0.25d0, 0.75d0, 0.0d0, 2.25d0 /), (/3,3/))
  jj = jac(oo, nn)
  call put9(jj)
  bb = transform2rank(mm, jj)
  call put9(bb)
contains
  subroutine put3(x)
    real(8), intent(in) :: x(3)
    write(*,'(3(ES25.17E3,1X))') x
  end subroutine put3
  subroutine put9(x)
    real(8), intent(in) :: x(3,3)
    write(*,'(9(ES25.17E3,1X))') x
  end subroutine put9
end program probe_vec
EOF
gfortran -J"$WORK" -I"$FMOD_DIR" -c "$WORK/CoordinateTransform.F90" -o "$WORK/coord_orig.o"
test -f "$WORK/coord_orig.o"
gfortran -J"$WORK" -I"$FMOD_DIR" -c "$WORK/probe_vec.F90" -o "$WORK/probe.o"
test -f "$WORK/probe.o"
gfortran -o "$WORK/probe_orig" "$WORK/probe.o" "$WORK/coord_orig.o" \
  -L"$ELMER_LIB_DIR" -lelmersolver -Wl,-rpath,"$ELMER_LIB_DIR"
test -f "$WORK/probe_orig"
"$WORK/probe_orig" > "$WORK/orig.out" 2> "$WORK/orig.err"
test ! -s "$WORK/orig.err" || { echo "FAIL: orig stderr nonempty"; cat "$WORK/orig.err"; exit 1; }
echo "   reference lines: $(wc -l < "$WORK/orig.out")"
T_ORIG=$SECONDS

echo "-- port build + grade (substitute mechanics: drop Fortran .o, link Rust)"
(cd "$CRATE" && CARGO_TARGET_DIR="$WORK/target" cargo +nightly build -q)
PORT_LIB="$WORK/target/debug/libvecutils.a"
test -f "$PORT_LIB"
gfortran -o "$WORK/probe_ports" "$WORK/probe.o" \
  -Wl,--whole-archive "$PORT_LIB" -Wl,--no-whole-archive \
  -L"$ELMER_LIB_DIR" -lelmersolver -Wl,-rpath,"$ELMER_LIB_DIR" -lgfortran -lm
test -f "$WORK/probe_ports"
"$WORK/probe_ports" > "$WORK/ports.out" 2> "$WORK/ports.err"
test ! -s "$WORK/ports.err" || { echo "FAIL: port stderr nonempty"; cat "$WORK/ports.err"; exit 1; }
diff "$WORK/orig.out" "$WORK/ports.out"
echo "   GREEN norm2 + normalized + jac + transform2rank: byte-identical"
echo "   (4 vector cases x 2 kernels + 3 matrix cases x 2 kernels = 14 lines"
echo "    at 17 significant figures; the vector cases pass a rank-1 section,"
echo "    the matrix cases pass whole 3x3 arrays)"
T_PORT=$SECONDS

echo "-- refined strong-symbol gate (runbook §2: zero STRONG port-level dups)"
nm -g --defined-only "$WORK/coord_orig.o" | awk 'NF==3{print $2, $3}' | sort > "$WORK/orig.syms"
nm -g --defined-only "$PORT_LIB" | awk 'NF==3{print $2, $3}' | sort -u > "$WORK/port.syms"
for sym in __vecutils_MOD_norm2 __vecutils_MOD_normalized __vecutils_MOD_jac __vecutils_MOD_transform2rank; do
  grep -qx "T $sym" "$WORK/port.syms" || { echo "FAIL: port does not define $sym"; exit 1; }
done
echo "   port defines all four unit symbols as T"
awk '{print $2}' "$WORK/orig.syms" | sort -u > "$WORK/orig.names"
awk '{print $2}' "$WORK/port.syms" | sort -u > "$WORK/port.names"
comm -12 "$WORK/orig.names" "$WORK/port.names" > "$WORK/inter.txt"
printf '__vecutils_MOD_jac\n__vecutils_MOD_norm2\n__vecutils_MOD_normalized\n__vecutils_MOD_transform2rank\n' > "$WORK/inter.want"
diff "$WORK/inter.want" "$WORK/inter.txt"
echo "   zero STRONG port-level dups beyond the four replaced symbols"
if grep -qx "T rotmsolver_" "$WORK/port.syms"; then
  echo "FAIL: port claims the refused sibling rotmsolver_"; exit 1
fi
echo "   port claims no sibling symbol (rotmsolver_ / rotmsolver_init_ absent)"
echo "   HONEST LIMITATION: the file's two F77 globals stay refused (derived-type"
echo "   dummies), so the substitution is driver-level, not a whole-file splice."
echo "   port-only T (toolchain class, T003 precedent — one-provider rule at merge):"
awk '$1=="T" && $2 !~ /^(_ZN|_RN|anon|DW)/{print}' "$WORK/port.syms" | head -8 || true
T_SYM=$SECONDS

echo "-- machine rows to store.db (grading transaction)"
mkdir -p "$(dirname "$STORE_DB")"
python3 - "$STORE_DB" "$GFORTRAN_VERSION" "$RUSTC_VERSION" "$ELMER_COMMIT" "$SRC_MD5" \
  "$((T_PIN - T0))" "$((T_GATE - T_PIN))" "$((T_ORIG - T_GATE))" "$((T_PORT - T_ORIG))" "$((T_SYM - T_PORT))" <<'EOF'
import sqlite3, sys, json, time
db, gfver, rsver, commit, md5, w_pin, w_gate, w_orig, w_port, w_sym = sys.argv[1:11]
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
run = 't017-module-dummy-grade'
con.execute('INSERT OR REPLACE INTO runs VALUES (?,?,?,?,?,?,?,?)',
  (run, 'elmer-fork', 'fortran', 'passed', 'grade', now, now, None))
unit = 'fortran:fem/src/modules/CoordinateTransform.F90#vecutils'
spec = {'shape': 'module procedure with explicit-shape array dummies (T017 shape B)',
        'exports': ['__vecutils_MOD_norm2', '__vecutils_MOD_normalized',
                    '__vecutils_MOD_jac', '__vecutils_MOD_transform2rank'],
        'result_wire': 'array result via a gfortran array descriptor (hidden first argument)',
        'gfortran': gfver, 'rustc': rsver, 'elmer_commit': commit, 'src_md5': md5,
        'crate': 'crates/elmer-fem-b/ports/vecutils',
        'substitution': 'driver-level (the file\'s F77 siblings stay refused)'}
con.execute('INSERT OR REPLACE INTO units VALUES (?,?,?,?,?,?,?,?,?,?,?)',
  (unit, run, 'fortran-shim', json.dumps(spec), None, 'passed', 1, None, commit, 0, 0))
gates = [
  ('check_substitutable-admit', 1, {'wall_s': int(w_gate),
    'refused_sibling': 'rotmsolver_init (derived-type dummies)',
    'refused': ['COMMON', 'optional', 'allocatable', 'derived-type', 'character',
                'non-REAL8-assumed-shape', 'bare-REAL-assumed-shape', 'rank3',
                'assumed-shape-with-bounds']}),
  ('equivalence-corpus', 1, {'cases': 7, 'lines': 14, 'divergence': 0,
    'wall_s': int(w_port), 'precision': 'ES25.17E3',
    'shapes': ['rank-1 section actual', 'whole 3x3 actual',
               'identity matrix', 'non-trivial matrix'],
    'stderr': 'empty-both-sides'}),
  ('strong-symbol', 1, {'wall_s': int(w_sym), 'dup_beyond_replaced': 0,
    'replaced': ['__vecutils_MOD_norm2', '__vecutils_MOD_normalized',
                 '__vecutils_MOD_jac', '__vecutils_MOD_transform2rank'],
    'sibling_claimed': False, 'whole_file_portable': False}),
  ('negative-controls', 1, {'refused': ['module-dummy-common', 'module-dummy-optional',
    'module-dummy-allocatable', 'module-dummy-derived', 'rotmsolver_init']}),
]
for g, ok, det in gates:
    con.execute('INSERT INTO gate_results (unit_id, gate, passed, detail_json, ran_at) VALUES (?,?,?,?,?)',
      (unit, g, ok, json.dumps(det), now))
con.commit()
print('   rows:', con.execute('SELECT COUNT(*) FROM gate_results WHERE unit_id=?', (unit,)).fetchone()[0], 'gates for', unit)
EOF

echo "MODULE-DUMMY PROOF GREEN (pin=$((T_PIN - T0))s gate=$((T_GATE - T_PIN))s orig=$((T_ORIG - T_GATE))s port=$((T_PORT - T_ORIG))s sym=$((T_SYM - T_PORT))s)"
echo "-- timing (ADR-045): the lane-fem-b bench_sample_unit"
echo "   (fortran:fem/src/modules/HeatSolve.F90) stays refused (external scope with"
echo "   derived-type dummies), so no timing sample runs for this shape;"
echo "   per-unit grades carry no timing claim."
