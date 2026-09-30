spec: task
name: "Staged Coverage"
tags: [knowledge, requirements, plan]
satisfies: [REQ-STAGED-COVERAGE]
risk: B
---

## Intent

让 `requirements plan` 把 `specs/roadmap/` 里的合约计为 staged 覆盖：只有
staged 合约的就绪需求报 Info `requirement-staged` 而不是 Error
`requirement-uncovered`，使「规格领先实现一轮」时知识门禁与合约门禁能同时为绿
（LEP-007 Gap 1，ADR-006 D1）。

## Decisions

- staged 层的界定与 `requirements status` 相同：`build_satisfies_index(specs_dir.join("roadmap"))`，只计 `REQ-` 前缀的 id；staged 合约不成为 plan 的 spec 节点，不产生 Satisfies 边。
- `RequirementSpecCoverage` 新增 `staged_spec_paths: Vec<PathBuf>`，序列化用 `skip_serializing_if = "Vec::is_empty"`，没有 staged 合约的工作区 plan JSON 逐字节不变。
- 分类（仅对 Ready 需求）：active ≥2 → Error `requirement-multiple-specs`；active 1 且 staged ≥1 → Warning `requirement-staged-and-active`；active 0 且 staged 0 → Error `requirement-uncovered`；active 0 且 staged 1 → Info `requirement-staged`；active 0 且 staged ≥2 → Error `requirement-multiple-staged-specs`。
- `lint-knowledge` 沿用现有的 plan 诊断转发（`"info"` → `Severity::Info`），不另加转发逻辑。
- `TestObligation` 新增 `tier: Option<String>`（`skip_serializing_if = "Option::is_none"`）；无 active 合约、恰有一个 staged 合约时 `spec_path` 取该 staged 合约，`tier` 为 `"staged"`。

## Boundaries

### Allowed Changes
- src/spec_knowledge/requirement_plan.rs
- src/spec_knowledge/test_obligations.rs
- src/spec_knowledge/worktrees.rs
- src/main.rs
- CHANGELOG.md
- specs/task-staged-coverage.spec.md
- specs/roadmap/**

### Forbidden
- 不改变 `requirement-uncovered` 与 `requirement-multiple-specs` 在无 staged 合约时的判定与级别
- 不把 staged 合约计入 active 覆盖、worktree 或 Satisfies 边

## Out of Scope

- 可配置的 staged 目录位置
- staged 需求的滞留时长阈值
- `requirements worktrees` 对 staged 合约的处理

## Completion Criteria

场景: roadmap 合约被计入 staged 覆盖且只报 Info
  测试: test_requirement_plan_counts_single_staged_spec_as_scheduled
  假设 一个就绪需求 REQ-S 只有 `specs/roadmap/` 下的一个合约声明满足它
  当 构建 requirement plan
  那么 REQ-S 的覆盖 staged_spec_paths 包含该合约路径，spec_paths 为空
  并且 诊断包含 severity 为 info 的 `requirement-staged`，不包含 `requirement-uncovered`

场景: 无任何合约时仍报 uncovered
  测试: test_requirement_plan_still_reports_uncovered_without_any_spec
  假设 一个就绪需求 REQ-U 在 `specs/` 与 `specs/roadmap/` 都没有合约
  当 构建 requirement plan
  那么 诊断包含 severity 为 error 的 `requirement-uncovered`

场景: 多个 staged 合约报 Error
  测试: test_requirement_plan_rejects_multiple_staged_specs
  假设 一个就绪需求 REQ-M 在 `specs/roadmap/` 下有两个合约，`specs/` 下没有
  当 构建 requirement plan
  那么 诊断包含 severity 为 error 的 `requirement-multiple-staged-specs`

场景: 晋升后遗留 staged 副本报 Warning
  测试: test_requirement_plan_warns_when_staged_copy_is_left_behind
  假设 一个就绪需求 REQ-L 在 `specs/` 与 `specs/roadmap/` 下各有一个合约
  当 构建 requirement plan
  那么 诊断包含 severity 为 warning 的 `requirement-staged-and-active`，不包含 `requirement-uncovered`

场景: 多个 active 合约的判定不受 staged 影响
  测试: test_requirement_plan_multiple_active_specs_ignore_staged_count
  假设 一个就绪需求 REQ-A 在 `specs/` 下有两个合约、在 `specs/roadmap/` 下有一个合约
  当 构建 requirement plan
  那么 诊断包含 severity 为 error 的 `requirement-multiple-specs`，不包含 `requirement-staged-and-active`

场景: 没有 staged 合约时 plan JSON 不含新字段
  测试: test_requirement_plan_json_omits_empty_staged_paths
  假设 一个就绪需求只有一个 active 合约，`specs/roadmap/` 不存在
  当 把 requirement plan 序列化为 JSON
  那么 输出不包含 `staged_spec_paths`

场景: 知识门禁在只有 staged 覆盖时通过
  测试: test_lint_knowledge_gate_passes_with_only_staged_coverage
  假设 一个知识库的唯一覆盖问题是一个就绪需求只有 staged 合约
  当 收集 lint-knowledge 的门禁 findings
  那么 Error 级 finding 数等于 0，findings 包含 rule 为 `requirement-staged`、severity 为 Info 的条目

场景: 知识门禁在无覆盖时失败
  测试: test_lint_knowledge_gate_fails_on_uncovered_requirement
  假设 一个知识库中一个就绪需求既无 active 也无 staged 合约
  当 收集 lint-knowledge 的门禁 findings
  那么 findings 包含 rule 为 `requirement-uncovered`、severity 为 Error 的条目

场景: 测试义务来自 staged 合约
  测试: test_test_obligations_fall_back_to_single_staged_spec
  假设 一个就绪需求 REQ-S 只有一个 staged 合约，该合约的场景绑定 `test_s_one`
  当 构建 test obligations
  那么 REQ-S 义务的 spec_path 等于该 staged 合约路径，tier 等于 `staged`，explicit_selector 等于 `test_s_one`
