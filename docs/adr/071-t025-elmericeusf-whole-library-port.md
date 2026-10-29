# ADR-071: Slice-4 ElmerIceUSF whole-library port (60 entries)

Date: 2026-10-29. Scope: rs-elmer-next Slice 4 (§5.2, §8) port PR (elmer-rust).
Evidence PR: rustsmith (this repo). No gate semantics change.

## 1. What was ported

- ElmerIceUSF.so: all 17 UserFunctions files (6,110 LOC) as one Rust cdylib,
  60/60 T dynamic symbols equal to the reference library's set
  (`nm -D --defined-only`, T only). 9 MPI_Fortran B-symbols are mpif90
  static-link artifacts, absent by definition (same disposition as the
  Slice-1 bundled ports).
- Fortran BIND(C) accessors compiled against the ice-enabled build's .mod
  files; thin Rust drivers; host-associated ENTRY SAVE converted to module
  SAVE; per-entry call history preserved.

## 2. Bugs found by the staged differentials (all fixed in the port)

- `sliding.rs` (EffectivePressure + Friction_Coulomb stress paths): flat
  `sig` indexed `j*3+i` (stride 3) over a `dim*dim` buffer — OOB panic in
  2D and transposed values in 3D. Now `i+j*dim` (column-major `Sig(i,j)`).
- `contact.rs` (`SlidCoef_Contact` mask writes): passed the perm to
  `var_set_at` whose shim perms again — double-perm, mask never updated,
  grounding-line retreat fired twice per node. Now passes the node number.
- `damage.rs` (SourceDamage): same stride pattern, `sig[j*3+i]` — silent
  transpose (no panic, wrong eigenvalues). Now `i+j*3` over the source's
  (3,3) array.
- `watertransfer.rs` (`EPLToIDS`): mask early-return before `FirstTime`
  shifted the init message to the EPL solver (reference inits during IDS).
  Now initializes first, then checks the mask (reference order).
- `accessors/usf_acc.F90` (`usf_list_get_real_array`): callee-allocated
  `f` pointer not null-initialized — `ListGetRealArray`'s ASSOCIATED check
  read an undefined pointer and freed garbage (`munmap_chunk` abort in
  caffeGetViscosity). Now `=> null()` + deallocate-after-copy.

## 3. Verification

- Main proof (`proof-usf.sh`): nm 60/60; 23-test elmerice subset byte-identical
  pristine-vs-port (stdout/stderr/passflag/rc) with SAVE re-entry reruns;
  pristine arm rerun; fem quick backstop at 479/482 K=3 in the ice-on tree.
- Staged differentials (clean-room templates, shadow module dirs, no test-tree
  .sif touched):
  - P (Poisson source slot): tenpowera, tenpowera_d, sqrta, asquare,
    asquare_d + reruns + TenPowerA-vs-SQRTA liveness.
  - S1 (ISMIP clone, Normal-Vector slot for VarIn(2), shifted mesh for
    log10a): derivative_tenpowera, derivative_asquare, tenpowera_masked,
    tenpowera_d_masked, asquare_scaled, asquare_d_scaled, log10a + reruns.
  - S2 (ISMIP + key injection): freeslipshelves, calculate_haf,
    caffegetviscosity (Viscosity slot), relativetemperature,
    arrheniusfactor (bulk BodyForce slot), sourcecalc (+hydraulic dummy).
  - S3 (ComputeDevStress clone + Normal solver): effectivepressure,
    gzept_scaling, sliding_budd.
  - S4 (ISMIP + Zs/Zs Top/Zs Bottom dummies + Mesh Update slot): zsini,
    zsmzsini, zstopini, zstopmzsini, zsbottommzsinicalving,
    zstopmzsinicalving, dyini, dymdyini.
- Transitive/unit coverage: lateralfriction/shapefactorgravity bases via
  their _x/_y wrappers in the LateralFriction/ShapeFactor tests;
  __iceproperties_MOD_* via the TemperateIce/Enthalpy tests;
  glaciermeshmetricaniso is uncallable in the frozen build (sole caller is
  the frozen-off MMG C++ module; no in-build caller of the 4-arg subroutine
  interface exists — verified by call-graph grep), exported (nm) on both
  arms, hence trivially behavior-identical.
- GlaDS_SSA is parity-only (passflag/rc): pre-existing 1-ulp run-to-run
  nondeterminism (pristine-vs-pristine 0.20461363E-01 vs 41E-01;
  lane proof/glads-flake-evidence.txt). Both USFs exhaustively verified
  bit-perfect (245,931 ub + 564,560 IcePress); call counts identical.
- Kill criteria: no dummy shapes needed (all slots are real variables);
  no SaveData-style visibility recurrence (all staged values observed through
  solver stdout/SaveScalars/mesh motion; S4 proven non-vacuous by cross-entry
  liveness).

## 4. Numbers (measured)

- Main (measured 2026-10-29, wall 2140s): nm 60/60; 22/23 ice tests
  byte-identical + GlaDS_SSA parity-only; 9/9 SAVE reruns byte-identical;
  fem backstop 479/482 K=3 with port spliced.
- Staged (all byte-identical ref-vs-port + port reruns, clean methodology):
  P: tenpowera, tenpowera_d, sqrta, asquare, asquare_d + TenPowerA-vs-SQRTA
  liveness; S1: derivative_tenpowera, derivative_asquare,
  tenpowera_masked, tenpowera_d_masked, asquare_scaled, asquare_d_scaled,
  log10a; S2: freeslipshelves, calculate_haf, caffegetviscosity,
  relativetemperature, arrheniusfactor, sourcecalc; S3: effectivepressure,
  gzept_scaling, sliding_budd; S4 (non-vacuous): zsini, zsmzsini, zstopini,
  zstopmzsini, zsbottommzsinicalving, zstopmzsinicalving, dyini, dymdyini.
