# Fire-and-Forget Proposal — Staff Engineering Review

Date: 2026-10-02

Verdict: **REWORK**

Scope: review only; no implementation. This document is a handoff for an independent LLM review of `docs/FIRE_AND_FORGET_PROPOSAL.md`. Check the findings against the current checkout before implementing changes.

The architecture is viable, but the proposal treats several unimplemented safeguards as established guarantees. The largest blockers are ineffective council review, grading outside the claimed isolation boundary, and release records that are neither bound to verified artifacts nor safe under concurrent retries.

All six requested ground-truth files were read in full:

- `docs/FIRE_AND_FORGET_PROPOSAL.md`
- `docs/RELEASE.md`
- `config/default.toml`
- `crates/rustsmith-release/src/lib.rs`
- `crates/rustsmith-cli/src/release.rs`
- `crates/rustsmith-council/src/lib.rs`

Relevant callers, oracle/gate implementations, storage, worker spawning, sandbox behavior, and release acceptance checks were also inspected. Line references below describe the reviewed checkout and may move in later revisions. External publishing documentation was checked during the review.

## 1. Correctness

### PyPI and TestPyPI

The upload and OIDC descriptions are broadly correct, with important qualifications:

- Uploads use the file-upload API; OIDC exchanges a supported publisher identity for a short-lived upload credential. TestPyPI needs its own publisher registration, audience, and minting endpoint. Prefer the official publishing action over implementing the token exchange: PyPI explicitly describes the underlying exchange details as lacking compatibility guarantees. [PyPI publishing documentation](https://docs.pypi.org/trusted-publishers/using-a-publisher/).
- Pending publishers apply to **new** projects; existing projects use their project publishing settings. Pending registration does **not reserve the name**. The proposal should say “no documented, supported registration API” instead of making an absolute claim about all APIs. [New-project setup](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/), [existing-project setup](https://docs.pypi.org/trusted-publishers/adding-a-publisher/).

### Crates.io trusted publishing

Proposal §2.2 and §3.4 materially misdescribe bootstrap. Registering a publisher does not make a new crate publishable through OIDC. Current crates.io documentation requires the crate to exist already: its initial publication needs an API token.

Subsequent GitHub publications use `rust-lang/crates-io-auth-action`, whose output is passed as `CARGO_REGISTRY_TOKEN`; `id-token: write` alone does not make `cargo publish` authenticate. Registration is per crate, under its settings. [Current crates.io documentation source](https://github.com/rust-lang/crates.io/blob/main/svelte/src/routes/docs/trusted-publishing/%2Bpage.svelte).

### Building versus uploading

- `maturin publish` combines building with publishing and therefore does not meet the promotion requirement. Its publishing commands are also deprecated in current maturin documentation. Use `maturin build`/`sdist`, followed by an uploader consuming explicit retained files. [Maturin changelog](https://www.maturin.rs/changelog.html).
- Proposal §3.3's `twine upload … dist/*` would include the `.crate` retained by `cmd_release_prep`. Upload only manifest-listed wheels and sdists. PyPI accepts `bdist_wheel` and `sdist`, not Rust crate archives. [PyPI Upload API](https://docs.pypi.org/api/upload/).
- `twine check` checks distribution description rendering; it is not behavioral or supply-chain verification. [Twine documentation](https://twine.readthedocs.io/en/stable/).

The retained `.crate` is not automatically the archive later published. `cargo publish` packages the source again; it does not accept an existing `.crate` as its input. Also, `crates/rustsmith-cli/src/release.rs:217`, `cmd_release_prep`, uses `cargo package --no-verify`, so its successful packaging check does not prove the packaged contents compile.

Add package verification and explicitly define how the final uploaded archive is matched to approved evidence. `cargo publish --dry-run` cannot prove that server-side upload checks will accept the release. [Cargo publishing reference](https://doc.rust-lang.org/cargo/reference/publishing.html), [cargo publish](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).

### Version discovery and retries

- `cargo search` is a limited textual search, not an exact version-existence query. `pip index versions` filters by compatibility and excludes prereleases by default. Use exact registry metadata/index queries, including yanked releases; distinguish absence from transport/authentication failure. Preflight remains advisory because publication can race it. [cargo search](https://doc.rust-lang.org/cargo/commands/cargo-search.html), [pip index](https://pip.pypa.io/en/stable/cli/pip_index/).
- “Version unused on both registries” blocks recovery after a partial production publication. An existing release must be accepted for recovery only when its remote artifacts match this release's approved identities.
- `skip-existing: true` is not identity verification. A same-name wheel with different bytes must halt. PyPI uploads files individually, so even one registry can be partially published. [Upload semantics](https://docs.pypi.org/api/upload/).
- Crates.io versions cannot be overwritten; yanking does not delete them. PyPI releases contain multiple files, and additional filenames can be uploaded, but previously used filenames cannot be reused—even after deletion. Avoid describing both systems as having identical version immutability. [Cargo reference](https://doc.rust-lang.org/cargo/reference/publishing.html), [PyPI filename rules](https://pypi.org/help/#file-name-reuse).

### Registry requirements versus rustsmith policy

Crates.io permits Python-related dependencies, binary crates, and alternative library paths; it does not generally require API documentation. A Python-independent `rlib` with `src/lib.rs` is a sensible rustsmith requirement, but label it accordingly. License/description metadata and registry validation are separate requirements.

`crates/rustsmith-release/src/lib.rs:290`, `validate_tree`, also checks Python independence through manifest substring searches, not the resolved transitive dependency graph.

### Generated workflow is not the functioning baseline described in §1

In `crates/rustsmith-release/src/lib.rs:630`, `render_workflow`:

- Smoke steps lack a `run:` key; their generated shell quoting is also unsafe. See the `smoke` construction at line 633 and its insertion under the smoke step names.
- `actions/setup-rust@v1` references a repository that returned HTTP 404 when checked during this review.
- `test-wheels` downloads every platform/interpreter wheel and runs `pip install dist/*.whl` on Ubuntu/Python 3.12. Incompatible wheels fail; multiple distributions of the same package cannot all be tested this way.
- Production has no source checkout for `cargo publish`, no crates.io authentication exchange, and publishes PyPI **before** the core.
- Core paths and the reference test cell are hardcoded. `release_workflow` does not determine the generated output filename; `cmd_release_prep` always writes `release.yml`.
- TestPyPI installation is unpinned, uses an extra index, and performs no version assertion. Pip gives indexes no priority, so this can test a production package instead. [Pip selection behavior](https://pip.pypa.io/en/stable/cli/pip_install/).
- Outcome recording is an `echo`, not `release-record`; release state and verification evidence are not carried through the jobs.

The existing workflow tests check string presence, not executable workflow semantics. See `workflow_uses_same_artifacts_and_matrix` in the release library and `tests/release_acceptance.sh:80`.

## 2. Soundness

**The proposed mechanisms do not currently prevent self-review, and they cannot establish universal behavioral identity.** They can enforce independent review participation and measured parity over a defined contract.

### 2.1 Mirror reviews are fabricated approvals

`crates/rustsmith-cli/src/mirror.rs:1238`, `record_review`, constructs always-approving `StubDriver`s and discards `Resolution`. Its caller at line 1106 supplies `"unit {id} diff"`, not an actual diff, before merging. Optimizer merges have no council decision in their merge path.

Real review, actual artifacts, and approval enforcement must precede publishing work.

### 2.2 Provider diversity is not enforced

`crates/rustsmith-council/src/lib.rs:185`, `model_for_seat`, extracts only a model string. Mirror `default_providers` hardcodes provider labels; `assign_reviewers` falls back to reviewers with duplicate providers. Shell commands can ignore configured model names.

Record actual provider/model identity, author identity, invocation identity, and reviewed content digest; reject missing or substituted reviewers. Distinct provider labels alone also do not demonstrate independent underlying models or uncorrelated errors.

### 2.3 Quorum is underspecified and inconsistent with the implementation

Proposal §3.2 names two reviewers but proposes 2/3. `crates/rustsmith-council/src/lib.rs:218`, `Council::decide`, instead collects proposer plus two critics, requires unanimity, and otherwise gives Architect the decision. It does not reject duplicate seats or author participation.

Define eligible voters explicitly. Deterministic failures and correctness objections must not be overturned by a majority or Architect. Approval must refer to the exact candidate that merges, and final merged source must be verified again.

### 2.4 The signature does not enforce blindness

`SeatDriver::critique` excludes other positions from its parameters, but implementations can read shared state, retain previous context, or use filesystem/network access. `WorkerSeatDriver` ignores the artifact bytes and receives proposer reasoning, which can contain injection or leaked judgments.

Use separate contexts and restricted capabilities, and treat artifact text as untrusted. A Rust function signature is not a confidentiality boundary for an external process.

### 2.5 The claimed grading boundary is false

`crates/rustsmith-cli/src/mirror.rs:517`, `run_oracle_in_venv`, and `run_heldout_in_venv` call the canonical **host** executor. `crates/rustsmith-oracle/src/lib.rs:174`, `Oracle::graded_run`, is also host-side.

The separate sandbox implementation permits a pytest host fallback when Docker fails; see `crates/rustsmith-sandbox/src/lib.rs:140`, `grading_run_impl`.

Release-eligible grading must require the isolated backend and refuse fallback. Keep grading network access disabled, with no publisher credentials, control-plane database, or writable audit files exposed to the candidate.

### 2.6 Held-out checks can pass without useful evidence

`crates/rustsmith-cli/src/mirror.rs:1535`, `parse_heldout_rate`, returns `1.0` when no passed/failed counts are found; its caller ignores process exit codes.

`crates/rustsmith-gates/src/lib.rs:145`, `heldout_divergence`, permits nonzero held-out failures and uses `<=`, contrary to the proposal's `<`.

Require nonempty, counted execution, successful completion, and zero correctness failures. A divergence screen is not a replacement for held-out correctness.

### 2.7 Redaction covers worker feedback, not all outputs

`grade_candidate` stores full gate details, including held-out values. Mirror reports retain divergence and are committed into the fork. The worker is given `RUN_EVENTS` and executes on the host.

`Store::learn_stats` currently avoids direct held-out numeric columns, but aggregates derived from repeated hidden-suite decisions can still enable adaptive overfitting.

Preserve restricted evaluator records; expose sanitized feedback, bound retries, and use a fresh final suite. Do not feed restricted records, report files, or hidden-suite measurements into review prompts or the learning pipeline.

### 2.8 Privacy enforcement is not wired into live spawning

`assert_training_tier_allowed` appears in tests/demo paths, not `Agent::spawn_worker` or live seat invocation. It compares model strings, while configuration lists `"worker"` as a seat.

Resolve privacy policy against actual model/provider identities before every invocation.

### 2.9 Differential fuzzing needs an evaluator-owned contract

Require evaluator-owned generators, a frozen upstream baseline, minimum case counts, independent executions, timeouts, and comparison of exceptions, side effects, and serialization—not just scalar outputs.

`crates/rustsmith-gates/src/lib.rs:167`, `differential`, currently accepts an empty comparison list; the new gate must reject empty or aborted campaigns.

Replanning into `BindInsteadOfPort` or `MarkPortOnly` also needs to disqualify a release from claiming a complete Rust replica. Provider diversity and finite fuzzing are useful evidence, not a behavioral-equivalence proof.

## 3. Playwright verdict

**Use the human checklist for M-B. Cut the bootstrap browser script.**

PyPI's documented registration flow is small and infrequent. Human authentication, account recovery, and selecting the correct publisher identity are the dominant concerns; selector maintenance adds little value. Automate repository/environment creation and inspection through GitHub's API instead. [GitHub environment API](https://docs.github.com/en/rest/deployments/environments).

The checklist must include the initial crates.io token-authenticated publication, subsequent publisher registration, and an actual check that required-reviewer protection exists. Proposal §3.4 creates a private repository, but required reviewers on private repositories are unavailable on GitHub Free/Pro/Team plans. [GitHub protection availability](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments).

A supervised bootstrap helper could become worthwhile at demonstrated onboarding volume. Keep sessions, cookies, traces, and screenshots out of release artifacts.

**Playwright-per-publish is unjustified for these registries.** Account recovery or publisher reconfiguration can require browser interaction; artifact upload should still use the upload protocol. PyPI removed browser uploads. [PyPI help](https://pypi.org/help/#browser-upload).

## 4. Missing risks and persistent-state hazards

### Risk register

| Risk | Severity | Concrete mitigation |
|---|---|---|
| Untrusted build scripts, Python imports, and Cargo configuration execute with host privileges | Critical | `release.rs::run_cmd` inherits the environment; `extra_env=&[]` does not remove secrets. Build/test in disposable, credential-free workers. Publisher credentials must never coexist with package execution. |
| Mutable inputs and substituted artifacts | Critical | `git_head` plus `copy_stage` includes dirty/untracked content while recording HEAD. Require an immutable accepted snapshot; bind source, lockfiles, toolchain, gates, reviews, and every artifact digest into release evidence. |
| Private-source disclosure through staging | Critical | Require explicit project authorization for public distribution before TestPyPI. A private GitHub repository does not make TestPyPI or sdists private. |
| Immutable releases and ambiguous upload outcomes | High | Record and reconcile individual remote files. Cargo can time out after a successful upload. Query remote checksums before retrying; mismatch halts. Keep a reviewed yank/fix-forward procedure. |
| License and attribution errors | High | `KNOWN_SPDX` and NOTICE substring matching establish neither licensing compatibility nor compliance. Verify upstream revision, copyright, license texts, modifications, and dependency obligations in wheel, sdist, and `.crate`. |
| Name ownership and import collisions | High | `validate_names` only avoids the configured upstream distribution collision. Approve new identities manually, check normalized names and ownership, and distinguish an existing owned project from an available new name. Document replacing upstream: two distributions owning `crc/` can collide. |
| Workflow/configuration compromise | Critical | Protect workflow and release policy from model edits; pin actions and build images; serialize releases per project/version; bind approval to a manifest digest. `smoke_exprs`, environment names, and runner names are interpolated into executable YAML. |
| Weak package/platform coverage | High | Test each wheel on its matching platform/interpreter in a fresh environment. Install and test the sdist-rebuilt wheel. Inspect license files and library dependencies in every distribution. Enforce publishable Linux tags. |
| TestPyPI availability and retention | Medium | Retain evidence independently and detect lost publisher/account setup. TestPyPI may be pruned; it cannot be the durable release ledger. |

Supporting registry and license documentation:

- Cargo can time out while waiting for index visibility after upload; that timeout does not undo the upload. [Cargo timeout semantics](https://doc.rust-lang.org/cargo/commands/cargo-publish.html).
- BSD redistribution requires notices and license conditions, not merely its SPDX identifier. Apache-2.0 also specifies license, change-notice, and attribution obligations. [BSD-2-Clause text](https://spdx.org/licenses/BSD-2-Clause.html), [Apache-2.0 text](https://www.apache.org/licenses/LICENSE-2.0).
- Linux wheel portability and supported upload tags need explicit handling. [Maturin distribution guidance](https://www.maturin.rs/distribution.html).
- TestPyPI's database may be periodically pruned. [TestPyPI guide](https://packaging.python.org/en/latest/guides/using-testpypi/).

For supply-chain provenance, distinguish **publisher identity** from **build/verification provenance**. The official PyPI action already generates publish attestations by default; that does not prove the uploaded package passed rustsmith's gates. [PyPI attestations](https://docs.pypi.org/attestations/producing-attestations/).

The grading and publishing network policies need a real process boundary. Giving the publisher network access must not also give candidate code access to publisher credentials. Prepare dependencies separately, then execute candidate builds/tests without secrets; use a dedicated publisher that consumes approved evidence and artifacts.

### Persistent-state hazards

These are data-loss and logical-audit-corruption risks. WAL does not solve cross-file consistency, lost JSON updates, or fabricated decision records.

| File | Existing hazard | Required change before automation |
|---|---|---|
| `release-state.json` | `crates/rustsmith-cli/src/release.rs:154`, `cmd_release_prep`, recursively deletes an existing output directory, erasing publication history. `cmd_release_record` performs unlocked read/modify/truncate/write: concurrent lanes lose outcomes; interruption can leave invalid JSON. | Immutable release directories; canonical output-path checks; one writer or locking plus atomic durable replacement; separate prep from resume. Output must not overlap source, evaluator state, or the workspace's persistent files. |
| `release-state.json` | `record_outcome` can demote success to failed, panics if a known registry is missing, and accepts success without remote evidence. `overall_status` checks only three status strings. | Validate schema and release identity; preserve confirmed publication facts; require remote filename/checksum evidence and separate publication from verification outcomes. |
| `verification.json` | A free-standing `passed:true` is written after packaging checks. It contains no bound source/artifact identity or council evidence. `--recon-out` is only checked for directory existence. `.crate` retention is optional, and the rebuilt wheel is not installed/tested. | Mandatory artifact set; evidence bound to the final accepted snapshot and manifest; explicit required checks and matrix coverage; no promotion based on a boolean alone. |
| `store.db` | `crates/rustsmith-store/src/lib.rs:286`, `create_run/create_unit`, use `INSERT OR REPLACE`, resetting halt/status/attempt metadata when identifiers recur. Optimizer candidate IDs reuse `run_id:technique`. Gate and decision rows do not independently bind the reviewed candidate digest/provider identities. | Stable release IDs, unique candidate/attempt IDs, transactional queue claims/leases, non-destructive resume, and explicit evidence relationships. Keep publication records separate from optimization yield tables. |
| `events.jsonl` | `crates/rustsmith-store/src/lib.rs:256`, `Store::append_event`, has no writer serialization or transaction with SQLite. Agent logging independently writes `ts:0`, `run_id:"m1"` and ignores errors. Workers receive the writable event path. | Evaluator-owned event writer; actual run/release IDs and sequence numbers; transactional outbox and replayable export; workers submit events through a restricted channel. |

The arbitrary output-directory deletion is particularly dangerous if a daemon places its `store.db`, `events.jsonl`, or previous release evidence below that directory. Reject overlaps before preparation, and never treat re-running preparation as publication recovery.

## 5. Sequencing

### M-A: establish the prerequisites

Replace the assumed safeguards with real ones:

- Effective live reviews over actual artifacts and rejection-enforced merges.
- Isolated grading with no release-eligible host fallback.
- Fail-closed held-out parsing and nonempty evaluator-owned fuzz execution.
- Privacy enforcement on actual invocations.
- Final-snapshot verification and durable release state.

Move online registry preflight out of local `release-prep`; preparation should remain useful offline. Also note that `run_pipeline` currently finishes at harvest: release preparation is a separate command, not an already-wired final pipeline stage.

### M-B: one complete TestPyPI path

Restrict initial scope to explicitly onboarded Python-spine projects. Fix the generated workflow, verify every artifact, retain a manifest and verification evidence, and reconcile remote hashes after upload.

Move per-file/per-registry retry and crash recovery from M-C into M-B: unattended staging already needs them. Cut Playwright and defer adding publish outcomes to `learn`.

### M-C: recoverable, human-gated production first

Publish/verify the core, then promote Python files; record each step durably and verify production installs.

Test process death after upload but before recording, mismatched existing files, concurrent attempts, stale approval, malformed state, and a partial multi-wheel upload.

Extend acceptance beyond the current string checks: workflow validation plus an actual onboarding/staging run is necessary. Mock uploads alone cannot validate OIDC identity, platform wheels, or registry restrictions.

### When fully automatic production is allowed

Only allow it for an explicitly authorized project and bounded change class after these mechanisms work. New names, changed licensing, workflows, dependencies, unsafe boundaries, or supported interfaces should return to human approval.

“Auto behind required human environment reviewers” remains human-gated. Define the modes explicitly; changing the trigger does not remove that gate.

## 6. Recommendations for every proposal §6 question

| Question | Recommendation | Tradeoff |
|---|---|---|
| OIDC-only or vaulted daemon token? | Standardize on CI OIDC. The daemon dispatches and monitors publishing. Allow a separately approved, narrowly scoped TestPyPI token exception; bootstrap the first crate with a short-lived API token, then remove it. | Adds CI dependence and latency, while keeping registry authority away from the builder host. |
| Crates.io first or parallel? | Strict core publication and remote verification first, then PyPI. | Slower, and partial publication remains possible. It gives predictable recovery. Wheels vendor the core, so the ordering is release policy rather than a wheel runtime dependency. |
| 2/3 or consensus? | Require verifier-and-scope unanimity for mirror correctness. A third reviewer can resolve questions through revision, but cannot override a correctness rejection. Use 2/3 for advisory performance choices. | More revision cycles; stronger accountability. At 16 workers, add provider quotas, bounded review queues, and fail-closed outage handling. |
| Playwright or checklist for M-B? | Human checklist, with GitHub setup/inspection through supported APIs. | Small onboarding effort; substantially less browser maintenance and credential/session exposure. |
| How many clean staging runs before auto production? | No TestPyPI-only count is sufficient. As an operational floor, use **10 distinct manually approved end-to-end releases per project over at least 30 days**, plus demonstrated recovery tests, independent final verification, and explicit owner authorization. | Conservative and slower. The count measures operational reliability; it does not prove correctness or justify relaxing hard gates. |

The numerical observation floor is a proposed policy choice, not a registry requirement or a statistical proof. Repeated uploads/tests of the same candidate do not count as distinct releases.

## 7. Verdict and highest-leverage changes

**REWORK**, with three priorities:

1. **Make verification real and mandatory:** replace stub reviews, enforce rejection, prove reviewer identity, and require isolated grading with nonempty held-out/fuzz execution.
2. **Bind approval to the complete release:** immutable accepted source, tested wheel matrix, verified sdist/core package, and digest-bound evidence consumed by a corrected publisher workflow.
3. **Build recoverable publication state before uploading:** durable serialized transitions, per-file reconciliation, exact-match retry semantics, and protection against prep deleting release history.

After those changes, automatic TestPyPI staging and gated production are reasonable to ship.
