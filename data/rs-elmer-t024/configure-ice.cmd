=== frozen configure rs-elmer-t024 (WITH_ElmerIce=TRUE reference build) ===
source: /home/john/runs/elmer-work/orig @ 9f6af2f85433fbe8ec71b85c3486ed38b88fcaf0 (pristine, git status clean)
date: 2026-10-29T02:42:03Z
env: HWLOC_COMPONENTS=-gl (required: MPI startup hangs without it, ADR-062 §7)
     CCACHE_DIR=/home/john/firstmate/data/elmer-lanes-cache
     CCACHE_BASEDIR=<lane>/scratch
     CCACHE_SLOPPINESS="file_macro include_file_mtime include_file_ctime time_macros pch_defines"
cmd:
cmake -S /home/john/runs/elmer-work/orig -B <lane>/scratch/build-ice \
  -DCMAKE_BUILD_TYPE=Debug \
  -DWITH_MPI=TRUE -DWITH_ElmerIce=TRUE \
  -DWITH_ELMERGUI=FALSE -DWITH_ELMERGUILOGGER=FALSE -DWITH_ELMERGUITESTER=FALSE \
  -DWITH_ELMERPOST=FALSE -DWITH_LUA=FALSE -DWITH_MKL=FALSE -DWITH_Mumps=FALSE \
  -DWITH_Hypre=FALSE -DWITH_Trilinos=FALSE -DWITH_FETI4I=FALSE -DWITH_Zoltan=FALSE \
  -DWITH_ROCALUTION=FALSE -DWITH_CHOLMOD=FALSE -DWITH_UMFPACK=TRUE -DWITH_CONTRIB=FALSE \
  -DWITH_OpenMP=FALSE -DWITH_NETCDF=FALSE -DWITH_MMG=FALSE -DWITH_PARMMG=FALSE \
  -DWITH_XIOS=FALSE -DWITH_PROJ=FALSE \
  -DELMERGRID_WITH_MATC=OFF \
  "-DCMAKE_EXE_LINKER_FLAGS=-Wl,--allow-multiple-definition" \
  "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,--allow-multiple-definition"
result: exit 0; ELMER_BROKEN_MPI_IN_PLACE=OFF; 66 elmerice / 29 elmerice-fast / 482 quick tests registered
