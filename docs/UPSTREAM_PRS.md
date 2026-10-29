# Upstream PRs — the end-to-end process for offers that get merged

How rustsmith offers pure-Python speedups (and, earlier, small Rust
accelerators) back to upstream projects so that maintainers actually
merge them. Docs only: no code, no gate changes, no upstream contact
beyond the PRs themselves. Per the repo rule, per-project patch work
lives in its fork or its own repo; this file holds the generic process
plus the evidence behind each rule.

Related: [`PYTHON_SPEEDUPS.md`](PYTHON_SPEEDUPS.md) (per-package
tracker — the upstream lane owns its PR URL + status cells),
[`PATTERN_REVIEW.md`](PATTERN_REVIEW.md) (the review pass never touches
those cells), [`PYTHON_PATTERNS.md`](PYTHON_PATTERNS.md) (the mechanisms
worth offering).

## Evidence base (all states as of 2026-10-29)

Ten upstream PRs from two generations of lanes. The first generation
offered small Rust accelerators / pure-Python hot-path fixes; the
second (tier-1 lanes, report in
`/home/john/firstmate/data/rs-upstream-tier1/report.md`) offers
measured pure-Python tier-1 patches with before/after wall + peak RSS.
Every rule below cites the row that taught it.

| PR | Shape | State | Maintainer signal |
|---|---|---|---|
| [Nicoretti/crc#210](https://github.com/Nicoretti/crc/pull/210) | pure-Python fast loop (~18x) | **closed, unmerged** | "AI PR with failing CI." — 12/18 checks red |
| [Nicoretti/crc#211](https://github.com/Nicoretti/crc/pull/211) | resubmit + missing tests | **closed, unmerged** | "code is not dry, test do not use best practices and does not fit existing style" |
| [Nicoretti/crc#212](https://github.com/Nicoretti/crc/pull/212) | single-impl loop, unittest-style tests | open | no maintainer reply yet; fork-CI awaiting approval, local proof posted in a comment instead |
| [JoshData/python-email-validator#182](https://github.com/JoshData/python-email-validator/pull/182) | 4 mechanical wins, 2.97x rep | open | "But why?" — motivation questioned, not mechanics |
| [arrow-py/arrow#1398](https://github.com/arrow-py/arrow/pull/1398) | class-level memo + parser-per-locale, 1.81x pipeline | open, `REVIEW_REQUIRED` | none yet; 24/27 CI legs green, 3 reds triaged in a comment |
| [horejsek/python-fastjsonschema#233](https://github.com/horejsek/python-fastjsonschema/pull/233) | draft04-only codegen shapes, 1.48x rep | open | none yet |
| [yaml/pyyaml#976](https://github.com/yaml/pyyaml/pull/976) | optional Rust scan core | open | none — no comments, no reviews (ignored so far) |
| [dateutil/dateutil#1609](https://github.com/dateutil/dateutil/pull/1609) | tokenizer by index, py2.7-compatible | open | none — 44/45 checks, no comments |
| [executablebooks/markdown-it-py#449](https://github.com/executablebooks/markdown-it-py/pull/449) | per-line markers | open | none — 2/2 checks, no comments |
| [Kludex/python-multipart#337](https://github.com/Kludex/python-multipart/pull/337) | callback hoist, same shape as their #316 | open, 2 reviews | codspeed bot: **+30.55%** on their own benchmark |

Plus: idna tier-1 (2.60x mixed / 7.52x ascii, note in
`/home/john/firstmate/data/rs-upstream-idna/upstream-patch-note.md`)
was PR-ready but unopened at time of writing — the offer lane was still
running. And the deliberate non-offers: pyasn1 (1.17x, low priority),
mako (1.015x noise — issue-only lead), networkx tier-1 (needs an API
change; the offer was a Rust kernel port instead, ADR-066).

Two patterns in the table matter most. First, **nothing has been merged
yet** — the process below is built from closes and silences, not from
victories; follow it to avoid the known failure modes, not because it
guarantees a merge. Second, the only maintainer sentences we have are
the two crc closes and one "But why?": every one of them is about
*trust* (red CI, foreign style, unclear motive), never about the
speedup number. The process optimizes for trust.

## 1. Target selection

- **Active maintainers first.** Check the commit log and recent merge
  rate before investing lane time: PRs into repos that merge weekly get
  answers; #976 / #1609 / #449 show that even live PRs with green CI
  can sit uncommented for weeks when maintainers are busy. Prefer the
  highest-download target whose maintainers merged *something* in the
  last month.
- **Read their AI-contribution policy and disclosure requirements
  before writing a line.** None of email-validator, arrow, or
  fastjsonschema had an AI clause in CONTRIBUTING as of 2026-10-29 —
  but crc's maintainer closed #210 as "AI PR with failing CI" with no
  written policy to point at. Policies appear without notice and norms
  apply without being written: assume every diff will be read as
  AI-authored, and pre-empt it (green gates, house style, human body
  text — §§4–6). If a repo *does* require disclosure or a sign-off,
  that is a hard gate, not guidance.
- **Don't offer what they can't take.** pyasn1 (1.17x) was held as low
  priority; mako (1.015x, inside noise) was never offered; networkx T2
  needed an API change so it stayed a note (ADR-066). Below ~1.2x on
  the representative track, or anything needing an API change, is not
  an offer — record it and walk away.

## 2. Issue-first versus PR-first

Discover per repo, every time: CONTRIBUTING, PR template, code of
conduct, dev docs, CI config. No shortcut from the last repo carries
over.

- None of email-validator / arrow / fastjsonschema required an issue
  first — PRs went directly. If the docs *do* require an issue (or a
  discussion), open it and stop at needs-decision until maintainers
  respond; never open the PR to force the question.
- If the repo has a PR template, it goes first verbatim (checklist
  included), evidence after — arrow #1398 kept the template checklist
  on top. No template → free-form body (§6).
- Changelog and sign-off rules are per repo and binding:
  email-validator and fastjsonschema both expect contributor CHANGELOG
  entries under the unreleased section (verified from their git log —
  both PRs added one); arrow's changelog is maintainer-curated at
  release time, so #1398 added none. Copy what recent merged PRs did,
  not what you prefer.

## 3. Smallest reviewable diff, no behavior change, their style

The #211 close is the whole section: "code is not dry, test do not use
best practices and does not fit existing style." Concretely:

- **One concern, minimal files.** fastjsonschema #233 kept to
  `draft04.py` even though the same shapes apply to draft06/07 —
  "kept this PR to `draft04` to stay reviewable." Offer the follow-ups
  in prose, not in the diff.
- **No behavior change, ever.** Same exceptions, messages, normalized
  forms, error paths. Prove it with a differential corpus on top of
  their suite (email-validator: 468/468 identical stock-vs-patched
  including exception type and message; fastjsonschema: sha256 outcome
  checksums + error samples agree on all three tracks; arrow: 20/20
  including 5 error-path cases).
- **Match the file's existing idiom, not yours.** #212 exists because
  #211's tests weren't in the module's unittest style and the loop was
  two near-identical copies. Constraints propagate: dateutil #1609
  kept Python 2.7-compatible syntax because the codebase is; arrow
  #1398 added tests because their template and 99%-coverage gate demand
  them.
- **New tests where the repo's gates need them, not blanket coverage.**
  Arrow got cross-instance memo sharing, locale isolation, eviction,
  and factory tests (coverage gate 99% → achieved 99.93%). crc #210
  died on `fail-under=100` at 98% — five uncovered lines — so #211/#212
  added exactly the delegation-guard and bypassed-body tests. Run
  *their* coverage gate locally and close the gap before opening.

## 4. Reproduce their environment, run their gates, rebase, re-measure

Do this on current upstream main, not on the lane's pinned base — the
rebase itself finds real bugs:

- **Fresh venv per repo, installed per their docs.**
  email-validator: `test_requirements.txt` + `pip install -e .`, then
  `make`-equivalent flake8/mypy/pytest. Arrow: `requirements-tests.txt`
  + editable install, `pytest` with `--cov-fail-under=99`, `pre-commit
  run --all-files`. fastjsonschema: `.[devel]` + the
  `JSON-Schema-Test-Suite` submodule init (`make test` needs it).
- **Rebase the patch onto main HEAD and re-measure there.** Stock arm =
  pristine `git archive` of HEAD; patched arm = the fork branch; same
  frozen workload, fresh subprocesses, min-of-several walls, peak RSS
  both arms. Numbers measured against a stale base are not evidence.
- **Run their full suite on both arms.** Twice this caught real patch
  bugs, not flakes: arrow's naive class-level key broke
  `test_parse_unrecognized_token` (in-place `_BASE_INPUT_RE_MAP` mock
  vs stale cache entry) — fixed by keying the memo on a per-instance
  input-map fingerprint, no test modifications needed. fastjsonschema's
  `_keys`-line skip exposed codegen `IndentationError` on
  trivially-checkless schemas (`properties: {}`, solo
  `additionalProperties: true/{}`) — fixed by hoisting trivial returns
  above the `if`-emitting block. If their suite passes unmodified, say
  so; it is the strongest sentence in the body.

## 5. Proof format (PR body evidence section)

Every offer carries the same five items, workload-labeled, both sides.
Copy the template in §9. Rules:

- Before/after **wall time and peak RSS** on the same frozen workload,
  both sides, with machine + interpreter versions and exact
  reproduction commands. Min-of-N fresh-subprocess walls; VmHWM/`ru_maxrss`
  for RSS (note: `ru_maxrss` is constant-broken on some boxes — the tier-1
  lanes verified against VmHWM; say which you used).
- Representative track first, showcase second, near-unchanged tracks
  disclosed, not hidden (email-validator idn-1000 1.09x "idn-bound,
  near unchanged"; idna idn-4000 1.12x). A 1.00x row is a finding.
- Outcome counts identical both arms (parse counts, ok/err splits).
- Their suite result verbatim, including pre-existing failures with the
  stock-arm proof (email-validator IPv6 `::ffff:` rendering fails
  identically on unmodified main — py3.12 `ipaddress` drift; arrow's 3
  red legs triaged with master-HEAD evidence; see §7).
- No internal paths, tokens, fleet details, or Rust-port mentions. The
  bench script stays available on request ("say the word and I'll
  attach it") rather than inflating the diff.

## 6. Title, body, and the humanizer pass

Title: what + size + "no behavior change"
(`Leaner generated validation code (~1.5x on object-heavy schemas), no
behavior change`). Body: their template first, then What / Measured /
Test-proof, in that order — see the three tier-1 PR bodies
(`ev_pr_body.md`, `arrow_pr_body.md`, `fjs_pr_body.md` in the tier-1
lane scratch) as worked examples.

Then run the body text through the **humanizer skill in Claude Code**
before opening. Why: both crc closes read as reactions to
AI-sounding, process-ignoring output — "AI PR with failing CI" and a
style mismatch. The humanizer pass exists to remove the tells (stiff
diction, over-formatting, hedged throat-clearing) while keeping every
number and claim byte-identical. It edits prose only — never code,
never figures.

- **The exact skill and how a worker invokes it.** There is no skill
  named `humanizer` installed in this environment (checked 2026-10-29:
  `~/.claude/skills/` carries only `no-mistakes` + synced entries;
  installed plugins are only `rust-analyzer-lsp` from the official
  `anthropics/claude-plugins-official` marketplace, which ships no
  humanizer). The captain's "humanizer skill in Claude Code" therefore
  means a **third-party plugin/skill**, and installing any third-party
  plugin needs **captain approval first**. Proposed install once the
  captain names the marketplace and plugin:
  `claude plugin marketplace add <marketplace-repo>` then
  `claude plugin install humanizer@<marketplace>` (exact names from the
  captain — do not guess a repo). After install, the worker invokes it
  in Claude Code with the Skill tool (`humanizer`) on the draft
  title + body file, then diffs the result to confirm no number,
  filename, command, or claim changed.
- **Until it is installed:** write the body from the §9 template in a
  plain human register — short sentences, first person where the
  template allows, plain acknowledgment of anything red (the crc #212
  body is the model: "Fair catch… Happy to adjust anything — tests,
  style, or scope"). The #211→#212 tone shift plus genuine fixes is
  what a second chance looks like; no wording pass substitutes for
  green gates and house style.

## 7. Responding to review

Lane rule, unchanged from the first upstream brief: maintainer
questions come back as needs-decision with their words quoted verbatim
— never paraphrased, never answered unilaterally when they imply a
direction choice.

- **Latency:** acknowledge within one day (a short comment, even before
  the fix), fix within the lane's next run. #182's "But why?" (2026-10-29)
  is the live case: it questions *motive*, so the answer is data, not
  argument — who pays the cost (per-validation overhead on every
  `validate_email` call), the measured size, and the offer to split the
  PR (the email-validator body already pre-offers: "If (1) reads as too
  clever, (2)–(4) stand alone … happy to split the PR either way").
- **One concern per commit**, re-run their gates per commit, say what
  each commit addresses. #212's body models this: "the byte loop is now
  a single implementation … and the new tests are written in the
  unittest style the test module already uses" — naming both review
  points from #211 explicitly.
- **Never argue benchmarks without data.** If a maintainer doubts a
  number, re-measure on their terms (their workload, their box
  description) and report both arms again. If they doubt the approach,
  offer the split/narrowing first.
- **CI failures that are not ours: document politely, with evidence, in
  a comment — not in the body.** Arrow #1398's triage comment is the
  template: each red leg named, each shown failing identically on
  unmodified master HEAD (commit hash given), closing with "Happy to
  rebase/retest if you want a fresh full-green signal." Same for the
  email-validator `ipaddress` drift (fails identically on main) and crc
  #212's fork-approval wait (`action_required`, plus the local gate
  results in the comment). Never call their CI broken; show the
  stock-arm proof and offer the re-run.

## 8. Follow-up and closing the loop

- **Monitor via contributions records.** Every upstream PR gets a
  `contributions.json` entry (`fm-contributions.v1`) in its lane dir;
  re-check state, comments, reviews, and checks on a cadence — silence
  is also data (#976/#1609/#449: green, uncommented, unmerged for
  weeks). Report maintainer replies as needs-decision the same day.
- **Update [`PYTHON_SPEEDUPS.md`](PYTHON_SPEEDUPS.md) when state
  changes and nothing else does:** `being offered` → PR URL + `offered`
  when it opens → `merged` / `declined` on verdict. The upstream lane
  owns those cells; the pattern-review pass never touches them
  ([`PATTERN_REVIEW.md`](PATTERN_REVIEW.md) § "Upstream tracker").
- **A merged pure-Python change can unlock a Rust rewrite re-scope.**
  fastjsonschema is the receipt: tier-1 measured 1.48–1.63x, and the
  port then measured 2.44x rep (1.69x *over* the tier-1 baseline,
  ADR-061) — the merged-shape question and the port ceiling are
  re-computed against the new baseline. Conversely arrow and
  email-validator tier-1s confirmed no_gain (ceilings 1.78x / 1.66x <
  2x, ADR-059 / ADR-065): no rewrite follows, and the doc says so
  honestly.
- **If it stalls, say so in the tracker.** A PR ignored for a month is
  not "offered" forever — note the stall date in the lane, keep the
  fork branch, and move on. Do not bump the thread, do not close and
  re-open to game visibility.

## 9. Pre-flight checklist

Copy into the offer lane; every box answered before opening.

- [ ] Target selected on evidence: downloads, recent maintainer merges
      (merged something in the last month), no standing no-offer reason
      (saturated hot path, needs-API-change, <1.2x rep).
- [ ] AI-contribution policy + disclosure rules read today; hard gates
      (sign-off, issue-first, changelog) listed.
- [ ] Their dev env reproduced fresh (their install docs, their venv);
      no lane paths or tokens anywhere near the fork.
- [ ] Diff is minimal, behavior-identical, house-styled; new tests only
      where their gates need them; their coverage gate green locally.
- [ ] Rebased onto current upstream main; re-measured there (min-of-N
      fresh subprocesses, wall + peak RSS both arms, workload-labeled);
      differential equivalence corpus green on top of their suite.
- [ ] Their full suite + lint + typing green on the branch; any red
      shown failing identically on pristine main (hash recorded) with
      the triage ready as a comment, not in the body.
- [ ] PR body: their template first (if any), then What / Measured /
      Test-proof; title carries size + "no behavior change"; bench
      script offered on request, not inlined.
- [ ] Body run through the humanizer skill (or hand-written in a plain
      human register per §6); numbers/commands/claims diffed unchanged.
- [ ] Fork branch under our account, one branch per package; never
      touched their branches; PR opened ready-for-review, never draft.
- [ ] contributions.json entry + tracker cells (`being offered` → PR
      URL + `offered`) updated on open.

## 10. PR-body template

```md
<what changed, in which files> (~<rep speedup>x on <workload>), no behavior change

<If repo has a PR template: the template, filled, first. Then:>

## What

<files only>. <N> mechanical hot-path wins, each behavior-preserving
(<what is identical: sets, messages, forms, errors>):

1. <change> (`<symbol>`). <why it is safe in one sentence>.
2. …

<If part of the diff could read as clever: name the split offer here —
"If (1) reads as too clever, (2)–(N) stand alone — happy to split.">

## Measured

<box>, <interpreter + key dep versions>. Frozen workload: <generator/
corpus, sizes>, each arm a fresh subprocess, min-of-<N> walls, peak
RSS per arm:

| Track | Stock (main @ <sha>) | Patched | Speedup |
|---|---|---|---|
| <rep track> | <s> | <s> | <x> |
| <showcase> | <s> | <s> | <x> |
| <flat track> | <s> | <s> | <~1x, cause named> |

Peak RSS <flat/changed> (~<MB> both). Outcome counts identical both
arms (<counts>).

Reproduce (from a checkout at <sha> vs this branch):

```sh
<their install commands>
<bench command per track>
```

(Bench script with the frozen <generator/corpus> available on request —
say the word and I'll attach it.)

## Test / lint proof

Their own gates, run in a venv built exactly per <their docs>:

* `<suite command>`: **<result>** on both stock and patched; <any
  failure> fails identically on unmodified main — <cause>, unrelated.
* `<lint/typing commands>`: clean.
* Differential equivalence: <N>-case corpus (<coverage note>) dumped
  under stock main and under this branch — **<N>/<N> identical**.

<CHANGELOG entry added under "<section>" | No changelog entry: <repo's
rule, e.g. maintainer-curated at release>.> <License terms agreement if
their CONTRIBUTING requires it.>
```
