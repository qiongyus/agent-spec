spec: task
name: "Batched Test Execution"
tags: [verify, guard, lifecycle, performance]
satisfies: [REQ-BATCHED-TEST-EXECUTION]
risk: B
---

## Intent

把绑定测试按 (package, features, 是否 soak) 分组，每组只调用一次 cargo test，
再按 libtest 的逐测试结果行把判定回填到各场景；guard 跨全部 active 合约合并
分组，使完整 guard 的代价约为每个 package 一次 cargo 调用而不是每个场景一次
（LEP-007 Gap 7，ADR-006 D7）。

## Decisions

- 新模块 `src/spec_verify/test_batch.rs`：`TestBatchPlan::build(scenarios, options)` 按 `GroupKey { package, features, soak }` 分组，组内 filter 去重并保持首次出现顺序。
- 组调用参数：`test`，可选 `-p <package>`，可选 `--features <features>`，`--no-fail-fast`，`--`，组内全部 filter，`--test-threads=<N>`，soak 组追加 `--include-ignored`；不带 `-q`。`N` 默认为 1。
- 逐测试结果行：`test <path> ... ok|FAILED|ignored`（`ignored` 后可带 `, <reason>`），可选以 ` <1.500s>` 结尾的耗时。
- 回填：测试路径包含场景 filter 的结果行归属该场景；无匹配 → `skip`；匹配全为 ignored → `skip`（原因含 `bound test is #[ignore]d`）；已执行匹配中有 FAILED → `fail`；否则 `pass` / `pending_review`。
- 组调用非零退出且没有任何逐测试结果行 → 该组全部场景 `uncertain`（沿用 REQ-HONEST-TEST-VERDICTS 的失败类别）；成功退出但没有逐测试结果行 → 该组场景逐个以旧参数单独调用。
- `duration_ms`：匹配行带耗时时取其和（毫秒）；否则组墙钟时间除以组内场景数，证据 `duration_estimated: true`。
- `TestRunOptions` 新增 `batch: bool`（默认 true）与 `test_threads: Option<usize>`；`verify`、`lifecycle`、`guard` 新增 `--no-batch` 与 `--test-threads <N>`。
- guard 先对全部 active 合约的场景构建一个 `TestBatchPlan` 并执行，再把结果交给各合约的 `TestVerifier`，不并行验证 spec。

## Boundaries

### Allowed Changes
- src/spec_verify/**
- src/spec_gateway/**
- src/spec_core/verify.rs
- src/main.rs
- CHANGELOG.md
- specs/task-batched-test-execution.spec.md
- specs/roadmap/task-batched-test-execution.spec.md

### Forbidden
- 不改变单个场景判定的含义（与逐场景调用相同的输入得到相同判定）
- 不并行验证多个 spec
- 不新增配置文件机制

## Out of Scope

- `[verify] test_threads` 配置项（agent-spec 目前没有配置文件机制）
- 多个分组之间的并行执行

## Completion Criteria

场景: 同 package 的场景合并为一组
  测试: test_batch_plan_groups_scenarios_across_specs_by_package
  假设 两个 spec 共五个场景，全部绑定 package `probe` 且未声明 features
  当 构建批量计划并以假 runner 执行
  那么 计划只有一个组，假 runner 的调用次数等于 1

场景: 不同 features 与 soak 场景分开成组
  测试: test_batch_plan_splits_groups_by_features_and_soak
  假设 四个绑定 package `probe` 的场景，其中一个声明 `Features: slow-io`、一个为 soak 模式，以 include_soak 构建
  当 构建批量计划
  那么 计划有三个组，只有 soak 组的参数包含 `--include-ignored`

场景: 组调用的参数形状
  测试: test_batch_group_args_shape
  假设 一个组含 package `probe`、filter `alpha` 与 `beta`
  当 生成组调用参数
  那么 参数依次为 `test`、`-p`、`probe`、`--no-fail-fast`、`--`、`alpha`、`beta`、`--test-threads=1`
  并且 以 test_threads 4 生成时最后一个参数为 `--test-threads=4`

场景: 按 filter 子串回填判定
  测试: test_batch_attribution_by_filter_substring
  假设 组输出含 `test tests::alpha_one ... ok`、`test tests::beta_one ... FAILED`、`test tests::beta_two ... ok` 与 `test tests::delta_one ... ignored`
  当 为 filter 为 `alpha`、`beta`、`gamma`、`delta` 的四个场景回填判定
  那么 verdict 依次为 `pass`、`fail`、`skip`、`skip`，`delta` 的原因包含 `bound test is #[ignore]d`

场景: human review 场景全部通过时为 pending_review
  测试: test_batch_attribution_keeps_pending_review_for_human_scenarios
  假设 组输出含 `test tests::alpha_one ... ok`，filter 为 `alpha` 的场景声明 `Review: human`
  当 回填判定
  那么 verdict 等于 `pending_review`

场景: 构建失败时整组 uncertain
  测试: test_batch_build_failure_marks_group_uncertain
  假设 组调用以非零状态退出，输出含 `error[E0425]` 且没有任何逐测试结果行
  当 回填判定
  那么 组内每个场景的 verdict 都等于 `uncertain`，failure_class 等于 `compile-error`

场景: 无法识别的输出回退为逐场景调用
  测试: test_batch_unparseable_output_falls_back_to_per_scenario_runs
  假设 假 runner 对组调用成功退出但不输出逐测试结果行，组内有两个场景
  当 执行批量计划
  那么 假 runner 的调用次数等于 3，后两次参数包含 `-q`

场景: no-batch 退回逐场景调用
  测试: test_no_batch_runs_each_scenario_separately
  假设 两个绑定 package `probe` 的场景
  当 以 batch 为 false 通过 TestVerifier 验证
  那么 假 runner 的调用次数等于 2，每次参数包含 `-q`

场景: 耗时按报告值累加或均摊
  测试: test_batch_duration_sums_reported_times_or_estimates
  假设 一个组输出为 filter `alpha` 匹配的两行报告 `<1.500s>` 与 `<0.500s>`，另一个四场景组墙钟 8000 ms 且无耗时
  当 回填判定
  那么 `alpha` 场景的 duration_ms 等于 2000，第二组每个场景的 duration_ms 等于 2000 且 duration_estimated 为 true
