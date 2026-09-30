spec: task
name: "Soak Scenarios"
tags: [verify, guard, lifecycle, authoring]
satisfies: [REQ-SOAK-SCENARIOS]
risk: B
---

## Intent

让长时运行的场景声明 `Mode: soak`：默认的 verify、lifecycle 与 guard 不运行它，
以新判定 `deferred` 报告且不阻塞；`--include-soak` 时显式运行（带
`--include-ignored`）；严格验收路径不把未运行的 soak 场景当作通过。同时让
spec lint 对 `Level: soak` 与 `Mode:` 不一致、以及未知的 `Mode:` / `Review:`
取值给出警告（LEP-007 Gap 4，ADR-006 D4）。

## Decisions

- `ScenarioMode` 新增 `Soak`；`模式:` / `Mode:` 取值 `soak`（大小写不敏感）解析为 `Soak`。
- `Verdict` 新增 `Deferred`（序列化为 `deferred`），`VerificationSummary` 新增 `deferred: usize`（`#[serde(default)]`）；`is_passing_with_review_mode` 在 `"auto"` 下忽略 deferred，在 `"strict"` 下要求 deferred 为 0。
- `TestRunOptions { include_soak: bool }` 由 `SpecGateway::with_test_options` 传入 `TestVerifier`；未包含 soak 时 soak 场景不调用 runner，直接产出 `deferred`（原因 `soak scenario deferred; run with --include-soak`）。
- 包含 soak 时，soak 场景的 cargo 参数在 filter 之后追加 `--` 与 `--include-ignored`。
- `verify` 与 `lifecycle` 新增 `--include-soak` 标志；guard 不提供该标志。
- `resolve-ai` 合并后仍为 `deferred` 的场景计为未通过：合并后的报告以 strict 语义判定。
- `Scenario` 新增 `unrecognized: Vec<UnrecognizedField { field, value }>`（`skip_serializing_if = "Vec::is_empty"`）；解析器在回落到 `Standard` / `Auto` 时记录原值。
- 新增 spec linters：`soak-level-without-mode`、`unknown-scenario-mode`、`unknown-review-mode`，均为 Warning。

## Boundaries

### Allowed Changes
- src/spec_core/**
- src/spec_parser/**
- src/spec_lint/**
- src/spec_verify/**
- src/spec_gateway/**
- src/spec_report/**
- src/spec_knowledge/**
- src/spec_mcp/**
- src/main.rs
- CHANGELOG.md
- AGENTS.md
- README.md
- book/src/ch05-scenario-dsl.md
- book/src/ch07-lifecycle.md
- book/src/appendix-b-dsl.md
- skills/agent-spec-authoring/SKILL.md
- skills/agent-spec-tool-first/SKILL.md
- .claude/skills/agent-spec-authoring/SKILL.md
- .claude/skills/agent-spec-tool-first/SKILL.md
- specs/task-soak-scenarios.spec.md
- specs/roadmap/task-soak-scenarios.spec.md

### Forbidden
- 不在默认审查模式下让 `deferred` 阻塞 guard 或 lifecycle
- 不改变 `optimize` 与 `human` 的现有解析结果

## Out of Scope

- 调度或定时运行 soak 测试
- guard 的 `--include-soak`

## Completion Criteria

场景: 解析 soak 模式
  测试: test_parse_soak_mode_in_both_languages
  假设 一个场景声明 `Mode: soak`，另一个声明 `模式：soak`
  当 解析该 spec
  那么 两个场景的 mode 都等于 Soak

场景: 默认不运行 soak 场景并报告 deferred
  测试: test_soak_scenario_is_deferred_without_running_tests
  假设 一个 soak 场景绑定测试，假 runner 记录每次调用
  当 TestVerifier 在默认选项下验证该场景
  那么 假 runner 的调用次数等于 0，该场景 verdict 等于 `deferred`

场景: 汇总含 deferred 计数且默认不阻塞
  测试: test_deferred_is_counted_and_passes_in_auto_mode
  假设 一份报告含两个 `pass` 场景与一个 `deferred` 场景
  当 计算汇总与 auto 模式下的通过判定
  那么 summary 的 deferred 等于 1、passed 等于 2，通过判定为 true

场景: 严格模式下 deferred 不算通过
  测试: test_deferred_fails_strict_review_mode
  假设 同上报告
  当 计算 strict 模式下的通过判定
  那么 通过判定为 false

场景: resolve-ai 后仍为 deferred 的场景计为未通过
  测试: test_resolve_ai_keeps_deferred_non_passing
  假设 一份含一个 `deferred` 场景的报告，以及不含该场景的 decisions
  当 合并 decisions 并判定报告
  那么 合并后的报告通过判定为 false

场景: 显式运行 soak 场景带 include-ignored
  测试: test_include_soak_runs_with_include_ignored
  假设 一个 soak 场景绑定 filter `soak_case`，假 runner 返回一个失败测试的输出
  当 TestVerifier 以 include_soak 验证该场景
  那么 假 runner 收到的参数以 `soak_case`、`--`、`--include-ignored` 结尾，verdict 等于 `fail`

场景: Level soak 而无 Mode soak 时警告
  测试: test_lint_warns_soak_level_without_mode
  假设 一个场景的 selector 含 `Level: soak` 而场景未声明 `Mode:`
  当 运行 spec lint
  那么 输出包含一条 `soak-level-without-mode` Warning

场景: 未知 Mode 与 Review 取值时警告且保持回落
  测试: test_lint_warns_unknown_mode_and_review_values
  假设 一个场景声明 `Mode: soke` 与 `Review: humans`
  当 解析并运行 spec lint
  那么 场景的 mode 等于 Standard、review 等于 Auto
  并且 输出包含 `unknown-scenario-mode` 与 `unknown-review-mode` 各一条 Warning
