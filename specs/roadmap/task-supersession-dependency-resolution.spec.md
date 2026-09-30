spec: task
name: "Supersession Dependency Resolution"
tags: [knowledge, requirements, plan, governance]
satisfies: [REQ-SUPERSESSION-DEPENDENCY-RESOLUTION]
risk: B
---

## Intent

`## Dependencies` 指向被 supersede 的块时，沿 `supersedes:` 解析到当前替代块，
让 `requirements plan` 的批次不再因替换而静默出错（LEP-007 Gap 5，ADR-006 D5）。
首版实现 `e38c34f` 已完成单一后继的前行解析、`declared_as`、work-units 与环检测；
本合约补上两处裁决：拆分（多个后继）解析到全部后继，无法解析升级为 Error。

## Decisions

- `RequirementGraph::follow_supersession` 返回解析出的全部当前块：遇到多个后继时对每个后继分别前行，结果去重排序；任何一支没有后继或成环时整体为 Unresolved。
- `current_dependencies` 对一个声明 id 可产出多个 `ResolvedDependency`，每个都带 `declared_as` 为声明 id；解析到依赖方自身的项被丢弃。
- `dependency-supersession-unresolved` 的级别改为 `"error"`；消息保留「at」与原因。
- 声明的依赖 id 仍原样保存在 `RequirementNode::dependencies`，`requirements export` 不受影响；`references-superseded` 不变。

## Boundaries

### Allowed Changes
- src/spec_knowledge/requirement_graph.rs
- src/spec_knowledge/requirement_plan.rs
- src/spec_knowledge/work_units.rs
- CHANGELOG.md
- specs/task-supersession-dependency-resolution.spec.md
- specs/roadmap/task-supersession-dependency-resolution.spec.md

### Forbidden
- 不改变 `RequirementNode::dependencies` 保存的声明 id
- 不改变依赖指向非 Ready 块时边脱离分批的既有规则

## Out of Scope

- 依赖指向非 Ready 块时的排期规则（留待后续提案）
- `requirements supersede` 支持一次声明多个后继

## Completion Criteria

场景: 单一后继的前行解析保持不变
  测试: test_requirement_graph_resolves_dependencies_through_supersession
  假设 REQ-P 为 superseded，REQ-P-V2 为 accepted 且 `supersedes: REQ-P`，REQ-Q 声明依赖 REQ-P
  当 计算 REQ-Q 的当前依赖
  那么 结果只含 REQ-P-V2，其 declared_as 等于 REQ-P
  并且 REQ-Q 的声明依赖仍为 REQ-P

场景: proposed 后继不替换 accepted 块且批次随解析排序
  测试: test_requirement_plan_resolves_dependencies_on_superseded_requirements
  假设 REQ-BASE 经 V2 被 V3（accepted）取代，V4 为 proposed，REQ-MID 声明依赖 REQ-BASE
  当 构建 requirement plan
  那么 批次依次为 REQ-BASE-V3、REQ-MID、REQ-TOP，Dependency 边从 REQ-BASE-V3 出发且 declared_as 等于 REQ-BASE

场景: 拆分解析到全部后继
  测试: test_requirement_graph_resolves_split_supersession_to_every_successor
  假设 REQ-X 为 superseded，REQ-X-A 与 REQ-X-B 都为 accepted 且 `supersedes: REQ-X`，REQ-C 声明依赖 REQ-X
  当 计算 REQ-C 的当前依赖并校验 graph
  那么 当前依赖为 REQ-X-A 与 REQ-X-B，两者的 declared_as 都等于 REQ-X
  并且 诊断不包含 `dependency-supersession-unresolved`

场景: 拆分后的批次排在全部后继之后
  测试: test_requirement_plan_orders_split_dependents_after_every_successor
  假设 REQ-X 被 REQ-X-A 与 REQ-X-B 拆分取代，REQ-X-B 声明依赖 REQ-X-A，REQ-C 声明依赖 REQ-X
  当 构建 requirement plan
  那么 REQ-C 所在批次序号大于 REQ-X-B 所在批次序号
  并且 REQ-C 的 work unit depends_on 等于 REQ-X-A 与 REQ-X-B

场景: 无后继时报 Error
  测试: test_requirement_graph_reports_unresolved_supersession_as_error
  假设 REQ-ORPHAN 为 superseded 且没有任何块 `supersedes: REQ-ORPHAN`，REQ-USER 声明依赖 REQ-ORPHAN
  当 校验 graph
  那么 诊断包含针对 REQ-USER、severity 为 error 的 `dependency-supersession-unresolved`
  并且 REQ-USER 的当前依赖保留声明 id REQ-ORPHAN

场景: supersedes 成环时报 Error
  测试: test_requirement_graph_reports_looping_supersession_as_error
  假设 REQ-L1 与 REQ-L2 都为 superseded 且互相 `supersedes:`，REQ-USER 声明依赖 REQ-L1
  当 校验 graph
  那么 诊断包含针对 REQ-USER、severity 为 error 的 `dependency-supersession-unresolved`

场景: 拆分中的一支无法解析时整体报 Error
  测试: test_requirement_graph_reports_split_with_dead_branch_as_error
  假设 REQ-X 被 REQ-X-A（accepted）与 REQ-X-B 拆分取代，REQ-X-B 为 superseded 且没有后继，REQ-C 声明依赖 REQ-X
  当 校验 graph
  那么 诊断包含针对 REQ-C、severity 为 error 的 `dependency-supersession-unresolved`
