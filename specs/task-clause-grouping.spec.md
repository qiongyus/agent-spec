spec: task
name: "Clause Grouping"
tags: [knowledge, lint, authoring]
satisfies: [REQ-CLAUSE-GROUPING]
risk: B
---

## Intent

让需求文档能在 `## Requirements` 与 `## Scenarios` 内用 `###` 标题表达层次
（先总后分、主流程 → 各步分支），而不把标题误算成条款：标题只是分组标签，
不计数、不进 lint，graph 输出以可选 `group` 字段透出。

## Decisions

- 分组标题的判定：节体内任何以 `#` 开头的行（`##` 已被分节器消费，不会出现）。
  标题文本 = 去掉全部前导 `#` 后 trim；为空则清除当前分组。
- `RequirementClause` 与 `RequirementClauseView`、`RequirementScenario` 各加
  `group: Option<String>`；序列化用 `skip_serializing_if = "Option::is_none"`，
  无分组文档的 JSON 逐字节不变。
- `## Scenarios` 内 `### Rule: <id>` 归属行沿用 REQ-CLAUSE-COVERAGE 语义，
  不作为分组、不改变当前分组。
- 不新增诊断，不改动 lint 规则集；条款编号仍只数条款。

## Boundaries

### Allowed Changes
- src/spec_knowledge/requirement.rs
- src/spec_knowledge/requirement_graph.rs
- src/spec_knowledge/lint.rs
- src/spec_knowledge/draft_specs.rs
- src/spec_knowledge/work_units.rs
- CHANGELOG.md
- book/src/ch10-intake.md
- knowledge/requirements/req-clause-grouping.md
- specs/task-clause-grouping.spec.md

### Forbidden
- 不把粗体行、HTML 注释或其他非 `#` 行推断为分组
- 不改变 `Rule:` 归属与 clause-uncovered 的判定
- 不改变无分组文档的任何输出

## Out of Scope

- 分组的嵌套层级语义（`###` 与 `####` 目前同为「最近的标题」）
- 按分组统计覆盖率或在 `requirements status` 中展示分组
- YAML 导出方言中的分组投影

## Completion Criteria

场景: 分组标题不计为条款且条款携带分组
  测试: test_group_heading_is_not_a_clause_and_scopes_following_clauses
  假设 一份 `## Requirements` 含 `### Envelope`、`#### Fields`、空 `###` 三个标题与三条条款
  当 抽取条款
  那么 得到三条条款，group 依次为 Envelope、Fields、空

场景: 场景分组与 Rule 归属互不干扰
  测试: test_scenario_groups_follow_headings_but_rule_headings_are_attribution
  假设 `## Scenarios` 依次含 `### Envelope`、场景 A、`### Rule: REQ-X-ALPHA`、场景 B、`### Validation chain`、场景 C
  当 解析场景
  那么 A 与 B 的 group 为 Envelope，C 的 group 为 Validation chain 且步骤完整

场景: 分组标题不产生条款级诊断
  测试: test_group_headings_produce_no_clause_diagnostics
  假设 一份在 Requirements 与 Scenarios 内各有一个 `###` 标题的需求文档
  当 lint_requirement 运行
  那么 不输出 requirement-needs-id、requirement-bcp14-keyword 与 requirement-empty
