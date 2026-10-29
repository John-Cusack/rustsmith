# Writer brief: upstream PR or reply

You prepare one upstream PR (or one reply on an upstream thread) for
review. You do not post anything. The process is in
`docs/UPSTREAM_PRS.md`; this brief is the part you act on.

Never start a shell command with `cd`; use absolute paths or
`git -C <path>`.

## 1. Write the why first

Before touching the diff, write two sentences: who calls this code, how
often, and what it costs them today. Use the project's README for its
stated use, and find a real caller (a dependent library, an issue, a
framework integration) with a link. If you cannot write these sentences,
stop and report that the offer has no why. Do not continue to a PR.

## 2. Make the diff small

- Start from the smallest change that could carry the gain, then add
  hunks only when they earn it.
- Measure each hunk on its own (leave one out, or add one at a time) on
  the workload that matches the why. Keep a hunk only if it carries at
  least about 10% of the total gain or is needed for correctness. Record
  the table in `evidence/`.
- Write it the way the maintainer would: their names, their comment
  density, no new module state unless it earns its place, no terms from
  our own tooling.
- If you would offer to split it, split it now and post the smaller part.

## 3. Measure the costs

Put raw output for each of these in `evidence/`, both arms:

- the speed number on the why workload (fresh processes, min of several
  runs, arms interleaved);
- `python -X importtime -c "import <pkg>"` and memory after import
  (VmHWM), if the diff touches imports or module-level code;
- their full test suite, lint and type checks;
- a differential run over generated inputs that compares return values
  and exception type and message;
- every documented runtime setting (module globals, config objects) still
  takes effect when changed after the first call.

## 4. Draft the text

Use `prompts/upstream_pr/pr_body_template.md` or
`prompts/upstream_pr/reply_template.md`. Then run the humanizer skill on
the draft in embedded mode (Skill tool, `humanizer:humanizer`) and diff
the result against your draft: no number, name, command, or claim may
change.

## 5. Build the packet and hand off

```sh
scripts/upstream_pr_packet.sh --repo <upstream checkout> --base <sha> \
    --head <branch> --evidence <dir> --title <file> --body <file> \
    [--thread <file>] --out <packet dir>
```

Put only raw measurements and test output in `evidence/`. Your notes,
plans, earlier drafts, and this conversation stay out: the reviewers must
judge what the maintainer will see.
