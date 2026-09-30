---
kind: requirement
id: REQ-STAGED-COVERAGE
title: "Staged Coverage"
status: accepted
liveness: auto
tags: [knowledge, requirements, plan]
---

# Staged Coverage

## Problem

`requirements plan` 只把 active 层（`specs/` 顶层）的合约算作覆盖：
`specs/roadmap/` 里的合约不被索引，已接受但尚未实现的需求一律报 Error
`requirement-uncovered`，`lint-knowledge --gate` 随之失败。把合约挪进
`specs/` 又让 guard 因测试不存在（`skip`）而失败，pre-commit 连补测试的提交
都拦下。规格领先实现一轮的项目因此永远无法让知识门禁与合约门禁同时为绿。
`requirements status` 早已区分 active / staged / archived 三层，只有覆盖校验
忽略 staged 层。本需求让覆盖校验认识 staged 层。

## Requirements

### 覆盖分类

[REQ-STAGED-COVERAGE-INDEX] `requirements plan` MUST 把 `specs/roadmap/` 下声明 `satisfies:` 的合约作为 staged 覆盖纳入统计，且默认开启、不设开关。

[REQ-STAGED-COVERAGE-STAGED-ONLY] 一个就绪需求没有 active 合约、恰有一个 staged 合约时，plan MUST 报 Info `requirement-staged`。

[REQ-STAGED-COVERAGE-NOT-UNCOVERED] 有 staged 合约的就绪需求 MUST NOT 报 `requirement-uncovered`。

[REQ-STAGED-COVERAGE-UNCOVERED] 一个就绪需求既没有 active 合约也没有 staged 合约时，plan MUST 继续报 Error `requirement-uncovered`。

[REQ-STAGED-COVERAGE-MULTIPLE-STAGED] 一个就绪需求没有 active 合约、有两个或更多 staged 合约时，plan MUST 报 Error `requirement-multiple-staged-specs`。

[REQ-STAGED-COVERAGE-LEFT-BEHIND] 一个就绪需求恰有一个 active 合约、同时有至少一个 staged 合约时，plan MUST 报 Warning `requirement-staged-and-active`。

[REQ-STAGED-COVERAGE-ACTIVE-UNCHANGED] 有两个或更多 active 合约的就绪需求 MUST 继续报 Error `requirement-multiple-specs`，与 staged 合约数量无关。

### 下游输出

[REQ-STAGED-COVERAGE-FORWARD] `lint-knowledge` MUST 以与 plan 相同的诊断码和级别转发上述诊断。

[REQ-STAGED-COVERAGE-GATE] 覆盖类诊断只有 `requirement-staged` 时，`lint-knowledge --gate` MUST 以退出码 0 通过。

[REQ-STAGED-COVERAGE-OBLIGATIONS] 一个就绪需求没有 active 合约、恰有一个 staged 合约时，`requirements test-obligations` MUST 以该 staged 合约为义务来源，并把该义务标注为 `staged`。

## Scenarios

### 覆盖分类

Rule: REQ-STAGED-COVERAGE-INDEX

Scenario: roadmap 合约被计入覆盖
  Given 一个就绪需求 REQ-X，唯一声明 `satisfies: [REQ-X]` 的合约位于 `specs/roadmap/`
  When 运行 requirements plan --format json
  Then 输出的 REQ-X 覆盖信息中 staged 合约列表包含该合约路径

Rule: REQ-STAGED-COVERAGE-STAGED-ONLY

Scenario: 只有 staged 合约时报 requirement-staged
  Given 同上工作区
  When 运行 requirements plan --format json
  Then 输出的诊断列表包含 severity 为 info 的 `requirement-staged`

Rule: REQ-STAGED-COVERAGE-NOT-UNCOVERED

Scenario: 有 staged 合约时不报 uncovered
  Given 同上工作区
  When 运行 requirements plan --format json
  Then 输出的诊断列表不包含 `requirement-uncovered`

Rule: REQ-STAGED-COVERAGE-UNCOVERED

Scenario: 无任何合约时仍报 uncovered
  Given 一个就绪需求 REQ-Y，`specs/` 与 `specs/roadmap/` 都没有合约声明满足它
  When 运行 requirements plan --format json
  Then 输出的诊断列表包含 severity 为 error 的 `requirement-uncovered`

Rule: REQ-STAGED-COVERAGE-MULTIPLE-STAGED

Scenario: 多个 staged 合约报错
  Given 一个就绪需求 REQ-X，`specs/roadmap/` 下有两个合约都声明满足它，`specs/` 下没有
  When 运行 requirements plan --format json
  Then 输出的诊断列表包含 severity 为 error 的 `requirement-multiple-staged-specs`

Rule: REQ-STAGED-COVERAGE-LEFT-BEHIND

Scenario: 晋升后遗留副本报警告
  Given 一个就绪需求 REQ-X，`specs/` 与 `specs/roadmap/` 下各有一个合约声明满足它
  When 运行 requirements plan --format json
  Then 输出的诊断列表包含 severity 为 warning 的 `requirement-staged-and-active`

Rule: REQ-STAGED-COVERAGE-ACTIVE-UNCHANGED

Scenario: 多个 active 合约仍报错
  Given 一个就绪需求 REQ-X，`specs/` 下有两个合约、`specs/roadmap/` 下有一个合约都声明满足它
  When 运行 requirements plan --format json
  Then 输出的诊断列表包含 severity 为 error 的 `requirement-multiple-specs`

### 下游输出

Rule: REQ-STAGED-COVERAGE-FORWARD

Scenario: 知识 lint 转发 staged 诊断
  Given 一个知识库，其中一个就绪需求只有 staged 合约
  When 运行 lint-knowledge --format json
  Then 输出包含 rule 为 `requirement-staged`、severity 为 Info 的条目

Rule: REQ-STAGED-COVERAGE-GATE

Scenario: 只有 staged 覆盖时知识门禁通过
  Given 一个知识库，其中唯一的覆盖问题是一个就绪需求只有 staged 合约
  When 运行 lint-knowledge --gate
  Then 进程以退出码 0 exits

Scenario: 无覆盖时知识门禁仍失败
  Given 一个知识库，其中一个就绪需求既无 active 也无 staged 合约
  When 运行 lint-knowledge --gate
  Then 进程以非零退出码 exits，stderr 包含 `requirement-uncovered`

Rule: REQ-STAGED-COVERAGE-OBLIGATIONS

Scenario: 测试义务来自 staged 合约
  Given 一个就绪需求 REQ-X 只有一个 staged 合约，该合约含一个绑定测试的场景
  When 运行 requirements test-obligations --format json
  Then 输出的 REQ-X 义务的 spec_path 等于该 staged 合约路径，tier 字段等于 `staged`

## Dependencies

- REQ-REQUIREMENTS-COMPILER-PLAN-DAG（plan 的覆盖统计与既有的未覆盖、多合约诊断由它定义）
- REQ-REQUIREMENT-STATUS-QUERY（staged 层的界定与三轴状态查询一致）

## Source Trace

- proposal: LEP-007 Gap 1 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D1（staged 覆盖默认开启、`requirement-staged` 为 Info，2026-09-30）
- 实测证据: amberlog 2026-09-28，17 个已接受需求块的合约位于 `specs/roadmap/` 时 `lint-knowledge --gate` 以 `requirement-uncovered` 失败；把合约移入 `specs/` 后 guard 因 `skip` 失败
- 先例: README 对 `specs/roadmap/` 的描述；`requirements status` 的 active / staged / archived 三层

## Open Questions

None.
