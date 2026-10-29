# PR title and body template

Fill this in, then delete every line in angle brackets. If the project has
a PR template, put it first, filled in, and fit the paragraphs below under
its headings.

The visible body (everything outside code blocks, tables and `<details>`)
stays under 200 words; aim for about 100. `scripts/upstream_pr_check.sh`
counts it.

---

Title:

```
<What the change does, imperative, like a commit subject, at most 72 characters.
No speedup multiplier and no "no behavior change".>
```

Body:

```md
<Why, one to three sentences. Who calls this code, how often, and what it
costs them today. Name a real caller and link it when it is another
project. If the only caller you can name is a benchmark loop, stop: the PR
is not ready.>

<What changed, one to three sentences, in plain words. Say why it is safe
if that is not obvious from the diff.>

<The number, one sentence: before and after on the workload that matches
the why, per call or per batch, whichever the reader would use. If the
diff touches imports or module-level code, add import time and memory,
even when unchanged.>

<Tests, one sentence: "The test suite passes, and outputs and errors match
master on N generated inputs." Mention a failure that also happens on
master only if CI will show it.>

<details>
<summary>Benchmark</summary>

<Machine, Python version, dependency versions. The commands to reproduce,
or a short script (under about 40 lines). The full table if there is one.>

</details>
```

<Changelog entry only if recent merged PRs from outside contributors add
one. Licence or sign-off lines only if CONTRIBUTING requires them.>

## Example

From kjd/idna#287, which the maintainer wrote for the same
optimization we had proposed:

> Title: Skip full validation for plain lowercase LDH labels in check_label
>
> Addresses the underlying optimization sought in #279 and #286, i.e.
> short-circuiting assessment if the label is LDH.

The diff is three lines. An outside contributor needs a little more than
that (the why and one number), but not much more.
