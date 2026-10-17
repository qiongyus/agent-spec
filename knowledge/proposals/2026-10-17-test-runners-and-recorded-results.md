---
kind: proposal
id: LEP-008
title: "Test Runners and Recorded Test Results"
status: proposed
liveness: n/a
tags: [verify, guard, ci]
---

# Test Runners and Recorded Test Results

## Context

LEP-007 D7 made guard run one `cargo test` per (package, features, soak)
group instead of one per scenario. That removed the per-invocation overhead,
but the groups still run one after another, and inside each group cargo runs
the package's test binaries one after another; `--test-threads N` only sets
libtest's parallelism inside one binary (`src/spec_verify/test_batch.rs`,
`group_args` and `execute_plan`, at v1.5.0).

The downstream workspace (amberlog) now has 43 contracts and 931 bound
selectors. Almost all of them bind system tests in one package
(`amberlog-server`, dozens of test binaries). Its CI runs the same tests twice:

| CI job | what it runs | wall time (2026-10-16, runner with an 8-CPU limit per job) |
|---|---|---|
| `test` | every test in the workspace with cargo-nextest, one process per test, 8 at a time across all binaries, JUnit report written | 47 min (1337 tests, pipeline #462) |
| `knowledge` | `agent-spec guard --test-threads 4`: lint, then the bound tests again through `cargo test`, binaries in sequence, 2 builds of the server package (with and without the `fault-injection` feature) | 92 min (pipeline #460, 39 contracts) → 114 min (#462) |

`knowledge` grows with every contract moved into `specs/` (22 min at 17
contracts on 2026-10-01). It is now the longest job, and because it re-runs
tests that `test` has already run in the same pipeline, most of its cost is
duplicate work. Two gaps cause this.

**Gap 1 — guard can only execute tests through `cargo test`.** The runner is
the literal `cargo` (`CargoRunner`, `src/spec_verify/test_verifier.rs`); the
`TestCommandRunner` trait is swapped only in unit tests. A workspace whose
test binaries are many and slow cannot use a runner that parallelises across
binaries, such as cargo-nextest, even though nextest's filter expressions can
express exactly guard's selection (`test(~<filter>)` is libtest's substring
match) and nextest writes a machine-readable JUnit report.

**Gap 2 — guard cannot take results from a run that already happened.** CI
pipelines commonly run the full test suite in an earlier job and keep a JUnit
report. guard has no way to read it: it always builds and runs the bound tests
itself. The tests guard needs are a subset of what the earlier job ran.

## Motivation

A contract gate that takes two hours stops being a per-push gate. Users then
either drop it, run it on a schedule, or split it across machines; each of
those weakens the promise that a green pipeline means every active contract
holds. The fix should keep guard's verdict rules (LEP-007 D2, D3, D8) exactly
as they are, and change only where test outcomes come from.

## Goals

- guard, verify and lifecycle can execute bound tests with cargo-nextest,
  with the same grouping, the same selection and the same verdicts as
  today's cargo runner.
- guard, verify and lifecycle can judge scenarios from one or more existing
  JUnit reports without building or running anything.
- A verdict taken from a report never claims more than the report shows: a
  test that is absent from the report is never a pass, and a scenario that
  requires features the recorded run did not enable is never a pass.
- The default behaviour (cargo runner, no reports) is unchanged.

## Non-Goals

- Timeouts, retries or flaky-test policy. Whatever the runner or the
  report says is the outcome.
- Proving that a report belongs to the commit being checked. The caller
  supplies the report and owns its provenance (see Risks).
- Runners other than cargo test and cargo-nextest, and report formats other
  than JUnit XML.

## Decision

**D1. `--runner cargo|nextest`** (verify, lifecycle, guard; default `cargo`).
With `nextest`, each batch group runs as one invocation:

```
cargo nextest run [-p <package>] [--features <F>] --no-fail-fast
    [--test-threads <N>] [--run-ignored all]            # soak groups only
    --tool-config-file agent-spec:<tmp>/nextest.toml --profile agent-spec
    -E 'test(~<filter1>) | test(~<filter2>) | …'
```

- The tool config file defines a profile `agent-spec` whose only setting is a
  JUnit path in a temporary directory. A custom nextest profile inherits the
  project's `default` profile, so the project's own test groups and
  overrides still apply.
- The group's outcome is read from that JUnit report (D3), not from the
  human-readable output. The same `attribute` rules then judge each scenario.
- `--test-threads` keeps its meaning of "tests at a time"; for nextest that
  is across all binaries of the group.
- If `cargo nextest` is not installed, every scenario of the run is
  `uncertain` with the reason `runner-unavailable`.
- nextest does not run doctests. A selector that only matches a doctest
  matches nothing under this runner and is `skip` (`zero-matched`), as it
  would be for any missing test. The `cargo` runner remains the default for
  that reason.
- `--no-batch` with `--runner nextest` is rejected at argument parsing.

**D2. `--test-results <junit.xml>`** (repeatable; verify, lifecycle, guard).
When given, guard builds no group and runs nothing. It loads every report,
and judges each scenario from the test cases that match its selector:

- **Package.** nextest names each test suite by binary id: `<package>` for
  the library unit tests, `<package>::<target>` for other targets. A
  selector with `Package: P` matches only test cases whose binary id is `P`
  or starts with `P::`. A selector without a package matches any test case.
- **Filter.** The test case name contains the filter (libtest's substring
  rule, as in LEP-007 D7).
- **Outcome.** A test case with `<failure>` or `<error>` failed; one with
  `<skipped>` did not execute; any other test case passed. The existing
  `attribute` rules then apply unchanged: nothing matched → `skip`
  (`zero-matched`); nothing executed → `skip` (`all-ignored`); any failure →
  `fail`; otherwise `pass` (or `pending_review` under `Review: human`).
  nextest leaves ignored tests out of its report, so a selector that matches
  only ignored tests comes out `zero-matched`, which is still not a pass.
- **Duplicates.** If several reports contain the same binary id and test
  name, any failure wins; otherwise the test passed.
- **Soak.** Soak scenarios stay `deferred` unless `--include-soak`; with it,
  they are judged from the report like any other scenario.
- `--test-results` together with `--runner` or `--no-batch` is rejected at
  argument parsing.

**D3. `--test-results-features <pkg/feature,…>`** (with `--test-results`).
A report does not record which cargo features the run enabled, so the caller
declares them, in cargo's workspace form (`amberlog-server/fault-injection`).

- A scenario that declares `Features: F` with `Package: P` is judged from
  the reports only if every feature in F is declared as `P/<feature>`.
  Otherwise its verdict is `uncertain` with the reason
  `results-feature-mismatch`, and guard prints the missing features.
- A scenario that declares no features is judged from the reports whatever
  features the recorded run enabled. Cargo features are additive, so a
  test that passes with extra features enabled is accepted as passing
  without them. This is a deliberate relaxation of how a live run judges such
  a scenario, and it is the reason this flag exists: the caller states the
  feature set in the command line, so the verdict's conditions are visible.
- guard's summary line names the source of the verdicts:
  `N spec(s) passed (results from 1 report: 1337 test cases; features: amberlog-server/fault-injection)`.

**D4. One outcome type for all sources.** The cargo runner's libtest lines,
the nextest runner's JUnit report and supplied reports all produce the same
per-test record (binary id where known, test path, outcome, duration), and
the same `attribute` function judges scenarios from it. Verdict rules stay
in one place, so the three sources cannot drift apart.

## Compatibility

- CLI and public API: three new flags on verify, lifecycle and guard. The
  defaults reproduce v1.5.0 exactly. No flag changes meaning.
- File formats: none. Specs, selectors and reports in the existing
  `--format json` output are unchanged; a scenario's evidence gains an
  optional `source` field (`cargo`, `nextest`, or the report path).
- Existing specs and KLL artifacts: unaffected.

## Migration Plan

N/A -- no migration required. Projects opt in per invocation.

For amberlog, the intended use after release is: the `test` job keeps
writing its JUnit report and also publishes it as a job artifact; the
`knowledge` job depends on `test`, drops its own build and test services,
and runs
`agent-spec guard … --test-results target/nextest/ci/junit.xml --test-results-features amberlog-server/fault-injection`.

## Security Considerations

A report is trusted input: whoever can write it can turn guard green. guard
reads only the paths it is given and does not fetch reports. In CI the report
must come from a job in the same pipeline, through the CI system's artifact
passing, not from a cache or a shared directory.

## Privacy Considerations

JUnit reports can contain test output (`<system-out>`, `<system-err>`).
guard prints that output only for failing scenarios, as it already does for
live runs, and stores nothing new.

## Risks and Assumptions

### Assumptions

- nextest keeps its JUnit naming (test suite = binary id, test case = test
  path) across 0.9 releases. Invalidated if: nextest changes the scheme; the
  parser then fails loudly (no test case matches a package) rather than
  passing silently.
- The run that produced a report used the same source tree as the guard
  invocation. Invalidated if: the caller passes a stale report; see Risks.

### Risks

- Stale report: a report from an older commit makes guard judge old code.
  Mitigation: documentation requires same-pipeline artifacts, and guard's
  summary line names the report it used. A provenance check (for example a
  commit id written next to the report) is left to a later proposal.
- Over-acceptance through D3: a test whose behaviour differs only when an
  extra feature is enabled could pass in the report and fail in a live run
  without the feature. Mitigation: the feature set is explicit on the
  command line and in the summary; projects that cannot accept this keep
  running guard live.
- Filtered runs: if the earlier job ran with a filter or partition, the
  scenarios whose tests it skipped are `zero-matched` and fail the gate.
  This is the intended, honest result.

## Consequences

Good, because a project that already runs its whole suite once per pipeline
no longer pays for a second run: amberlog's `knowledge` job would drop from
about 114 minutes to a few minutes, and the pipeline from about 170 to about
60 minutes.
Good, because projects that run guard live (locally, or for soak) can use
nextest's cross-binary parallelism without changing their contracts.
Good, because verdict rules stay in one function for every source.
Bad, because guard gains a mode in which its verdict is only as good as the
report it is handed.
Bad, because D3 accepts, for scenarios without declared features, a run that
enabled extra features — a weaker condition than a live run.

## Alternatives Considered

- Raise `--test-threads` only. Downstream measured little gain locally
  (7 min 50 s at 16 threads versus 9 min at 4, 17 contracts): binaries still
  run in sequence and slow system tests dominate. It is the interim step
  downstream takes, not a fix.
- Shard guard across several CI jobs by contract. Each shard rebuilds the
  workspace, and the shards compete for the same runner slots the duplicate
  run already wastes.
- A `cargo` wrapper script that translates guard's `cargo test` calls into
  nextest and fakes libtest output lines. Rejected: guard's verdicts would
  depend on a text imitation outside agent-spec.
- Make D3 strict (scenarios without features need a run without extra
  features). Rejected for this proposal by the downstream decision of
  2026-10-17 (qiongyus): the CI test run enables the test-only feature for
  the whole workspace, so a strict rule would leave D2 unusable there.

## Prior Art

- cargo-nextest filter expressions and JUnit reports; tool config files
  (`--tool-config-file`) exist for tools that drive nextest.
- GitLab and GitHub CI test-report ingestion, which also treats JUnit as the
  exchange format between the job that runs tests and the jobs that read
  results.

## Unresolved Questions

- Flag names: `--runner`, `--test-results`, `--test-results-features`, or
  a shorter pair.
- Should guard warn when `--test-results` is given together with
  `--change` / `--change-scope` options that limit which specs it checks?
  (Not needed downstream, which uses `--change-scope none`.)
- Implementation order: D2 + D3 + D4 first (the CI win), then D1. Both in
  one release (1.6.0) or two.

## Produces

- On acceptance: ADR-007 (test outcome sources) and governed requirements
  for D1, D2, D3 (D4 is an implementation constraint recorded in ADR-007).
