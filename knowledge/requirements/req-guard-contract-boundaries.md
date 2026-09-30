---
kind: requirement
id: REQ-GUARD-CONTRACT-BOUNDARIES
title: "Guard Per-Contract Boundaries"
status: accepted
liveness: auto
tags: [guard, boundaries, verify]
---

# Guard Per-Contract Boundaries

## Problem

guard 为整个提交解析一个改动集，然后对 `--spec-dir` 里的每个合约都用这整个
改动集做边界检查：任何不在某合约 `Allowed changes` 里的路径都被这个合约判为
越界。结果是属于另一个合约、或不属于任何合约的改动被每个不恰好允许它的合约
拦下。amberlog 只有一个合约（范围限于 ingest 与 server crate）处于 active 时，
改 `crates/query/src/lib.rs`、`docs/…`、`tools/…` 或 `.gitlab-ci.yml` 都报
「not covered by any allowed boundary」；把这个合约移入 `specs/` 的提交本身
也被拦下；17 个合约都禁止 `knowledge/**`，任一合约 active 后所有
`requirements supersede` 提交都被拦下。按指引收窄 `Allowed changes` 只会让
误报更多。下游最终以 `guard --change-scope none` 彻底关闭边界检查。

## Requirements

### 领地

[REQ-GUARD-CONTRACT-BOUNDARIES-TERRITORY] guard MUST 仅在至少一条改动路径匹配某合约的 `Allowed changes`、是该合约文件本身、或是该合约文件从 `specs/roadmap/` 移入时的源路径时，判定提交触及该合约的领地。

[REQ-GUARD-CONTRACT-BOUNDARIES-UNTOUCHED] guard MUST NOT 对未触及其领地的合约做任何边界检查。

### 触及领地时的检查

[REQ-GUARD-CONTRACT-BOUNDARIES-FORBIDDEN] 对于领地被触及的合约，guard MUST 用该合约的 `Forbidden` 检查提交中的每一条改动路径。

[REQ-GUARD-CONTRACT-BOUNDARIES-OTHER-CONTRACT] 匹配另一个 active 合约的 `Allowed changes`、且不匹配本合约 `Allowed changes` 的路径，MUST NOT 被本合约判为越界。

[REQ-GUARD-CONTRACT-BOUNDARIES-SELF] 合约文件本身及其从 `specs/roadmap/` 移入时的源路径 MUST NOT 被该合约判为越界。

[REQ-GUARD-CONTRACT-BOUNDARIES-UNOWNED] 不匹配任何 active 合约 `Allowed changes` 的路径，MUST 被每个领地被触及的合约判为越界，诊断与现有越界诊断相同。

### 兼容

[REQ-GUARD-CONTRACT-BOUNDARIES-NO-TERRITORY] 一个不触及任何 active 合约领地的提交，guard MUST NOT 报告任何边界诊断。

[REQ-GUARD-CONTRACT-BOUNDARIES-STRICT] 指定 `--strict-boundaries` 时，guard MUST 对每个 active 合约用整个改动集检查其 `Allowed changes` 与 `Forbidden`。

## Scenarios

以下场景的工作区都含两个 active 合约：合约 A 的 `Allowed changes` 为
`crates/ingest/**`，`Forbidden` 为 `knowledge/**`；合约 B 的
`Allowed changes` 为 `crates/query/**`。

### 领地

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-TERRITORY

Scenario: 移入 specs 的提交触及该合约领地
  Given 一个暂存提交只把合约 C 从 `specs/roadmap/` 重命名到 `specs/`，并修改 `knowledge/x.md`，合约 C 的 `Forbidden` 为 `knowledge/**`
  When 运行 guard --change-scope staged
  Then stderr 包含合约 C 对 `knowledge/x.md` 的越界诊断

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-UNTOUCHED

Scenario: 未触及领地的合约不检查
  Given 一个暂存提交只修改 `crates/query/src/lib.rs`
  When 运行 guard --change-scope staged
  Then 输出不包含合约 A 的任何边界诊断

### 触及领地时的检查

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-FORBIDDEN

Scenario: 实现提交不能改 Forbidden 路径
  Given 一个暂存提交修改 `crates/ingest/src/lib.rs` 与 `knowledge/x.md`
  When 运行 guard --change-scope staged
  Then 进程以非零退出码 exits，输出包含合约 A 对 `knowledge/x.md` 的越界诊断

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-OTHER-CONTRACT

Scenario: 跨合约提交各自在边界内
  Given 一个暂存提交修改 `crates/ingest/src/lib.rs` 与 `crates/query/src/lib.rs`
  When 运行 guard --change-scope staged
  Then 输出不包含任何边界诊断

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-SELF

Scenario: 合约晋升提交不被自身拦下
  Given 一个暂存提交把合约 C 从 `specs/roadmap/` 重命名到 `specs/`，合约 C 的 `Allowed changes` 为 `crates/server/**`
  When 运行 guard --change-scope staged
  Then 输出不包含合约 C 对其合约文件或源路径的越界诊断

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-UNOWNED

Scenario: 实现提交夹带无主路径时报越界
  Given 一个暂存提交修改 `crates/ingest/src/lib.rs` 与 `docs/guide.md`
  When 运行 guard --change-scope staged
  Then 进程以非零退出码 exits，输出包含合约 A 对 `docs/guide.md` 的越界诊断

Scenario: 跨合约提交夹带无主路径时各合约都报越界
  Given 一个暂存提交修改 `crates/ingest/src/lib.rs`、`crates/query/src/lib.rs` 与 `docs/guide.md`
  When 运行 guard --change-scope staged
  Then 输出包含合约 A 与合约 B 各自对 `docs/guide.md` 的越界诊断

### 兼容

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-NO-TERRITORY

Scenario: 纯文档提交不做边界检查
  Given 一个暂存提交只修改 `docs/guide.md` 与 `.gitlab-ci.yml`
  When 运行 guard --change-scope staged
  Then 输出不包含任何边界诊断

Scenario: 纯治理提交不被 Forbidden 拦下
  Given 一个暂存提交只修改 `knowledge/requirements/req-x.md`
  When 运行 guard --change-scope staged
  Then 输出不包含任何边界诊断

Rule: REQ-GUARD-CONTRACT-BOUNDARIES-STRICT

Scenario: 严格模式保留原行为
  Given 一个暂存提交只修改 `docs/guide.md`
  When 运行 guard --change-scope staged --strict-boundaries
  Then 进程以非零退出码 exits，输出包含合约 A 与合约 B 各自对 `docs/guide.md` 的越界诊断

## Dependencies

None.

## Source Trace

- proposal: LEP-007 Gap 6 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D6（边界只约束触及合约领地的提交；无主路径在这类提交中报 Error，其余不报；`--strict-boundaries` 保留原行为，2026-09-30）
- 实测证据: amberlog 2026-09-29 至 2026-09-30，单个 active 合约拦下 `crates/query/**`、`docs/**`、`tools/**`、`.gitlab-ci.yml` 与合约自身移入的提交；`knowledge/**` 的 Forbidden 拦下 supersede 提交；下游改用 `--change-scope none`
- 代码位置: `src/main.rs` 的 `cmd_guard` 对每个 spec 以同一改动集调用 `verify_with_changes`
- 约束: 区分治理提交与实现提交不得依赖提交者身份（ADR-001）

## Open Questions

None.
