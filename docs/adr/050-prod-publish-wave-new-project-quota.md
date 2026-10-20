# ADR-050: prod publish wave discipline under PyPI's new-project quota

Date: 2026-10-20
Status: accepted

## Context

PyPI does not limit uploads; it limits **project creation**. The
limiter is `q=4; w=86400` per user and per IP, with a rolling 24 h
window that frees one slot at a time (pypi/support#11827):

```
Ratelimit-Policy: "project.create.ip";q=4;w=86400;pk=:ip:,
                   "project.create.user";q=4;w=86400;pk=:user:
```

A first upload of a name that does not exist yet is a *project
creation*; a later upload of another version of an existing name is
not. So the number of **new distribution names** an account can land per
day is four, fleet-wide, no matter how many lanes are publishing.

The 2026-10-20 prod wave showed the consequence. The sympy/docutils
lane had already created two names that day (21:18:15Z,
21:19:47Z). The wave could therefore land exactly two more:

| # | project | created |
|---|---|---|
| 1 | sympy-poly-rust 0.1.0 | 21:18:15Z |
| 2 | docutils-rust 0.1.0 | 21:19:47Z |
| 3 | python-multipart-rust 0.0.32 | 21:32:28Z |
| 4 | pyparsing-rust 3.3.3 | 21:34:20Z |
| 5 | pyyaml-rust 6.0.2 | 21:36:06Z → **429 Too Many Requests** |

No ordering of the four targets could have produced four landings that
day. The 429 is also account-level, not package-level: the next target
in the queue (`pygments-rust`) would have been refused identically.

Two more lane facts from the same wave:

- `release-publish`'s prod gate requires a TestPyPI success **younger
  than 24 h**; the carried bundles had successes from 2026-10-09/13, so
  the gate refused the prod lane. TestPyPI accepts a re-upload of the
  same version, so re-uploading the *identical* bytes refreshes the
  proof of exactly the bytes that will be promoted, with no version
  bump and no change to the same-SHA256 invariant.
- The post-upload check is index-propagation sensitive (ADR-049): a
  successful twine upload was reported as a failure because the
  immediate `pip install` could not yet see the new project. PyPI's CDN
  also serves a cached 404 for a new project for a couple of minutes,
  which pip reports as "no matching distribution" without retrying.
  The human-facing `/project/<name>/` page is not a usable existence
  probe at all: it answers 200 with a bot "Client Challenge" page for
  names that do not exist.

## Decision

1. **A prod wave is serialized, one upload attempt per package, in a
   fixed order.** A 429 stops that package immediately and parks it with
   the exact PyPI error text and the attempt timestamp; there is no
   retry. Because the limiter is account-level, a 429 also ends the
   wave: the remaining targets are parked **without an attempt** rather
   than probed, since a rejected attempt may itself extend the window.
2. **The TestPyPI lane is refreshed before the prod attempt** (identical
   bytes re-uploaded, pinned install + smoke vectors re-run) whenever
   the recorded TestPyPI success is older than the gate's 24 h window.
3. **A landing is proven by bytes, not by a status code**: the prod
   index JSON must serve every file with the prep SHA-256, and a
   clean-venv pinned install from `https://pypi.org/simple/` must run
   the prep smoke vectors and at least one real call. When the upload
   landed but the tool's immediate index check failed, the outcome is
   re-recorded with `release-record --result success` (sticky success) —
   never re-uploaded.
4. **crates.io is verified, not re-published.** When the core crate is
   already live (index `cksum` == the prep `.crate` SHA-256), a second
   `cargo publish` is a duplicate attempt, not a new landing.

## Consequences

- A release campaign must budget **account-wide new project names per
  day** (four), including the names other lanes already created that
  day. A wave of N new dists needs `ceil(N / 4)` days, and a lane that
  lands two names on day one leaves the next lane two slots short.
- A parked package keeps its crates.io core crate and its TestPyPI
  dist; only the prod PyPI name is missing. The resume point is
  24 h after the oldest creation in the window (one slot per release),
  and the lane must refresh the TestPyPI lane again before the prod
  attempt.
- The tool gaps this ADR exposes (each a candidate tool PR, not a
  hand-applied change): `release-publish` should refresh the TestPyPI
  lane itself when its success is stale; it should retry the post-upload
  index install with a short backoff and treat "twine ok + index JSON
  sha256 match" as success; and the lane report should record the
  `Ratelimit-Policy` header so the quota state is visible without
  guessing.
- A lane that reports "prod URL 200" as landing evidence may have
  recorded a false positive (the bot-challenge page). Existence proofs
  use the JSON API and the simple index.
