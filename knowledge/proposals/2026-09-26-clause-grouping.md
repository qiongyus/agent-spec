---
kind: proposal
id: LEP-006
title: "Clause Grouping"
status: proposed
liveness: n/a
tags: [knowledge, lint, authoring]
---

# Clause Grouping

## Context

A requirement document's `## Requirements` section is a flat list: one
normative clause per line, in whatever order the author appended them. The
same holds for `## Scenarios`. Nothing in the format lets an author say
"these five clauses describe the shape of the response, those fifteen
describe what happens at each step of the validation chain" — the reader
reconstructs that structure from prose, or does not.

The gap is measurable. A downstream workspace (amberlog, 2026-09-26) carries
17 requirement blocks with 26 to 74 clauses each; the largest has 74 clauses
and 18 scenarios. Its authors tried the obvious Markdown answer — a `###`
heading between groups — and hit the parser: `extract_requirements` treats
every non-empty, non-comment line as a clause, so each heading became a
clause with no id and no BCP-14 keyword. On a 26-clause block the clause count
rose to 28 and lint raised two `requirement-needs-id` plus two
`requirement-bcp14-keyword` findings. Bold labels behaved the same way. The
only marker the parser ignores is an HTML comment, which is invisible in every
rendered view and therefore useless as a reading aid.

The ordering problem the headings were meant to solve is real. Blocks in that
workspace had grown by accretion: each governance ruling appended or inserted
clauses near whatever they amended, so a 404 mapping sat at clause 14, the
matching 401 mapping at clause 21, and the validation-chain clause they both
hang off at clause 19. Reviewers reading the block for acceptance had no way
to see the intended "envelope shape first, then step-by-step mapping" order,
because the format had no way to express it.

## Motivation

Requirement documents are the layer humans read most carefully — acceptance
is a human act on exactly this text — and the one layer whose readability the
toolchain currently makes worse rather than better. Task contracts already
have `Rule:` grouping for scenarios; requirements have nothing above the
clause. As blocks grow past twenty clauses, the flat list stops being a
document and becomes a lookup table.

## Goals

- An author can put a `###` (or deeper) heading inside `## Requirements` and
  `## Scenarios` and have it read as a heading: not a clause, not counted,
  not linted as one.
- The grouping is visible to tooling, not only to renderers: `requirements
  graph` exposes the enclosing heading on each clause and scenario so
  downstream consumers (status reports, draft-specs, MCP) can use it.
- Documents without headings are untouched: identical clause extraction,
  identical diagnostics, byte-identical graph output.

## Non-Goals

- Hierarchy semantics. `###` and `####` are both "the nearest heading"; the
  graph carries one `group` string, not a path. A later proposal may add
  levels once there is a consumer that needs them.
- Coverage or status reporting per group.
- Projecting groups into the YAML export dialects.
- Any change to `Rule: <clause-id>` attribution (LEP-003 / ADR-004), which
  keeps its meaning whether or not it is written as a heading.

## Decision

Treat any line beginning with `#` inside `## Requirements` or `## Scenarios`
as a grouping heading. Level-2 headings never reach these sections (the
section splitter consumes them), so no ambiguity arises. The heading's text is
the group label; an empty heading clears the current group. Clauses and
scenarios carry the nearest preceding label as an optional `group`. Inside
`## Scenarios`, a heading whose text starts with `Rule:` is attribution, not a
group, and leaves the current group unchanged.

The change is confined to `extract_requirements` and the graph's scenario
parser plus one optional field on the clause and scenario views; no lint rule
changes meaning and no new diagnostic is introduced.

## Compatibility

- CLI and public API: additive. `requirements graph --format json` gains an
  optional `group` on clause and scenario objects, serialized only when
  present.
- File formats: additive. Headings inside the two sections were previously
  mis-parsed as clauses; they now parse as headings. A document that relied on
  a `#`-prefixed line being a clause would lose that clause — no such document
  exists in this repository, and the behaviour was a defect rather than a
  feature.
- Existing specs and KLL artifacts: unaffected. Every document in
  `knowledge/` produces the same clauses, diagnostics, and graph output before
  and after.

## Migration Plan

N/A -- no migration required. Authors may add headings to existing documents
at their own pace; the requirement block produced by this proposal does so
itself.

## Security Considerations

None. The parser reads lines it already read and records a label; no new
execution path, file write, or external input.

## Privacy Considerations

Heading text appears in graph output alongside clause text that is already
there. No new paths, metadata, or telemetry.

## Risks and Assumptions

### Assumptions

- Authors want a single label per clause. Invalidated if a consumer needs the
  full heading path (e.g. "B · Mapping / Authentication"), in which case the
  `group` field would need to become a list.
- `#` at the start of a trimmed line is never legitimate clause text.
  Invalidated if a clause needs to begin with a literal `#`; it can be
  prefixed with its `[REQ-…]` id, which every clause should carry anyway.

### Risks

- Headings that smuggle assertions: an author writes "### The API MUST be
  fast" and the obligation escapes lint. Mitigation: headings are labels by
  convention and review; a follow-up rule could warn on BCP-14 keywords in a
  heading if the pattern appears.
- Group labels drift from content as clauses are re-ordered. Mitigation: the
  label is adjacent to what it labels, unlike a frontmatter map; drift is
  visible in the same diff hunk.

## Consequences

Good, because a 74-clause block can be read as a document with sections
instead of a numbered list, and the section structure survives into the graph
rather than living only in the renderer.

Good, because the ordering discipline that motivated this — shape before
behaviour, behaviour in main-flow order, normal before exceptional — can now
be expressed in the file rather than described in a side document.

Bad, because the toolchain now has two kinds of structure inside
`## Scenarios` (`Rule:` attribution and grouping headings) that look alike and
mean different things; the `Rule:` exception must be documented wherever
grouping is.

Bad, because a single-level `group` will eventually be asked to become a
path, and that will be a second change to the same field.

## Alternatives Considered

- Keep HTML comments as the grouping marker — rejected: invisible in every
  rendered view, so it helps the parser and not the reader, which is the
  wrong way round.
- Introduce a dedicated syntax such as `Group: <label>` — rejected: Markdown
  already has headings, every renderer already draws them, and inventing a
  second spelling for "heading" would need its own lint and its own
  documentation.
- Split large requirement documents into several smaller ones — rejected as
  the general answer: it moves the problem into `## Dependencies` and file
  naming, and a 26-clause block about one error envelope is one requirement,
  not three.
- Carry the full heading path as a list — deferred, not rejected: no consumer
  needs it yet, and a string is trivially widened to a list later.

## Prior Art

- BDD `Rule:` grouping in the contract dialect (LEP-003 / ADR-004): explicit
  structure inside a scenario section, already parsed by trimming leading `#`.
- Use-case specifications (Cockburn): main success scenario first, extensions
  numbered by the step they branch from — the ordering that grouping makes
  writable here.
- ISO/IEC/IEEE 29148 §5.2.6 on organizing requirements so that related
  statements are co-located and their structure is visible.

## Unresolved Questions

- Should `group` be a single label (nearest heading) or the full heading
  path? This proposal ships the label and defers the path.
- Should a heading containing a BCP-14 keyword be a lint warning from day one,
  or only if the pattern appears in practice?

## Produces: ADR-005

Proposed 2026-09-26. The produced decision records the parsing rule, the
`Rule:` exception, the single-label choice, and the byte-identical output
guarantee for ungrouped documents.
