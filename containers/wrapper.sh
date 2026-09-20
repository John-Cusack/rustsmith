#!/bin/sh
# PATH wrapper: intercepts git and cargo, confines to $UNIT_WORKTREE.
# Installed first on PATH in run image. Deny + log, never prompt-text.
# Env: UNIT_WORKTREE (required), RUN_EVENTS (events.jsonl path, optional),
#      UNIT_ID, RUN_ID, RUN_BRANCH (run branch workers must never checkout).
# Invoked either as `git`/`cargo` (via symlink/shim with that basename) or as
# `wrapper.sh git ...` (shim exec form). Both are supported.
if [ "$(basename "$0")" = "wrapper.sh" ] || [ "$(basename "$0")" = "wrapper" ]; then
  prog="${1:-}"
  shift || true
else
  prog="$(basename "$0")"
fi
WT="${UNIT_WORKTREE:-}"
EV="${RUN_EVENTS:-}"
UIDN="${UNIT_ID:-unknown}"
ORIG_ARGS="$*"
deny() {
  msg="$1"
  echo "$prog: denied (outside \$UNIT_WORKTREE): $msg" >&2
  if [ -n "$EV" ]; then
    printf '{"ts":%s,"run_id":"%s","kind":"halt_trigger","detail":{"unit_id":"%s","command":"%s","reason":"%s"}}\n' \
      "$(date +%s)" "${RUN_ID:-m1}" "$UIDN" "$(printf '%s' "$prog $ORIG_ARGS" | sed 's/"/\\"/g')" "$(printf '%s' "$msg" | sed 's/"/\\"/g')" >> "$EV" 2>/dev/null || true
  fi
  exit 127
}
# No worktree set: only allow --version style probes.
if [ -z "$WT" ]; then
  case "$*" in
    --version*|version) exec "/usr/bin/$prog" "$@" ;;
    *) echo "$prog: UNIT_WORKTREE not set" >&2; exit 127 ;;
  esac
fi
case "$prog" in
  git)
    prev=""
    for a in "$@"; do
      if [ "$prev" = "-C" ]; then
        case "$a" in
          "$WT"/*|"$WT") ;;
          *) deny "git -C $a" ;;
        esac
      fi
      case "$a" in
        --git-dir=*) v="${a#--git-dir=}"; case "$v" in "$WT"/*|"$WT") ;; *) deny "$a" ;; esac ;;
        --work-tree=*) v="${a#--work-tree=}"; case "$v" in "$WT"/*|"$WT") ;; *) deny "$a" ;; esac ;;
      esac
      prev="$a"
    done
    # Run-branch checkout is always denied (halt trigger 5).
    if [ -n "${RUN_BRANCH:-}" ]; then
      saw_checkout=0
      for a in "$@"; do
        if [ "$a" = "checkout" ]; then saw_checkout=1; fi
        if [ "$saw_checkout" = "1" ] && [ "$a" = "$RUN_BRANCH" ]; then
          deny "git checkout $RUN_BRANCH"
        fi
      done
    fi
    # cwd must be inside worktree.
    case "$(pwd)" in
      "$WT"/*|"$WT") ;;
      *) deny "git cwd $(pwd) outside $WT" ;;
    esac
    # Absolute path args outside worktree are denied (allow toolchains).
    for a in "$@"; do
      case "$a" in
        /*) case "$a" in "$WT"/*|"$WT"|/usr/*|/bin/*|/tmp|/tmp/*) ;; *) deny "absolute path $a" ;; esac ;;
      esac
    done
    exec "/usr/bin/$prog" "$@"
    ;;
  cargo)
    prev=""
    for a in "$@"; do
      if [ "$prev" = "--manifest-path" ]; then
        case "$a" in "$WT"/*|"$WT") ;; *) deny "cargo --manifest-path $a" ;; esac
      fi
      case "$a" in
        --manifest-path=*) v="${a#--manifest-path=}"; case "$v" in "$WT"/*) ;; *) deny "$a" ;; esac ;;
      esac
      prev="$a"
    done
    case "$(pwd)" in
      "$WT"/*|"$WT") ;;
      *) deny "cargo cwd $(pwd) outside $WT" ;;
    esac
    exec "/usr/bin/$prog" "$@"
    ;;
  *)
    exec "/usr/bin/$prog" "$@"
    ;;
esac
