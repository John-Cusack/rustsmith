#!/bin/bash
# M1 acceptance (SPEC §15, IMPLEMENTATION_M1 §3):
# "spawn 8 concurrent OMP subprocesses in one container; confirm each is confined
#  to its worktree; confirm an attempt to run `git checkout` on the run branch
#  is blocked and logged as a halt trigger."
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
echo "== M1: work=$WORK"

echo "-- unit tests (lease, cgroup, agent)"
cargo test -q -p rustsmith-sandbox -p rustsmith-agent || (echo "FAIL: unit tests"; exit 1)

echo "-- rebuild run image with wrapper"
docker build -q -f "$ROOT/containers/run.Dockerfile" -t rustsmith-run:0.1.0 "$ROOT/containers/" > /dev/null
if ! docker run --rm rustsmith-run:0.1.0 sh -c "command -v git && command -v cargo"; then
  echo "FAIL: run image missing git/cargo"; exit 1
fi
echo "run image OK"

echo "-- init run repo (simulates one run container's repo)"
git init -q -b main "$WORK/repo"
git -C "$WORK/repo" config user.email t@t
git -C "$WORK/repo" config user.name t
echo "base" > "$WORK/repo/file.txt"
git -C "$WORK/repo" add .
git -C "$WORK/repo" commit -qm init
BEFORE_HEAD=$(git -C "$WORK/repo" rev-parse HEAD)
echo "HEAD=$BEFORE_HEAD branch=$(git -C "$WORK/repo" branch --show-current)"

echo "-- shims (wrapper first on PATH)"
mkdir -p "$WORK/shims"
printf '#!/bin/sh\nexec /bin/sh %s/containers/wrapper.sh git "$@"\n' "$ROOT" > "$WORK/shims/git"
printf '#!/bin/sh\nexec /bin/sh %s/containers/wrapper.sh cargo "$@"\n' "$ROOT" > "$WORK/shims/cargo"
chmod +x "$WORK/shims/git" "$WORK/shims/cargo"
# Env-leak scrub check: GIT_DIR/CARGO_TARGET_DIR must not bypass the wrapper.
# (Agent scrubs them; prove the bypass fails even when set.)
echo "-- m1probe: 8 concurrent confined workers + negatives"
cargo run -q -p rustsmith-cli -- m1probe --repo "$WORK/repo" --run-id m1 --units 8 --events "$WORK/events.jsonl" --shims "$WORK/shims"
echo "-- halt-trigger log"
cat "$WORK/events.jsonl"
if ! grep -q halt_trigger "$WORK/events.jsonl"; then echo "FAIL: no halt_trigger logged"; exit 1; fi
if ! grep -q "checkout" "$WORK/events.jsonl"; then echo "FAIL: checkout block not logged"; exit 1; fi
if ! grep -q "manifest-path" "$WORK/events.jsonl"; then echo "FAIL: cargo escape block not logged"; exit 1; fi
echo "-- run branch unchanged"
AFTER_HEAD=$(git -C "$WORK/repo" rev-parse HEAD)
if [ "$BEFORE_HEAD" != "$AFTER_HEAD" ]; then echo "FAIL: HEAD moved"; exit 1; fi
echo "-- worktrees clean"
if git -C "$WORK/repo" worktree list | grep -q "unit/u"; then
  echo "FAIL: worktrees not cleaned"; git -C "$WORK/repo" worktree list; exit 1
fi
echo "-- no instruction-based confinement"
if grep -rn 'prompt.*instruct.*not to\|do not run git\|never run cargo' "$ROOT/prompts" "$ROOT/containers" 2>/dev/null | grep .; then
  echo "FAIL: instruction-based confinement found"; exit 1
fi
echo "-- M0 still green (spot check: gates unit tests)"
cargo test -q -p rustsmith-gates || (echo "FAIL: M0 gates broke"; exit 1)
echo "M1 ACCEPTANCE GREEN"
