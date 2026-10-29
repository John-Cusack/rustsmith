# Reply template

For any comment we post on an upstream PR or issue: answering a question,
replying to a review, closing our own PR, or a follow-up. The visible
reply stays under 120 words; most should be under 60.

```md
<First sentence answers what they asked, or says what you are doing
(closing, pushing a fix). No thanks-for-the-question opener.>

<One to three sentences with the one fact they lack: a number in their
terms, a link, or what the new commit changes. Every number must be in
evidence/ or the thread already.>

<If you concede a point, say so plainly once. If you offer to change the
PR, offer only a version you have measured, and give its number.>
```

Rules:

- Quote nothing back at them; they know what they wrote.
- No promise you have not measured ("I can cut it to X" needs the number
  for X in `evidence/`).
- If a maintainer found a real problem, say it is a problem and fix it.
  Do not explain how it happened.
- When they have already done the work themselves (kjd/idna#287), thank
  them in one clause and close. Do not compare the two PRs.
- Closing our own PR needs the same human sign-off as opening one.

Examples, both passed by all three reviewers (see `docs/UPSTREAM_PRS.md`,
"First use"). On kjd/idna#286, after the maintainer closed it and merged
their own three-line version:

> Thanks for #287, which gets most of the win in far less code. You're
> right about the import move too. I didn't measure import time, and it
> costs everyone who never touches uts46.

On JoshData/python-email-validator#182, answering "But why?":

> pydantic's `EmailStr` calls `validate_email()` for each address it
> validates, so bulk imports pay it once per record: about 56 µs per
> address on our mixed benchmark, or a minute of CPU per million.
>
> This PR has a bug: the cached defaults ignore settings such as
> `email_validator.CHECK_DELIVERABILITY` when they change after the first
> call. I'll cut it down to the 9-line change that skips the IDNA round
> trip for plain ASCII domains, which on its own brings the benchmark to
> about 35 µs per address, and correct the title and changelog to match.
