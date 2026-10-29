# Upstream PRs

How we offer speedups to upstream Python projects, and how we reply on
those threads afterwards. Every PR and every comment goes through the same
steps: a reason a real user needs it, the smallest change that delivers
it, measured costs, short text, three independent reviews, and a person
who signs off before anything is posted.

Related: [`PYTHON_SPEEDUPS.md`](PYTHON_SPEEDUPS.md) is the per-package
tracker (the offering lane owns its PR URL and status cells),
[`PYTHON_PATTERNS.md`](PYTHON_PATTERNS.md) lists the mechanisms worth
offering, and [`PATTERN_REVIEW.md`](PATTERN_REVIEW.md) never touches the
tracker's PR cells.

## What maintainers have told us

As of 2026-10-29 none of our upstream PRs has been merged. Every
maintainer reply so far has been about motive, size, or whether a person
checked the work. None was about the speedup number.

| PR | What happened |
|---|---|
| [Nicoretti/crc#210](https://github.com/Nicoretti/crc/pull/210) | Closed: "AI PR with failing CI." Their coverage gate (`fail-under=100`) fell to 98%. |
| [Nicoretti/crc#211](https://github.com/Nicoretti/crc/pull/211) | Closed: "code is not dry, test do not use best practices and does not fit existing style". |
| [JoshData/python-email-validator#182](https://github.com/JoshData/python-email-validator/pull/182) | "But why?" The body said what changed and how fast, but not who needs it. |
| [kjd/idna#286](https://github.com/kjd/idna/pull/286) | Closed: "highly complicated for what is a trivial small optimization", and the import move "causes a significant regression by forcing a significant dependency that most users of the library will not use. It looks like it was AI generated and I can't imagine it has had any legitimate human review." The maintainer merged the same idea as [kjd/idna#287](https://github.com/kjd/idna/pull/287): three added lines. |
| [arrow-py/arrow#1398](https://github.com/arrow-py/arrow/pull/1398), [horejsek/python-fastjsonschema#233](https://github.com/horejsek/python-fastjsonschema/pull/233), [yaml/pyyaml#976](https://github.com/yaml/pyyaml/pull/976), [dateutil/dateutil#1609](https://github.com/dateutil/dateutil/pull/1609), [executablebooks/markdown-it-py#449](https://github.com/executablebooks/markdown-it-py/pull/449) | Open, no maintainer reply yet. |
| [Kludex/python-multipart#337](https://github.com/Kludex/python-multipart/pull/337) | Open. Their own benchmark bot reported +30.55%. |

So the process is built against three complaints: no clear reason, too
much code and text for the gain, and no sign that a person checked it.

## The steps

1. Write the why.
2. Make the diff small.
3. Measure the gain and every cost a user could see.
4. Write the text and run the humanizer skill on it.
5. Run the mechanical check.
6. Run the review panel: three fresh agents, one packet.
7. Resolve the verdicts. Shrink or hold when they say so.
8. A person signs off and posts.

Replies and comments on upstream threads go through steps 4 to 8 as
well (see [Replies and comments](#replies-and-comments)).

The pieces live here:

| File | Used in |
|---|---|
| [`prompts/upstream_pr/writer.md`](../prompts/upstream_pr/writer.md) | Steps 1 to 4: brief for whoever prepares the PR |
| [`prompts/upstream_pr/pr_body_template.md`](../prompts/upstream_pr/pr_body_template.md) | Step 4: PR title and body |
| [`prompts/upstream_pr/reply_template.md`](../prompts/upstream_pr/reply_template.md) | Step 4: replies and comments |
| [`scripts/upstream_pr_check.sh`](../scripts/upstream_pr_check.sh) | Step 5: word counts, banned terms, dashes, title, diff warnings |
| [`scripts/upstream_pr_packet.sh`](../scripts/upstream_pr_packet.sh) | Step 6: builds the packet the reviewers see, runs the check |
| [`prompts/upstream_pr/review_maintainer.md`](../prompts/upstream_pr/review_maintainer.md) | Step 6: skeptical maintainer |
| [`prompts/upstream_pr/review_ai_tells.md`](../prompts/upstream_pr/review_ai_tells.md) | Step 6: AI tells, using the humanizer skill's patterns |
| [`prompts/upstream_pr/review_facts.md`](../prompts/upstream_pr/review_facts.md) | Step 6: facts and numbers |

## 1. The why

Before touching code, write two sentences: who calls this code, how often,
and what it costs them today. Then check them.

- Read the project's README for the use it names. email-validator says it
  is for registration and login forms: one address per request, often
  followed by a DNS check that takes milliseconds. Saving 37 µs per
  address means nothing there. A why for that project has to name a
  different caller, and one exists: pydantic's `EmailStr` calls
  `email_validator.validate_email(..., check_deliverability=False)` for
  every value it validates, so bulk imports pay it once per record.
- Name a real caller and link it: a dependent library, a framework
  integration, an issue that asks for speed.
- If the only caller you can name is our benchmark loop, there is no PR.
  Mark it held in the tracker.

The older target rules still apply. Prefer projects whose maintainers
merged something in the last month. Read CONTRIBUTING, the PR template,
and any AI or disclosure policy before writing a line; a required issue,
sign-off, or disclosure is a hard gate. Do not offer anything below about
1.2x on the workload that matches the why, or anything that needs an API
change.

## 2. The diff

Start from the smallest change that could carry the gain, and add a hunk
only when it earns its place.

- Measure each hunk on its own (leave it out, or add it alone) on the why
  workload, and keep the table in the evidence. A hunk stays if it
  carries roughly 10% or more of the total gain, or if another hunk needs
  it to be correct. idna#287 kept 89% of the time idna#286 saved on the
  mixed workload, with 3 added lines against 11. The import move in #286
  bought nothing we could measure (our own body said it "barely changes
  the numbers") and cost every user at import.
- Write it the way the maintainer would: their names, their comment
  density, no new module state unless it pays for itself. Keep our terms
  out of the code. #182 shipped a comment that began "Tier-1 (pure-Python)
  fast paths".
- If you would offer to split the PR, split it first and post the smaller
  part. An offer to split in the body tells the maintainer the PR is
  bigger than it needs to be.
- No behavior change: same return values, exceptions, and messages. Match
  the file's idiom (crc#211 failed on style). Add tests only where their
  gates need them, and run their coverage gate locally (crc#210 failed on
  it).

## 3. Measure

Reproduce their environment from their own docs in a fresh venv, rebase
onto current upstream main, and measure there. Both arms (main and the
branch) run in fresh processes, arms interleaved, min of several runs.
Use VmHWM for memory; `ru_maxrss` reads constant on some of our machines.
Time nothing while reviewers are running: they run code too, and one
fastjsonschema run made during a panel read 549 ms for master against
347 ms on a quiet box. Every number in the text comes from a run whose
script and output are in the evidence directory.

The evidence directory holds raw output, both arms, for:

- the speed number on the why workload, and the per-hunk table;
- `python -X importtime -c "import <pkg>"` and VmHWM after import,
  whenever the diff touches imports or module-level code;
- their full test suite, lint, and type checks (a failure that also
  happens on main gets the main-branch proof, with the commit);
- a differential run over generated inputs comparing return values and
  exception type and message;
- every documented runtime setting, changed after the first call, still
  taking effect.

The last item is new because #182 broke it. The PR cached the module
defaults on the first call, so `email_validator.CHECK_DELIVERABILITY =
False`, which the README tells users to set, is ignored on the PR branch
once one address has been validated. Its 468-case differential corpus
passed because no case changed a setting.

## 4. The text

Use [`pr_body_template.md`](../prompts/upstream_pr/pr_body_template.md).
In short:

- The title says what the change does, like a commit subject. No speedup
  multiplier and no "no behavior change".
- The body leads with the why, then what changed, then one number, then
  one line on tests. Under 200 visible words, and aim for about 100.
  Benchmark details, scripts, and any table go in a collapsed `<details>`
  block.
- Write to the maintainer. "The test suite passes", not "their gates are
  green".
- No offers to attach more, no pre-offered splits, no internal terms, no
  dashes, no bold.
- Their PR template goes first if they have one. Add a changelog entry
  only if recent outside PRs did.

Then run the humanizer skill on the draft (in Claude Code, the Skill tool
with `humanizer:humanizer`, embedded mode) and diff the result against
your draft. No number, name, command, or claim may change.

On AI assistance: follow the project's policy, disclose when it asks, and
never deny it when someone asks. The humanizer pass removes padding and
habits that generated text carries; it does not change how the work was
done or who is answerable for it. Step 8 is what answers "has a person
reviewed this".

## 5. Mechanical check

`scripts/upstream_pr_check.sh --body <draft> [--title <file>] [--diff
<patch>] [--kind pr|reply]` runs without any model. It fails on a visible
body over 200 words (120 for a reply), internal terms in the text or the
added lines, dashes in prose, a title over 90 characters or one carrying
a multiplier or "no behavior change", and headings or tables in a reply.
It warns on chat residue and pre-offers, stock words, bold, new
module-level imports, regexes and `global` statements, and comments that
narrate the optimization. Fix every FAIL before the panel runs. The
packet builder runs it and saves the output as `checks.txt`.

Against our own past PRs: idna#286 fails four ways (656 visible words for
a 13-line diff, title length, multiplier and "no behavior change" in the
title) and email-validator#182 fails twelve. kjd's #287 passes.

## 6. Review panel

Build the packet:

```sh
scripts/upstream_pr_packet.sh --repo <upstream checkout> --base <sha> \
    --head <branch> --evidence <dir> --title <title.txt> --body <draft.md> \
    --out <packet dir>
```

For a reply, add `--thread <thread.md>` with the conversation so far, and
drop `--title`. The packet holds the diff, the raw evidence, the draft,
the thread, the check output, and a `MANIFEST.md` with an id hashed from
all of them. It does not hold the writer's notes, plans, earlier drafts,
or earlier verdicts, so the reviewers judge only what the maintainer
will see.

Run three reviewers in parallel, each a fresh agent. In Claude Code that
is the Agent tool with a general-purpose agent, never a fork: a fork
inherits the writer's context. The whole prompt is: "Read your role
instructions in `prompts/upstream_pr/review_<role>.md` and follow them.
Packet: `<dir>`." Add the read-only and no-posting constraints, and
nothing else. No summary of the change and no hint of what worries you.

| Reviewer | Asks | Verdicts |
|---|---|---|
| Maintainer | Would I merge this? Is there a why in the first two sentences? Is it too big for the gain? What cost is unmeasured? | PASS, SHRINK, HOLD, FAIL |
| AI tells | Where would a maintainer first think "generated"? Uses the humanizer skill's numbered patterns plus PR-specific ones (length, method narration, title taglines, pre-offers, our jargon in code comments). | PASS, FAIL |
| Facts | Does every number, claim, and "no behavior change" trace to evidence or a public source? Are derived numbers right? Is any required cost measurement missing? For a reply, does the live thread still match? | PASS, FAIL |

Each returns a fixed block with the packet id, the verdict, and numbered
findings. Save them as `reviews/<role>.txt` in the packet.

## 7. Resolving verdicts

- Post only when all three say PASS on the same packet id.
- The strictest verdict wins: HOLD, then SHRINK, then FAIL, then PASS.
- Any edit to the diff or the text makes a new packet, and all three
  reviewers run again on it. A rewrite for tone can break a fact, and a
  fact fix can add a tell.
- Facts FAIL: correct the claim, delete it, or measure what is missing.
  It is never argued.
- AI-tells FAIL: the writer rewrites the flagged parts (the reviewer only
  flags), then runs the humanizer again.
- SHRINK: cut what the maintainer reviewer named, re-measure, rebuild.
  If the smaller version falls under the gain bar, it becomes HOLD.
- HOLD: do not post. Record the reason in the tracker. If the only gap is
  the why and the project welcomes questions, the person signing off may
  open a short issue asking whether they would want it.
- When reviewers pull in different directions (the maintainer wants a
  sentence of context, the AI-tells reviewer wants it shorter), the
  shorter text wins and the extra goes into `<details>` or is dropped.
  Numbers always follow the facts reviewer.
- The writer may contest one finding per round, in writing, citing the
  packet. A new agent of the same role gets the packet, the finding, and
  the rebuttal, and rules UPHOLD or DROP; that ruling stands. A facts
  finding can only be answered with evidence.
- Three rounds at most. If the packet still has no unanimous PASS, it is
  held and goes to a person with the packet and the last verdicts.

## 8. Sign-off and posting

A person reads the final diff, the text, and the three verdicts, and
posts only what they would defend line by line in the thread. This is the
review kjd said was missing. Before posting, re-fetch the thread: if
anything changed since the packet was built, rebuild it and review again.

Post from a fork branch under our account, one branch per package, ready
for review, not as a draft. Update the tracker cells (`being offered`,
then the PR URL and `offered`) and the lane's contributions record.

## Replies and comments

Every comment we post on an upstream PR or issue goes through steps 4
to 8: answering a question, replying to a review, closing our own PR, a
CI triage note, a follow-up. Use
[`reply_template.md`](../prompts/upstream_pr/reply_template.md) and build
the packet with `--thread`. The same three reviewers run on it; the
maintainer reviewer asks whether the first sentence answers what was
asked, and the facts reviewer checks the live thread.

Rules for replies:

- The first sentence answers the question or says what you are doing.
- Under 120 words, and most should be under 60. No headings, no tables.
- Every number is already in the thread or in the evidence.
- Offer only a version you have measured, with its number.
- When the maintainer found a real problem, say so once and fix it.
- When the maintainer has already done it themselves, thank them in a
  clause and stop.

The older reply rules still hold. A maintainer's comment goes to the
person signing off the same day, quoted word for word, before anyone
drafts an answer. Acknowledge within a day. Push one concern per commit
and say what each commit addresses. Never argue a benchmark without new
data; re-measure on their terms instead. A CI failure that also happens
on main goes in a comment with the main-branch proof (arrow#1398 has the
model), never in the body.

### First use: the two pending replies

Both drafts below went through the full pipeline on 2026-10-29. Neither
has been posted.

idna#286, draft as written:

> Thanks for looking at it, and for #287, which gets the same win in far
> less code. You're right about the import move too. I didn't measure
> import time, and it costs everyone who never touches uts46. Closing this
> in favour of #287.

Round 1: maintainer PASS, AI tells PASS, facts FAIL. #287 recovers 89%
of the time #286 saved on the mixed workload and 95% on plain ASCII, so
"the same win" overstates it. The facts reviewer also checked the live
thread: kjd had closed #286 and merged #287 a few minutes after our
snapshot, so "Closing this" was stale.

Round 2, after re-fetching the thread: all three PASS. Final text:

> Thanks for #287, which gets most of the win in far less code. You're
> right about the import move too. I didn't measure import time, and it
> costs everyone who never touches uts46.

The maintainer reviewer advised against adding the measured cost (about
2 ms and 1 MiB per import), because numbers would read as arguing with
"significant", and against offering the `ulabel()` hunk as a follow-up.

email-validator#182, draft as written:

> Fair question. The main reason is that pydantic's EmailStr calls
> validate_email for every value, so it runs on every request or record
> with an email field, for example API servers and bulk imports. In the
> mixed benchmark above it takes about 55 µs per address today, so a
> million addresses is roughly a minute of CPU. With this change it's
> about 18 seconds. Nobody filed an issue asking for this, so if the
> extra regex code isn't worth it to you, that's fine. Changes 2-4 are
> the simplest part, and I can cut the PR down to just those.

The pydantic claim checks out: `EmailStr` calls
`pydantic.networks.validate_email`, which calls
`email_validator.validate_email(email, check_deliverability=False)`, the
same setting as the benchmark. The rest did not survive. We measured each
change on its own for this review:

| Version | Lines | Time per address (mixed-2000) |
|---|---|---|
| main | | 56 µs |
| change 1 only (skip the IDNA round trip for plain ASCII domains) | +9 | 35 µs (1.63x) |
| changes 2 to 4 | +73 -27 | 38 µs (1.47x) |
| changes 2 to 4 without the module-level caches | +60 -23 | 40 µs (1.41x) |
| whole PR | +83 -27 | 19 µs (2.93x) |

Round 1: all three FAIL. The offer kept the wrong part: changes 2 to 4
are the regex code the sentence offers to drop, carry less of the gain
than the 9-line change 1, and include the defaults cache that breaks
documented settings. The facts reviewer found that reassigning
`email_validator.SPECIAL_USE_DOMAIN_NAMES` after the first call is
ignored too, and that the new regexes add about 11 ms to `import
email_validator` (23.1 ms to 33.9 ms median, mostly compiling the
character classes in `syntax.py`; change 1 alone adds nothing). The
18-second figure describes code that cannot ship. "Fair question." and
the closing offer are chat habits, and the reply ran to 99 words. API
servers were the wrong example: 37 µs per request does not show there.

Round 2 led with pydantic, disclosed the bug, and offered change 1 with
its own number. Maintainer PASS and AI tells PASS. Facts FAIL: "56 µs per
address" was stated as a general cost when it holds for one synthetic
workload, the reply offered the cut without saying the title and
changelog ("~3x", "no behavior change") would be corrected, and the
evidence had no test, differential, or memory result for the change-1-only
version. We ran them: the test suite gives the same result on main and on
change 1 alone, 10,044 differential cases are identical, and memory after
import is flat.

Round 3, with that evidence in the packet: all three PASS. Final text:

> pydantic's `EmailStr` calls `validate_email()` for each address it
> validates, so bulk imports pay it once per record: about 56 µs per
> address on our mixed benchmark, or a minute of CPU per million.
>
> This PR has a bug: the cached defaults ignore settings such as
> `email_validator.CHECK_DELIVERABILITY` when they change after the first
> call. I'll cut it down to the 9-line change that skips the IDNA round
> trip for plain ASCII domains, which on its own brings the benchmark to
> about 35 µs per address, and correct the title and changelog to match.

It cannot be posted as is: it promises a change, so the cut-down branch
(change 1 only, its comment shortened to one line, title and changelog
corrected) goes through the PR pipeline first. The reviewers' minor notes
for whoever signs off: say "my" benchmark rather than "our", consider
stating the saving (about 20 µs per address) instead of today's cost, and
link the pydantic source line. Any of those edits means another packet
and another review.

The cut-down branch was pushed on 2026-10-29, so this draft is
superseded: the reply that replaces it, and the review rounds behind it,
are in [the three open PRs through the panel](#retrospective-the-three-open-prs-through-the-panel).

## Follow-up

- Re-check every open PR on a cadence: state, comments, reviews, checks.
  Record a lack of replies too: pyyaml#976, dateutil#1609 and
  markdown-it-py#449 have been green and uncommented for weeks.
- Update [`PYTHON_SPEEDUPS.md`](PYTHON_SPEEDUPS.md) when state changes:
  `being offered`, then PR URL and `offered`, then `merged` or `declined`.
  Add `held` with the reason when the panel holds an offer.
- A PR ignored for a month gets its stall date noted in the tracker. Do
  not bump the thread, and do not close and reopen it for visibility.
- A merged pure-Python change can change the case for a Rust port; redo
  the port's ceiling against the new baseline (fastjsonschema went from
  1.48x pure Python to a 2.44x port, ADR-061).

## Retrospective: idna#286 and email-validator#182

What each reviewer would have said if the panel had existed, and what the
PR should have been. Numbers below are from re-running on 2026-10-29: a
shared Ryzen 9 5950X, CPython 3.12.3, the benchmark script from the #286
body, and `python -X importtime` over 41 interleaved runs.

### kjd/idna#286

| | master | #286 | #287 |
|---|---|---|---|
| Lines changed | | +11 -2 | +3 -0 |
| mixed-8000 encode+decode | 0.228 s | 0.096 s | 0.110 s |
| ascii-16000 | 0.416 s | 0.087 s | 0.102 s |
| `import idna`, median | 10.0 ms | 11.8 ms | 11.0 ms |
| VmHWM after import | 11.5 MiB | 12.5 MiB | 11.7 MiB |
| `uts46data` loaded by `import idna` | no | yes | no |

Maintainer reviewer: SHRINK. The import move has no measured gain of its
own (the body says so) and loads `uts46data`, a 237 KB module, into every
process that imports idna, including the many that never use UTS 46
remapping. The `ulabel()` shortcut is a second branch for about 10% of
the gain. The paragraphs on how it fits with #279 add length and decide
nothing for the maintainer. Smallest version: the `check_label()` early return, which is
what kjd wrote.

AI-tells reviewer: FAIL. 656 visible words (1,514 with the script) for a
13-line diff. Title tagline "(~2.3x on ordinary hostnames), no behavior
change". A numbered list that explains every hunk; method narration ("best
of 7 passes; each cell's figure is the best of 3 such runs"); two tables;
"It goes with #279 rather than competing with it" (humanizer §1); "I can
attach that script too if it's useful" (§22). Each one is minor alone.
Together they read as generated, and length was the first thing kjd
named.

Facts reviewer: FAIL. The diff adds a module-level import and the
evidence has no import-time or import-memory measurement for either arm.
"Barely changes the numbers" has no per-hunk table behind it.

What it should have been: kjd's #287 diff, with a body like this (numbers
from the table above):

> Title: Skip full validation for plain lowercase LDH labels in
> check_label
>
> Most labels that reach `check_label()` are plain lowercase LDH
> (`mail`, `example`, `com`), and those pass every check after the empty
> test. This returns early for them; `xn--` labels and anything else take
> the full path as before. encode+decode over 8,000 mixed hostnames goes
> from 0.228 s to 0.110 s. The test suite passes.

That is 53 words. It still lacks a named caller, which the maintainer
reviewer would push on; for idna the honest answer is callers like
email-validator that run every domain through `idna.encode()` and
`idna.decode()`.

### JoshData/python-email-validator#182

Maintainer reviewer: HOLD. The README names registration forms as the
use, where 37 µs per address is invisible, and the body names no other
caller. Four changes, +84 -27 across three files, nine new module-level
regexes, two module-level caches, and no per-change numbers, so nobody
could say which change carried the gain. The defaults cache breaks
`CHECK_DELIVERABILITY` and the other documented settings once the first
address has been validated, and a reassigned `SPECIAL_USE_DOMAIN_NAMES`
is ignored the same way. The pre-offered split ("If (1) reads as too
clever") says the PR is too big.

AI-tells reviewer: FAIL. Title tagline "(~3x mixed, ~5x valid-ASCII), no
behavior change", repeated as the first line of the body. What / Measured
/ Test proof headings and a table for a small change. Bold test counts.
"Their own gates", which addresses the maintainer in the third person.
"Say the word and I'll attach it". Seven dashes. "Tier-1" in a code
comment, and four comments that start "Fast path:".

Facts reviewer: FAIL. "No behavior change" is false (the settings test
above). The changelog line says "typical ASCII email addresses" about a
seeded synthetic generator. Import time was not measured although the
diff adds nine regexes compiled at import. Measured now, they add about
11 ms to `import email_validator`, which is more than the whole PR saves
for any process that validates fewer than about 300 addresses. That
covers the registration-form use the README names.

What it should have been: change 1 alone, led by the pydantic why.

> Title: Skip the IDNA round trip for plain ASCII domains
>
> pydantic's `EmailStr` calls `validate_email()` (with
> `check_deliverability=False`) for every value it validates, so bulk
> imports run it once per record. For a domain that is plain ASCII with no
> `xn--` label, the `idna.decode()`/`idna.encode()` round trip at the end
> of `validate_email_domain_name()` only lowercases it. This returns the
> lowercased domain directly in that case. On 2,000 generated addresses it
> takes validation from 56 µs to 35 µs per address.

The test and differential sentence would need a fresh run on that diff;
we have not run it, so it is not written here.

That body is wrong in one place, and our own review above missed it: the
round trip does not lowercase anything, because `idna.uts46_remap()` has
already case-folded the domain. The panel caught it when the cut-down
branch went through (next section).

### The other open PRs

The mechanical check on each before this work (the next section has
what the panel found):

| PR | Visible words | Diff | Why in the first two sentences | Import time measured | Check |
|---|---|---|---|---|---|
| arrow#1398 | 456 | +106 -5 | partly (every `arrow.get` rebuilds a parser; no caller named) | no (adds `import threading`) | FAIL: length, dashes |
| fastjsonschema#233 | 342 | +42 -15 | no | not applicable (no new imports) | FAIL: length, dashes, title |

## Retrospective: the three open PRs through the panel

On 2026-10-29 we ran email-validator#182 (cut down), arrow#1398 and
fastjsonschema#233 through steps 1 to 7. Same box as above: shared Ryzen 9
5950X, CPython 3.12.3. Each round below is a fresh packet and three fresh
reviewers.

### email-validator#182: cut down and pushed

The branch now holds change 1 only (+7 in `syntax.py`, +1 changelog line)
on top of current main (4f32939), force-pushed to the PR branch, with a new
title and body.

Round 1 (packet 743521c86e91): maintainer FAIL, AI tells PASS, facts FAIL.
Both FAILs caught the same false claim, one we had also written into the
retrospective above: the IDNA round trip does not "only lowercase" the
domain. `idna.uts46_remap()` at the top of the function has already
case-folded it, so the round trip returns it unchanged. That made the
`.isascii()` and `.lower()` calls in change 1 dead code: the maintainer
reviewer wanted `if "xn--" not in ascii_domain:` and nothing else. Facts
also found the pydantic link one line off (the evidence copy carries a
header line), "60,044 generated addresses" counting cases rather than
addresses, and no CPU in the benchmark evidence.

Round 2 (packet 3db899700976): all three PASS. Remaining minors: no test
pins the case-sensitive `xn--` check (the fuzz corpus covers `XN--`), and
the old "For ASCII-only domains, the conversion out of IDNA just gives the
same thing back" comment below the new return is now slightly stale.

| | main | branch |
|---|---|---|
| Lines | | +8 (2 files) |
| PR body script, µs per address (5 rounds) | 53.5 to 54.6 | 29.8 to 30.8 |
| mixed-2000 / ascii-5000 / idn-1000 | 54.7 / 64.4 / 92.9 µs | 32.8 / 28.9 / 92.0 µs |
| `import email_validator`, median of 41 | 27.4 ms | 27.6 ms |
| VmHWM after import | 12,360 KiB | 12,368 KiB |
| Test suite, 3.12.3 | 319 passed, 1 failed | same (`test_domain_literal`, fails on main too) |
| Test suite, 3.10 and 3.14 | 320 passed | 320 passed |
| Differential, 60,044 cases (51,928 addresses) | | identical |
| 11 documented settings changed after the first call | all take effect | all take effect |

The settings check on the first PR head (4e2ddc7) showed the bug was wider
than "First use" says: every documented setting changed after the first
call was ignored, including `SPECIAL_USE_DOMAIN_NAMES.remove("test")`,
the README's own recipe for test domains.

The reply to "But why?" took three rounds (packets 0f5efe178f30,
d1f55145b3cb, b909be143128). Round 1 failed facts: "about 54 µs to 30 µs
per address" read as a per-address cost for the fast-path class, and the
packet lacked the branch's settings, test and differential output. Round 2
failed facts on one word: "Mostly pydantic" claims pydantic is the main
caller, which nothing shows. Round 3 passed all three:

> pydantic's `EmailStr` calls `validate_email()` with
> `check_deliverability=False` on every value, so bulk imports run it once
> per record.
>
> The first version had a bug. It cached module settings on the first
> call, so later changes like `email_validator.CHECK_DELIVERABILITY =
> False` were ignored. I've cut it to one early return that skips the IDNA
> round trip for domains without Punycode labels; the description has the
> numbers.

It replaces the "First use" draft, which promised a change that has now
been pushed. Not posted.

### arrow#1398: close recommended

Measured on master 2224255 against the PR head and three arms built from
it: the factory change alone (A), the class-level cache alone (B), and S,
which wraps `DateTimeParser(locale, cache_size=64)` in
`functools.lru_cache(maxsize=64)` in `factory.py` and reuses the
existing `cache_size` option (+8 -4, one file).

| arrow.get, µs per call | master | PR | A | B | S |
|---|---|---|---|---|---|
| `arrow.get(s, "YYYY-MM-DD HH:mm:ss")` | 22.24 | 5.63 | 14.33 | 16.26 | 5.29 |
| `arrow.get(s)` ISO | 31.72 | 11.40 | 23.40 | 23.86 | 11.14 |

S gets all of the PR's gain without the module cache, the lock or the
`threading` import. `import arrow` time and memory are flat on every arm
(`threading` is already loaded before `arrow.parser`), the test suite and
pre-commit pass, and a 24,553-case differential through `arrow.get` is
identical on all of them. Arrow documents no module-level settings, and
duplicate locale names raise, so a cached parser cannot go stale through
the locale registry.

The why is the problem. The only real caller we found is ics 0.7.3 (about
330k downloads a month), which calls `arrow.get(string, fmt)` for every
date field. Parsing a generated 500-event calendar with ics takes 847 to 878 ms
(five rounds), and the PR saves 1.7% to 4.9% of it per round, because ics spends
most of its time in its own grammar parser. That is far below the 1.2x
bar, and the project's last merge was in April.

Panel on the PR as posted (packet 39c2645c7c0e): maintainer SHRINK (to
S, with one test), AI tells FAIL (eleven blocking: 456 words, headings,
table, bold counts, eight dashes, "say the word and I'll attach it",
"their own gates", our track names, a parser.py comment that the same
diff makes false), facts FAIL. Facts found a real defect: the class-level
cache is keyed by class name and a snapshot of the input map taken in
`__init__`, so a parser whose `_input_re_map` changes later, or a
same-named subclass with its own `_ESCAPE_RE`, gets another parser's
pattern. Once such a parser has parsed `YYYY`, a plain
`arrow.get("2013", "YYYY")` raises `ParserMatchError` on the PR branch
until the entry is evicted; master and S parse it. Facts also found that
most of the body's numbers (3.86x, 2.86x, the 1.81x "pipeline", 19.0 MB,
the "20-case corpus") match no evidence we hold.

S falls under the gain bar on the ics workload, so by step 7 it is HOLD,
and the open PR has the defect above. The recommendation is to close it.
The closing comment used all three rounds without a unanimous PASS.
Round 1 failed facts on "real callers" (one caller is not callers),
round 2 on "fails for the rest of the process" (the cache is a 128-entry
FIFO). Round 3 (packet 6f85231f3779) passed the maintainer and AI-tells
reviewers and failed facts once more: the failure depends on order.
Whichever parser fills the `YYYY` entry first breaks the other, so the
sentence holds only when the customized parser goes first. Under step 7
the comment is held for the person signing off, with this last draft and
the facts reviewer's fix (state the order):

> Closing this. The shared pattern cache is keyed on a snapshot of the
> input map taken in `__init__`. If a parser's `_input_re_map` is changed
> after that and it then parses `YYYY`, a following
> `arrow.get("2013", "YYYY")` fails. The gain for a real caller is also
> small: in ics 0.7.3, parsing a generated 500-event calendar was less
> than 5% faster.

Not posted; closing needs the same sign-off as opening.

### fastjsonschema#233: close recommended

fastjsonschema is about speed (the README publishes `make performance`
numbers), so its own `performance.py` schema is the workload a maintainer
will look at. Per hunk, against master 738da70, five interleaved rounds
with no reviewers running:

| | `performance.py` valid | `performance.py` invalid | order docs, half invalid | order docs, valid |
|---|---|---|---|---|
| PR (+42 -15) | 1.07x | 1.06x | 1.49x | 1.71x |
| `required` list comprehension only (1 line) | 1.06x | 1.03x | 1.18x | 1.32x |
| `_keys` elision only (about 35 lines) | 0.99x | 0.99x | 1.14x | 1.17x |
| enum tuple membership only | 0.99x | 0.99x | 1.01x | 1.03x |

The order documents are our own synthetic schema with no
`additionalProperties`, the case the `_keys` change targets. The test
suite (4539 passed, identical on both arms), pylint and a 15,448-case
differential (the whole JSON-Schema-Test-Suite for drafts 4, 6 and 7 plus
mutated order documents) agree with master. The diff touches no imports
or module-level state.

Panel on the PR as posted (packet cb8b7cf37557): maintainer SHRINK (to
the one-line `required` change; drop the enum hunk, which has no gain of
its own, and the `_keys` change, whose `propertyNames: false` branch is
dead code), AI tells FAIL (fourteen blocking, from the title tagline to
"say the word and I'll attach them"), facts FAIL. The body says the
change is "draft04 only" and offers to do draft06 and draft07, but both
subclass draft04, so the PR already changes all three. "No behavior
change" is false: `{"properties": []}` used to raise `AttributeError` at
compile and now compiles, and an all-string `enum` now accepts a
non-string object whose `__eq__` matches one of the strings. The body's
table (1.48x, 1.73x, 1.92x, 23 MB) matches no evidence we hold.

The one-line version is correct (both probes behave as on master) but
gives about 3% to 6% on `performance.py`, under the gain bar, so it is
HOLD. The recommendation is to close the PR. Closing comment, all three
PASS on packet 42cf54f74716:

> Closing this. The title and changelog say no behavior change, which
> isn't true: `{"properties": []}` now compiles instead of raising
> `AttributeError`, and an all-string `enum` now accepts a non-string
> object whose `__eq__` matches one of the strings. On the repo's
> `performance.py` schema it is also only about 6% faster.

Not posted.

### What the process learned

- The facts reviewer is the one that finds bugs. On all three PRs it ran
  code against the claims, and each time it found something the author
  and the earlier review had missed: the "only lowercases" claim, the
  arrow cache returning another parser's pattern, the two fastjsonschema
  behavior changes.
- Check what the posted numbers rest on. Neither the arrow nor the
  fastjsonschema body's table could be reproduced from anything we kept.
  Put the script that produced each number in the evidence, and quote
  only what it printed.
- Benchmarks and reviewers compete for the CPU. A fastjsonschema run made
  while six reviewers were working read 549 ms for master against 347 ms
  on a quiet box. Measure before launching the panel, or after it ends.
- The settings check belongs on the old code too. Run on the first #182
  head, it showed every documented setting ignored after the first call,
  not just `CHECK_DELIVERABILITY`.
- The project's own benchmark is the honest workload when the project is
  about speed. Our order-document corpus showed 1.5x to 1.7x for #233;
  `performance.py` shows 1.06x.
