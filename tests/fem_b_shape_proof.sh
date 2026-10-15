#!/bin/bash
# T014 proof: one proven non-BIND(C) shape through the gfortran shim.
# Real fem-b unit `HarmUtils` (SinSum/CosSum: scalars + assumed-shape
# REAL(8) rank 2) from fem/src/modules/HarmonicSignals.F90 grades green
# instead of refusing; sibling `source` (derived-type dummies) still
# refuses, as do later shapes and forbidden features.
# Self-contained except the Elmer checkout (ELMER_FORK), its module files
# (FMOD_DIR), and its built lib (ELMER_LIB_DIR): the driver and the Rust
# port below are clean-room proof code, not upstream text.
# Honest halts: any unexpected green is a FAIL.
# NEVER edit this script to make it pass — fix the proof code or the gate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STORE_DB="${STORE_DB:-/home/john/firstmate/data/rs-elmer-t014/proof/store.db}"
ELMER_FORK="${ELMER_FORK:-/home/john/runs/elmer-fork}"
FMOD_DIR="${FMOD_DIR:-/home/john/firstmate/data/rs-elmer-s6/build-pristine/fmodules}"
ELMER_LIB_DIR="${ELMER_LIB_DIR:-/home/john/firstmate/data/rs-elmer-s6/build-pristine/fem/src}"
SRC="$ELMER_FORK/fem/src/modules/HarmonicSignals.F90"
if [ ! -f "$SRC" ]; then
  echo "SKIP: no Elmer checkout at $SRC (set ELMER_FORK)"
  exit 0
fi
if [ ! -f "$FMOD_DIR/defutils.mod" ] || [ ! -f "$ELMER_LIB_DIR/libelmersolver.so" ]; then
  echo "SKIP: no Elmer module/lib inputs (FMOD_DIR=$FMOD_DIR ELMER_LIB_DIR=$ELMER_LIB_DIR)"
  exit 0
fi
WORK="$(mktemp -d /tmp/harm-proof-XXXXXX)"
trap 'rm -rf "$WORK"' EXIT
echo "== fem-b shape proof (T014): work=$WORK"
T0=$SECONDS

echo "-- pin toolchain + sources (layout is version-specific, ADR-027 §2)"
GFORTRAN_VERSION="$(gfortran --version | head -1)"
echo "   $GFORTRAN_VERSION"
case "$GFORTRAN_VERSION" in
  *"13.3.0"*) ;;
  *) echo "FAIL: proof pinned to gfortran 13.3.0, re-dump on mismatch"; exit 1 ;;
esac
ELMER_COMMIT="$(git -C "$ELMER_FORK" rev-parse HEAD 2>/dev/null || echo unknown)"
SRC_MD5="$(md5sum "$SRC" | cut -d' ' -f1)"
echo "   elmer_fork=$ELMER_COMMIT src_md5=$SRC_MD5"
T_PIN=$SECONDS

echo "-- gate probe on the real file (admit #harmutils, refuse #source)"
(cd "$ROOT" && ELMER_FORK="$ELMER_FORK" cargo test -q -p rustsmith-adapters real_harmutils -- --nocapture 2>&1 | tail -4)
echo "-- negative cargo gates (later shapes, forbidden features, COMMON pin)"
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_gate_refuses_later_shapes 2>&1 | tail -2)
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_gate_refuses_forbidden_features 2>&1 | tail -2)
(cd "$ROOT" && cargo test -q -p rustsmith-adapters fortran_shim_common_refusal_pins_message 2>&1 | tail -2)
echo "   OBSERVED refuses stand (explicit/assumed-size/non-REAL8/rank3/optional/allocatable/derived/character/COMMON/F77)"
T_GATE=$SECONDS

echo "-- orig build + reference (whole .o drags source_: link the Elmer world)"
cp "$SRC" "$WORK/HarmonicSignals.F90"
cat > "$WORK/probe_harm.F90" <<'EOF'
program probe_harm
  use harmutils
  implicit none
  real(8) :: f, t, ph, s
  real(8) :: amp(4,2), big(8,2), one(1,2)
  integer :: i
  amp = reshape((/ 1.0d0,2.0d0,3.0d0,4.0d0, 0.5d0,1.5d0,2.5d0,3.5d0 /), (/4,2/))
  do i = 1, 16
    big(mod(i-1,8)+1, (i-1)/8+1) = dble(i)*0.25d0 - 2.0d0
  end do
  one(1,1) = 3.0d0
  one(1,2) = -0.75d0
  f = 50.0d0; t = 0.01d0; ph = 0.3d0
  s = sinsum(f, amp, t, ph); call put(s)
  s = cossum(f, amp, t, ph); call put(s)
  s = sinsum(f, amp(2:2,:), t, ph); call put(s)
  s = sinsum(f, amp(2:4,:), t, ph); call put(s)
  s = cossum(f, amp(4:1:-2,:), t, ph); call put(s)
  s = sinsum(60.0d0, big, 0.0d0, 0.0d0); call put(s)
  s = cossum(7.5d0, big(1:7:2,:), 0.125d0, -1.0d0); call put(s)
  s = sinsum(f, one, t, ph); call put(s)
  s = cossum(f, one, t, ph); call put(s)
contains
  subroutine put(x)
    real(8), intent(in) :: x
    write(*,'(ES25.17E3)') x
  end subroutine put
end program probe_harm
EOF
gfortran -I"$FMOD_DIR" -c "$WORK/HarmonicSignals.F90" -o "$WORK/harm_orig.o"
gfortran -I"$FMOD_DIR" -c "$WORK/probe_harm.F90" -o "$WORK/probe.o"
gfortran -o "$WORK/probe_orig" "$WORK/probe.o" "$WORK/harm_orig.o" \
  -L"$ELMER_LIB_DIR" -lelmersolver -Wl,-rpath,"$ELMER_LIB_DIR"
"$WORK/probe_orig" > "$WORK/orig.out"
wc -l < "$WORK/orig.out"
T_ORIG=$SECONDS

echo "-- shim port build + grade (substitute mechanics: drop Fortran .o, link Rust)"
mkdir -p "$WORK/port/src"
cat > "$WORK/port/Cargo.toml" <<'EOF'
[package]
name = "harm_ports"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib"]
EOF
cat > "$WORK/port/src/lib.rs" <<'EOF'
//! T014 proof port: Elmer `HarmUtils` (`SinSum`/`CosSum`) through the
//! gfortran ABI shim (ADR-027). Clean-room reimplementation of two tiny
//! numerical kernels; calling convention pinned to gfortran 13.3.0 / x86-64.
//! Only the `BT_REAL` (`REAL(KIND=dp)`, 8-byte) dtype is admitted by the
//! gate; the descriptor layout below is the T003-measured one.

/// One dimension of a gfortran array descriptor: element stride (signed),
/// lower bound, upper bound.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GfDim {
    pub stride: i64,
    pub lower: i64,
    pub upper: i64,
}

/// gfortran dtype word: version 0, rank, BT_* type code, attribute.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GfDtype {
    pub version: i32,
    pub rank: u8,
    pub type_code: u8,
    pub attribute: u16,
}

/// gfortran array descriptor for `REAL(8)` rank 2: 40-byte header +
/// 24 bytes per dimension (ADR-027 §2). The callee never retains it.
#[repr(C)]
pub struct GfDesc2 {
    pub base_addr: *const f64,
    pub offset: i64,
    pub elem_size: i64,
    pub dtype: GfDtype,
    pub spare: i64,
    pub dims: [GfDim; 2],
}

const SQRT2: f64 = std::f64::consts::SQRT_2;

fn elem(d: &GfDesc2, i: i64, j: i64) -> f64 {
    let idx = d.offset + i * d.dims[0].stride + j * d.dims[1].stride;
    unsafe { *d.base_addr.offset(idx as isize) }
}

/// `SinSum`: sum over rows of `amp(i,2)*sqrt(2)*sin(amp(i,1)*w*t+phase)`.
/// Scalar dummies travel by reference; `amps` is an assumed-shape
/// descriptor (whole arrays and sections alike: base + offset + strides).
#[export_name = "__harmutils_MOD_sinsum"]
pub extern "C" fn port_sinsum(
    f: *const f64,
    amps: *const GfDesc2,
    t: *const f64,
    phase: *const f64,
) -> f64 {
    let d = unsafe { &*amps };
    debug_assert_eq!(d.elem_size, 8);
    debug_assert_eq!(d.dtype.version, 0);
    debug_assert_eq!(d.dtype.rank, 2);
    debug_assert_eq!(d.dtype.type_code, 3);
    let (f, t, phase) = unsafe { (*f, *t, *phase) };
    let w = 2.0 * std::f64::consts::PI * f;
    let mut s = 0.0;
    for i in d.dims[0].lower..=d.dims[0].upper {
        s += elem(d, i, 2) * SQRT2 * (elem(d, i, 1) * w * t + phase).sin();
    }
    s
}

/// `CosSum`: same with `cos`.
#[export_name = "__harmutils_MOD_cossum"]
pub extern "C" fn port_cossum(
    f: *const f64,
    amps: *const GfDesc2,
    t: *const f64,
    phase: *const f64,
) -> f64 {
    let d = unsafe { &*amps };
    debug_assert_eq!(d.elem_size, 8);
    debug_assert_eq!(d.dtype.version, 0);
    debug_assert_eq!(d.dtype.rank, 2);
    debug_assert_eq!(d.dtype.type_code, 3);
    let (f, t, phase) = unsafe { (*f, *t, *phase) };
    let w = 2.0 * std::f64::consts::PI * f;
    let mut s = 0.0;
    for i in d.dims[0].lower..=d.dims[0].upper {
        s += elem(d, i, 2) * SQRT2 * (elem(d, i, 1) * w * t + phase).cos();
    }
    s
}
EOF
(cd "$WORK/port" && CARGO_TARGET_DIR="$WORK/target" cargo build --offline -q 2>/dev/null || (cd "$WORK/port" && CARGO_TARGET_DIR="$WORK/target" cargo build -q))
PORT_LIB="$WORK/target/debug/libharm_ports.a"
gfortran -o "$WORK/probe_ports" "$WORK/probe.o" "$PORT_LIB" -lgfortran -lm
"$WORK/probe_ports" > "$WORK/ports.out"
diff "$WORK/orig.out" "$WORK/ports.out"
echo "   GREEN SinSum + CosSum on 9-case corpus (whole/offset/negative-stride/strided/single-row/edge): divergence 0"
T_PORT=$SECONDS

echo "-- refined strong-symbol gate (adapted single-unit, runbook §2)"
nm -g --defined-only "$WORK/harm_orig.o" | awk 'NF==3{print $2, $3}' | sort > "$WORK/orig.syms"
nm -g --defined-only "$PORT_LIB" | awk 'NF==3{print $2, $3}' | sort -u > "$WORK/port.syms"
grep -qx "T __harmutils_MOD_sinsum" "$WORK/port.syms"
grep -qx "T __harmutils_MOD_cossum" "$WORK/port.syms"
echo "   port defines both unit symbols as T"
if grep -q " source_" "$WORK/port.syms"; then
  echo "FAIL: port claims the unported sibling source_"; exit 1
fi
echo "   port claims no sibling symbol (source_ absent)"
awk '{print $2}' "$WORK/orig.syms" | sort -u > "$WORK/orig.names"
awk '{print $2}' "$WORK/port.syms" | sort -u > "$WORK/port.names"
comm -12 "$WORK/orig.names" "$WORK/port.names" > "$WORK/inter.txt"
printf '__harmutils_MOD_cossum\n__harmutils_MOD_sinsum\n' > "$WORK/inter.want"
diff "$WORK/inter.want" "$WORK/inter.txt"
echo "   zero STRONG port-level dups beyond the two replaced symbols"
echo "   port-only non-namespaced T (toolchain, same class as T003):"
awk '$1=="T" && $2 !~ /^(_ZN|_RNv|anon|DW)/{print}' "$WORK/port.syms" | head -8 || true
T_SYM=$SECONDS

echo "-- machine rows to store.db (grading transaction)"
python3 - "$STORE_DB" "$GFORTRAN_VERSION" "$ELMER_COMMIT" "$SRC_MD5" \
  "$((T_PIN - T0))" "$((T_GATE - T_PIN))" "$((T_ORIG - T_GATE))" "$((T_PORT - T_ORIG))" "$((T_SYM - T_PORT))" <<'EOF'
import sqlite3, sys, json, time
db, gfver, commit, md5, w_pin, w_gate, w_orig, w_port, w_sym = sys.argv[1:10]
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
run = 't014-harmutils-proof'
con.execute('INSERT OR REPLACE INTO runs VALUES (?,?,?,?,?,?,?,?)',
  (run, 'elmer-fork', 'fortran', 'passed', 'grade', now, now, None))
unit = 'fortran:fem/src/modules/HarmonicSignals.F90#harmutils'
spec = {'shape': 'assumed-shape REAL(8) rank 2 + scalar intrinsics',
        'exports': ['__harmutils_MOD_sinsum', '__harmutils_MOD_cossum'],
        'sibling_refused': 'source_', 'gfortran': gfver,
        'elmer_commit': commit, 'src_md5': md5}
con.execute('INSERT OR REPLACE INTO units VALUES (?,?,?,?,?,?,?,?,?,?,?)',
  (unit, run, 'fortran-shim', json.dumps(spec), None, 'passed', 1, None, commit, 0, 0))
gates = [
  ('check_substitutable-admit', 1, {'wall_s': int(w_gate), 'refused_sibling': 'source_'}),
  ('equivalence-corpus', 1, {'cases': 9, 'divergence': 0, 'wall_s': int(w_port),
    'sections': ['whole', 'offset', 'negative-stride', 'strided', 'single-row', 'edge']}),
  ('strong-symbol', 1, {'wall_s': int(w_sym),
    'dup_beyond_replaced': 0, 'sibling_claimed': False}),
  ('negative-controls', 1, {'refused': ['source_', 'explicit-shape', 'assumed-size',
    'non-REAL8-shape', 'rank3', 'optional', 'allocatable', 'derived', 'character', 'COMMON', 'F77']}),
]
for g, ok, det in gates:
    con.execute('INSERT INTO gate_results (unit_id, gate, passed, detail_json, ran_at) VALUES (?,?,?,?,?)',
      (unit, g, ok, json.dumps(det), now))
con.commit()
print('   rows:', con.execute('SELECT COUNT(*) FROM gate_results').fetchone()[0], 'gates for', unit)
EOF

echo "HARM PROOF GREEN (pin=$((T_PIN - T0))s gate=$((T_GATE - T_PIN))s orig=$((T_ORIG - T_GATE))s port=$((T_PORT - T_ORIG))s sym=$((T_SYM - T_PORT))s)"
