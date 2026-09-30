---
kind: proposal
id: LEP-007
title: "Staged Coverage and Honest Test Verdicts"
status: accepted
liveness: n/a
tags: [knowledge, verify, guard, requirements]
---

# Staged Coverage and Honest Test Verdicts

## Context

A downstream workspace (amberlog, 2026-09-28) accepted 17 requirement blocks in
one governance step and then drafted one task contract per block — 313
scenarios in total. Wiring those contracts into CI surfaced four gaps, and
ordering the first round's work from `requirements plan` surfaced a fifth. Moving the first fully green contract into `specs/`
(2026-09-29) surfaced a sixth. Running guard as a pre-commit hook once all 17
contracts were active (2026-09-30) surfaced a seventh and an eighth. Each was
reproduced against agent-spec 1.4.0 (`main` at `2483398`; the
`feat/clause-grouping` fork at `5ae1dc1` behaves identically in every code path
cited below).

**Gap 1 — "accepted" and "implemented" cannot both be green.**
`requirements plan` counts a requirement as covered only by specs in the active
tier: `collect_spec_files` skips `roadmap/` through `should_skip_spec_dir`
(`src/spec_knowledge/requirement_plan.rs`), and `validate_requirement_plan`
emits the Error `requirement-uncovered` for every ready requirement with no
active spec. `lint-knowledge` forwards that diagnostic, so
`lint-knowledge --gate` fails. The obvious fix — move the contract into
`specs/` — trades one red gate for another: `guard` verifies every spec in the
active tier, a scenario whose test does not exist yet is `skip`, and
`is_passing` treats any skip as non-passing. The local pre-commit hook runs the
same guard, so after the move no commit can land at all, including the commit
that adds the tests. The README already names the intended workflow ("Future
work lives in `specs/roadmap/` … promote it to the top-level `specs/`
directory"), and `requirements status` already reports an active, a staged
(`roadmap/`) and an archived tier (`src/spec_knowledge/status.rs`, the block
under "satisfying specs per tier"). Only coverage validation ignores the staged
tier.

The conflict is not a one-off start-up cost. A project that keeps its
specification one round ahead of implementation always has accepted but
unimplemented requirements, so under the current rules the knowledge gate and
the contract gate are never green at the same time.

**Gap 2 — an ignored test is reported as a pass.**
`TestVerifier::verify` (`src/spec_verify/test_verifier.rs`) shells out to
`cargo test -q [-p <package>] <filter>` and `test_run_verdict` decides from the exit status and from
`cargo_test_executed_count`, which sums the `running N tests` lines. libtest
prints `running 1 test` for a filter that matches an `#[ignore]` test even
though it never runs it:

```
running 1 test
i
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 1 filtered out
```

The count is therefore 1, the exit status is 0, and the verdict is `pass`.
Reproduced with a two-scenario probe spec: the scenario bound to an ignored
test came back `pass`. Any contract test that someone marks `#[ignore]`
silently turns its scenario green.

**Gap 3 — a selector cannot request cargo features.**
`TestSelector` carries `filter`, `package`, `level`, `test_double` and
`targets`; `build_cargo_test_args` passes only `-p` and the filter. A contract
test that needs a test-only feature (amberlog compiles crash-injection pause
points behind a `fault-injection` feature so they never reach a release build)
is compiled out under guard: the filter matches nothing, the run reports
`running 0 tests`, and the verdict is `skip`. Reproduced with the same probe.
The downstream workaround is a self dev-dependency
(`<crate> = { path = ".", features = ["fault-injection"] }`), which works —
test builds and `CARGO_BIN_EXE_*` get the feature, `cargo build` and
`cargo build --release` do not — but it is non-obvious, adds a self-edge to
`Cargo.lock`, and cannot express "this one scenario needs feature X".

**Gap 4 — long-running scenarios have no verification tier.**
Some accepted scenarios are soak tests by definition (amberlog: "an instance
that has ingested continuously for 24 hours has deleted no file"). guard runs
every bound test on every commit, so such a scenario can only be bound to a
test that is skipped (Gap 1's red gate), ignored (Gap 2's false pass), or
shortened (which silently narrows what the verdict means). `Mode: optimize`
and `Review: human` do not help: both still require the test to run under
guard, and `Review: human` yields `pending_review`, which guard's default
`auto` review mode counts as a pass.

**Gap 5 — dependencies on a superseded block are silently dropped.**
`build_requirement_graph` does not carry `supersedes:` into `RequirementNode`,
and `build_batches` (`src/spec_knowledge/requirement_plan.rs`) keeps a
`Dependency` edge only when both ends are `Ready`. A superseded block is never
`Ready`, so an edge from `## Dependencies: REQ-X` to a dependent block vanishes
as soon as `REQ-X` is replaced by `REQ-X-V2` — the dependent floats up to the
first batch. amberlog (2026-09-29) has 45 such edges: 9 blocks were
superseded, and most dependents still name the original id. `requirements plan`
reported 5 batches with `REQ-AMB-STO-MANIFEST-COMMIT` in batch 1; resolving
each dependency through the `supersedes:` chain gives the 9 layers the
downstream plan derived by hand (`amberlog-spec`
`campaigns/first-release/increments-plan.md` §5), with that block in layer 4.
The governance lint already warns `references-superseded` for such ids, but a
warning does not repair the ordering, and a batch list that is wrong without
any error is worse than one that fails.

**Gap 6 — `guard` checks every contract's boundaries against every change.**
`cmd_guard` (`src/main.rs`) resolves one change set for the whole commit and
then calls `verify_with_changes(code, &effective_changes)` for *each* spec in
`--spec-dir`; the boundaries verifier checks every changed path against that
spec's `Allowed changes` / `Forbidden` lists. A change that belongs to a
different contract, or to no contract at all, is therefore reported as out of
bounds by every contract that does not happen to allow it. In amberlog, with a
single contract (`REQ-AMB-ING-ACK-RESPONSE`, scoped to the ingest and server
crates) moved into `specs/`, a staged edit to `crates/query/src/lib.rs`,
`docs/…`, `tools/…` or `.gitlab-ci.yml` failed guard as "not covered by any
allowed boundary"; so did the commit that moved the contract itself (neither
`specs/task-amb-ing-ack-response.spec.md` nor its `specs/roadmap/` source path
is in its allow-list). All 17 contracts forbid `knowledge/**`, so once any of
them is active, every `requirements supersede` commit is blocked too. Narrowing
`Allowed changes` — which is what the contract author is told to do — makes the
failure more frequent, not less. The downstream workaround is
`agent-spec guard --change-scope none` in both pre-commit and CI, which keeps
lint and test verification but drops boundary enforcement entirely.

**Gap 7 — guard runs one `cargo test` per scenario, sequentially.**
`TestVerifier::verify` spawns `cargo test -q [-p <package>] <filter>` once for
every bound scenario, and `cmd_guard` verifies the specs one after another. In
amberlog (17 contracts, 318 scenarios, 305 of them bound to one package) a
full guard takes about 26 minutes: 51 tests that each take longer than 20 s
(system tests waiting on flush intervals, plus one shortened soak) account for
about 15 minutes, and the remaining ~285 invocations about 11 minutes, of
which roughly a second each is per-invocation overhead (cargo's up-to-date
check, and in amberlog a remote build wrapper's sync and ssh round trip), not
test time. The tests themselves
would run in a few minutes as one `cargo test -p <package>`: libtest runs a
binary's tests in parallel and accepts several filters at once. At this cost
guard cannot stay in a pre-commit hook; the downstream workspace moved it to
CI and runs `agent-spec lint` plus a selector-existence script locally, so a
contract whose tests are red is now only caught after push. Narrowing guard to
the changed contracts does not help when most scenarios share one package.

**Gap 8 — an `uncertain` verdict carries no reason.**
`test_run_verdict` returns `uncertain` whenever cargo exits non-zero before any
`running N tests` line appears, and there is no timeout. That lumps together a
compile error, a toolchain failure and a test runner that never started: in
amberlog (2026-09-30) a remote build wrapper failed to start under load (exit
code 255, no cargo output), one scenario came back `uncertain`, and the commit
was blocked; re-running the scenario alone passed. `guard` prints only the
per-spec summary (`… 1 uncertain`), so telling a flaky runner from a real
build failure means re-running by hand. The exit code and the tail of the
output are captured in `Evidence::TestOutput` but never shown by guard.

## Motivation

agent-spec's value is that a green gate means "the contract holds". Gap 2 and
Gap 4 break that promise silently; Gap 1 and Gap 3 break it loudly and push
users into workarounds (the downstream workspace now runs a shell script that
parses `lint-knowledge` text output and waives `requirement-uncovered` when a
single staged spec exists). Workarounds of that kind rot: they depend on the
text format and must be re-derived by every adopter.

## Goals

- A requirement whose contract sits in `specs/roadmap/` is reported as
  *scheduled*, not *uncovered*, and does not fail `lint-knowledge --gate`.
- A scenario whose bound test did not execute is never reported as `pass`.
- A scenario can name the cargo features its test needs.
- A scenario can declare itself long-running so that per-commit gates defer it
  while an explicit verification run still executes and records it.
- A dependency on a superseded requirement orders work after its current
  replacement, never after nothing.
- `guard` holds each contract to its own boundaries without failing commits
  that only touch other contracts' territory.
- A full guard costs about one `cargo test` per package, not one per scenario.
- A non-passing test verdict says why, without re-running it by hand.

## Non-Goals

- Changing the governance state machine or what `accepted` means.
- Supporting non-cargo test runners (the selector extensions are cargo-only,
  like the existing `Package:` field).
- Scheduling or running soak tests; agent-spec only classifies and reports.

## Decision

**D1. Staged coverage.** `build_requirement_plan` additionally indexes
`specs/roadmap/` (the staged tier `requirements status` already computes).
`validate_requirement_plan` then classifies each ready requirement:

| active specs | staged specs | diagnostic | severity |
|---|---|---|---|
| 1 | 0 | — | — |
| 0 | 1 | `requirement-staged` ("scheduled, not yet active") | Info |
| 0 | 0 | `requirement-uncovered` (unchanged) | Error |
| ≥2 | any | `requirement-multiple-specs` (unchanged) | Error |
| 0 | ≥2 | `requirement-multiple-staged-specs` | Error |
| 1 | ≥1 | `requirement-staged-and-active` (promotion left a copy behind) | Warning |

`lint-knowledge` forwards the new diagnostics like the existing ones.
`test-obligations` lists staged specs as the obligation source when no active
spec exists, marked `staged`, so unimplemented obligations stay visible.
Staged coverage is on by default, as the README already describes
`specs/roadmap/`; there is no opt-in flag.

**D2. Ignored is not executed.** `test_run_verdict` takes the per-binary
`test result:` summary instead of `running N tests`: executed = passed +
failed, and ignored is tracked separately. If executed is 0 and ignored is
greater than 0, the verdict is `skip` with the evidence note "bound test is
#[ignore]d". If some matched tests ran and others were ignored, the verdict is
computed from the executed ones and the ignored count is recorded in the
evidence. A malformed or missing summary falls back to `uncertain`, never to
`pass`.

**D3. `Features:` / `特性:` selector field.** `TestSelector` gains
`features: Option<String>` (comma-separated, cargo syntax). When present,
`build_cargo_test_args` appends `--features <value>`. The field is optional and
additive; existing selectors are unaffected.

**D4. `Mode: soak` / `模式: soak`.** A scenario-level mode alongside
`optimize`. `guard` and `lifecycle` do not run soak scenarios; they report them
under a new summary count `deferred`, which does not make `is_passing` false.
`agent-spec verify --include-soak` (and `lifecycle --include-soak`) runs them,
passing `-- --include-ignored` to cargo so the test may stay `#[ignore]` for
plain `cargo test`. Strict acceptance paths (`resolve-ai`, and `lifecycle`
with `--review-mode strict`) treat a deferred soak scenario as non-passing
unless it has a recorded result, so a release gate cannot close on an unrun
soak test.

`Mode:` is the only field that defers a scenario. The selector's free-form
`Level:` value stays descriptive; `agent-spec lint` warns
`soak-level-without-mode` when a selector says `Level: soak` and its scenario
does not say `Mode: soak`, so the two statements cannot drift apart. In the
same change, `lint` warns `unknown-scenario-mode` and `unknown-review-mode` for
any `Mode:` / `Review:` value it does not recognise, instead of silently mapping
it to `Standard` / `Auto`, so the next extension of either field is not silent.

**D5. Dependencies resolve through supersession.** `RequirementNode` carries
`supersedes`. `RequirementGraph::current_dependencies` resolves each
`## Dependencies` id: while the named block is `status: superseded`, follow the
blocks whose `supersedes:` names it; stop at the first block that is not
itself superseded. So `REQ-X` (superseded) → `REQ-X-V2` (accepted) →
`REQ-X-V3` (proposed) resolves to `REQ-X-V2`: a proposed successor never
replaces an accepted block. A superseded block with several successors was
split, and a dependency on it resolves to every successor, each followed by
the same rule. The declared ids stay as written in the graph
(`requirements export` round-trips them unchanged); resolution is a view used
by:

- `requirements plan`: `Dependency` edges start at the resolved block, and
  carry `declared_as: <superseded id>` when resolution rewrote them; batches
  are computed from those edges.
- `requirements work-units`: `depends_on` lists the resolved ids (and
  `draft-specs` inherits them).
- `requirements graph` cycle detection, so a cycle that only closes through a
  replacement is reported as `dependency-cycle` instead of silently removing
  its blocks from every batch.

A dependency that resolves to the depending block itself (it named its own
predecessor) is dropped. A superseded block with no successor, or on a looping
`supersedes:` chain, stays unresolved: the edge keeps its declared id (and, as
today, drops out of batching), and the graph reports the new Error
`dependency-supersession-unresolved` on each non-superseded dependent — an
Error, because a batch list that is silently wrong is what this decision
exists to remove. `references-superseded` is unchanged: resolution
keeps the plan correct, and the lint still asks authors to update the
declaration.

**D6. Guard checks boundaries per contract, against the changes that concern it.**
`cmd_guard` stops applying every contract's boundaries to the whole change set.
A contract's boundaries — `Forbidden` and `Allowed changes` alike — apply only
to a commit that *touches its territory*: at least one changed path matches
the contract's allow-list, or is the contract file itself (or its move source,
for a rename out of `specs/roadmap/`). For each touched contract:

- every changed path is checked against its `Forbidden` list, so an
  implementation commit still cannot edit what the contract forbids;
- a changed path is checked against its `Allowed changes` list when the path
  is *attributed* to it: the path matches that allow-list, is the contract file
  or its move source, or matches no active contract's allow-list at all
  (*unowned*).

An unowned path in a commit that touches any contract's territory is therefore
out of bounds for each touched contract, with the same Error as today; a
multi-contract commit is held to the same rule as a single-contract one. A
commit that touches no contract's territory — documentation, CI configuration,
a governance commit that only edits `knowledge/` — is not boundary-checked and
passes silently. `guard --strict-boundaries` keeps today's behavior for
workspaces that want every commit to stay inside every contract.

**D7. Batch test selectors per package.** Before verifying, guard (and
`lifecycle` / `verify` over several scenarios) groups the bound selectors by
`(package, features)` and runs one `cargo test [-p <package>]
[--features …] --no-fail-fast -- <filter>…` per group, without `-q`, so
libtest prints one `test <path> ... ok | FAILED | ignored` line per test.
Each scenario's verdict is then computed from the tests whose path contains
its filter (cargo's substring semantics), with D2's rules: none matched →
`skip`; all matched ignored → `skip` with the ignored note; any matched
failed → `fail`; otherwise `pass` (or `pending_review`). A group whose
cargo run produced no per-test lines at all (build failure) marks all its
scenarios `uncertain`, as today. `duration_ms` becomes the sum of the matched
tests' reported times when libtest reports them, else the group's wall time
divided evenly and flagged as estimated. Isolation stays the default:
the batched run passes `--test-threads=1` unless the new flag
`--test-threads <N>` (or a `[verify] test_threads` setting) says otherwise,
so tests validated one at a time are not suddenly run concurrently; the
per-invocation overhead disappears either way. Batching is the default from
the release that ships it; `--no-batch` keeps the current
one-invocation-per-scenario behavior. guard does not also verify specs in
parallel: grouping already merges selectors across specs, so each package is
built and run once per guard.

**D8. Say why a test verdict is not a pass.** For every scenario that is
`fail` or `uncertain`, guard prints the scenario name, the selector, cargo's
exit status (code or signal) and the last lines of the combined output
(already in `Evidence::TestOutput`), below the per-spec summary. `uncertain`
additionally distinguishes "cargo reported a compile error" (the output has an
`error[E…]` / `could not compile` line) from "the runner exited without cargo
output" (anything else), because the second is usually an environment
problem worth one retry and the first never is. agent-spec does not retry on
its own and offers no retry flag; the distinction lets the caller decide.

## Compatibility

- CLI and public API: D1 adds three diagnostic codes and relaxes one outcome
  (0 active + 1 staged: Error → Info). D2 changes an outcome that was wrong
  (ignored → `pass` becomes `skip`); specs that were green only because of
  ignored tests will turn red under guard — intended, and called out in the
  release notes. D3 and D4 are additive, except that D4's `lint` warnings can
  appear on existing specs that carry an unrecognised `Mode:` / `Review:` value
  or `Level: soak`.
- D5 changes `requirements plan` batches, `work-units` `depends_on` and
  `requirements graph` cycle diagnostics for any workspace whose
  `## Dependencies` name superseded blocks — intended, since the previous
  output was wrong. Plan JSON edges gain an optional `declared_as` field;
  graph JSON nodes gain an optional `supersedes` field. Workspaces without
  superseded dependencies see identical output (agent-spec's own
  `requirements plan` output is byte-identical). A workspace whose
  dependencies name a superseded block with no successor, or a looping
  `supersedes:` chain, now fails the graph and knowledge gates.
- D6 relaxes boundary enforcement: a commit that touches no active contract's
  territory is no longer boundary-checked, and `Forbidden` applies only to
  commits that touch the forbidding contract's territory.
  `guard --strict-boundaries` restores the previous check.
- File formats: the selector gains one optional field; scenarios gain one
  `Mode` value. Older agent-spec versions do not reject it: the parser maps any
  mode other than `optimize` to `Standard` (`src/spec_parser/parser.rs`), so an
  older guard would run a soak scenario as an ordinary one. Workspaces that
  adopt `Mode: soak` must pin an agent-spec version that understands it.
- D7 changes how tests are invoked, not what a verdict means; `--no-batch`
  restores the old invocation. Timing-sensitive tests are protected by the
  `--test-threads=1` default. D8 only adds output lines to non-passing
  guard runs.
- Existing specs and KLL artifacts: agent-spec's own `specs/roadmap/` contracts
  that satisfy accepted requirements would move from `requirement-uncovered`
  to `requirement-staged` — the diagnostic set of this repository changes
  accordingly.

## Migration Plan

- D1: none required. Downstream workspaces that built a waiver (amberlog's
  `tools/knowledge-gate.sh` and its `knowledge-lint` CI job) drop it and go
  back to `agent-spec lint-knowledge --gate`.
- D2: run `guard` once after upgrading; any spec that turns red was relying on
  an ignored test and needs a real test or `Mode: soak`.
- D3: downstream self dev-dependency workarounds may be removed after adding
  `Features:` to the affected selectors; keeping them is harmless.
- D4: none.
- D5: none required. Updating `## Dependencies` to the current ids remains
  good hygiene (and clears `references-superseded`), but the plan no longer
  depends on it. The first implementation (`e38c34f`) reports
  `dependency-supersession-unresolved` as a Warning and treats a split as
  unresolved; both change to the rule above.
- D6: downstream workspaces that run `guard --change-scope none` (amberlog's
  pre-commit hook and CI) can drop the flag and regain boundary enforcement.
- D7/D8: none required. amberlog can move guard back into its pre-commit hook
  once a full run fits in a few minutes (it currently runs guard only in CI and
  `agent-spec lint` plus `tools/contract-test-names.sh` locally).

## Security Considerations

D2 closes a verification-integrity hole: a contributor could make any scenario
pass by adding `#[ignore]` to its test. D3 lets a spec author choose features
for test builds only; it grants no capability that editing `Cargo.toml` does not
already grant. D4 adds a deferral that strict acceptance paths refuse to treat
as a pass. D6 stops checking commits that touch no contract's territory, so a
change that would be out of bounds inside an implementation commit passes when
committed on its own; the split stays visible in history, and
`--strict-boundaries` closes it for workspaces that need that guarantee.

## Privacy Considerations

None. No new data is collected, logged or transmitted; diagnostics carry the
same requirement ids and paths as today.

## Risks and Assumptions

### Assumptions

- libtest's `test result: ok. N passed; M failed; K ignored; …` summary line is
  stable. Invalidated if: libtest changes the summary format; mitigated by the
  D2 fallback to `uncertain`.
- `specs/roadmap/` remains the staged tier's only location. Invalidated if: a
  configurable staged directory is introduced; D1 then reads the same setting.

- libtest prints one `test <path> ... <result>` line per test when run
  without `-q` (D7). Invalidated if: the format changes or a custom test
  harness (`harness = false`) prints something else; mitigated by falling back
  to per-scenario invocation for a group whose output cannot be parsed.

### Risks

- D1 lets a requirement stay "scheduled" indefinitely. Mitigation:
  `requirements status` already shows the tier; a later proposal may add an age
  threshold.
- D2 turns previously green specs red on upgrade. Mitigation: release note plus
  a one-line evidence message naming the ignored test.
- D7 runs a package's bound tests in one process tree; a test that leaks
  global state (environment variables, a fixed port, the working directory)
  into later tests can change their outcome even with `--test-threads=1`.
  Mitigation: `--no-batch`, and the per-test result lines make the culprit
  visible.
- D6 lets an out-of-bounds change land by committing it separately from the
  implementation it belongs to. Mitigation: `--strict-boundaries`; contract
  review reads the commit range, not single commits.

## Consequences

Good, because a green `lint-knowledge --gate` and a green `guard` can hold at
the same time throughout a spec-ahead-of-implementation workflow, and because
no scenario can pass without its test having executed.
Bad, because the diagnostic vocabulary grows by four knowledge codes, three
spec lint warnings and one summary count, and D2, D5 and D6 change behavior on
upgrade.

## Alternatives Considered

- Downstream waiver scripts (what amberlog runs today): rejected as the
  long-term answer because they parse text output and every adopter must
  rebuild them.
- `allow_failure` on the knowledge gate in CI: rejected because it also hides
  every other Error the gate exists to catch.
- Keep contracts in `specs/` and bind placeholder tests: rejected by the
  downstream construction rules (placeholders must not reach implementation)
  and by D2's own logic — a placeholder that does not run must not pass.
- Treat `Review: human` as the soak mechanism: rejected because the test still
  runs under guard and `pending_review` passes in `auto` mode.

## Prior Art

- libtest's `--ignored` / `--include-ignored` split between default and
  explicitly requested tests.
- cargo's `required-features` for targets that need features.
- agent-spec's own `requirements status` tiers (active / staged / archived).

## Resolved Questions

Settled in human review on 2026-09-30; the Decision section above states
each outcome.

- D1: staged coverage is the default, with no opt-in flag.
- D1: `requirement-staged` is Info. Both levels leave `--gate` green;
  staying one round ahead of implementation is a normal state, and
  `requirements status` already shows the tier.
- D2: when a filter matches several tests and only some are ignored, the
  verdict comes from the executed ones and the ignored count is recorded.
  A filter whose only matches are ignored is still `skip`.
- D4: `lint` warns on unrecognised `Mode:` and `Review:` values in the same
  change; `Review:` has the same silent fallback as `Mode:`.
- D4: `Mode: soak` is the deferral switch. `Level: soak` stays descriptive, and
  `lint` warns when it appears without `Mode: soak`.
- D5: a split (several successors) resolves to every successor; no successor
  or a looping chain is an Error, not a Warning.
- D5: a dependency on a replacement that is not `Ready` still drops out of
  batching. That rule predates this proposal and applies to every dependency,
  so changing it is left to a follow-up proposal.
- D6: an unowned path is an Error when the commit touches any contract's
  territory, and is not reported otherwise.
- D6: `Forbidden` applies only to commits that touch the forbidding contract's
  territory. Distinguishing commits by who makes them would need an actor
  field, which ADR-001 rules out.
- D7: batching is the default immediately; guard does not verify specs in
  parallel.
- D8: agent-spec offers no retry flag; retries stay with the caller.

## Unresolved Questions

None.

## Produces: ADR-006

Accepted 2026-09-30. All eleven unresolved questions were put to the human as
structured multiple-choice and answered; `## Resolved Questions` above lists
the rulings, and the Decision section states them. The produced decision
records D1–D8 as settled, including the two rulings that change the first D5
implementation (a split resolves to every successor; an unresolved dependency
is an Error) and the rule that a contract's boundaries apply only to commits
that touch its territory.
