# data/elmer-craft — measured rows for the Elmer tight-loop craft

Machine-written rows for the recraft that produced the port template
(ADR-050, `docs/PORT_CRAFT_CHECKLIST.md`).

**Source of truth: the port repo.** These files are copies of the rows the
bench wrote in `John-Cusack/elmer-rust` (PR
https://github.com/John-Cusack/elmer-rust/pull/9):

| row here | source in elmer-rust |
|---|---|
| `ludecomp-lu-n256/` | `crates/elmer-fem-a/bench/ludecomp/` |
| `ludecomp-inv-n256/` | `crates/elmer-fem-a/bench/mtrinv/` |
| `ludecomp-lu-n512/` | `crates/elmer-fem-a/bench/ludecomp-n512/` |
| `ludecomp-inv-n512/` | `crates/elmer-fem-a/bench/mtrinv-n512/` |
| `mtr-sum-n256/` | `crates/elmer-gui-c/bench/mtr-sum-n256/` |
| `mtr-sum-n1024/` | `crates/elmer-gui-c/bench/mtr-sum-n1024/` |
| `mtr-sum-n4096/` | `crates/elmer-gui-c/bench/mtr-sum-n4096/` |
| `jacobi-nogain/` | `crates/elmer-gui-c/bench/jacobi-nogain/` |

Refresh them from that repo (never by hand):

```sh
cd <elmer-rust checkout>
cp -a crates/elmer-fem-a/bench/ludecomp     <rustsmith>/data/elmer-craft/ludecomp-lu-n256
cp -a crates/elmer-fem-a/bench/mtrinv       <rustsmith>/data/elmer-craft/ludecomp-inv-n256
cp -a crates/elmer-fem-a/bench/ludecomp-n512 <rustsmith>/data/elmer-craft/ludecomp-lu-n512
cp -a crates/elmer-fem-a/bench/mtrinv-n512   <rustsmith>/data/elmer-craft/ludecomp-inv-n512
for n in 256 1024 4096; do
  cp -a "crates/elmer-gui-c/bench/mtr-sum-n$n" <rustsmith>/data/elmer-craft/
done
cp -a crates/elmer-gui-c/bench/jacobi-nogain <rustsmith>/data/elmer-craft/jacobi-nogain
```

Each directory holds the harness output verbatim: `results.json` (full
record: per-sample wall times, MAD, peak RSS samples, digest, verdict) and
`rows.md` (the board row, copied 1:1 per ADR-039).

The rows are the whole point of the craft pattern: a recraft that does
not beat the 3% noise band is **reverted** and kept only as a row — see
`jacobi-nogain/` (1.01x, no-change; the port source is unchanged).
