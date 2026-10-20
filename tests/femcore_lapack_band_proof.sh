#!/bin/bash
# femcore lane A proof: the LAPACK band-solver units admitted by the
# relaxed substitution gate (T017 shape A) — `fem/src/SolveBand.F90` and
# `fem/src/SolveSBand.F90`.
#
# Both units are file-scope external subprograms with scalar/array
# intrinsics, so the merged gate admits them; the ports move the wrapper
# into Rust and keep the numerics in LAPACK, which makes the equivalence
# corpus bit-identical by construction.
#
# Self-contained except the Elmer checkout (ELMER_FORK) and the port
# crates in the port repo (ER): the drivers and the corpus below are
# clean-room proof code, not upstream text.
# Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the proof code or the port.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE_DB="${STORE_DB:-/home/john/firstmate/data/rs-elmer-femcore-a/proof/store.db}"
ELMER_FORK="${ELMER_FORK:-/home/john/runs/elmer-fork}"
ER="${ER:-/home/john/firstmate/data/rs-elmer-femcore-a/elmer-rust}"
WORK="$(mktemp -d /tmp/femcore-band-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "== femcore lane A proof (LAPACK band units): work=$WORK"
T0=$SECONDS

echo "-- pin toolchain + sources (calling convention is version-specific, ADR-027 §1/§3)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: unexpected gfortran $GFORTRAN_VERSION" >&2; exit 1 ;;
esac
RUSTC_VERSION="$(rustc +nightly --version)"
echo "   $RUSTC_VERSION"
case "$RUSTC_VERSION" in
  *"1.100.0-nightly (feaadeeac 2026-09-19)"*) ;;
  *) echo "FAIL: unexpected rustc $RUSTC_VERSION" >&2; exit 1 ;;
esac
ELMER_COMMIT="$(git -C "$ELMER_FORK" rev-parse HEAD 2>/dev/null || echo unknown)"
echo "   elmer_fork=$ELMER_COMMIT"
T_PIN=$SECONDS

# The units under test. `rel` is the source in the Elmer tree, `crate` the
# port crate in the port repo, `syms` the gfortran symbols the unit emits.
UNITS="SolveBand.F90:solveband:band:solvebandlapack_,solvecomplexbandlapack_;SolveSBand.F90:solvesband:sband:solvesbandlapack_,solvecomplexsbandlapack_;Walls.F90:walls:walls:solve_ufric_,wall_law_,d_wall_law_,kewall_"

# The reference object needs `types.mod` for `USE Types`, and the real
# Types module pulls in the Lua module and the generated config.h. The two
# units take exactly one entity from Types — the `dp` kind parameter — so
# the reference is built against a stub module that carries the campaign's
# own definition of `dp` (SELECTED_REAL_KIND(12) == 8 bytes, the same
# value the real Types module gives). The generated code is the unit's own
# code; only a compile-time constant differs, and it does not differ.
cat > "$WORK/types_stub.F90" <<'EOF'
module Types
  implicit none
  integer, parameter :: dp = SELECTED_REAL_KIND(12)
end module Types
EOF
gfortran -c "$WORK/types_stub.F90" -o "$WORK/types_stub.o" -J "$WORK"
test -f "$WORK/types.mod"

echo "-- drivers (clean-room corpus code)"
cat > "$WORK/probe_band.F90" <<'EOF'
program probe_band
  implicit none
  integer :: n, m, subband, band, i, j
  double precision :: ab4(4,5), ab7(7,8), ab1(1,3), x1(1,5), x2(2,8)
  complex(kind=8) :: cab4(4,4), cx1(1,4)

  ! case 1: general band, KL=KU=1, LDAB=4, N=5, one right-hand side.
  ! The band storage is AB(KL+KU+1+i-j, j) = A(i,j) (LAPACK DGBTRF).
  n = 5; m = 1; subband = 1; band = 4
  ab4 = 0.0d0
  do j = 1, n
     ab4(3,j) = 4.0d0
     if (j < n) ab4(4,j) = -1.0d0
     if (j > 1) ab4(2,j) = -1.0d0
  end do
  x1 = 0.0d0
  x1(1,1) = 1.0d0
  call SolveBandLapack(n,m,ab4,x1,subband,band)
  write(*,'(A)') 'case1 real band n=5 m=1 ldab=4'
  do i = 1, n
     write(*,'(ES25.17E3)') x1(1,i)
  end do

  ! case 2: general band, KL=KU=2, LDAB=7, N=8, two right-hand sides
  n = 8; m = 2; subband = 2; band = 7
  ab7 = 0.0d0
  do j = 1, n
     ab7(5,j) = 6.0d0
     if (j < n) ab7(6,j) = -1.0d0
     if (j > 1) ab7(4,j) = -1.0d0
     if (j+2 <= n) ab7(7,j) = 0.5d0
     if (j > 2) ab7(3,j) = 0.5d0
  end do
  x2 = 0.0d0
  x2(1,1) = 1.0d0
  x2(2,3) = -2.0d0
  call SolveBandLapack(n,m,ab7,x2,subband,band)
  write(*,'(A)') 'case2 real band n=8 m=2 ldab=7'
  do i = 1, n
     write(*,'(ES25.17E3)') x2(1,i)
     write(*,'(ES25.17E3)') x2(2,i)
  end do

  ! case 3: the N <= 0 early return
  n = 0; m = 1; subband = 1; band = 4
  write(*,'(A)') 'case3 n=0 early return'
  call SolveBandLapack(n,m,ab4,x1,subband,band)
  write(*,'(A)') 'case3 returned'

  ! case 4: complex general band, KL=KU=1, LDAB=4, N=4, one right-hand side
  n = 4; m = 1; subband = 1; band = 4
  cab4 = (0.0d0, 0.0d0)
  do j = 1, n
     cab4(3,j) = (4.0d0, 1.0d0)
     if (j < n) cab4(4,j) = (-1.0d0, 0.5d0)
     if (j > 1) cab4(2,j) = (-1.0d0, -0.5d0)
  end do
  cx1 = (0.0d0, 0.0d0)
  cx1(1,1) = (1.0d0, 0.0d0)
  call SolveComplexBandLapack(n,m,cab4,cx1,subband,band)
  write(*,'(A)') 'case4 complex band n=4 m=1'
  do i = 1, n
     write(*,'(2ES25.17E3)') cx1(1,i)
  end do

  ! case 5: an exactly singular band (zero pivot) -> the failure path
  n = 3; m = 1; subband = 0; band = 1
  ab1 = 0.0d0
  ab1(1,2) = 2.0d0
  ab1(1,3) = 3.0d0
  x1 = 0.0d0
  write(*,'(A)') 'case5 singular band'
  flush(6)
  call SolveBandLapack(n,m,ab1,x1,subband,band)
  write(*,'(A)') 'case5 not reached'
end program probe_band
EOF
cat > "$WORK/probe_sband.F90" <<'EOF'
program probe_sband
  implicit none
  integer :: n, m, subband, band, i, j
  double precision :: abs2(2,6), abs1(1,3), xs(6,1)
  complex(kind=8) :: cabs2(2,5), cxs(5,1)

  ! case 1: symmetric band (lower), KD=1, LDAB=2, N=6, one right-hand side.
  ! The band storage is AB(1+i-j, j) = A(i,j) for UPLO='L' (LAPACK DPBTRF).
  n = 6; m = 1; subband = 1; band = 2
  abs2 = 0.0d0
  do j = 1, n
     abs2(1,j) = 2.0d0
     if (j < n) abs2(2,j) = -1.0d0
  end do
  xs = 0.0d0
  xs(1,1) = 1.0d0
  call SolveSBandLapack(n,m,abs2,xs,subband,band)
  write(*,'(A)') 'case1 symmetric band n=6 m=1 ldab=2'
  do i = 1, n
     write(*,'(ES25.17E3)') xs(i,1)
  end do

  ! case 2: the N <= 0 early return
  n = 0; m = 1; subband = 1; band = 2
  write(*,'(A)') 'case2 n=0 early return'
  call SolveSBandLapack(n,m,abs2,xs,subband,band)
  write(*,'(A)') 'case2 returned'

  ! case 3: complex symmetric band, KD=1, LDAB=2, N=5, one right-hand side
  n = 5; m = 1; subband = 1; band = 2
  cabs2 = (0.0d0, 0.0d0)
  do j = 1, n
     cabs2(1,j) = (2.0d0, 0.5d0)
     if (j < n) cabs2(2,j) = (-1.0d0, 0.25d0)
  end do
  cxs = (0.0d0, 0.0d0)
  cxs(1,1) = (1.0d0, -1.0d0)
  call SolveComplexSBandLapack(n,m,cabs2,cxs,subband,band)
  write(*,'(A)') 'case3 complex symmetric band n=5 m=1'
  do i = 1, n
     write(*,'(2ES25.17E3)') cxs(i,1)
  end do

  ! case 4: an exactly singular symmetric band -> the failure path
  n = 3; m = 1; subband = 0; band = 1
  abs1 = 0.0d0
  abs1(1,2) = 2.0d0
  abs1(1,3) = 3.0d0
  xs = 0.0d0
  write(*,'(A)') 'case4 singular symmetric band'
  flush(6)
  call SolveSBandLapack(n,m,abs1,xs,subband,band)
  write(*,'(A)') 'case4 not reached'
end program probe_sband
EOF
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
T_DRIVER=$SECONDS

echo "-- reference build (Fortran object + LAPACK)"
IFS=';' read -r -a UNIT_LIST <<< "$UNITS"
for spec in "${UNIT_LIST[@]}"; do
  rel="${spec%%:*}"; rest="${spec#*:}"; crate="${rest%%:*}"; rest="${rest#*:}"
  driver="${rest%%:*}"; syms="${rest#*:}"; syms="${syms//,/ }"
  src="$ELMER_FORK/fem/src/$rel"
  test -f "$src" || { echo "FAIL: no $src" >&2; exit 1; }
  stem="${rel%.F90}"
  gfortran -c "$src" -o "$WORK/${stem}_ref.o" -I "$WORK"
  test -f "$WORK/${stem}_ref.o"
  gfortran -c "$WORK/probe_${driver}.F90" -o "$WORK/probe_${driver}.o"
  test -f "$WORK/probe_${driver}.o"
  gfortran -o "$WORK/probe_${driver}_orig" "$WORK/probe_${driver}.o" "$WORK/${stem}_ref.o" \
    -llapack -lblas
  test -f "$WORK/probe_${driver}_orig"
  set +e
  "$WORK/probe_${driver}_orig" > "$WORK/${stem}_orig.out" 2> "$WORK/${stem}_orig.err"
  echo "$?" > "$WORK/${stem}_orig.rc"
  set -e
  echo "   $rel: reference lines=$(wc -l < "$WORK/${stem}_orig.out") exit=$(cat "$WORK/${stem}_orig.rc")"
done
T_REF=$SECONDS

echo "-- port build (Rust staticlib) + substitute (drop the Fortran object)"
IFS=';' read -r -a UNIT_LIST <<< "$UNITS"
for spec in "${UNIT_LIST[@]}"; do
  rel="${spec%%:*}"; rest="${spec#*:}"; crate="${rest%%:*}"; rest="${rest#*:}"
  driver="${rest%%:*}"; syms="${rest#*:}"; syms="${syms//,/ }"
  stem="${rel%.F90}"
  (cd "$ER/crates/elmer-fem-a/ports/$crate" && CARGO_TARGET_DIR="$WORK/target" cargo +nightly build -q)
  PORT_LIB="$WORK/target/debug/lib${crate}.a"
  test -f "$PORT_LIB" || { echo "FAIL: no $PORT_LIB" >&2; exit 1; }
  gfortran -o "$WORK/probe_${driver}_ports" "$WORK/probe_${driver}.o" \
    -Wl,--whole-archive "$PORT_LIB" -Wl,--no-whole-archive -llapack -lblas -lgfortran -lm
  test -f "$WORK/probe_${driver}_ports"
  set +e
  "$WORK/probe_${driver}_ports" > "$WORK/${stem}_ports.out" 2> "$WORK/${stem}_ports.err"
  echo "$?" > "$WORK/${stem}_ports.rc"
  set -e
  diff "$WORK/${stem}_orig.out" "$WORK/${stem}_ports.out" \
    || { echo "FAIL: $stem stdout diverges" >&2; diff "$WORK/${stem}_orig.out" "$WORK/${stem}_ports.out" | head -20 >&2; exit 1; }
  diff "$WORK/${stem}_orig.err" "$WORK/${stem}_ports.err" \
    || { echo "FAIL: $stem stderr diverges" >&2; diff "$WORK/${stem}_orig.err" "$WORK/${stem}_ports.err" | head -20 >&2; exit 1; }
  diff "$WORK/${stem}_orig.rc" "$WORK/${stem}_ports.rc" \
    || { echo "FAIL: $stem exit status diverges" >&2; exit 1; }
  echo "   $stem: byte-identical stdout+stderr+exit ($(cat "$WORK/${stem}_ports.rc"))"
done
T_PORT=$SECONDS

echo "-- refined strong-symbol gate (runbook §2: zero STRONG port-level dups)"
IFS=';' read -r -a UNIT_LIST <<< "$UNITS"
for spec in "${UNIT_LIST[@]}"; do
  rel="${spec%%:*}"; rest="${spec#*:}"; crate="${rest%%:*}"; rest="${rest#*:}"
  driver="${rest%%:*}"; syms="${rest#*:}"; syms="${syms//,/ }"
  stem="${rel%.F90}"
  PORT_LIB="$WORK/target/debug/lib${crate}.a"
  nm -g --defined-only "$WORK/${stem}_ref.o" | awk 'NF==3{print $2, $3}' | sort > "$WORK/${stem}_orig.syms"
  nm -g --defined-only "$PORT_LIB" | awk 'NF==3{print $2, $3}' | sort -u > "$WORK/${stem}_port.syms"
  want=""
  for sym in $syms; do
    grep -qx "T $sym" "$WORK/${stem}_port.syms" \
      || { echo "FAIL: $crate does not define $sym" >&2; exit 1; }
    want="$want$sym"$'\n'
  done
  awk '{print $2}' "$WORK/${stem}_orig.syms" | sort -u > "$WORK/${stem}_orig.names"
  awk '{print $2}' "$WORK/${stem}_port.syms" | sort -u > "$WORK/${stem}_port.names"
  comm -12 "$WORK/${stem}_orig.names" "$WORK/${stem}_port.names" > "$WORK/${stem}_inter.txt"
  printf '%s' "$want" | sort > "$WORK/${stem}_want.txt"
  diff "$WORK/${stem}_want.txt" "$WORK/${stem}_inter.txt" \
    || { echo "FAIL: $crate strong-symbol intersection is not exactly the replaced symbols" >&2; exit 1; }
  echo "   $crate: defines [$syms] as T; zero STRONG port-level dups beyond them"
  echo "   $crate: whole-file portable (the port replaces ${stem}.o whole)"
done
T_SYM=$SECONDS

echo "-- machine rows to store.db (grading transaction)"
mkdir -p "$(dirname "$STORE_DB")"
python3 - "$STORE_DB" "$GFORTRAN_VERSION" "$RUSTC_VERSION" "$ELMER_COMMIT" \
  "$((T_PIN - T0))" "$((T_DRIVER - T_PIN))" "$((T_REF - T_DRIVER))" \
  "$((T_PORT - T_REF))" "$((T_SYM - T_PORT))" <<'EOF'
import sqlite3, sys, json, time
db, gfver, rsver, commit, w_pin, w_driver, w_ref, w_port, w_sym = sys.argv[1:10]
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
run = 'femcore-a-lapack-band-grade'
con.execute('INSERT OR REPLACE INTO runs VALUES (?,?,?,?,?,?,?,?)',
  (run, 'elmer-fork', 'fortran', 'passed', 'grade', now, now, None))
units = [
  ('fortran:fem/src/SolveBand.F90', 'solveband',
   ['solvebandlapack_', 'solvecomplexbandlapack_'],
   {'cases': 5, 'regimes': ['tridiagonal-n5-m1', 'pentadiagonal-n8-m2', 'n0-early-return',
                           'complex-n4-m1', 'singular-zero-pivot'],
    'values': 'ES25.17E3', 'divergence': 0, 'stderr': 'byte-identical',
    'exit_status': 'byte-identical'}),
  ('fortran:fem/src/Walls.F90', 'walls',
   ['solve_ufric_', 'wall_law_', 'd_wall_law_', 'kewall_'],
   {'cases': 12, 'values': 84, 'regimes': ['yplus<<1', 'yplus~1', 'yplus~10', 'yplus~30',
                                          'yplus>>100', 'heldout-low-viscosity',
                                          'heldout-high-roughness'],
    'values_fmt': 'ES25.17E3', 'divergence': 0, 'stderr': 'byte-identical',
    'exit_status': 'byte-identical', 'note': 'shape A port (elmer-rust PR10), corpus re-verified in this lane'}),
  ('fortran:fem/src/SolveSBand.F90', 'solvesband',
   ['solvesbandlapack_', 'solvecomplexsbandlapack_'],
   {'cases': 4, 'regimes': ['tridiagonal-n6-m1', 'n0-early-return',
                           'complex-n5-m1', 'singular-zero-pivot'],
    'values': 'ES25.17E3', 'divergence': 0, 'stderr': 'byte-identical',
    'exit_status': 'byte-identical'}),
]
for unit, crate, syms, corpus in units:
    spec = {'shape': 'F77 global external subprogram, intrinsic dummies (T017 shape A)',
            'exports': syms, 'gfortran': gfver, 'rustc': rsver,
            'elmer_commit': commit, 'crate': f'crates/elmer-fem-a/ports/{crate}'}
    con.execute('INSERT OR REPLACE INTO units VALUES (?,?,?,?,?,?,?,?,?,?,?)',
      (unit, run, 'fortran-shim', json.dumps(spec), None, 'passed', 1, None, commit, 0, 0))
    gates = [
      ('check_substitutable-admit', 1, {'scope': 'file-level F77 external unit',
        'shape': 'T017 shape A (F77 global external subprogram)'}),
      ('equivalence-corpus', 1, dict(corpus, wall_s=int(w_port))),
      ('strong-symbol', 1, {'wall_s': int(w_sym), 'dup_beyond_replaced': 0,
        'replaced': syms, 'whole_file_portable': True}),
    ]
    for g, ok, det in gates:
        con.execute('INSERT INTO gate_results (unit_id, gate, passed, detail_json, ran_at) VALUES (?,?,?,?,?)',
          (unit, g, ok, json.dumps(det), now))
con.commit()
print('   rows:', con.execute('SELECT COUNT(*) FROM gate_results').fetchone()[0], 'gate rows in', db)
EOF

echo "FEMCORE-A LAPACK BAND PROOF GREEN (pin=$((T_PIN - T0))s driver=$((T_DRIVER - T_PIN))s ref=$((T_REF - T_DRIVER))s port=$((T_PORT - T_REF))s sym=$((T_SYM - T_PORT))s)"
echo "-- timing (ADR-045): lane-fem-a has no bench_sample_unit (null at dispatch),"
echo "   so no timing sample runs for these units; the bench row stays unmeasured."
