# ADR-046: merge gate is per-binary; one provider per final binary

Date: 2026-10-14. Lane: merge lane (first proof + `tool/elmer-merge/`).
Status: accepted (proof evidence in `/home/john/firstmate/data/rs-elmer-merge/`, tool scripts dogfooded green).

## Context

The spike (§3.1) gated 23 archives globally (zero cross-archive dups) and
prescribed "exactly one `--whole-archive` provider object per final binary".
The merge-lane 22-unit minimal-batch proof (matc, incl. `libmatc.so` + exe,
both negative controls green 16/16) hit two cases the global/one-provider
formulations do not cover.

## Decision

1. **Gate per binary link inputs, not globally.** `main` is strongly defined
   in both the matc-`main` port (Matc_bin exe) and the elmergrid-`fempre`
   port (`#[export_name = "main"]`, ElmerGrid exe) — two legitimate C mains
   for disjoint binaries that never share a link line. A global gate
   false-positives here. `strong-gate.sh` therefore takes a linkset
   (`<binary>: <archive>` lines) and asserts zero strong port-level dups
   *within* each binary's inputs; cross-binary collisions print as
   informational (`xbin`). Measured: 474 strong port symbols, 0 per-binary
   dups in each of the matc (22) and elmergrid (8) sets; exactly 1 xbin
   (`main`, partitioned as designed). `--strict-global` remains for slices
   that must be globally disjoint.
2. **One provider per final binary means the exe too.** The `matc` exe links
   `main.c.o + libmatc.so`; the `.so` hides `__rdl_alloc*` (defined only in
   12 MB `std` / 0.9 MB `alloc` members), so the provider inside the `.so`
   does not serve the exe link (observed: `undefined reference to
   __rdl_alloc*` until fixed). `main.c.o` carries its own whole-archive
   provider (`libmain.a`); `eval.c.o` carries the `.so` provider
   (`libeval.a`). `--allow-multiple-definition` absorbs the identical alloc
   dups at the exe link (dedup-safe, same toolchain).

## Consequences

- Merge tooling (`tool/elmer-merge/`: `select-members.sh`,
  `strong-gate.sh` with `(dev,ino,size,mtime)` inventory cache,
  `splice-batch.sh` with `test -f` phantom warnings, `rsp-link.sh`)
  encodes both rules; grade lanes run the gate per merge.
- Cost anchors (32-core, Debug): minimal object ~0.2 MB/unit, provider
  ~23 MB; `libmatc.so` 311 KB → 7.9 MB, `matc` 19 KB → 6.6 MB; matc relink
  0.81 s / 78 MB RSS; full-tree build 65.7 s / 301 MB RSS.
