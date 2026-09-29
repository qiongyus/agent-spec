---
kind: proposal
id: LEP-007
title: "Staged Coverage and Honest Test Verdicts"
status: proposed
liveness: n/a
tags: [knowledge, verify, guard, requirements]
---

# Staged Coverage and Honest Test Verdicts

## Context

A downstream workspace (amberlog, 2026-09-28) accepted 17 requirement blocks in
one governance step and then drafted one task contract per block — 313
scenarios in total. Wiring those contracts into CI surfaced four gaps, and
ordering the first round's work from `requirements plan` surfaced a fifth. Each was
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

**D5. Dependencies resolve through supersession.** `RequirementNode` carries
`supersedes`. `RequirementGraph::current_dependencies` resolves each
`## Dependencies` id: while the named block is `status: superseded`, follow the
unique block whose `supersedes:` names it; stop at the first block that is not
itself superseded. So `REQ-X` (superseded) → `REQ-X-V2` (accepted) →
`REQ-X-V3` (proposed) resolves to `REQ-X-V2`: a proposed successor never
replaces an accepted block. The declared ids stay as written in the graph
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
predecessor) is dropped. A superseded block with no successor, with more than
one successor, or on a looping `supersedes:` chain stays unresolved: the edge
keeps its declared id (and, as today, drops out of batching), and the graph
reports the new Warning `dependency-supersession-unresolved` on each
non-superseded dependent. `references-superseded` is unchanged: resolution
keeps the plan correct, and the lint still asks authors to update the
declaration.

## Compatibility

- CLI and public API: D1 adds three diagnostic codes and relaxes one outcome
  (0 active + 1 staged: Error → Info). D2 changes an outcome that was wrong
  (ignored → `pass` becomes `skip`); specs that were green only because of
  ignored tests will turn red under guard — intended, and called out in the
  release notes. D3 and D4 are additive.
- D5 changes `requirements plan` batches, `work-units` `depends_on` and
  `requirements graph` cycle diagnostics for any workspace whose
  `## Dependencies` name superseded blocks — intended, since the previous
  output was wrong. Plan JSON edges gain an optional `declared_as` field;
  graph JSON nodes gain an optional `supersedes` field. Workspaces without
  superseded dependencies see identical output (agent-spec's own
  `requirements plan` output is byte-identical).
- File formats: the selector gains one optional field; scenarios gain one
  `Mode` value. Older agent-spec versions do not reject it: the parser maps any
  mode other than `optimize` to `Standard` (`src/spec_parser/parser.rs`), so an
  older guard would run a soak scenario as an ordinary one. Workspaces that
  adopt `Mode: soak` must pin an agent-spec version that understands it.
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
  depends on it.

## Security Considerations

D2 closes a verification-integrity hole: a contributor could make any scenario
pass by adding `#[ignore]` to its test. D3 lets a spec author choose features
for test builds only; it grants no capability that editing `Cargo.toml` does not
already grant. D4 adds a deferral that strict acceptance paths refuse to treat
as a pass.

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

### Risks

- D1 lets a requirement stay "scheduled" indefinitely. Mitigation:
  `requirements status` already shows the tier; a later proposal may add an age
  threshold.
- D2 turns previously green specs red on upgrade. Mitigation: release note plus
  a one-line evidence message naming the ignored test.

## Consequences

Good, because a green `lint-knowledge --gate` and a green `guard` can hold at
the same time throughout a spec-ahead-of-implementation workflow, and because
no scenario can pass without its test having executed.
Bad, because the diagnostic vocabulary grows by three codes and one summary
count, and D2 is a behavior change on upgrade.

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

## Unresolved Questions

- D1: should staged coverage be the default or opt-in (for example
  `--staged-counts-as-scheduled`)? Default-on matches the README's description
  of `specs/roadmap/`; opt-in avoids changing any existing workspace's output.
- D1: Info or Warning for `requirement-staged`?
- D2: when a filter matches several tests and only some are ignored, is
  "verdict from the executed ones" enough, or should mixed results be
  `uncertain`?
- D4: older versions silently treat `Mode: soak` as `Standard` (see
  Compatibility). Is a warning for unknown mode values worth adding in the same
  change, so the next such extension is not silent?
- D5: should an unresolved superseded dependency be an Error instead of a
  Warning? It degrades to today's behavior (the edge drops out of batching), so
  the batch list can still be wrong.
- D5: when the current replacement is not `Ready` (for example it is blocked
  on open questions), the edge still drops out of batching, as any dependency
  on a non-ready block does today. Should that pre-existing rule change too, or
  stay out of this proposal?

## Produces

- On acceptance, mint the produced decision or requirement ids here
  (inline form: `## Produces: ADR-NNN`).
