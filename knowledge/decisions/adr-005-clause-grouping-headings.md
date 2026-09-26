---
kind: decision
id: ADR-005
title: "Clause Grouping Headings"
status: proposed
tags: [knowledge, lint, authoring]
---

# Clause Grouping Headings

## Context

LEP-006（2026-09-26 提出）指出：需求文档的 `## Requirements` 与
`## Scenarios` 只能平铺，作者无法在块内表达「先总后分」或「主流程 → 各步
分支」这类层次。下游工作区（amberlog）17 个块、最大 74 条条款，作者试着用
`###` 标题分组，结果每个标题都被 `extract_requirements` 当成一条无 id、无
BCP-14 关键字的条款：26 条的块条款数变成 28，lint 多报 2 条
`requirement-needs-id` 与 2 条 `requirement-bcp14-keyword`。粗体行同样被算
作条款；唯一被解析器忽略的 HTML 注释在所有渲染视图里都不可见，帮不了读者。

块内顺序问题本身是真的：条款按裁定时间追加，404 映射在第 14 条、对应的 401
在第 21 条、它们共同依附的校验链条款在第 19 条。评审者读不出「先信封形状、
后逐步映射」的意图，因为格式表达不了。

## Decision

采纳 LEP-006，节内标题为分组标签：

- **判定规则：** `## Requirements` 与 `## Scenarios` 节体内任何以 `#` 开头的
  行是分组标题。二级标题不会出现在节体里（分节器已消费），因此没有歧义。
  标题文本 = 去掉全部前导 `#` 后 trim；为空则清除当前分组。
- **不计条款、不进 lint：** 标题不进入条款列表，不参与条款编号，不触发任何
  条款级诊断；不新增诊断规则。
- **作用范围：** 标题之后、下一个标题之前的条款与场景携带该标题文本作为
  `group`。只取最近一级标题，不保留路径；`###` 与 `####` 同为「最近的标题」。
  路径形态留待有消费者需要时再扩（LEP-006 未决问题一）。
- **`Rule:` 例外：** `## Scenarios` 内去掉 `#` 后以 `Rule:` 开头的行沿用
  ADR-004 的条款归属语义，不作为分组，也不改变当前分组。
- **输出兼容：** `requirements graph` 的 JSON 仅在有分组时输出 `group`
  字段（`skip_serializing_if`）；不含标题的文档，其条款、诊断与 graph 输出
  与引入前逐字节相同。
- **落点：** `extract_requirements` 与 graph 的场景解析器各加一处标题识别，
  `RequirementClause`／`RequirementClauseView`／`RequirementScenario` 各加
  `group: Option<String>`。

## Consequences

Good, because 74 条的块可以按段落阅读，段落结构进入 graph 而不只活在渲染器里。

Good, because 「先结构后行为、行为按主流程时序、先正常后异常」这类排序纪律
可以写在文件里，而不是写在旁边的说明文档里。

Good, because 对既有语料零影响：本仓库全部知识文档的条款、诊断与 graph 输出
前后相同，由 REQ-CLAUSE-GROUPING 的场景锁定。

Bad, because `## Scenarios` 里现在有两种长得像的结构（`Rule:` 归属与分组标题），
含义不同；凡是文档化分组的地方都必须一并写明 `Rule:` 例外。

Bad, because 单级 `group` 迟早会被要求变成路径，届时同一字段要改第二次。

Bad, because 标题可以夹带断言（「### 系统 MUST 很快」）而逃过 lint；本轮靠
约定与评审，若出现再加规则（LEP-006 未决问题二）。

## Alternatives Considered

- 保留 HTML 注释作为分组标记 —— 被否：渲染视图里不可见，只帮解析器不帮读者，
  方向反了。
- 新增专用语法如 `Group: <标签>` —— 被否：Markdown 已有标题、渲染器已会画，
  再发明一种「标题」的写法需要自己的 lint 与文档。
- 把大块拆成多个小块 —— 被否（作为通用答案）：问题只是转移到
  `## Dependencies` 与文件命名上，26 条关于同一个错误信封的条款是一个需求，
  不是三个。
- `group` 直接做成标题路径列表 —— 推迟而非否决：目前没有消费者需要，字符串
  日后扩成列表代价很小。

## Source Trace

- proposal: LEP-006
  (knowledge/proposals/2026-09-26-clause-grouping.md, proposed 2026-09-26)
- 实测证据: amberlog@2026-09-26 `knowledge/requirements/req-amb-err-envelope-requirement.md`
  （26 条 / 11 场景）加 `###` 后条款数 26 → 28、新增 2 Info + 2 Warning；
  粗体行同样被计为条款；HTML 注释对工具透明但不可见
- 先例: `extract_requirements` 已排除 `<!--` 注释行；`scenario_attributions`
  已对 `#` 前缀容错；ADR-004 的 `Rule:` 归属语义
- governed requirements: REQ-CLAUSE-GROUPING
