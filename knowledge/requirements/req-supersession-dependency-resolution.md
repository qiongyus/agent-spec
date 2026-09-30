---
kind: requirement
id: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION
title: "Supersession Dependency Resolution"
status: accepted
liveness: auto
tags: [knowledge, requirements, plan, governance]
---

# Supersession Dependency Resolution

## Problem

`requirements plan` 只在依赖边两端都 `Ready` 时保留它，而被 supersede 的块
永远不是 `Ready`。于是 `REQ-X` 一旦被 `REQ-X-V2` 取代，所有在
`## Dependencies` 里写着 `REQ-X` 的块都丢掉这条依赖、浮到第一批，且没有任何
错误。amberlog（2026-09-29）有 45 条这样的边：plan 给出 5 批，按 supersedes
链手工解析后是 9 层，一个本应在第 4 层的块被排进了第 1 批。治理 lint 的
`references-superseded` 只提醒作者更新声明，不修正排序；一份静默出错的批次
列表比失败更糟。

## Requirements

### 解析规则

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-FORWARD] 依赖声明的 id 指向 `status: superseded` 的块时，解析 MUST 沿 `supersedes:` 指向它的块前行，直到第一个本身未被 supersede 的块。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-ACCEPTED-KEPT] 已接受的替代块 MUST NOT 被一个 proposed 的后继替换为解析结果。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-SPLIT] 一个被 supersede 的块有多个后继时，依赖 MUST 解析到每一个后继，且每个后继按同一规则继续前行。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-SELF] 解析结果是依赖方自身时，该依赖 MUST 被丢弃。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-UNRESOLVED] 被 supersede 的块没有后继、或位于成环的 `supersedes:` 链上时，graph MUST 对每个未被 supersede 的依赖方报 Error `dependency-supersession-unresolved`。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-UNRESOLVED-ID] 无法解析的依赖边 MUST 保留其声明 id。

### 解析的使用者

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-DECLARED] requirement graph MUST 按声明原样保存依赖 id。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-EXPORT] `requirements export` 输出的依赖列表 MUST 与声明一致。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-PLAN] `requirements plan` 的 Dependency 边 MUST 从解析后的块出发。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-BATCHES] `requirements plan` 的批次 MUST 由解析后的 Dependency 边计算。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-DECLARED-AS] 解析改写了依赖 id 的 plan 边 MUST 携带字段 `declared_as`，其值为声明的 id。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-WORK-UNITS] `requirements work-units` 的 `depends_on` MUST 列出解析后的 id。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-CYCLE] 只经由替代块闭合的依赖环 MUST 报 `dependency-cycle`。

[REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-LINT-KEPT] 依赖声明指向被 supersede 的块时，治理 lint MUST 继续报 `references-superseded`。

## Scenarios

### 解析规则

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-FORWARD

Scenario: 依赖解析到当前替代块
  Given REQ-X 为 superseded，REQ-X-V2 为 accepted 且 `supersedes: REQ-X`，REQ-B 声明依赖 REQ-X
  When 运行 requirements plan --format json
  Then 输出的 Dependency 边包含从 REQ-X-V2 到 REQ-B 的边

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-ACCEPTED-KEPT

Scenario: proposed 后继不替换 accepted 块
  Given REQ-X 为 superseded，REQ-X-V2 为 accepted 且 `supersedes: REQ-X`，REQ-X-V3 为 proposed 且 `supersedes: REQ-X-V2`，REQ-B 声明依赖 REQ-X
  When 运行 requirements plan --format json
  Then 输出的 Dependency 边包含从 REQ-X-V2 到 REQ-B 的边，且不包含从 REQ-X-V3 出发的边

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-SPLIT

Scenario: 拆分解析到全部后继
  Given REQ-X 为 superseded，REQ-X-A 与 REQ-X-B 均为 accepted 且 `supersedes: REQ-X`，REQ-C 声明依赖 REQ-X
  When 运行 requirements plan --format json
  Then 输出的 Dependency 边包含从 REQ-X-A 到 REQ-C 与从 REQ-X-B 到 REQ-C 的两条边

Scenario: 拆分不再报无法解析
  Given 同上知识库
  When 运行 requirements graph --format json
  Then 输出的诊断列表不包含 `dependency-supersession-unresolved`

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-SELF

Scenario: 指向自身前身的依赖被丢弃
  Given REQ-X 为 superseded，REQ-X-V2 为 accepted、`supersedes: REQ-X` 且声明依赖 REQ-X
  When 运行 requirements plan --format json
  Then 输出的 Dependency 边不包含以 REQ-X-V2 为终点的边

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-UNRESOLVED

Scenario: 无后继时报 Error
  Given REQ-X 为 superseded 且没有任何块 `supersedes: REQ-X`，REQ-B 声明依赖 REQ-X
  When 运行 requirements graph --gate
  Then 进程以非零退出码 exits，输出包含 severity 为 error 的 `dependency-supersession-unresolved`

Scenario: supersedes 成环时报 Error
  Given REQ-X 与 REQ-Y 均为 superseded 且互相 `supersedes:`，REQ-B 声明依赖 REQ-X
  When 运行 requirements graph --format json
  Then 输出的诊断列表包含 severity 为 error 的 `dependency-supersession-unresolved`

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-UNRESOLVED-ID

Scenario: 无法解析的边保留声明 id
  Given REQ-X 为 superseded 且没有后继，REQ-B 声明依赖 REQ-X
  When 运行 requirements work-units
  Then 输出的 REQ-B 的 depends_on 包含 REQ-X

### 解析的使用者

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-DECLARED

Scenario: graph 保存声明 id
  Given REQ-B 声明依赖 superseded 的 REQ-X，REQ-X-V2 取代 REQ-X
  When 运行 requirements graph --format json
  Then 输出的 REQ-B 节点的依赖列表包含 REQ-X

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-EXPORT

Scenario: export 保持声明 id
  Given REQ-B 声明依赖 superseded 的 REQ-X，REQ-X-V2 取代 REQ-X
  When 运行 requirements export
  Then 输出的 REQ-B 依赖列表包含 REQ-X，且不包含 REQ-X-V2

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-PLAN

Scenario: Dependency 边从解析后的块出发
  Given REQ-X-V2 取代 REQ-X，REQ-B 声明依赖 REQ-X
  When 运行 requirements plan --format json
  Then 输出的 Dependency 边不包含从 REQ-X 出发的边

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-BATCHES

Scenario: 批次随解析后的边排序
  Given REQ-X-V2 取代 REQ-X，REQ-B 声明依赖 REQ-X，REQ-X-V2 与 REQ-B 都为 ready
  When 运行 requirements plan --format json
  Then 输出的批次中 REQ-B 所在批次序号大于 REQ-X-V2 所在批次序号

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-DECLARED-AS

Scenario: 改写的边携带 declared_as
  Given REQ-X-V2 取代 REQ-X，REQ-B 声明依赖 REQ-X
  When 运行 requirements plan --format json
  Then 输出的从 REQ-X-V2 到 REQ-B 的边的 declared_as 字段等于 REQ-X

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-WORK-UNITS

Scenario: work-units 列出解析后的 id
  Given REQ-X-V2 取代 REQ-X，REQ-B 声明依赖 REQ-X
  When 运行 requirements work-units
  Then 输出的 REQ-B 的 depends_on 包含 REQ-X-V2，且不包含 REQ-X

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-CYCLE

Scenario: 经替代块闭合的环被检出
  Given REQ-X-V2 取代 REQ-X，REQ-X-V2 声明依赖 REQ-B，REQ-B 声明依赖 REQ-X
  When 运行 requirements graph --format json
  Then 输出的诊断列表包含 `dependency-cycle`

Rule: REQ-SUPERSESSION-DEPENDENCY-RESOLUTION-LINT-KEPT

Scenario: references-superseded 保持不变
  Given REQ-X-V2 取代 REQ-X，REQ-B 声明依赖 REQ-X
  When 运行 lint-knowledge --format json
  Then 输出包含针对 REQ-B 的 `references-superseded`

## Dependencies

- REQ-REQUIREMENT-GOVERNANCE（supersede 的治理状态与 supersedes 链由它定义）
- REQ-REQUIREMENTS-COMPILER-PLAN-DAG（plan 的 Dependency 边与批次由它定义）

## Source Trace

- proposal: LEP-007 Gap 5 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D5（沿 supersedes 链解析；拆分解析到全部后继；无后继或成环为 Error，2026-09-30）
- 实测证据: amberlog 2026-09-29，9 个块被 supersede、45 条依赖边指向原 id；plan 5 批对手工推导 9 层（amberlog-spec `campaigns/first-release/increments-plan.md` §5）
- 首版实现: `e38c34f` 已实现前行解析、`declared_as`、work-units 与环检测；无法解析时报 Warning、把拆分视为无法解析，两处须按本需求修正
- 后续: 依赖指向非 Ready 块时依赖边脱离分批的既有规则不在本需求范围内

## Open Questions

None.
