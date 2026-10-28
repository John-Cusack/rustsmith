#!/bin/bash
# femcore lane B proof: the admitted units of the claim-B re-grade —
# `fem/src/solve_real.F90` (T017 shape A: F77 global external subprograms)
# and `fem/src/mpif_stubs.F90` (T017 shape A: F77 stubs + one module
# variable).
#
# Both units are file-scope external subprograms with scalar/intrinsic
# dummies, so the merged gate admits them; the ports move the wrappers
# into Rust and keep the numerics where they were (LAPACK for solve_real,
# the degenerate single-rank communicator for mpif_stubs), which makes
# the equivalence corpus bit-identical by construction.
#
# Self-contained except the Elmer checkout (ELMER_FORK) and the port
# crates in the port repo (ER): the drivers and the corpus below are
# clean-room proof code, not upstream text.
# Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the proof code or the port.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ELMER_FORK="${ELMER_FORK:-/home/john/runs/elmer-fork}"
ER="${ER:-/home/john/firstmate/data/rs-elmer-t016/scratch/elmer-rust}"
WORK="$(mktemp -d /tmp/femcore-bandb-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "== femcore lane B proof (LAPACK dense + MPI stubs): work=$WORK"
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
# ccache (ADR-045 §cache): the campaign's shared cache, when the host has
# it. The lane compiles a handful of files, so the cache is not
# load-bearing here, but the recipe is the campaign's.
if command -v ccache >/dev/null 2>&1; then
  export CCACHE_DIR="${ELMER_LANES_CACHE:-/home/john/firstmate/data/elmer-lanes-cache}"
  export CCACHE_SLOPPINESS="file_macro include_file_mtime include_file_ctime time_macros pch_defines"
  export CCACHE_MAXSIZE="20G"
  FC="ccache gfortran"
  echo "   ccache: $(ccache --version | head -1), CCACHE_DIR=$CCACHE_DIR"
else
  FC="gfortran"
  echo "   ccache: not installed on this host; compiling with plain gfortran"
fi
T_PIN=$SECONDS

# The units under test. `rel` is the source in the Elmer tree, `crate` the
# port crate in the port repo, `syms` the gfortran T symbols the unit
# emits, `bvars` its BSS (B) symbols (empty when none).
MPI_SYMS="mpi_init_,mpi_initialized_,mpi_init_thread_,mpi_finalize_,mpi_comm_size_,mpi_comm_rank_,mpi_comm_split_,mpi_scan_,mpi_allreduce_,mpi_buffer_detach_,mpi_recv_,mpi_buffer_attach_,mpi_send_,mpi_barrier_,mpi_wait_,mpi_waitany_,mpi_bsend_,mpi_comm_free_,mpi_waitall_,mpi_comm_group_,mpi_group_incl_,mpi_comm_create_,mpi_irecv_,mpi_isend_,mpi_bcast_,mpi_allgather_,mpi_allgatherv_,mpi_alltoallv_,mpi_alltoall_,mpi_gatherv_,mpi_gather_,mpi_reduce_,mpi_get_count_,pdseupd_,pdneupd_,pzneupd_,pdsaupd_,pdnaupd_,pznaupd_"
UNITS="solve_real.F90:solvereal:real:solvelapack_,solvelapacksym_:;mpif_stubs.F90:mpistubs:mpi:$MPI_SYMS:__mpi_stub_MOD_mpi_stub_init"

echo "-- drivers (clean-room corpus code)"
cat > "$WORK/probe_real.F90" <<'EOF'
program probe_real
implicit none
integer :: n
double precision :: a(9), x(3), s(4), y(2)
! case 1: 3x3 dense, b = A*(1,2,3)
n = 3
a = (/ 2.0d0, 1.0d0, 1.0d0, 1.0d0, 3.0d0, 2.0d0, 1.0d0, 2.0d0, 4.0d0 /)
x = (/ 7.0d0, 13.0d0, 17.0d0 /)
call solvelapack(n, a, x)
print *, 'X=', x
call flush(6)
! case 2: N=0 early return (no output from the wrapper)
n = 0
call solvelapack(n, a, x)
print *, 'N0-OK'
call flush(6)
! case 3: singular zero matrix (DGETRF info print, then DGETRS)
n = 3
a = 0.0d0
x = (/ 1.0d0, 1.0d0, 1.0d0 /)
call solvelapack(n, a, x)
print *, 'SING-DONE'
call flush(6)
! case 4: symmetric 2x2 SPD, b = A*(1,2)
n = 2
s = (/ 4.0d0, 1.0d0, 1.0d0, 3.0d0 /)
y = (/ 6.0d0, 7.0d0 /)
call solvelapacksym(n, s, y)
print *, 'Y=', y
call flush(6)
! case 5: symmetric singular (DPOTRF info print, then DPOTRS)
n = 2
s = 0.0d0
y = (/ 1.0d0, 2.0d0 /)
call solvelapacksym(n, s, y)
print *, 'SSING-DONE'
call flush(6)
end program probe_real
EOF
cat > "$WORK/probe_mpi.F90" <<'EOF'
program probe_mpi
use mpi_stub
implicit none
integer :: ierr, csize, rank, newcomm, prov, req
logical :: flag
! behaviour stubs first (their effects are observable)
call mpi_init(ierr)
print *, 'INIT', ierr, mpi_stub_init
call mpi_comm_size(0, csize, ierr)
print *, 'SIZE', csize, ierr
call mpi_comm_rank(0, rank, ierr)
print *, 'RANK', rank, ierr
req = 3
call mpi_init_thread(req, prov, ierr)
print *, 'THREAD', prov, ierr
call mpi_comm_split(7, 0, 0, newcomm, ierr)
print *, 'SPLIT', newcomm, ierr
flag = .TRUE.
call mpi_initialized(flag, ierr)
print *, 'INITIALIZED', flag, ierr
! empty stubs: no observable effect, they must simply return
call mpi_scan
call mpi_allreduce
call mpi_buffer_detach
call mpi_recv
call mpi_buffer_attach
call mpi_send
call mpi_barrier
call mpi_wait
call mpi_waitany
call mpi_bsend
call mpi_comm_free
call mpi_waitall
call mpi_comm_group
call mpi_group_incl
call mpi_comm_create
call mpi_irecv
call mpi_isend
call mpi_bcast
call mpi_allgather
call mpi_allgatherv
call mpi_alltoallv
call mpi_alltoall
call mpi_gatherv
call mpi_gather
call mpi_reduce
call mpi_get_count
call pdseupd
call pdneupd
call pzneupd
call pdsaupd
call pdnaupd
call pznaupd
print *, 'EMPTIES-OK'
call mpi_finalize(ierr)
print *, 'FINAL', ierr, mpi_stub_init
end program probe_mpi
EOF
T_DRIVER=$SECONDS

echo "-- reference build (Fortran objects [+ LAPACK])"
IFS=';' read -r -a UNIT_LIST <<< "$UNITS"
for spec in "${UNIT_LIST[@]}"; do
  rel="${spec%%:*}"; rest="${spec#*:}"; crate="${rest%%:*}"; rest="${rest#*:}"
  driver="${rest%%:*}"; rest="${rest#*:}"; syms="${rest%%:*}"; bvars="${rest#*:}"
  syms="${syms//,/ }"
  src="$ELMER_FORK/fem/src/$rel"
  test -f "$src" || { echo "FAIL: no $src" >&2; exit 1; }
  stem="${rel%.F90}"
  $FC -c "$src" -o "$WORK/${stem}_ref.o" -J "$WORK"
  test -f "$WORK/${stem}_ref.o"
  $FC -c "$WORK/probe_${driver}.F90" -o "$WORK/probe_${driver}.o" -I "$WORK"
  test -f "$WORK/probe_${driver}.o"
  $FC -o "$WORK/probe_${driver}_orig" "$WORK/probe_${driver}.o" "$WORK/${stem}_ref.o" \
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
  driver="${rest%%:*}"; rest="${rest#*:}"; syms="${rest%%:*}"; bvars="${rest#*:}"
  syms="${syms//,/ }"
  stem="${rel%.F90}"
  (cd "$ER/crates/elmer-fem-a/ports/$crate" && CARGO_TARGET_DIR="$WORK/target" cargo +nightly build -q)
  PORT_LIB="$WORK/target/debug/lib${crate}.a"
  test -f "$PORT_LIB" || { echo "FAIL: no $PORT_LIB" >&2; exit 1; }
  $FC -o "$WORK/probe_${driver}_ports" "$WORK/probe_${driver}.o" \
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
  driver="${rest%%:*}"; rest="${rest#*:}"; syms="${rest%%:*}"; bvars="${rest#*:}"
  syms="${syms//,/ }"; bvars="${bvars//,/ }"
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
  for sym in $bvars; do
    grep -qx "B $sym" "$WORK/${stem}_port.syms" \
      || { echo "FAIL: $crate does not define BSS $sym" >&2; exit 1; }
    want="$want$sym"$'\n'
  done
  awk '{print $2}' "$WORK/${stem}_orig.syms" | sort -u > "$WORK/${stem}_orig.names"
  awk '{print $2}' "$WORK/${stem}_port.syms" | sort -u > "$WORK/${stem}_port.names"
  comm -12 "$WORK/${stem}_orig.names" "$WORK/${stem}_port.names" > "$WORK/${stem}_inter.txt"
  printf '%s' "$want" | sort > "$WORK/${stem}_want.txt"
  diff "$WORK/${stem}_want.txt" "$WORK/${stem}_inter.txt" \
    || { echo "FAIL: $crate strong-symbol intersection is not exactly the replaced symbols" >&2; exit 1; }
  echo "   $crate: defines [$(echo $syms $bvars | tr ' ' ',')] as T/B; zero STRONG port-level dups beyond them"
  echo "   $crate: whole-file portable (the port replaces ${stem}.o whole)"
done
T_SYM=$SECONDS

echo "FEMCORE-B PROOF GREEN (pin=$((T_PIN - T0))s driver=$((T_DRIVER - T_PIN))s ref=$((T_REF - T_DRIVER))s port=$((T_PORT - T_REF))s sym=$((T_SYM - T_PORT))s)"
echo "-- timing (ADR-045): lane-fem-a/lane-fem-b have no bench_sample_unit in this claim"
echo "   (the fem-b sample unit HeatSolve.F90 is not claim-B), so no timing sample runs;"
echo "   the bench row stays unmeasured."
