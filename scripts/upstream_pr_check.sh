#!/usr/bin/env bash
# upstream_pr_check.sh — mechanical checks on an upstream PR draft or reply.
#
# Runs before the review agents (docs/UPSTREAM_PRS.md, "Mechanical
# check"), so they spend their time on judgment, not on counting words.
# No LLM calls; every rule is a regex or a count.
#
# Usage:
#   scripts/upstream_pr_check.sh --body <draft.md> [--title <title.txt>] \
#       [--diff <diff.patch>] [--kind pr|reply]
#
# Prints one line per finding (FAIL or WARN) and a final RESULT line.
# Exit codes: 0 no FAIL, 1 at least one FAIL, 2 usage/input error.

set -euo pipefail

usage() {
    echo "usage: $0 --body <draft.md> [--title <title.txt>] [--diff <diff.patch>] [--kind pr|reply]" >&2
    exit 2
}

BODY=""; TITLE=""; DIFF=""; KIND="pr"
while [ $# -gt 0 ]; do
    case $1 in
        --body)  [ $# -ge 2 ] || usage; BODY=$2; shift 2 ;;
        --title) [ $# -ge 2 ] || usage; TITLE=$2; shift 2 ;;
        --diff)  [ $# -ge 2 ] || usage; DIFF=$2; shift 2 ;;
        --kind)  [ $# -ge 2 ] || usage; KIND=$2; shift 2 ;;
        *) echo "unknown argument: $1" >&2; usage ;;
    esac
done
[ -n "$BODY" ] || usage
case $KIND in pr|reply) ;; *) usage ;; esac
for f in "$BODY" "$TITLE" "$DIFF"; do
    [ -z "$f" ] || [ -f "$f" ] || { echo "not a file: $f" >&2; exit 2; }
done

python3 -I - "$KIND" "$BODY" "$TITLE" "$DIFF" <<'PY'
import re
import sys

kind, body_path, title_path, diff_path = sys.argv[1:5]
fails, warns = [], []

# Words from our own tooling that must never reach an upstream thread.
INTERNAL = re.compile(
    r"\b(rustsmith|firstmate|crewmate|captain|fleet|lanes?|tier[- ]?1|oracle|"
    r"rust[- ]port|representative track|showcase track|ADR-?\d*)\b", re.I)
# Humanizer §22 chat residue and the offer habits our old bodies had.
RESIDUE = re.compile(
    r"(happy to|let me know|hope this helps|feel free|say the word|"
    r"if (it'?s|that'?s) useful|i can attach|great question)", re.I)
# Humanizer §12 stock words (warn only: weak alone).
STOCK = re.compile(
    r"\b(additionally|crucial|delve|enhance[sd]?|pivotal|robust|seamless(ly)?|"
    r"showcas\w*|leverag\w*|meticulous\w*|comprehensive|significantly)\b", re.I)
# An en dash between digits is a range (1.6–1.7x), not a connector.
DASH = re.compile("—|(?<!\\d)–|–(?!\\d)|\\s--\\s")
MULTIPLIER = re.compile(r"~?\d+(\.\d+)?x\b", re.I)


def visible_lines(text):
    """Prose lines: drop fenced code, <details> blocks, tables and inline code."""
    out, fence, details = [], False, 0
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("```") or s.startswith("~~~"):
            fence = not fence
            continue
        if fence:
            continue
        details += s.count("<details") - s.count("</details>")
        if details > 0 or "</details>" in s:
            continue
        if s.startswith("|"):
            continue
        out.append(re.sub(r"`[^`]*`", "", line))
    return out


body = open(body_path, encoding="utf-8").read()
prose = visible_lines(body)
words = sum(len(l.split()) for l in prose)
limit = 200 if kind == "pr" else 120
target = 150 if kind == "pr" else 60
if words > limit:
    fails.append(f"visible body is {words} words; limit {limit} for a {kind}")
elif words > target:
    warns.append(f"visible body is {words} words; aim under {target}")

for i, l in enumerate(prose, 1):
    for m in INTERNAL.finditer(l):
        fails.append(f"internal term {m.group(0)!r} in body: {l.strip()[:80]}")
    if DASH.search(l):
        fails.append(f"dash in prose (humanizer §8): {l.strip()[:80]}")
    for m in RESIDUE.finditer(l):
        warns.append(f"chat residue or pre-offer {m.group(0)!r} (humanizer §22): {l.strip()[:80]}")
    for m in STOCK.finditer(l):
        warns.append(f"stock word {m.group(0)!r} (humanizer §12): {l.strip()[:80]}")

headings = sum(1 for l in prose if l.lstrip().startswith("#"))
bold = sum(len(re.findall(r"\*\*[^*]+\*\*", l)) for l in prose)
tables = len(re.findall(r"^\s*\|.*\|\s*$\n^\s*\|[\s:|-]+\|\s*$", body, re.M))
if kind == "reply" and (headings or tables):
    fails.append(f"reply has {headings} heading(s) and {tables} table(s); a reply has neither")
if kind == "pr" and headings > 3:
    warns.append(f"{headings} headings; a small PR needs none unless the repo template has them")
if kind == "pr" and tables > 1:
    warns.append(f"{tables} tables in the body; keep at most one, in <details> if possible")
if bold > 1:
    warns.append(f"{bold} bold spans (humanizer §19)")

if title_path:
    title = open(title_path, encoding="utf-8").read().strip()
    if len(title) > 90:
        fails.append(f"title is {len(title)} characters; limit 90")
    elif len(title) > 72:
        warns.append(f"title is {len(title)} characters; aim for 72")
    if MULTIPLIER.search(title):
        fails.append(f"speedup multiplier in title: {title!r}")
    if re.search(r"no behaviou?r change", title, re.I):
        fails.append("'no behavior change' in title")
    if INTERNAL.search(title):
        fails.append(f"internal term in title: {title!r}")
    if DASH.search(title):
        fails.append(f"dash in title: {title!r}")

if diff_path:
    # On a reply the diff is already posted: report its problems, but they
    # are not the reply's to fail.
    diff_fails = fails if kind == "pr" else warns
    added, removed, files = 0, 0, set()
    for line in open(diff_path, encoding="utf-8", errors="replace"):
        if line.startswith("+++ b/"):
            files.add(line[6:].strip())
            continue
        if line.startswith("---") or line.startswith("+++"):
            continue
        if line.startswith("+"):
            added += 1
            text = line[1:]
            for m in INTERNAL.finditer(text):
                diff_fails.append(f"internal term {m.group(0)!r} in diff: {text.strip()[:80]}")
            if re.match(r"(import|from)\s+\S+", text):
                warns.append(f"new module-level import; measure import time and memory: {text.strip()[:80]}")
            if re.match(r"\s+global\s+\w", text):
                warns.append(f"new 'global' (module state); check documented settings still apply after the first call: {text.strip()[:80]}")
            if re.match(r"_?[A-Za-z_]\w*\s*(:[^=]+)?=\s*re\.compile", text):
                warns.append(f"new module-level regex (import-time compile): {text.strip()[:80]}")
            if re.search(r"#.*\b(fast path|hot path|profil\w*|speedup)\b", text, re.I):
                warns.append(f"comment narrates the optimization: {text.strip()[:80]}")
        elif line.startswith("-"):
            removed += 1
    print(f"INFO: diff +{added} -{removed} in {len(files)} file(s)")
    if kind == "pr" and added + removed > 0 and words > 15 * (added + removed):
        warns.append(f"{words} body words for a {added + removed}-line diff; the text is long for the change")

print(f"INFO: visible body {words} words ({kind})")
for w in warns:
    print(f"WARN: {w}")
for f in fails:
    print(f"FAIL: {f}")
print(f"RESULT: {'FAIL' if fails else 'PASS'} ({len(fails)} fail, {len(warns)} warn)")
sys.exit(1 if fails else 0)
PY
