---
kind: requirement
id: REQ-CLAUSE-GROUPING
title: "Clause Grouping"
status: proposed
liveness: auto
tags: [knowledge, lint, authoring]
---

# Clause Grouping

## Problem

`## Requirements` 与 `## Scenarios` 只有平铺的一行一条，作者无法在块内表达
「先总后分」或「主流程 → 各步分支」这类层次：任何 `###` 行都会被当成一条
没有 id、没有 BCP-14 关键字的条款（`requirement-needs-id` +
`requirement-bcp14-keyword`），并把 `requirements graph` 的条款计数撑大。
2026-09-26 在 amberlog 仓库对一个 26 条 / 11 场景的块实测：`###` 与粗体标签
都被计为条款，只有 HTML 注释对工具透明，但注释在渲染视图里不可见。
本需求让节内 `###` 标题成为分组标签：不计条款、不进 lint，并在 graph 输出
里以可选字段 `group` 暴露给下游。

## Requirements

### 分组标题的判定

[REQ-CLAUSE-GROUPING-HEADING-NOT-CLAUSE] `## Requirements` 节内以 `#` 开头的行 MUST 被解析为分组标题而非条款。

[REQ-CLAUSE-GROUPING-NOT-COUNTED] 分组标题 MUST NOT 计入条款数。

[REQ-CLAUSE-GROUPING-NO-DIAGNOSTIC] 分组标题 MUST NOT 触发任何条款级诊断。

### 分组的作用范围

[REQ-CLAUSE-GROUPING-SCOPE] 分组标题之后、下一个分组标题之前的每条条款 MUST 携带该标题文本作为 `group`。

[REQ-CLAUSE-GROUPING-EMPTY-HEADING] 标题文本为空时，其后条款的 `group` MUST 为空。

[REQ-CLAUSE-GROUPING-SCENARIOS] `## Scenarios` 节内以 `#` 开头且去掉 `#` 后不以 `Rule:` 开头的行 MUST 作为分组标题作用于其后的场景。

[REQ-CLAUSE-GROUPING-RULE-KEPT] `### Rule: <id>` 行 MUST NOT 改变当前分组（其归属语义由 REQ-CLAUSE-COVERAGE 定义）。

### 输出与兼容

[REQ-CLAUSE-GROUPING-OUTPUT-OPTIONAL] `requirements graph` 的 JSON 输出 MUST 仅在条款或场景有分组时输出 `group` 字段。

[REQ-CLAUSE-GROUPING-OUTPUT-UNCHANGED] 不含分组标题的文档，其 `requirements graph` 输出 MUST 与引入本需求前逐字节相同。

[REQ-CLAUSE-GROUPING-PRESENTATIONAL] 分组 MUST NOT 改变条款的 id、关键字、编号、覆盖判定或任何门禁的退出码。

## Scenarios

### 分组标题的判定

Rule: REQ-CLAUSE-GROUPING-HEADING-NOT-CLAUSE

Scenario: 分组标题不计为条款
  Given 一份 `## Requirements` 含 `### Envelope`、`#### Fields`、空 `###` 三个标题与三条条款的需求文档
  When 抽取条款
  Then 返回的条款数等于 3

Rule: REQ-CLAUSE-GROUPING-NOT-COUNTED

Scenario: 条款编号跳过标题
  Given 同上文档
  When lint_requirement 运行
  Then 输出中不含针对「clause 4」或更大编号的诊断

Rule: REQ-CLAUSE-GROUPING-NO-DIAGNOSTIC

Scenario: 分组标题不产生条款级诊断
  Given 一份在 `## Requirements` 与 `## Scenarios` 内各有一个 `###` 标题的需求文档
  When lint_requirement 运行
  Then 输出中不含 requirement-needs-id、requirement-bcp14-keyword 与 requirement-empty

### 分组的作用范围

Rule: REQ-CLAUSE-GROUPING-SCOPE

Scenario: 条款携带最近的分组标题
  Given 条款 A 在 `### Envelope` 之后、条款 B 在 `#### Fields` 之后
  When 抽取条款
  Then 返回的条款 A 的 group 字段等于 `Envelope`，返回的条款 B 的 group 字段等于 `Fields`

Rule: REQ-CLAUSE-GROUPING-EMPTY-HEADING

Scenario: 空标题清除分组
  Given 条款 C 在空标题 `###` 之后
  When 抽取条款
  Then 返回的条款 C 的 group 字段为 null

Rule: REQ-CLAUSE-GROUPING-SCENARIOS

Scenario: 场景携带最近的分组标题
  Given `## Scenarios` 依次含 `### Envelope`、场景 A、`### Validation chain`、场景 C
  When 解析场景
  Then 返回的场景 A 的 group 字段等于 `Envelope`，返回的场景 C 的 group 字段等于 `Validation chain`

Rule: REQ-CLAUSE-GROUPING-RULE-KEPT

Scenario: Rule 归属行不改变分组
  Given `## Scenarios` 依次含 `### Envelope`、场景 A、`### Rule: REQ-X-ALPHA`、场景 B
  When 解析场景
  Then 返回的场景 B 的 group 字段等于 `Envelope`

### 输出与兼容

Rule: REQ-CLAUSE-GROUPING-OUTPUT-OPTIONAL

Scenario: 有分组时输出 group 字段
  Given 一份含 `###` 标题的需求文档
  When 运行 requirements graph --format json
  Then 输出的 JSON 中该文档的条款对象含 group 字段

Rule: REQ-CLAUSE-GROUPING-OUTPUT-UNCHANGED

Scenario: 无分组文档的输出不变
  Given 一份不含任何 `###` 行的需求文档
  When 运行 requirements graph --format json
  Then 输出的 JSON 中不出现 group 字段

Rule: REQ-CLAUSE-GROUPING-PRESENTATIONAL

Scenario: 分组不影响门禁
  Given 同一份需求文档加与不加分组标题两个版本
  When 分别运行 lint-knowledge --gate 与 requirements graph --gate
  Then 两个版本的退出码都等于 0，且两个版本输出的条款级诊断行数相等

## Dependencies

- REQ-CLAUSE-COVERAGE（`### Rule: <id>` 行的归属语义由该需求定义，本需求不得改变）

## Source Trace

- 实测证据: amberlog@2026-09-26 `knowledge/requirements/req-amb-err-envelope-requirement.md`（26 条 / 11 场景）加 `###` 后条款数 26 → 28、新增 2 Info + 2 Warning
- 先例: `extract_requirements` 已把 `<!--` 注释行排除在条款之外；`scenario_attributions` 已对 `#` 前缀容错
- 写法动机: 用例规格的「主流程 → 各步分支、先正常后异常」排序需要块内层次

## Open Questions

None.
